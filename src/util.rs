//! Formatting helpers and ANSI palette for the plain-text report.

use std::fs;

pub const CLK_TCK: f64 = 100.0; // standard USER_HZ on Linux
pub const PAGE_SIZE: u64 = 4096;

pub fn read_to_string(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Add thousands separators to "1234.5"-style strings produced by format!.
fn group(s: &str) -> String {
    let (int_part, rest) = match s.find('.') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    let digits: Vec<char> = int_part.chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    out + rest
}

/// Binary-unit byte formatting: 4.0 GiB, 481.1 MiB, ...
pub fn human_bytes(n: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = n;
    for (i, u) in units.iter().enumerate() {
        if v.abs() < 1024.0 || i == units.len() - 1 {
            return if *u == "B" {
                format!("{v:.0} {u}")
            } else {
                format!("{} {}", group(&format!("{v:.1}")), u)
            };
        }
        v /= 1024.0;
    }
    unreachable!()
}

pub fn human_bps(n: f64) -> String {
    let units = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let mut v = n;
    for (i, u) in units.iter().enumerate() {
        if v.abs() < 1024.0 || i == units.len() - 1 {
            return if *u == "B/s" {
                format!("{v:.0} {u}")
            } else {
                format!("{} {}", group(&format!("{v:.1}")), u)
            };
        }
        v /= 1024.0;
    }
    unreachable!()
}

pub fn human_uptime(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let (days, rem) = (s / 86400, s % 86400);
    let (hours, rem) = (rem / 3600, rem % 3600);
    let minutes = rem / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m {}s", rem % 60)
    }
}

/// ANSI colors for the plain-text report; empty strings when disabled.
pub struct Palette {
    pub enabled: bool,
}

impl Palette {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn wrap(&self, code: &str, s: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }

    pub fn pct(&self, v: f64, warn: f64, crit: f64) -> String {
        let text = format!("{v:5.1}%");
        if v >= crit {
            self.red(&text)
        } else if v >= warn {
            self.yellow(&text)
        } else {
            self.green(&text)
        }
    }

    pub fn temp(&self, c: f64) -> String {
        let text = format!("{c:5.1}°C");
        if c >= 80.0 {
            self.red(&text)
        } else if c >= 60.0 {
            self.yellow(&text)
        } else {
            self.green(&text)
        }
    }
}

/// Unicode block bar for a 0-100 percentage.
pub fn bar(pct: f64, width: usize, p: &Palette) -> String {
    let pct = pct.clamp(0.0, 100.0);
    let filled = (pct / 100.0 * width as f64).round() as usize;
    let filled_s = "█".repeat(filled);
    let empty_s = "░".repeat(width.saturating_sub(filled));
    if !p.enabled {
        return format!("{filled_s}{empty_s}");
    }
    let color_code = if pct >= 85.0 {
        "31"
    } else if pct >= 60.0 {
        "33"
    } else {
        "32"
    };
    format!("\x1b[{color_code}m{filled_s}\x1b[2m{empty_s}\x1b[0m")
}
