//! Plain-text report (same layout as the Python version) and JSON output.

use std::ffi::CStr;

use crate::collector::{Data, HeaderData};
use crate::sources::cpu;
use crate::util::{bar, human_bps, human_bytes, human_uptime, Palette};

pub fn header_data() -> Option<HeaderData> {
    let host = unsafe {
        let mut buf = [0u8; 256];
        if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
            CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
                .to_string_lossy()
                .into_owned()
        } else {
            "unknown".to_string()
        }
    };
    let kernel = unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) == 0 {
            CStr::from_ptr(uts.release.as_ptr()).to_string_lossy().into_owned()
        } else {
            "unknown".to_string()
        }
    };
    Some(HeaderData {
        host,
        kernel,
        uptime: cpu::uptime_secs(),
        load: cpu::load_average(),
    })
}

pub fn render(data: &Data, only: Option<&[String]>, colors: bool) -> String {
    let p = Palette::new(colors);
    let width = if terminal_width() >= 100 { 22 } else { 16 };
    let want = |s: &str| only.map_or(true, |o| o.iter().any(|x| x == s));
    let mut out: Vec<String> = Vec::new();

    // Header
    if let Some(h) = &data.header {
        out.push(p.bold(&format!("{}  {}", h.host, h.kernel)));
        out.push(format!(
            "up {:<15} load {:.2} {:.2} {:.2}",
            human_uptime(h.uptime),
            h.load[0],
            h.load[1],
            h.load[2]
        ));
    }

    // CPU
    if want("cpu") {
        if let Some(c) = &data.cpu {
            out.push(String::new());
            let freq = c.freq_mhz.map(|f| format!("  {:.2} GHz", f / 1000.0)).unwrap_or_default();
            out.push(format!("{} {}", p.bold("CPU"), p.dim(&short_model(&c.model))));
            out.push(format!(
                "  {}{}",
                p.dim(&format!("{} cores", c.ncores)),
                p.dim(&freq)
            ));
            out.push(format!(
                "  total  {}  {}",
                bar(c.overall, width, &p),
                p.pct(c.overall, 60.0, 85.0)
            ));
            for (i, core) in c.cores.iter().enumerate() {
                out.push(format!(
                    "  c{i:<4} {}  {}",
                    bar(*core, width, &p),
                    p.pct(*core, 60.0, 85.0)
                ));
            }
        }
    }

    // Memory
    if want("mem") {
        if let Some(m) = &data.mem {
            out.push(String::new());
            out.push(p.bold("MEMORY"));
            out.push(format!(
                "  RAM    {}  {}  {} / {}",
                bar(m.pct, width, &p),
                p.pct(m.pct, 60.0, 85.0),
                human_bytes(m.used as f64),
                human_bytes(m.total as f64)
            ));
            if m.swap_total > 0 {
                out.push(format!(
                    "  SWAP   {}  {}  {} / {}",
                    bar(m.swap_pct, width, &p),
                    p.pct(m.swap_pct, 60.0, 85.0),
                    human_bytes(m.swap_used as f64),
                    human_bytes(m.swap_total as f64)
                ));
            }
        }
    }

    // Storage + disk I/O
    if want("disk") {
        if !data.disks.is_empty() {
            out.push(String::new());
            out.push(p.bold("STORAGE"));
            for fs in &data.disks {
                out.push(format!(
                    "  {:<18} {}  {}  {} / {}",
                    fs.mount,
                    bar(fs.pct, width, &p),
                    p.pct(fs.pct, 60.0, 85.0),
                    human_bytes(fs.used as f64),
                    human_bytes(fs.total as f64)
                ));
                if fs.inodes_pct >= 80.0 {
                    out.push(format!(
                        "    {}",
                        p.yellow(&format!("inodes {:.1}% used", fs.inodes_pct))
                    ));
                }
            }
        }
        if let Some(ios) = &data.diskio {
            out.push(String::new());
            out.push(p.bold("DISK I/O"));
            for io in ios {
                out.push(format!(
                    "  {:<8} R {:>8} W {:>8} IOPS {:6.0} {} {:5.1}%",
                    io.name,
                    human_bps(io.read_bps),
                    human_bps(io.write_bps),
                    io.read_iops + io.write_iops,
                    bar(io.util_pct, 14, &p),
                    io.util_pct
                ));
            }
        }
    }

    // Network
    if want("net") {
        if let Some(n) = &data.net {
            out.push(String::new());
            out.push(p.bold("NETWORK"));
            for i in n {
                let (symbol, color) = match i.state.as_str() {
                    "up" => ("●", "32"),
                    "down" => ("○", "2"),
                    _ => ("?", "2"),
                };
                let marked = if p.enabled && color != "2" {
                    format!("\x1b[{color}m{symbol}\x1b[0m")
                } else if p.enabled {
                    format!("\x1b[2m{symbol}\x1b[0m")
                } else {
                    symbol.to_string()
                };
                out.push(format!(
                    "  {} {:<12} ↓ {:>10}  ↑ {:>10}  {}",
                    marked,
                    i.iface,
                    human_bps(i.rx_bps),
                    human_bps(i.tx_bps),
                    p.dim(&format!("{:>7}", i.state))
                ));
            }
        }
    }

    // Top processes
    if want("procs") {
        if let Some(pr) = &data.procs {
            out.push(String::new());
            out.push(p.bold("TOP PROCESSES"));
            let mut table = |title: &str, rows: &[crate::sources::procs::ProcRow], by_cpu: bool| {
                out.push(format!("  {}", p.dim(title)));
                for r in rows.iter().take(5) {
                    let pct = if by_cpu { r.cpu_pct } else { r.mem_pct };
                    let (warn, crit) = if by_cpu { (60.0, 85.0) } else { (30.0, 60.0) };
                    out.push(format!(
                        "    {:<24} {:<10} {} {:>10}",
                        r.name,
                        r.user,
                        p.pct(pct, warn, crit),
                        human_bytes(r.rss as f64)
                    ));
                }
            };
            table("by CPU", &pr.by_cpu, true);
            table("by memory", &pr.by_mem, false);
        }
    }

    // Temperatures
    if want("temp") && !data.temps.is_empty() {
        out.push(String::new());
        out.push(p.bold("TEMPERATURES"));
        for (label, t) in data.temps.iter().take(8) {
            out.push(format!("  {:<28} {}", label, p.temp(*t)));
        }
    }

    // Speed test
    if let Some(s) = &data.speedtest {
        out.push(String::new());
        out.push(p.bold("SPEED TEST"));
        match &s.error {
            Some(e) => out.push(format!("  {}", p.yellow(&format!("unavailable: {e}")))),
            None => out.push(format!(
                "  ping {:>7.1} ms   ↓ {:>8.1} Mbit/s   ↑ {:>8.1} Mbit/s",
                s.ping_ms, s.down_mbps, s.up_mbps
            )),
        }
    }

    out.join("\n")
}

