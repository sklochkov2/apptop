use std::path::Path;

use crate::proc::read_proc_file;

/// Resolves a human-readable application identity for a process using a
/// priority cascade:
///   1. systemd cgroup scope name (best for desktop / systemd-managed apps)
///   2. Interpreter-aware cmdline parsing (python, node, java, …)
///   3. `GIO_LAUNCHED_DESKTOP_FILE` / `BAMF_DESKTOP_FILE_HINT` env vars
///   4. Raw executable path (fallback — always works)
///
/// Each level is attempted lazily and in cost order: cgroup and cmdline are
/// small files (~100-700 bytes), while environ can be multiple KB and is
/// tried last to avoid the expense for the common case.
pub fn resolve(pid_dir: &Path, exe: &str, buf: &mut Vec<u8>) -> String {
    try_cgroup(pid_dir, exe, buf)
        .or_else(|| try_interpreter(pid_dir, exe, buf))
        .or_else(|| try_environ(pid_dir, buf))
        .unwrap_or_else(|| exe.to_string())
}

// ---------------------------------------------------------------------------
// 1. Cgroup scope / service
// ---------------------------------------------------------------------------

fn try_cgroup(pid_dir: &Path, exe: &str, buf: &mut Vec<u8>) -> Option<String> {
    read_proc_file(&pid_dir.join("cgroup"), buf)?;
    let content = std::str::from_utf8(buf).ok()?;
    let cg_path = cgroup_path(content)?;
    let component = cg_path.rsplit('/').next()?;

    if component.starts_with("vte-spawn-")
        || component.starts_with("session-")
        || component == "init.scope"
    {
        return None;
    }

    let name = parse_app_scope(component)
        .or_else(|| parse_snap_unit(component))
        .or_else(|| parse_system_service(&cg_path))?;

    // Electron/CEF apps that don't override their WM_CLASS / Wayland app-id
    // inherit Chromium's default, so systemd places them in an
    // `org.chromium.Chromium` scope even though they're a different app
    // (e.g. Cisco Webex).  Cross-check against the exe path: if it doesn't
    // look like an actual Chromium binary, let the cascade fall through to a
    // more accurate heuristic.
    if is_chromium_generic(&name) && !exe_looks_chromium(exe) {
        return None;
    }

    Some(name)
}

/// Extract the cgroup path from `/proc/<pid>/cgroup` (supports v1 + v2).
fn cgroup_path(content: &str) -> Option<String> {
    // cgroup v2 (unified hierarchy): single line "0::<path>"
    for line in content.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            return Some(strip_deleted(path).to_string());
        }
    }
    // cgroup v1 fallback: look for the systemd named hierarchy
    for line in content.lines() {
        if line.contains("name=systemd:") {
            let path = line.rsplit(':').next()?;
            return Some(strip_deleted(path).to_string());
        }
    }
    None
}

/// `app-gnome-firefox-1167899.scope`             → "firefox"
/// `app-gnome-google\x2dchrome-258456.scope`     → "google-chrome"
/// `app-org.chromium.Chromium-1425167.scope`      → "org.chromium.Chromium"
/// `app-gnome-org.gnome.Software-9150.scope`      → "org.gnome.Software"
fn parse_app_scope(scope: &str) -> Option<String> {
    let scope = strip_deleted(scope);
    let inner = scope.strip_prefix("app-")?.strip_suffix(".scope")?;

    let last_sep = inner.rfind('-')?;
    let raw_name = &inner[..last_sep];
    if raw_name.is_empty() {
        return None;
    }

    let raw_name = strip_launcher_prefix(raw_name);
    let name = unescape_systemd(raw_name);

    if is_terminal(&name) {
        return None;
    }
    Some(name)
}

