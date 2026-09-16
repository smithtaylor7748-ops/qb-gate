//! 区域格式与显示语言的对齐（0.19.0）。
//!
//! # 跟时区是同一档事，分两层
//!
//! 时区在 [`super`]（`tzutil`）。这里是另外两层：
//!
//! | 层 | 改什么 | 用什么 | 什么时候生效 |
//! |---|---|---|---|
//! | 区域格式 | 日期、数字、货币怎么显示（`Get-Culture`） | `Set-Culture` | 之后**新开的**程序 |
//! | 显示语言 | 界面语言（`Set-WinUILanguageOverride`） | 同名 cmdlet | **要注销之后** |
//!
//! 两层的代价差得很远，所以**不合并成一个开关**：区域格式改完立刻能用，
//! 显示语言要先装语言包、还要注销一次。合成一个开关的话，使用者点一下
//! 得到的是「一半立刻生效、一半要注销」，而界面说不清到底生效了没有。
//!
//! # ⛔ 查不到就不猜
//!
//! [`locale_for_zone`] 查不到对应关系时返回 `None`，由调用方报错 ——
//! 跟 `super::iana_to_windows` 是同一条规矩。猜错的后果是把使用者的机器
//! 设成一个他没要过的区域，比不改糟得多。
//!
//! # 这不是「防封」
//!
//! DISCLAIMER 写死了「去除中文环境特征不会降低任何封禁概率」。对齐只是让
//! Claude Code 读到的时区与格式跟出口一致，界面上不许写成安全措施。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{GateError, Result};

/// IANA 时区 → BCP-47 区域标签。
///
/// 只收 [`super::MAP`] 里那些常用时区对应的地区。一个时区可能横跨多个地区
/// （`Europe/Zurich` 有德语区也有法语区），所以这里给的是**那个时区最常见的
/// 那一个**，而不是唯一正确答案 —— 使用者要别的自己在设置里改。
const LOCALES: &[(&str, &str)] = &[
    ("America/New_York", "en-US"),
    ("America/Chicago", "en-US"),
    ("America/Denver", "en-US"),
    ("America/Phoenix", "en-US"),
    ("America/Los_Angeles", "en-US"),
    ("America/Anchorage", "en-US"),
    ("America/Toronto", "en-CA"),
    ("America/Vancouver", "en-CA"),
    ("America/Sao_Paulo", "pt-BR"),
    ("Europe/London", "en-GB"),
    ("Europe/Dublin", "en-IE"),
    ("Europe/Paris", "fr-FR"),
    ("Europe/Berlin", "de-DE"),
    ("Europe/Amsterdam", "nl-NL"),
    ("Europe/Madrid", "es-ES"),
    ("Europe/Rome", "it-IT"),
    ("Europe/Stockholm", "sv-SE"),
    ("Europe/Warsaw", "pl-PL"),
    ("Europe/Moscow", "ru-RU"),
    ("Asia/Tokyo", "ja-JP"),
    ("Asia/Seoul", "ko-KR"),
    ("Asia/Shanghai", "zh-CN"),
    ("Asia/Hong_Kong", "zh-HK"),
    ("Asia/Taipei", "zh-TW"),
    ("Asia/Singapore", "en-SG"),
    ("Asia/Kolkata", "en-IN"),
    ("Asia/Dubai", "ar-AE"),
    ("Australia/Sydney", "en-AU"),
    ("Pacific/Auckland", "en-NZ"),
];

/// 这个 IANA 时区对应哪个区域标签。**查不到返回 `None`，不猜。**
pub fn locale_for_zone(iana: &str) -> Option<&'static str> {
    LOCALES.iter().find(|(k, _)| *k == iana).map(|(_, v)| *v)
}

/// 改之前的样子，用来还原。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename = "LocaleState")]
pub struct LocaleState {
    /// 改之前的区域格式（`Get-Culture` 的 Name）。
    pub original_culture: String,
    pub applied_culture: String,
    /// 改之前的显示语言覆盖。**`None` = 本来就没设过覆盖**，
    /// 跟「设成了空字符串」是两回事，还原时要把覆盖整个清掉。
    pub original_ui: Option<String>,
    /// 这一轮有没有动显示语言。没动就是 `None`。
    pub applied_ui: Option<String>,
}