fn short_model(model: &str) -> String {
    model.chars().take(60).collect()
}

fn terminal_width() -> usize {
    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    let mut ws = Winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 };
    if ok && ws.ws_col > 0 {
        ws.ws_col as usize
    } else {
        80
    }
}

pub fn render_json(data: &Data) -> String {
    use serde_json::json;
    let mut j = json!({});
    if let Some(h) = &data.header {
        j["header"] = json!({
            "host": h.host,
            "kernel": h.kernel,
            "uptime": human_uptime(h.uptime),
            "load": [r2(h.load[0]), r2(h.load[1]), r2(h.load[2])],
        });
    }
    if let Some(c) = &data.cpu {
        j["cpu"] = json!({
            "overall": r1(c.overall),
            "cores": c.cores.iter().map(|x| r1(*x)).collect::<Vec<_>>(),
            "ncores": c.ncores,
            "model": c.model,
            "freq_mhz": c.freq_mhz,
            "load": [r2(c.load[0]), r2(c.load[1]), r2(c.load[2])],
        });
    }
    if let Some(m) = &data.mem {
        j["mem"] = json!({
            "total": m.total, "available": m.available, "used": m.used,
            "pct": r1(m.pct), "cached": m.cached,
            "swap_total": m.swap_total, "swap_used": m.swap_used, "swap_pct": r1(m.swap_pct),
        });
    }
    if !data.disks.is_empty() {
        j["disks"] = json!(data.disks.iter().map(|f| json!({
            "device": f.device, "mount": f.mount, "fstype": f.fstype,
            "total": f.total, "used": f.used, "avail": f.avail,
            "pct": r1(f.pct), "inodes_pct": r1(f.inodes_pct),
        })).collect::<Vec<_>>());
    }
    if let Some(ios) = &data.diskio {
        j["diskio"] = json!(ios.iter().map(|io| json!({
            "name": io.name,
            "read_bps": io.read_bps, "write_bps": io.write_bps,
            "read_iops": io.read_iops, "write_iops": io.write_iops,
            "util_pct": r1(io.util_pct),
        })).collect::<Vec<_>>());
    }
    if let Some(n) = &data.net {
        j["net"] = json!(n.iter().map(|i| json!({
            "iface": i.iface, "state": i.state,
            "rx_bps": i.rx_bps, "tx_bps": i.tx_bps,
            "rx_total": i.rx_total, "tx_total": i.tx_total,
        })).collect::<Vec<_>>());
    }
    if let Some(pr) = &data.procs {
        let rows = |rs: &[crate::sources::procs::ProcRow]| json!(rs.iter().map(|r| json!({
            "pid": r.pid, "name": r.name, "user": r.user,
            "cpu_pct": r1(r.cpu_pct), "rss": r.rss, "mem_pct": r1(r.mem_pct),
        })).collect::<Vec<_>>());
        j["procs"] = json!({ "by_cpu": rows(&pr.by_cpu), "by_mem": rows(&pr.by_mem) });
    }
    if !data.temps.is_empty() {
        j["temps"] = json!(data.temps.iter().map(|(l, t)| json!({
            "label": l, "temp_c": r1(*t),
        })).collect::<Vec<_>>());
    }
    if let Some(s) = &data.speedtest {
        match &s.error {
            Some(e) => j["speedtest"] = json!({ "error": e }),
            None => j["speedtest"] = json!({
                "ping_ms": s.ping_ms,
                "download_mbps": s.down_mbps,
                "upload_mbps": s.up_mbps,
            }),
        }
    }
    serde_json::to_string_pretty(&j).unwrap_or_else(|_| "{}".into())
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
