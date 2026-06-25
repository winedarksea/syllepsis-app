use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

pub fn detect_power_source() -> PowerSource {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("pmset")
            .args(["-g", "batt"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains("AC Power") {
                return PowerSource::Ac;
            }
            if text.contains("Battery Power") {
                return PowerSource::Battery;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                if let Ok(online) = std::fs::read_to_string(entry.path().join("online")) {
                    if online.trim() == "1" {
                        return PowerSource::Ac;
                    }
                }
            }
            return PowerSource::Battery;
        }
    }
    PowerSource::Unknown
}
