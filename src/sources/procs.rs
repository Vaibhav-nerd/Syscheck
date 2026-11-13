//! Top processes by CPU and RAM, read directly from /proc/[pid].

use std::collections::HashMap;
use std::fs;
use std::time::Instant;

use crate::util::{read_to_string, CLK_TCK, PAGE_SIZE};

#[derive(Clone)]
pub struct ProcRow {
    pub pid: i32,
    pub name: String,
    pub user: String,
    pub cpu_pct: f64,
    pub rss: u64,
    pub mem_pct: f64,
}

#[derive(Clone)]
pub struct ProcData {
    pub by_cpu: Vec<ProcRow>,
    pub by_mem: Vec<ProcRow>,
}

struct ProcInfo {
    name: String,
    user: String,
    rss: u64,
    ticks: u64,
}

fn user_name(uid: u32, cache: &mut HashMap<u32, String>) -> String {
    if let Some(n) = cache.get(&uid) {
        return n.clone();
    }
    let name = unsafe {
        let pw = libc::getpwuid(uid);
        if !pw.is_null() {
            std::ffi::CStr::from_ptr((*pw).pw_name)
                .to_string_lossy()
                .into_owned()
        } else {
            uid.to_string()
        }
    };
    cache.insert(uid, name.clone());
    name
}

fn scan(cache: &mut HashMap<u32, String>) -> HashMap<i32, ProcInfo> {
    let mut out = HashMap::new();
    let Ok(dir) = fs::read_dir("/proc") else { return out };
    for entry in dir.flatten() {
        let fname = entry.file_name().to_string_lossy().into_owned();
        if !fname.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid: i32 = match fname.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let base = format!("/proc/{pid}");
        let Some(stat) = read_to_string(&format!("{base}/stat")) else { continue };

        // comm sits inside the last ')'; fields after it are stable.
        let Some(rparen) = stat.rfind(')') else { continue };
        let comm = stat[..rparen].split('(').last().unwrap_or("").to_string();
        let rest: Vec<&str> = stat[rparen + 2..].split_whitespace().collect();
        // rest[i] == /proc/stat field (i + 3)
        if rest.len() < 22 || rest[0] == "Z" {
            continue;
        }
        let num = |i: usize| rest[i].parse::<u64>().unwrap_or(0);
        let utime = num(11);
        let stime = num(12);
        let rss = num(21).saturating_mul(PAGE_SIZE);

        // Prefer a clean argv[0] path over the 15-char comm.
        let name = read_to_string(&format!("{base}/cmdline"))
            .map(|c| c.split('\0').next().unwrap_or("").trim().to_string())
            .filter(|argv0| {
                !argv0.is_empty()
                    && argv0.contains('/')
                    && !std::path::Path::new(argv0)
                        .file_name()
                        .map(|f| f.to_string_lossy().contains(' '))
                        .unwrap_or(true)
            })
            .map(|argv0| {
                std::path::Path::new(&argv0)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or(comm.clone())
            })
            .unwrap_or(comm);

        let uid = read_to_string(&format!("{base}/status")).and_then(|status| {
            status.lines().find(|l| l.starts_with("Uid:")).and_then(|l| {
                l.split_whitespace().nth(1).and_then(|v| v.parse::<u32>().ok())
            })
        });
        let user = uid.map(|u| user_name(u, cache)).unwrap_or_else(|| "?".into());

        out.insert(
            pid,
            ProcInfo {
                name: name.chars().take(24).collect(),
                user,
                rss,
                ticks: utime + stime,
            },
        );
    }
    out
}

pub struct ProcCollector {
    prev_ticks: Option<HashMap<i32, u64>>,
    prev_t: Option<Instant>,
    mem_total: u64,
    ncores: usize,
}

impl ProcCollector {
    pub fn new(mem_total: u64) -> Self {
        Self {
            prev_ticks: None,
            prev_t: None,
            mem_total,
            ncores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        }
    }

    pub fn measure(&mut self) -> Option<ProcData> {
        let mut cache = HashMap::new();
        let cur = scan(&mut cache);
        let now = Instant::now();
        let prev_ticks = self.prev_ticks.clone();
        let elapsed = self
            .prev_t
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|e| *e > 0.0);
        self.prev_ticks = Some(cur.iter().map(|(p, i)| (*p, i.ticks)).collect());
        self.prev_t = Some(now);
        let (Some(prev_ticks), Some(elapsed)) = (prev_ticks, elapsed) else { return None };

        let tick_sec = CLK_TCK * elapsed * self.ncores as f64;
        let mut rows: Vec<ProcRow> = cur
            .into_iter()
            .map(|(pid, p)| {
                let cpu_pct = prev_ticks
                    .get(&pid)
                    .map(|old| 100.0 * (p.ticks.saturating_sub(*old)) as f64 / tick_sec)
                    .unwrap_or(0.0);
                ProcRow {
                    pid,
                    name: p.name,
                    user: p.user,
                    cpu_pct: cpu_pct.max(0.0),
                    rss: p.rss,
                    mem_pct: if self.mem_total > 0 {
                        100.0 * p.rss as f64 / self.mem_total as f64
                    } else {
                        0.0
                    },
                }
            })
            .collect();

        rows.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
        let by_cpu: Vec<ProcRow> = rows.iter().filter(|r| r.cpu_pct > 0.05).take(10).cloned().collect();
        let mut by_mem = rows.clone();
        by_mem.sort_by(|a, b| b.rss.cmp(&a.rss));
        by_mem.truncate(10);
        Some(ProcData { by_cpu, by_mem })
    }
}