#[cfg(windows)]
fn powershell(script: &str) -> Result<String> {
    let out = crate::process::powershell_std(script).output()?;
    if !out.status.success() {
        return Err(GateError::Other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(windows))]
fn powershell(_script: &str) -> Result<String> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

/// 当前区域格式（形如 `zh-CN`）。
pub fn current_culture() -> Result<String> {
    let s = powershell("(Get-Culture).Name")?;
    if s.is_empty() {
        return Err(GateError::Other("读不到当前区域格式".into()));
    }
    Ok(s)
}

/// 当前显示语言覆盖。**没设过覆盖时返回 `None`**，不返回空串 ——
/// 「没设过」和「设成空」在还原时要走不同的分支。
pub fn current_ui_override() -> Result<Option<String>> {
    let s = powershell("Get-WinUILanguageOverride | Select-Object -ExpandProperty Name")?;
    Ok(if s.is_empty() { None } else { Some(s) })
}

/// 改区域格式。**不需要管理员权限，也不用重启** —— 之后新开的程序才读到新值。
pub fn apply_culture(tag: &str) -> Result<()> {
    powershell(&format!("Set-Culture -CultureInfo '{}'", escape(tag)))?;
    crate::audit::write(&format!("区域格式 -> {tag}"));
    Ok(())
}

/// 改显示语言覆盖。
///
/// ⚠ **要先装好对应语言包，而且要注销之后才生效。** 这个函数只负责设，
/// 设完是否真的能用由系统说了算 —— 界面必须把「要注销」说在前面。
pub fn apply_ui_language(tag: &str) -> Result<()> {
    powershell(&format!(
        "Set-WinUILanguageOverride -Language '{}'",
        escape(tag)
    ))?;
    crate::audit::write(&format!("显示语言覆盖 -> {tag}（注销后生效）"));
    Ok(())
}

/// 还原到 [`LocaleState`] 记下来的样子。
pub fn restore(st: &LocaleState) -> Result<()> {
    apply_culture(&st.original_culture)?;
    if st.applied_ui.is_some() {
        match &st.original_ui {
            Some(prev) => apply_ui_language(prev)?,
            // 本来就没设过覆盖 —— 要把覆盖整个清掉，而不是设成空串。
            None => {
                powershell("Set-WinUILanguageOverride -Language $null")?;
                crate::audit::write("显示语言覆盖已清除（注销后生效）");
            }
        }
    }
    Ok(())
}

/// 单引号在 PowerShell 单引号字符串里要写成两个。
///
/// 区域标签本来只该是字母和连字符，但这个值一路来自 IP 归属查询的返回，
/// **不是我们自己造的常量** —— 不转义就是一条命令注入。
fn escape(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_zones_to_locales() {
        assert_eq!(locale_for_zone("America/New_York"), Some("en-US"));
        assert_eq!(locale_for_zone("Europe/Paris"), Some("fr-FR"));
        assert_eq!(locale_for_zone("Asia/Shanghai"), Some("zh-CN"));
    }

    /// 查不到就是查不到 —— 不许模糊匹配到别的地区。
    /// 跟 `iana_to_windows` 那条「大小写敏感是有意的」是同一条规矩。
    #[test]
    fn unknown_zone_returns_none_rather_than_guessing() {
        assert_eq!(locale_for_zone("Mars/Olympus"), None);
        assert_eq!(locale_for_zone("america/new_york"), None);
    }

    /// 时区表里有的，这里都要有 —— 否则「时区能对齐、区域格式却说查不到」，
    /// 而使用者看不出为什么两个开关行为不一致。
    #[test]
    fn every_zone_we_can_set_also_has_a_locale() {
        for (zone, _) in super::super::MAP {
            assert!(
                locale_for_zone(zone).is_some(),
                "{zone} 能切时区却没有对应的区域标签"
            );
        }
    }

    /// 这个值来自第三方查询结果，不是我们自己造的常量 —— 必须转义。
    #[test]
    fn single_quotes_are_escaped() {
        assert_eq!(escape("en-US"), "en-US");
        assert_eq!(
            escape("x'; Remove-Item C:\\ -Recurse; '"),
            "x''; Remove-Item C:\\ -Recurse; ''"
        );
    }
}
