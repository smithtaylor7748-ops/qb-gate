//! 系统时区与显示语言对齐。
//!
//! 切换由「设置 › 启动时对齐」在面板启动时做（`usecase::locale_ops`），**不还原**：
//! 专机长期跑 Claude 时，时区保持一致比来回切更稳。切之前的原值写进审计日志。
//! （早先那张时区卡的「退出时还原」与手动「恢复时区」2026-09-27 删了 ——
//! 启动对齐从来不记还原点，那颗按钮点了什么都不做。）
//!
//! 切时区和 IPv6 网卡绑定需要管理员权限。
//!
//! ⚠ 整个面板里还有第二处：`platform::firewall` 的加/撤出站锁规则。
//! 面板自己**不提权**（安装是 `currentUser`，没有 requireAdministrator 清单），
//! 时区/防火墙需要使用者以管理员身份启动面板；IPv6 单独提权一个受限的
//! PowerShell 操作，不提升整个面板，失败如实报告。

pub mod checkup;
pub mod ipv6;
pub mod locale;
pub mod proxy;

use crate::error::{GateError, Result};

/// IANA -> Windows 时区 ID。
///
/// 只收常用的那些。查不到时**不猜**，直接报错让用户手动选 ——
/// 猜错会把系统时间拨到别的地方，比不切换更糟。
const MAP: &[(&str, &str)] = &[
    ("America/New_York", "Eastern Standard Time"),
    ("America/Chicago", "Central Standard Time"),
    ("America/Denver", "Mountain Standard Time"),
    ("America/Phoenix", "US Mountain Standard Time"),
    ("America/Los_Angeles", "Pacific Standard Time"),
    ("America/Anchorage", "Alaskan Standard Time"),
    ("America/Toronto", "Eastern Standard Time"),
    ("America/Vancouver", "Pacific Standard Time"),
    ("America/Sao_Paulo", "E. South America Standard Time"),
    ("Europe/London", "GMT Standard Time"),
    ("Europe/Dublin", "GMT Standard Time"),
    ("Europe/Paris", "Romance Standard Time"),
    ("Europe/Berlin", "W. Europe Standard Time"),
    ("Europe/Amsterdam", "W. Europe Standard Time"),
    ("Europe/Madrid", "Romance Standard Time"),
    ("Europe/Rome", "W. Europe Standard Time"),
    ("Europe/Stockholm", "W. Europe Standard Time"),
    ("Europe/Warsaw", "Central European Standard Time"),
    ("Europe/Moscow", "Russian Standard Time"),
    ("Asia/Tokyo", "Tokyo Standard Time"),
    ("Asia/Seoul", "Korea Standard Time"),
    ("Asia/Shanghai", "China Standard Time"),
    ("Asia/Hong_Kong", "China Standard Time"),
    ("Asia/Taipei", "Taipei Standard Time"),
    ("Asia/Singapore", "Singapore Standard Time"),
    ("Asia/Kolkata", "India Standard Time"),
    ("Asia/Dubai", "Arabian Standard Time"),
    ("Australia/Sydney", "AUS Eastern Standard Time"),
    ("Pacific/Auckland", "New Zealand Standard Time"),
];

pub fn iana_to_windows(iana: &str) -> Option<&'static str> {
    MAP.iter().find(|(k, _)| *k == iana).map(|(_, v)| *v)
}

#[cfg(windows)]
pub fn current_windows_tz() -> Result<String> {
    let out = crate::process::hidden_std(std::process::Command::new("tzutil"))
        .arg("/g")
        .output()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(GateError::Other("读不到当前系统时区".into()));
    }
    Ok(s)
}

#[cfg(not(windows))]
pub fn current_windows_tz() -> Result<String> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

/// 按 IP 归属的 IANA 时区切换系统时区。需要管理员权限。
///
/// 已经是目标时区就什么都不做。切了的话，原值只写进审计日志（「系统时区 A -> B」）。
#[cfg(windows)]
pub fn apply_timezone(iana: &str) -> Result<()> {
    let target = iana_to_windows(iana).ok_or_else(|| {
        GateError::Other(format!(
            "没有 {iana} 对应的 Windows 时区，请在设置里手动选择，不要让程序猜"
        ))
    })?;
    let original = current_windows_tz()?;
    if original == target {
        return Ok(());
    }
    let out = crate::process::hidden_std(std::process::Command::new("tzutil"))
        .args(["/s", target])
        .output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "切换时区失败（通常是没有管理员权限）：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    crate::audit::write(&format!("系统时区 {original} -> {target}"));
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_timezone(_iana: &str) -> Result<()> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_zones() {
        assert_eq!(
            iana_to_windows("America/New_York"),
            Some("Eastern Standard Time")
        );
        assert_eq!(
            iana_to_windows("Asia/Shanghai"),
            Some("China Standard Time")
        );
    }

    #[test]
    fn unknown_zone_returns_none_rather_than_guessing() {
        assert_eq!(iana_to_windows("Mars/Olympus"), None);
        // 大小写敏感是有意的：拿不准就报错，不要模糊匹配到别的时区。
        assert_eq!(iana_to_windows("america/new_york"), None);
    }
}
