# apptop

A `top`-like terminal utility that aggregates memory usage **by application** rather than by individual process. Many desktop applications (browsers, Electron apps, etc.) spawn multiple processes that share memory — `apptop` reads `/proc/<pid>/smaps_rollup` and rolls up proportional set size (PSS) and swap usage by application, giving you one line per app.

## Building

```bash
make build          # debug build
make release        # optimized release build
```

Or directly with Cargo:

```bash
cargo build --release
```

## Installation

```bash
sudo make install           # installs to /usr/local/bin
sudo make PREFIX=/usr install   # installs to /usr/bin
```

## Usage

```
apptop [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-d`, `--delay <SECS>` | Refresh interval in seconds | `2.0` |
| `-b`, `--batch` | Batch mode (non-interactive, prints to stdout) | off |
| `-n`, `--iterations <N>` | Number of iterations (implies batch mode; 0 = unlimited) | `0` |
| `-s`, `--sort <COL>` | Sort column: `pss`, `swap`, `total`, `procs`, `name` | `total` |

### Interactive Keys

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `s` | Cycle sort column |
| `r` | Reverse sort order |
| `1`–`5` | Sort by column (1=NPROC, 2=PSS, 3=SWAP, 4=TOTAL, 5=COMMAND) |
| `↑`/`k`, `↓`/`j` | Scroll up / down |
| `PgUp` / `PgDn` | Page up / down |
| `Home` / `End` | Jump to top / bottom |

### Examples

```bash
# Interactive TUI, refresh every 3 seconds, sorted by PSS
apptop -d 3 -s pss

# Batch: print a single snapshot and exit
apptop -b -n 1

# Batch: print every 5 seconds until interrupted
apptop -b -d 5
```

## How It Works

### Memory collection

For every process directory in `/proc`, `apptop`:

1. Resolves `/proc/<pid>/exe` to get the executable path.
2. Reads `/proc/<pid>/smaps_rollup` and extracts **Pss** and **SwapPss** (in KiB).
3. Identifies which *application* the process belongs to (see below).
4. Aggregates memory values by resolved application identity.
5. Displays sorted results in a terminal UI or batch output.

**PSS** (Proportional Set Size) fairly splits shared pages among all processes that map them, so the sum across all applications gives a realistic picture of total physical memory consumption.

### Application identification

Simply grouping by executable path is insufficient — all Python apps would merge under `/usr/bin/python3`, while a browser and its GPU/renderer helpers already share the same binary. `apptop` uses a hybrid cascade that tries progressively more expensive heuristics, stopping at the first one that produces a useful identity:

| Priority | Source | What it reads | Example result |
|----------|--------|---------------|----------------|
| 1 | **systemd cgroup scope** | `/proc/<pid>/cgroup` | `firefox`, `google-chrome`, `snap:chromium` |
| 2 | **Desktop environment env vars** | `/proc/<pid>/environ` | `firefox` (from `GIO_LAUNCHED_DESKTOP_FILE`) |
| 3 | **Interpreter-aware cmdline** | `/proc/<pid>/cmdline` | `python: -m gunicorn`, `java: /opt/app.jar` |
| 4 | **Executable path** (fallback) | `/proc/<pid>/exe` | `/usr/bin/gnome-shell` |

#### 1. Cgroup scope (best for desktop & systemd-managed apps)

On systemd-based systems, desktop environments assign each launched application its own **scope unit**. `apptop` parses the cgroup path and extracts a clean app name:

- `app-gnome-firefox-<pid>.scope` → **firefox**
- `app-gnome-google\x2dchrome-<pid>.scope` → **google-chrome** (systemd `\xNN` escapes are decoded)
- `app-org.chromium.Chromium-<pid>.scope` → **org.chromium.Chromium**
- `snap.cups.cupsd.service` → **snap:cups**
- `/system.slice/mysql.service` → **mysql**

Desktop-environment launcher prefixes (`gnome-`, `kde-`, `plasma-`, …) are stripped automatically. Terminal emulator scopes (`gnome-terminal-server`, `alacritty`, `kitty`, …) and VTE child scopes (`vte-spawn-*`) are recognized and skipped so that processes launched *inside* a terminal fall through to the next identification level.

**Chromium cross-validation:** Electron/CEF apps that don't set their own `WM_CLASS` or Wayland `app-id` inherit Chromium's default, causing systemd to place them in an `org.chromium.Chromium` scope despite being a different application (e.g. Cisco Webex). To handle this, `apptop` cross-checks Chromium-generic cgroup names against the `/proc/<pid>/exe` path — if the executable doesn't look like an actual Chromium binary, the cgroup result is discarded and the cascade falls through to a later heuristic that can provide the real identity.

This is the strongest signal: all of Firefox's renderer/GPU/utility processes share one scope, and Steam's `steam` + `steamwebhelper` binaries are correctly merged into a single **steam** entry.

#### 2. Environment variable hints

If the cgroup didn't produce a useful identity, `apptop` checks `GIO_LAUNCHED_DESKTOP_FILE` and `BAMF_DESKTOP_FILE_HINT` in `/proc/<pid>/environ`. These are set by GNOME/Unity when launching apps from `.desktop` files. The `.desktop` filename (minus the extension) becomes the app name. Terminal-related desktop files are skipped.

#### 3. Interpreter-aware cmdline parsing

For known interpreter binaries — `python`, `node`, `ruby`, `perl`, `java`, `dotnet`, `php` — `apptop` reads `/proc/<pid>/cmdline` and extracts the actual script or module being run:

| cmdline | Resolved identity |
|---------|-------------------|
| `python3 -m gunicorn main:app` | **python: -m gunicorn** |
| `python3 /opt/myapp/server.py` | **python: /opt/myapp/server.py** |
| `java -jar /opt/idea/lib/app.jar` | **java: /opt/idea/lib/app.jar** |
| `java -cp lib/* com.example.Main` | **java: com.example.Main** |
| `node /usr/lib/slack/app.js` | **node: /usr/lib/slack/app.js** |

This correctly separates different Python/Node/Java applications that would otherwise all collapse into a single `/usr/bin/python3` entry.

#### 4. Executable path (fallback)

When none of the above signals apply (kernel-adjacent daemons, statically linked binaries, etc.), the raw `/proc/<pid>/exe` symlink target is used. This is the same behaviour as the traditional `ps`-based approach and always works.

## Requirements

- Linux (uses `/proc` filesystem)
- Rust 2024 edition (1.85+)

## License

MIT
