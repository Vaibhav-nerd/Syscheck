//! RAM and swap usage from /proc/meminfo.

use std::fs;

#[derive(Clone)]
pub struct MemData {
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub pct: f64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub swap_pct: f64,
}

fn meminfo() -> Vec<(String, u64)> {
    let keys: [&str; 9] = [
        "MemTotal", "MemAvailable", "MemFree", "Buffers", "Cached",
        "SReclaimable", "Shmem", "SwapTotal", "SwapFree",
    ];
    let mut out = Vec::new();
    if let Ok(text) = fs::read_to_string("/proc/meminfo") {
        for line in text.lines() {
            let Some((key, val)) = line.split_once(':') else { continue };
            if keys.contains(&key) {
                if let Some(first) = val.trim().split_whitespace().next() {
                    if let Ok(kb) = first.parse::<u64>() {
                        out.push((key.to_string(), kb * 1024));
                    }
                }
            }
        }
    }
    out
}

pub fn measure() -> MemData {
    let m = meminfo();
    let get = |k: &str| m.iter().find(|(key, _)| key == k).map(|(_, v)| *v).unwrap_or(0);
    let total = get("MemTotal");
    let available = if get("MemAvailable") > 0 { get("MemAvailable") } else { get("MemFree") };
    let used = total.saturating_sub(available);
    let swap_total = get("SwapTotal");
    let swap_used = swap_total.saturating_sub(get("SwapFree"));
    let cached = (get("Cached") + get("SReclaimable")).saturating_sub(get("Shmem"));
    MemData {
        total,
        available,
        used,
        pct: if total > 0 { 100.0 * used as f64 / total as f64 } else { 0.0 },
        cached,
        swap_total,
        swap_used,
        swap_pct: if swap_total > 0 { 100.0 * swap_used as f64 / swap_total as f64 } else { 0.0 },
    }
}
