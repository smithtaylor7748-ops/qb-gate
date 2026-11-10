//! Runtime platform capabilities. An unsupported mechanism is never reported as protection.
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PlatformCapabilities {
    pub os: String,
    pub arch: String,
    pub system_execution_lock: bool,
    pub firewall: bool,
    pub persistent_env_cleanup: bool,
    pub managed_launch: bool,
    pub autostart: bool,
}

pub fn current() -> PlatformCapabilities {
    PlatformCapabilities {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        system_execution_lock: cfg!(windows),
        firewall: cfg!(windows),
        persistent_env_cleanup: cfg!(windows),
        managed_launch: cfg!(any(windows, target_os = "macos")),
        autostart: cfg!(any(windows, target_os = "macos")),
    }
}
