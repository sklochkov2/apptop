use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AppMemInfo {
    pub exe: String,
    pub pss_kib: u64,
    pub swap_kib: u64,
    pub total_kib: u64,
    pub num_procs: u32,
}

fn parse_smaps_rollup(pid_dir: &Path) -> Option<(u64, u64)> {
    let smaps = pid_dir.join("smaps_rollup");
    let content = fs::read_to_string(smaps).ok()?;

    let mut pss: u64 = 0;
    let mut swap_pss: u64 = 0;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Pss:") {
            pss = parse_kib_value(rest);
        } else if let Some(rest) = line.strip_prefix("SwapPss:") {
            swap_pss = parse_kib_value(rest);
        }
    }

    Some((pss, swap_pss))
}

fn parse_kib_value(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn resolve_exe(pid_dir: &Path) -> Option<String> {
    let exe_link = pid_dir.join("exe");
    let target = fs::read_link(exe_link).ok()?;
    let s = target.to_string_lossy().to_string();
    if s.is_empty() || s.ends_with(" (deleted)") {
        return None;
    }
    Some(s)
}

pub fn collect_app_memory() -> Vec<AppMemInfo> {
    let mut pss_map: HashMap<String, u64> = HashMap::new();
    let mut swp_map: HashMap<String, u64> = HashMap::new();
    let mut cnt_map: HashMap<String, u32> = HashMap::new();

    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid_dir = entry.path();

        let Some(exe) = resolve_exe(&pid_dir) else {
            continue;
        };
        let Some((pss, swp)) = parse_smaps_rollup(&pid_dir) else {
            continue;
        };

        *pss_map.entry(exe.clone()).or_default() += pss;
        *swp_map.entry(exe.clone()).or_default() += swp;
        *cnt_map.entry(exe).or_default() += 1;
    }

    pss_map
        .into_iter()
        .map(|(exe, pss_kib)| {
            let swap_kib = swp_map.get(&exe).copied().unwrap_or(0);
            AppMemInfo {
                num_procs: cnt_map.get(&exe).copied().unwrap_or(0),
                exe,
                pss_kib,
                swap_kib,
                total_kib: pss_kib + swap_kib,
            }
        })
        .collect()
}
