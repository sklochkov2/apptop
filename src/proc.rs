use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use crate::identify;

#[derive(Debug, Clone)]
pub struct AppMemInfo {
    pub name: String,
    pub pss_kib: u64,
    pub swap_kib: u64,
    pub total_kib: u64,
    pub num_procs: u32,
}

/// Read a proc file into a pre-allocated buffer using a single data read
/// (proc files report size 0, so `fs::read_to_string` starts with a tiny
/// buffer and grows it across many syscalls; pre-allocating avoids that).
pub fn read_proc_file(path: &Path, buf: &mut Vec<u8>) -> Option<()> {
    buf.clear();
    let mut file = File::open(path).ok()?;
    file.read_to_end(buf).ok()?;
    Some(())
}

fn parse_smaps_rollup(pid_dir: &Path, buf: &mut Vec<u8>) -> Option<(u64, u64)> {
    read_proc_file(&pid_dir.join("smaps_rollup"), buf)?;
    let content = std::str::from_utf8(buf).ok()?;

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

    let mut buf = Vec::with_capacity(8192);

    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname_str = fname.to_string_lossy();
        if !fname_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid_dir = entry.path();

        let Some(exe) = resolve_exe(&pid_dir) else {
            continue;
        };
        let Some((pss, swp)) = parse_smaps_rollup(&pid_dir, &mut buf) else {
            continue;
        };

        let app_name = identify::resolve(&pid_dir, &exe, &mut buf);

        *pss_map.entry(app_name.clone()).or_default() += pss;
        *swp_map.entry(app_name.clone()).or_default() += swp;
        *cnt_map.entry(app_name).or_default() += 1;
    }

    pss_map
        .into_iter()
        .map(|(name, pss_kib)| {
            let swap_kib = swp_map.get(&name).copied().unwrap_or(0);
            AppMemInfo {
                num_procs: cnt_map.get(&name).copied().unwrap_or(0),
                name,
                pss_kib,
                swap_kib,
                total_kib: pss_kib + swap_kib,
            }
        })
        .collect()
}
