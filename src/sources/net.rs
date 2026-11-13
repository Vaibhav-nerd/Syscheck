//! Per-interface network throughput from /proc/net/dev.

use std::collections::HashMap;
use std::fs;
use std::time::Instant;

use crate::util::read_to_string;

#[derive(Clone)]
pub struct NetIf {
    pub iface: String,
    pub state: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
}

fn operstate(iface: &str) -> String {
    fs::read_to_string(format!("/sys/class/net/{iface}/operstate"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".into())
}

fn snapshot() -> HashMap<String, (u64, u64)> {
    let mut map = HashMap::new();
    if let Some(text) = read_to_string("/proc/net/dev") {
        for (i, line) in text.lines().enumerate() {
            if i < 2 {
                continue; // header
            }
            let Some((iface, rest)) = line.split_once(':') else { continue };
            let vals: Vec<&str> = rest.split_whitespace().collect();
            if vals.len() < 16 {
                continue;
            }
            let parse = |v: &str| v.parse::<u64>().unwrap_or(0);
            map.insert(iface.trim().to_string(), (parse(vals[0]), parse(vals[8])));
        }
    }
    map
}

pub struct NetCollector {
    prev: Option<HashMap<String, (u64, u64)>>,
    prev_t: Option<Instant>,
}

impl NetCollector {
    pub fn new() -> Self {
        Self { prev: None, prev_t: None }
    }

    pub fn measure(&mut self) -> Option<Vec<NetIf>> {
        let cur = snapshot();
        let now = Instant::now();
        let prev = self.prev.clone();
        let elapsed = self
            .prev_t
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|e| *e > 0.0);
        self.prev = Some(cur.clone());
        self.prev_t = Some(now);
        let (Some(prev), Some(elapsed)) = (prev, elapsed) else { return None };

        let mut ifaces = Vec::new();
        for (name, (rx, tx)) in &cur {
            let Some((p_rx, p_tx)) = prev.get(name) else { continue };
            ifaces.push(NetIf {
                iface: name.clone(),
                state: operstate(name),
                rx_bps: rx.saturating_sub(*p_rx) as f64 / elapsed,
                tx_bps: tx.saturating_sub(*p_tx) as f64 / elapsed,
                rx_total: *rx,
                tx_total: *tx,
            });
        }
        // Connected interfaces first, then by traffic; 'lo' stays last.
        ifaces.sort_by(|a, b| {
            let key = |i: &NetIf| (i.iface == "lo", i.state != "up", -(i.rx_bps + i.tx_bps));
            let (ka, kb) = (key(a), key(b));
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        Some(ifaces)
    }
}
