//! Power/battery/chassis-class types and dispatcher. The actual query is
//! genuinely OS-specific (`GetSystemPowerStatus` on Windows, IOKit power
//! sources / `pmset` on macOS, `/sys/class/power_supply` on Linux) and
//! lives in `crate::platform::{windows,macos,linux}`. Laptop/desktop
//! classification is *derived* from battery presence and graded
//! `Inferred` on every platform, since battery presence is a strong but
//! not certain signal (e.g. a desktop with a UPS reporting through the
//! OS's power subsystem is a known, rare exception).

use super::HardwareField;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcLineStatus {
    Offline,
    Online,
    Unknown,
}

impl std::fmt::Display for AcLineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcLineStatus::Offline => write!(f, "offline (on battery)"),
            AcLineStatus::Online => write!(f, "online (on AC power)"),
            AcLineStatus::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChassisClass {
    Laptop,
    Desktop,
}

impl std::fmt::Display for ChassisClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChassisClass::Laptop => write!(f, "laptop"),
            ChassisClass::Desktop => write!(f, "desktop"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PowerReport {
    pub ac_line_status: HardwareField<AcLineStatus>,
    pub battery_percent: HardwareField<u8>,
    pub battery_present: HardwareField<bool>,
    pub chassis_class: HardwareField<ChassisClass>,
}

pub fn inspect_power() -> PowerReport {
    crate::platform::current::inspect_power()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_power_never_panics() {
        let report = inspect_power();
        // Don't assert a specific chassis class - this must degrade
        // gracefully on any machine, desktop or laptop, VM or bare metal.
        let _ = report.chassis_class.value;
    }
}
