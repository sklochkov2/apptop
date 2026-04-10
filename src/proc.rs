use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::identify;

#[derive(Debug, Clone)]
pub struct AppMemInfo {
    pub name: String,
    pub user: String,
    pub pss_kb: u64,
    pub swap_kb: u64,
    pub total_kb: u64,
    pub num_procs: u32,
    pub threads: u32,
    pub oom_max: u32,
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
            pss = parse_kb_value(rest);
        } else if let Some(rest) = line.strip_prefix("SwapPss:") {
            swap_pss = parse_kb_value(rest);
        }
    }

    Some((pss, swap_pss))
}

fn parse_kb_value(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn parse_oom_score(pid_dir: &Path, buf: &mut Vec<u8>) -> Option<u32> {
    read_proc_file(&pid_dir.join("oom_score"), buf)?;
    let s = std::str::from_utf8(buf).ok()?;
    s.trim().parse().ok()
}

fn parse_thread_count(pid_dir: &Path, buf: &mut Vec<u8>) -> Option<u32> {
    read_proc_file(&pid_dir.join("status"), buf)?;
    let content = std::str::from_utf8(buf).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Threads:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn resolve_username(uid: u32) -> String {
    let mut buf = vec![0u8; 1024];
    let mut passwd = unsafe { std::mem::zeroed::<libc::passwd>() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            &mut passwd,
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            &mut result,
        )
    };

    if ret == 0 && !result.is_null() {
        unsafe { std::ffi::CStr::from_ptr(passwd.pw_name) }
            .to_string_lossy()
            .into_owned()
    } else {
        uid.to_string()
    }
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
    let mut thr_map: HashMap<String, u32> = HashMap::new();
    let mut oom_map: HashMap<String, u32> = HashMap::new();
    let mut usr_map: HashMap<String, String> = HashMap::new();

    let mut uid_cache: HashMap<u32, String> = HashMap::new();

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

        let uid = fs::metadata(&pid_dir).map(|m| m.uid()).unwrap_or(u32::MAX);
        let username = uid_cache
            .entry(uid)
            .or_insert_with(|| resolve_username(uid))
            .clone();
        let oom = parse_oom_score(&pid_dir, &mut buf).unwrap_or(0);
        let threads = parse_thread_count(&pid_dir, &mut buf).unwrap_or(1);

        let app_name = identify::resolve(&pid_dir, &exe, &mut buf);

        *pss_map.entry(app_name.clone()).or_default() += pss;
        *swp_map.entry(app_name.clone()).or_default() += swp;
        *cnt_map.entry(app_name.clone()).or_default() += 1;
        *thr_map.entry(app_name.clone()).or_default() += threads;
        oom_map
            .entry(app_name.clone())
            .and_modify(|v| *v = (*v).max(oom))
            .or_insert(oom);
        usr_map.entry(app_name).or_insert(username);
    }

    pss_map
        .into_iter()
        .map(|(name, pss_kb)| {
            let swap_kb = swp_map.get(&name).copied().unwrap_or(0);
            AppMemInfo {
                num_procs: cnt_map.get(&name).copied().unwrap_or(0),
                threads: thr_map.get(&name).copied().unwrap_or(0),
                oom_max: oom_map.get(&name).copied().unwrap_or(0),
                user: usr_map.remove(&name).unwrap_or_default(),
                name,
                pss_kb,
                swap_kb,
                total_kb: pss_kb + swap_kb,
            }
        })
        .collect()
}
