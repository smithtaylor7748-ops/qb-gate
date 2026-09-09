//! Disable the known legacy launchers that can start a second watchdog.
//! Files are moved to a dated backup folder, never deleted.

use crate::error::Result;

const LEGACY_NAMES: &[&str] = &[
    "ClaudeIpGate.ps1",
    "ClaudeIpGate.cmd",
    "start-claude-pro-rp.ps1",
    "start-claude-pro-rp.cmd",
    "一键启动克劳德桌面.cmd",
];

pub fn disable_known_launchers() -> Result<Vec<String>> {
    let Some(desktop) = dirs::desktop_dir() else { return Ok(Vec::new()); };
    let backup = crate::gate::state_dir().join("legacy-scripts-backup");
    let mut moved = Vec::new();
    for name in LEGACY_NAMES {
        let src = desktop.join(name);
        if !src.is_file() { continue; }
        std::fs::create_dir_all(&backup)?;
        let mut dst = backup.join(name);
        if dst.exists() {
            dst = backup.join(format!("{}.{}", name, chrono::Local::now().format("%Y%m%d-%H%M%S")));
        }
        std::fs::rename(&src, &dst)?;
        moved.push(name.to_string());
    }
    if !moved.is_empty() {
        crate::gate::log::write(&format!("已停用旧启动脚本：{}", moved.join(", ")));
    }
    Ok(moved)
}