/// `snap.cups.cups-browsed.service`  → "snap:cups"
/// `snap.chromium.chromium.1234.scope` → "snap:chromium"
fn parse_snap_unit(unit: &str) -> Option<String> {
    let rest = unit.strip_prefix("snap.")?;
    let rest = rest
        .strip_suffix(".service")
        .or_else(|| rest.strip_suffix(".scope"))
        .unwrap_or(rest);
    let snap_name = rest.split('.').next().filter(|n| !n.is_empty())?;
    Some(format!("snap:{snap_name}"))
}

/// Only for services directly under `system.slice`:
/// `/system.slice/mysql.service` → "mysql"
fn parse_system_service(cg_path: &str) -> Option<String> {
    if !cg_path.contains("/system.slice/") {
        return None;
    }
    let last = cg_path.rsplit('/').next()?;
    let name = last.strip_suffix(".service")?;
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

// ---------------------------------------------------------------------------
// 2. Interpreter-aware cmdline sniffing
// ---------------------------------------------------------------------------

fn try_interpreter(pid_dir: &Path, exe: &str, buf: &mut Vec<u8>) -> Option<String> {
    let basename = exe.rsplit('/').next().unwrap_or(exe);
    let label = interpreter_label(basename)?;

    read_proc_file(&pid_dir.join("cmdline"), buf)?;
    let args: Vec<&str> = buf
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| std::str::from_utf8(s).ok())
        .collect();

    // argv[0] is the interpreter itself; we need at least one more arg
    if args.len() < 2 {
        return None;
    }

    let script = match label {
        "python" => extract_python_script(&args[1..]),
        "java" => extract_java_main(&args[1..]),
        _ => first_positional(&args[1..]),
    }?;

    Some(format!("{label}: {script}"))
}

fn interpreter_label(basename: &str) -> Option<&'static str> {
    if basename.starts_with("python") {
        return Some("python");
    }
    if basename == "node" || basename == "nodejs" {
        return Some("node");
    }
    if basename.starts_with("ruby") {
        return Some("ruby");
    }
    if basename.starts_with("perl") {
        return Some("perl");
    }
    if basename == "java" {
        return Some("java");
    }
    if basename == "dotnet" {
        return Some("dotnet");
    }
    if basename.starts_with("php") {
        return Some("php");
    }
    None
}

/// `["-m", "gunicorn", "…"]` → "-m gunicorn"
/// `["/opt/myapp/main.py", "…"]` → "/opt/myapp/main.py"
/// `["-u", "-c", "…"]` → "-c"
fn extract_python_script(args: &[&str]) -> Option<String> {
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        if arg == "-m" {
            return it.next().map(|m| format!("-m {m}"));
        }
        if arg == "-c" {
            return Some("-c".into());
        }
        if !arg.starts_with('-') {
            return Some(arg.to_string());
        }
    }
    None
}

/// `["-jar", "/opt/idea/lib/app.jar", …]` → "/opt/idea/lib/app.jar"
/// `["-Xmx4g", "com.example.Main", …]` → "com.example.Main"
fn extract_java_main(args: &[&str]) -> Option<String> {
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        if arg == "-jar" {
            return it.next().map(|j| j.to_string());
        }
        if arg.starts_with('-') {
            // flags that consume the next argument as a value
            if matches!(
                arg,
                "-cp"
                    | "-classpath"
                    | "--class-path"
                    | "--module-path"
                    | "-p"
                    | "--add-modules"
                    | "--add-exports"
                    | "--add-opens"
                    | "--add-reads"
            ) {
                it.next();
            }
            continue;
        }
        return Some(arg.to_string());
    }
    None
}

/// First non-flag argument.
fn first_positional(args: &[&str]) -> Option<String> {
    args.iter()
        .find(|a| !a.starts_with('-'))
        .map(|a| a.to_string())
}

// ---------------------------------------------------------------------------
// 3. Desktop-environment environment variable hints
// ---------------------------------------------------------------------------

