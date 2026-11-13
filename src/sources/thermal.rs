//! CPU/board temperature survey from /sys/class/thermal and /sys/class/hwmon.

use std::collections::HashMap;
use std::fs;

fn read_num(path: &std::path::Path) -> Option<f64> {
    fs::read_to_string(path).ok()?.trim().parse::<f64>().ok()
}

fn read_str(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn insert_max(zones: &mut HashMap<String, f64>, label: String, temp: f64) {
    zones
        .entry(label)
        .and_modify(|v| *v = v.max(temp))
        .or_insert(temp);
}

/// Returns {label, temp_c} sorted hottest first, deduplicated by label.
pub fn survey() -> Vec<(String, f64)> {
    let mut zones: HashMap<String, f64> = HashMap::new();

    // /sys/class/thermal/thermal_zone*/{temp,type}
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("thermal_zone") {
                continue;
            }
            let temp = read_num(&e.path().join("temp")).map(|t| t / 1000.0);
            let label = read_str(&e.path().join("type")).unwrap_or_else(|| "thermal".into());
            if let Some(temp) = temp {
                insert_max(&mut zones, label, temp);
            }
        }
    }

    // /sys/class/hwmon/hwmon*/temp*_input with optional temp*_label
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for e in entries.flatten() {
            let chip = read_str(&e.path().join("name")).unwrap_or_default();
            if chip.is_empty() {
                continue;
            }
            let Ok(inputs) = fs::read_dir(e.path()) else { continue };
            for input in inputs.flatten() {
                let name = input.file_name().to_string_lossy().into_owned();
                if !name.starts_with("temp") || !name.ends_with("_input") {
                    continue;
                }
                let Some(temp) = read_num(&input.path()).map(|t| t / 1000.0) else { continue };
                let label = fs::read_to_string(
                    input.path().to_string_lossy().replace("_input", "_label"),
                )
                .map(|s| format!("{chip} {}", s.trim()))
                .unwrap_or_else(|_| chip.clone());
                insert_max(&mut zones, label, temp);
            }
        }
    }

    let mut out: Vec<(String, f64)> = zones.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}
