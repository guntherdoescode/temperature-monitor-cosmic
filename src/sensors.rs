use serde_json::{Map, Value};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState { Cool, Normal, Hot }

#[derive(Debug, Clone)]
pub struct SensorReading {
    pub id: String,
    pub chip: String,
    pub label: String,
    pub temperature: f64,
    pub warning: f64,
    pub critical: f64,
    pub state: ThermalState,
    pub throttled: bool,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub readings: Vec<SensorReading>,
    pub throttle_total: Option<u64>,
    pub throttle_event: bool,
}

pub fn collect(previous_throttle_total: Option<u64>) -> Result<Snapshot, String> {
    let output = Command::new("sensors").arg("-j").output()
        .map_err(|error| format!("Could not run lm-sensors: {error}. Install the lm-sensors package."))?;
    if !output.status.success() {
        return Err(format!("lm-sensors exited with {}: {}", output.status, String::from_utf8_lossy(&output.stderr).trim()));
    }

    let document: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not parse sensors JSON: {error}"))?;
    let throttle_total = read_throttle_total();
    let throttle_event = matches!((previous_throttle_total, throttle_total), (Some(old), Some(new)) if new > old);

    let mut readings = Vec::new();
    if let Some(chips) = document.as_object() {
        for (chip, value) in chips {
            if let Some(object) = value.as_object() {
                walk_chip(chip, object, &mut Vec::new(), &mut readings, throttle_event);
            }
        }
    }
    readings.sort_by(|a, b| a.chip.cmp(&b.chip).then(a.label.cmp(&b.label)));
    readings.dedup_by(|a, b| a.id == b.id);

    Ok(Snapshot { readings, throttle_total, throttle_event })
}

fn walk_chip(chip: &str, object: &Map<String, Value>, path: &mut Vec<String>, readings: &mut Vec<SensorReading>, throttle_event: bool) {
    for (key, value) in object {
        if let Some(prefix) = key.strip_suffix("_input") {
            let Some(temperature) = value.as_f64() else { continue };
            let label = object.get(&format!("{prefix}_label")).and_then(Value::as_str)
                .map(str::to_owned).or_else(|| path.last().cloned()).unwrap_or_else(|| humanise(prefix));
            let (fallback_warning, fallback_critical) = fallback_limits(chip, &label);
            let warning = object.get(&format!("{prefix}_max")).and_then(Value::as_f64)
                .filter(|limit| *limit > 0.0).unwrap_or(fallback_warning);
            let critical = object.get(&format!("{prefix}_crit")).and_then(Value::as_f64)
                .filter(|limit| *limit > warning).unwrap_or(fallback_critical.max(warning + 5.0));
            let throttled = throttle_event && is_cpu_sensor(chip, &label);
            let state = if throttled || temperature >= warning { ThermalState::Hot }
                else if temperature < warning * 0.62 { ThermalState::Cool }
                else { ThermalState::Normal };

            readings.push(SensorReading {
                id: format!("{chip}:{}:{prefix}", path.join("/")),
                chip: chip.to_owned(), label, temperature, warning, critical, state, throttled,
            });
        } else if let Some(child) = value.as_object() {
            if key != "Adapter" {
                path.push(key.clone());
                walk_chip(chip, child, path, readings, throttle_event);
                path.pop();
            }
        }
    }
}

fn fallback_limits(chip: &str, label: &str) -> (f64, f64) {
    let hint = format!("{chip} {label}").to_ascii_lowercase();
    if hint.contains("nvme") || hint.contains("composite") { (65.0, 80.0) }
    else if hint.contains("amdgpu") || hint.contains("gpu") || hint.contains("junction") { (85.0, 100.0) }
    else if is_cpu_sensor(chip, label) { (85.0, 95.0) }
    else { (70.0, 85.0) }
}

fn is_cpu_sensor(chip: &str, label: &str) -> bool {
    let hint = format!("{chip} {label}").to_ascii_lowercase();
    ["k10temp", "coretemp", "cpu", "tctl", "tdie", "package id"].iter().any(|needle| hint.contains(needle))
}

fn humanise(value: &str) -> String {
    value.replace('_', " ").split_whitespace().map(|word| {
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }).collect::<Vec<_>>().join(" ")
}

fn read_throttle_total() -> Option<u64> {
    let mut total = 0_u64;
    let mut found = false;
    for path in glob::glob("/sys/devices/system/cpu/cpu*/thermal_throttle/*_throttle_count").ok()?.flatten() {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(value) = contents.trim().parse::<u64>() {
                total = total.saturating_add(value);
                found = true;
            }
        }
    }
    found.then_some(total)
}
