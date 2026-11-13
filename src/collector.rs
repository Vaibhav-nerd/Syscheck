//! Shared data model for one sample of all system metrics.

use crate::sources::cpu::CpuData;
use crate::sources::disk::{DiskIo, FsUsage};
use crate::sources::mem::MemData;
use crate::sources::net::NetIf;
use crate::sources::procs::ProcData;
use crate::sources::speedtest::SpeedResult;

#[derive(Clone)]
pub struct HeaderData {
    pub host: String,
    pub kernel: String,
    pub uptime: f64,
    pub load: [f64; 3],
}

#[derive(Clone, Default)]
pub struct Data {
    pub header: Option<HeaderData>,
    pub cpu: Option<CpuData>,
    pub mem: Option<MemData>,
    pub disks: Vec<FsUsage>,
    pub diskio: Option<Vec<DiskIo>>,
    pub net: Option<Vec<NetIf>>,
    pub procs: Option<ProcData>,
    pub temps: Vec<(String, f64)>,
    pub speedtest: Option<SpeedResult>,
}

pub const SECTIONS: [&str; 6] = ["cpu", "mem", "disk", "net", "procs", "temp"];

pub fn parse_only(value: Option<&str>) -> Result<Option<Vec<String>>, String> {
    let Some(value) = value else { return Ok(None) };
    let parts: Vec<String> = value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for p in &parts {
        if !SECTIONS.contains(&p.as_str()) {
            return Err(format!(
                "unknown section '{p}'. Valid: {}",
                SECTIONS.join(", ")
            ));
        }
    }
    Ok(Some(parts))
}
