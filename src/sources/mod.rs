//! Data sources: everything is read directly from /proc and /sys.

pub mod cpu;
pub mod disk;
pub mod mem;
pub mod net;
pub mod procs;
pub mod speedtest;
pub mod thermal;

use std::time::Instant;

use crate::collector::Data;
/// Holds all delta-sampling collectors and produces one `Data` per pass.
pub struct Collectors {
    pub cpu: cpu::CpuCollector,
    pub net: net::NetCollector,
    pub disk: disk::DiskIoCollector,
    pub procs: procs::ProcCollector,
}

impl Collectors {
    pub fn new() -> Self {
        let mem = mem::measure();
        Self {
            cpu: cpu::CpuCollector::new(),
            net: net::NetCollector::new(),
            disk: disk::DiskIoCollector::new(),
            procs: procs::ProcCollector::new(mem.total),
        }
    }

    /// Take the first (delta-less) sample from every rate collector.
    pub fn prime(&mut self) {
        let _ = self.cpu.measure();
        let _ = self.net.measure();
        let _ = self.disk.measure();
        let _ = self.procs.measure();
    }

    /// One sampling pass; collectors must already be primed.
    pub fn sample(&mut self, only: Option<&[String]>) -> Data {
        let want = |s: &str| only.map_or(true, |o| o.iter().any(|x| x == s));
        Data {
            header: crate::report::header_data(),
            cpu: if want("cpu") { self.cpu.measure() } else { None },
            mem: if want("mem") { Some(mem::measure()) } else { None },
            disks: if want("disk") { disk::mount_usage() } else { Vec::new() },
            diskio: if want("disk") { self.disk.measure() } else { None },
            net: if want("net") { self.net.measure() } else { None },
            procs: if want("procs") { self.procs.measure() } else { None },
            temps: if want("temp") { thermal::survey() } else { Vec::new() },
            speedtest: None,
        }
    }
}

/// Elapsed seconds since `t`, guarded against zero (avoids div by 0).
#[allow(dead_code)]
pub fn elapsed(prev: Option<Instant>, now: Instant) -> Option<f64> {
    prev.map(|p| now.duration_since(p).as_secs_f64()).filter(|e| *e > 0.0)
}