fn try_environ(pid_dir: &Path, buf: &mut Vec<u8>) -> Option<String> {
    read_proc_file(&pid_dir.join("environ"), buf)?;

    for entry in buf.split(|&b| b == 0) {
        let Ok(s) = std::str::from_utf8(entry) else {
            continue;
        };
        for prefix in ["GIO_LAUNCHED_DESKTOP_FILE=", "BAMF_DESKTOP_FILE_HINT="] {
            if let Some(path) = s.strip_prefix(prefix) {
                if let Some(name) = desktop_file_to_name(path) {
                    if !is_terminal(&name) {
                        return Some(name);
                    }
                }
            }
        }
    }
    None
}

fn desktop_file_to_name(path: &str) -> Option<String> {
    let filename = path.rsplit('/').next()?;
    let name = filename.strip_suffix(".desktop").unwrap_or(filename);
    (!name.is_empty()).then(|| name.to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip the DE-specific launcher prefix from a raw scope name segment.
/// `gnome-firefox`            → `firefox`
/// `gnome-org.gnome.Software` → `org.gnome.Software`
/// `org.chromium.Chromium`    → `org.chromium.Chromium` (no match, unchanged)
fn strip_launcher_prefix(s: &str) -> &str {
    const PREFIXES: &[&str] = &["gnome-", "kde-", "plasma-", "xfce-", "sway-", "hyprland-"];
    for prefix in PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            if !rest.is_empty() {
                return rest;
            }
        }
    }
    s
}

