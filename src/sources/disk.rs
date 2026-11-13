//! Storage: filesystem capacity (statvfs) and disk I/O rates (/proc/diskstats).

use std::collections::HashMap;
use std::ffi::CString;
use std::time::Instant;

use crate::util::read_to_string;

const SECTOR: u64 = 512;

const PSEUDO_FS: [&str; 27] = [
    "proc", "sysfs", "devtmpfs", "devpts", "tmpfs", "securityfs", "pstore",
    "bpf", "autofs", "debugfs", "tracefs", "configfs", "fusectl", "hugetlbfs",
    "efivarfs", "mqueue", "ramfs", "squashfs", "binfmt_misc", "nsfs",
    "overlay", "rpc_pipefs", "cgroup", "cgroup2", "procfs", "devfs", "9p",
];

fn is_pseudo(fstype: &str) -> bool {
    PSEUDO_FS.contains(&fstype) || fstype.starts_with("cgroup")
}

#[derive(Clone)]
pub struct FsUsage {
    pub device: String,
    pub mount: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub pct: f64,
    pub inodes_pct: f64,
}

fn real_mounts() -> Vec<(String, String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    if let Some(text) = read_to_string("/proc/mounts") {
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let (dev, mnt, fstype) = (parts[0], parts[1], parts[2]);
            if is_pseudo(fstype) || dev == "none" || dev == "tmpfs" {
                continue;
            }
            if seen.insert(mnt.to_string()) {
                out.push((dev.to_string(), mnt.to_string(), fstype.to_string()));
            }
        }
    }
    out
}

/// Per-filesystem capacity via statvfs(3).
pub fn mount_usage() -> Vec<FsUsage> {
    let mut out = Vec::new();
    for (dev, mnt, fstype) in real_mounts() {
        let c_path = match CString::new(mnt.as_str()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        unsafe {
            let mut st: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c_path.as_ptr(), &mut st) != 0 {
                continue;
            }
            let total = st.f_blocks as u64 * st.f_frsize as u64;
            if total == 0 {
                continue; // empty virtual mounts (gvfs portals, etc.)
            }
            let free = st.f_bfree as u64 * st.f_frsize as u64;
            let avail = st.f_bavail as u64 * st.f_frsize as u64;
            let used = total.saturating_sub(free);
            let files = st.f_files as f64;
            out.push(FsUsage {
                device: dev,
                mount: mnt,
                fstype,
                total,
                used,
                avail,
                pct: 100.0 * used as f64 / total as f64,
                inodes_pct: if files > 0.0 {
                    100.0 * (files - st.f_ffree as f64) / files
                } else {
                    0.0
                },
            });
        }
    }
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}

/// True for whole disks (sda, nvme0n1, dm-0), false for partitions and
/// pseudo devices (loop*, ram*, sr*).
fn is_whole_disk(name: &str) -> bool {
    if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("sr") {
        return false;
    }
    // nvme0n1p2 / mmcblk0p1 -> strip partition suffix
    let name = if name.starts_with("nvme") || name.starts_with("mmcblk") {
        match name.find('p') {
            Some(pos) if !name[pos + 1..].is_empty()
                && name[pos + 1..].chars().all(|c| c.is_ascii_digit()) =>
            {
                &name[..pos]
            }
            _ => name,
        }
    } else {
        name
    };

    if let Some(rest) = name.strip_prefix("dm-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    for prefix in ["sd", "vd", "hd", "xvd"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            let alpha_len = rest.chars().take_while(|c| c.is_ascii_alphabetic()).count();
            let digits = &rest[alpha_len..];
            return alpha_len > 0 && digits.is_empty();
        }
    }
    if let Some(rest) = name.strip_prefix("nvme") {
        // nvme<digits>n<digits>
        let parts: Vec<&str> = rest.split('n').collect();
        return parts.len() == 2
            && !parts[0].is_empty() && parts[0].chars().all(|c| c.is_ascii_digit())
            && !parts[1].is_empty() && parts[1].chars().all(|c| c.is_ascii_digit());
    }
    if let Some(rest) = name.strip_prefix("mmcblk") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}

#[derive(Clone)]
pub struct DiskStat {
    pub reads: u64,
    pub sectors_read: u64,
    pub writes: u64,
    pub sectors_written: u64,
    pub ms_io: u64,
}

#[derive(Clone)]
pub struct DiskIo {
    pub name: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub read_iops: f64,
    pub write_iops: f64,
    pub util_pct: f64,
}

pub struct DiskIoCollector {
    prev: Option<HashMap<String, DiskStat>>,
    prev_t: Option<Instant>,
}

impl DiskIoCollector {
    pub fn new() -> Self {
        Self { prev: None, prev_t: None }
    }

    fn snapshot(&self) -> HashMap<String, DiskStat> {
        let mut map = HashMap::new();
        if let Some(text) = read_to_string("/proc/diskstats") {
            for line in text.lines() {
                let p: Vec<&str> = line.split_whitespace().collect();
                if p.len() < 13 || !is_whole_disk(p[2]) {
                    continue;
                }
                let num = |i: usize| p[i].parse::<u64>().unwrap_or(0);
                map.insert(
                    p[2].to_string(),
                    DiskStat {
                        reads: num(3),
                        sectors_read: num(5),
                        writes: num(7),
                        sectors_written: num(9),
                        ms_io: num(12),
                    },
                );
            }
        }
        map
    }

    pub fn measure(&mut self) -> Option<Vec<DiskIo>> {
        let cur = self.snapshot();
        let now = Instant::now();
        let prev = self.prev.clone();
        let elapsed = self
            .prev_t
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|e| *e > 0.0);
        self.prev = Some(cur.clone());
        self.prev_t = Some(now);
        let (Some(prev), Some(elapsed)) = (prev, elapsed) else { return None };

        let mut disks = Vec::new();
        for (name, c) in &cur {
            let Some(p) = prev.get(name) else { continue };
            let d = |a: u64, b: u64| a.saturating_sub(b) as f64;
            disks.push(DiskIo {
                name: name.clone(),
                read_bps: d(c.sectors_read, p.sectors_read) * SECTOR as f64 / elapsed,
                write_bps: d(c.sectors_written, p.sectors_written) * SECTOR as f64 / elapsed,
                read_iops: d(c.reads, p.reads) / elapsed,
                write_iops: d(c.writes, p.writes) / elapsed,
                util_pct: (d(c.ms_io, p.ms_io) / 1000.0 / elapsed * 100.0).clamp(0.0, 100.0),
            });
        }
        disks.sort_by(|a, b| b.util_pct.partial_cmp(&a.util_pct).unwrap_or(std::cmp::Ordering::Equal));
        Some(disks)
    }
}
