//! CPU usage, frequency, load average and uptime from /proc.

use std::fs;
use std::time::Instant;

fn read(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

#[derive(Clone)]
struct StatVals([u64; 10]);

fn snapshot() -> Vec<(String, StatVals)> {
    let mut out = Vec::new();
    if let Some(text) = read("/proc/stat") {
        for line in text.lines() {
            if !line.starts_with("cpu") {
                continue;
            }
            let mut parts = line.split_whitespace();
            let name = parts.next().unwrap_or("").to_string();
            let is_cpu = name == "cpu"
                || name.strip_prefix("cpu").is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
            if !is_cpu {
                continue;
            }
            let mut vals = [0u64; 10];
            for (i, v) in parts.take(10).enumerate() {
                vals[i] = v.parse().unwrap_or(0);
            }
            out.push((name, StatVals(vals)));
        }
    }
    out
}

/// (busy%, total jiffy delta) between two snapshots.
fn usage(cur: &[u64; 10], prev: &[u64; 10]) -> f64 {
    let idle: u64 = cur[3] + cur[4];
    let p_idle: u64 = prev[3] + prev[4];
    let total: u64 = cur[..8].iter().sum();
    let p_total: u64 = prev[..8].iter().sum();
    let busy = (total - p_total) as f64 - (idle - p_idle) as f64;
    let total = (total - p_total) as f64;
    if total <= 0.0 {
        0.0
    } else {
        100.0 * busy / total
    }
}

fn cpu_model() -> String {
    read("/proc/cpuinfo")
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

fn cpu_freq_mhz() -> Option<f64> {
    let mut freqs = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy().into_owned();
            if !name.starts_with("cpu") {
                continue;
            }
            let digits = &name[3..];
            if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Ok(f) = fs::read_to_string(e.path().join("cpufreq/scaling_cur_freq")) {
                if let Ok(khz) = f.trim().parse::<f64>() {
                    freqs.push(khz / 1000.0);
                }
            }
        }
    }
    if freqs.is_empty() {
        if let Some(info) = read("/proc/cpuinfo") {
            for line in info.lines() {
                if let Some(v) = line.strip_prefix("cpu MHz") {
                    if let Some((_, v)) = v.split_once(':') {
                        if let Ok(mhz) = v.trim().parse::<f64>() {
                            freqs.push(mhz);
                        }
                    }
                }
            }
        }
    }
    if freqs.is_empty() {
        None
    } else {
        Some(freqs.iter().sum::<f64>() / freqs.len() as f64)
    }
}

pub fn load_average() -> [f64; 3] {
    read("/proc/loadavg")
        .and_then(|t| {
            let mut it = t.split_whitespace().take(3);
            let a = it.next()?.parse::<f64>().ok()?;
            let b = it.next()?.parse::<f64>().ok()?;
            let c = it.next()?.parse::<f64>().ok()?;
            Some([a, b, c])
        })
        .unwrap_or([0.0; 3])
}

pub fn uptime_secs() -> f64 {
    read("/proc/uptime")
        .and_then(|t| t.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.0)
}

#[derive(Clone)]
pub struct CpuData {
    pub overall: f64,
    pub cores: Vec<f64>,
    pub ncores: usize,
    pub model: String,
    pub freq_mhz: Option<f64>,
    pub load: [f64; 3],
}

pub struct CpuCollector {
    prev: Option<Vec<(String, StatVals)>>,
    prev_t: Option<Instant>,
    pub model: String,
    pub ncores: usize,
}

impl CpuCollector {
    pub fn new() -> Self {
        let ncores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        Self {
            prev: None,
            prev_t: None,
            model: cpu_model(),
            ncores,
        }
    }

    pub fn measure(&mut self) -> Option<CpuData> {
        let cur = snapshot();
        let now = Instant::now();
        let prev = self.prev.clone();
        let elapsed = self
            .prev_t
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|e| *e > 0.0);
        self.prev = Some(cur.clone());
        self.prev_t = Some(now);
        let (Some(prev), Some(_)) = (prev, elapsed) else { return None };

        let get = |s: &[(String, StatVals)], n: &str| s.iter().find(|(name, _)| name == n).map(|(_, v)| v.0);

        let mut cores = Vec::new();
        for (name, vals) in &cur {
            if name == "cpu" {
                continue;
            }
            if let Some(p) = get(&prev, name) {
                cores.push(usage(&vals.0, &p));
            }
        }
        let overall = match (get(&cur, "cpu"), get(&prev, "cpu")) {
            (Some(c), Some(p)) => usage(&c, &p),
            _ => return None,
        };
        Some(CpuData {
            overall,
            cores: cores.into_iter().map(|c| c.max(0.0)).collect(),
            ncores: self.ncores,
            model: self.model.clone(),
            freq_mhz: cpu_freq_mhz(),
            load: load_average(),
        })
    }
}