/// Undo systemd's `\xNN` hex escaping (e.g. `\x2d` → `-`).
fn unescape_systemd(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if chars.clone().next() == Some('x') {
                chars.next(); // consume 'x'
                let h1 = chars.next();
                let h2 = chars.next();
                if let (Some(a), Some(b)) = (h1, h2) {
                    let mut hex = [0u8; 2];
                    hex[0] = a as u8;
                    hex[1] = b as u8;
                    if let Ok(byte) =
                        u8::from_str_radix(std::str::from_utf8(&hex).unwrap_or(""), 16)
                    {
                        out.push(byte as char);
                        continue;
                    }
                    // malformed escape — emit literally
                    out.push('\\');
                    out.push('x');
                    out.push(a);
                    out.push(b);
                } else {
                    out.push('\\');
                    out.push('x');
                    if let Some(a) = h1 {
                        out.push(a);
                    }
                }
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_terminal(name: &str) -> bool {
    const TERMINALS: &[&str] = &[
        "gnome-terminal-server",
        "org.gnome.Terminal",
        "org.gnome.Ptyxis",
        "konsole",
        "org.kde.konsole",
        "xterm",
        "alacritty",
        "org.alacritty.Alacritty",
        "kitty",
        "wezterm",
        "wezterm-gui",
        "foot",
        "tilix",
        "terminator",
        "sakura",
        "guake",
        "xfce4-terminal",
        "mate-terminal",
        "lxterminal",
    ];
    TERMINALS.iter().any(|&t| name == t)
}

fn is_chromium_generic(name: &str) -> bool {
    name.eq_ignore_ascii_case("org.chromium.Chromium")
        || name.eq_ignore_ascii_case("chromium")
        || name.eq_ignore_ascii_case("chromium-browser")
}

fn exe_looks_chromium(exe: &str) -> bool {
    exe.as_bytes()
        .windows(8)
        .any(|w| w.eq_ignore_ascii_case(b"chromium"))
}

fn strip_deleted(s: &str) -> &str {
    s.strip_suffix(" (deleted)").unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_gnome_firefox() {
        assert_eq!(
            parse_app_scope("app-gnome-firefox-1167899.scope"),
            Some("firefox".into())
        );
    }

    #[test]
    fn scope_escaped_hyphens() {
        assert_eq!(
            parse_app_scope("app-gnome-google\\x2dchrome-258456.scope"),
            Some("google-chrome".into())
        );
    }

    #[test]
    fn scope_flatpak_style() {
        assert_eq!(
            parse_app_scope("app-org.chromium.Chromium-1425167.scope"),
            Some("org.chromium.Chromium".into())
        );
    }

    #[test]
    fn scope_gnome_dotted_app() {
        assert_eq!(
            parse_app_scope("app-gnome-org.gnome.Software-9150.scope"),
            Some("org.gnome.Software".into())
        );
    }

    #[test]
    fn scope_deleted_suffix() {
        assert_eq!(
            parse_app_scope("app-gnome-nvidia\\x2dsettings\\x2dautostart-9158.scope (deleted)"),
            Some("nvidia-settings-autostart".into())
        );
    }

    #[test]
    fn scope_terminal_skipped() {
        assert_eq!(
            parse_app_scope("app-gnome-gnome\\x2dterminal\\x2dserver-1234.scope"),
            None
        );
    }

    #[test]
    fn snap_service() {
        assert_eq!(
            parse_snap_unit("snap.cups.cups-browsed.service"),
            Some("snap:cups".into())
        );
    }

    #[test]
    fn snap_scope() {
        assert_eq!(
            parse_snap_unit("snap.chromium.chromium.1234.scope"),
            Some("snap:chromium".into())
        );
    }

    #[test]
    fn system_service_mysql() {
        assert_eq!(
            parse_system_service("/system.slice/mysql.service"),
            Some("mysql".into())
        );
    }

    #[test]
    fn system_service_ignores_user_slice() {
        assert_eq!(
            parse_system_service(
                "/user.slice/user-1000.slice/user@1000.service/app.slice/gnome-terminal-server.service"
            ),
            None
        );
    }

    #[test]
    fn python_module() {
        assert_eq!(
            extract_python_script(&["-m", "gunicorn", "main:app"]),
            Some("-m gunicorn".into())
        );
    }

    #[test]
    fn python_script() {
        assert_eq!(
            extract_python_script(&["/opt/myapp/server.py", "--port", "8080"]),
            Some("/opt/myapp/server.py".into())
        );
    }

    #[test]
    fn python_dash_c() {
        assert_eq!(
            extract_python_script(&["-c", "print(1)"]),
            Some("-c".into())
        );
    }

    #[test]
    fn java_jar() {
        assert_eq!(
            extract_java_main(&["-Xmx4g", "-jar", "/opt/idea/lib/app.jar"]),
            Some("/opt/idea/lib/app.jar".into())
        );
    }

    #[test]
    fn java_main_class() {
        assert_eq!(
            extract_java_main(&["-cp", "lib/*", "com.example.Main"]),
            Some("com.example.Main".into())
        );
    }

    #[test]
    fn unescape_mixed() {
        assert_eq!(unescape_systemd("hello\\x2dworld"), "hello-world");
        assert_eq!(unescape_systemd("no_escapes"), "no_escapes");
        assert_eq!(unescape_systemd("a\\x2db\\x2ec"), "a-b.c");
    }

    #[test]
    fn chromium_generic_matches() {
        assert!(is_chromium_generic("org.chromium.Chromium"));
        assert!(is_chromium_generic("chromium"));
        assert!(is_chromium_generic("chromium-browser"));
        assert!(is_chromium_generic("Chromium-Browser"));
        assert!(!is_chromium_generic("google-chrome"));
        assert!(!is_chromium_generic("firefox"));
    }

    #[test]
    fn exe_looks_chromium_positive() {
        assert!(exe_looks_chromium("/usr/lib/chromium/chromium"));
        assert!(exe_looks_chromium(
            "/usr/lib/chromium-browser/chromium-browser"
        ));
        assert!(exe_looks_chromium(
            "/snap/chromium/current/usr/lib/chromium-browser/chrome"
        ));
    }

    #[test]
    fn exe_looks_chromium_negative() {
        assert!(!exe_looks_chromium("/opt/Webex/bin/CiscoCollabHost"));
        assert!(!exe_looks_chromium("/usr/lib/firefox/firefox"));
        assert!(!exe_looks_chromium("/opt/google/chrome/chrome"));
    }
}
