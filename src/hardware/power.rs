//! Power state via `GetSystemPowerStatus` - a direct Win32 API, `Measured`
//! when it succeeds. Laptop/desktop classification is *derived* from
//! battery presence and graded `Inferred`, since battery presence is a
//! strong but not certain signal (a desktop with a UPS reporting through
//! Windows' power subsystem is a known, rare exception).

use super::HardwareField;
use serde::Serialize;
use windows::Win32::System::Power::GetSystemPowerStatus;

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
    let mut status = windows::Win32::System::Power::SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut status) };

    if ok.is_err() {
        return PowerReport {
            ac_line_status: HardwareField::unavailable("GetSystemPowerStatus failed"),
            battery_percent: HardwareField::unavailable("GetSystemPowerStatus failed"),
            battery_present: HardwareField::unavailable("GetSystemPowerStatus failed"),
            chassis_class: HardwareField::unavailable("GetSystemPowerStatus failed"),
        };
    }

    let ac_line_status = match status.ACLineStatus {
        0 => HardwareField::measured(
            AcLineStatus::Offline,
            "Win32 GetSystemPowerStatus.ACLineStatus",
        ),
        1 => HardwareField::measured(
            AcLineStatus::Online,
            "Win32 GetSystemPowerStatus.ACLineStatus",
        ),
        _ => HardwareField::measured(
            AcLineStatus::Unknown,
            "Win32 GetSystemPowerStatus.ACLineStatus",
        ),
    };

    // BatteryFlag bit 128 means "no system battery"; 255 means "unknown
    // status" (which we treat as unavailable rather than guessing either
    // way).
    let battery_flag = status.BatteryFlag;
    let battery_present = if battery_flag == 255 {
        HardwareField::unavailable("BatteryFlag reported unknown (0xFF)")
    } else {
        HardwareField::detected(
            battery_flag & 128 == 0,
            "Win32 GetSystemPowerStatus.BatteryFlag bit 7 (no-system-battery)",
        )
    };

    let battery_percent = if status.BatteryLifePercent == 255 {
        HardwareField::unavailable("BatteryLifePercent reported unknown (0xFF)")
    } else {
        HardwareField::measured(
            status.BatteryLifePercent,
            "Win32 GetSystemPowerStatus.BatteryLifePercent",
        )
    };

    let chassis_class = match battery_present.value {
        Some(true) => HardwareField::inferred(
            ChassisClass::Laptop,
            "inferred from a system battery being present (GetSystemPowerStatus); not a direct chassis-type query",
        ),
        Some(false) => HardwareField::inferred(
            ChassisClass::Desktop,
            "inferred from no system battery being present (GetSystemPowerStatus); not a direct chassis-type query",
        ),
        None => HardwareField::unavailable("battery presence could not be determined"),
    };

    PowerReport {
        ac_line_status,
        battery_percent,
        battery_present,
        chassis_class,
    }
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
