//! 升级官方 Claude Code。
//!
//! 移植自 `install-official-claude.ps1`。四条必须保留的行为：
//!
//!   1. **渠道不能写死 `stable`。** 实测 stable=2.1.236 而本机=2.1.258，
//!      照着它装就是降级。默认走 `latest`。
//!   2. **版本号统一取前三段再比。** 本机文件属性读出来是四段的 `2.1.258.0`，
//!      渠道接口返回三段的 `2.1.258`；直接按四段比会得出「渠道比本机旧」的
//!      错误结论 —— 恰好是这里要防的坑。
//!   3. **本机版本从文件属性读，不要运行 exe。** 没有租约时 claude.exe 上挂着
//!      Deny ExecuteFile，升级流程不能依赖「能把这个二进制启动起来」。
//!   4. **装完必须重新上锁。** 官方安装器写的是全新 exe，继承干净 ACL，
//!      门禁那条 Deny 跟着旧文件一起没了。重锁失败要报错，不能默默放过。

use crate::error::{GateError, Result};
use crate::events::Reporter;
use serde::Serialize;

const RELEASES_URL: &str = "https://downloads.claude.ai/claude-code-releases";
const INSTALLER_URL: &str = "https://claude.ai/install.ps1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Latest,
    Stable,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Channel::Latest => "latest",
            Channel::Stable => "stable",
        }
    }
}

impl Default for Channel {
    fn default() -> Self {
        // 默认 latest —— 见文件头第 1 条。
        Channel::Latest
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UpgradePlan {
    pub installed: Option<String>,
    pub available: Option<String>,
    pub action: Action,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 没装过，全新安装
    FreshInstall,
    /// 可以升级
    Upgrade,
    /// 已经是最新
    UpToDate,
    /// 渠道版本更旧，装下去是降级 —— 默认拒绝
    WouldDowngrade,
    /// 查不到渠道版本，交给官方安装器自己判断
    Unknown,
}

/// 版本号取前三段。取不到就是 `None`。
///
/// 这个函数是第 2 条教训的落点：两边位数不一样，必须归一化。
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for ch in text.trim().chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else {
            if !cur.is_empty() {
                nums.push(cur.parse::<u32>().ok()?);
                cur.clear();
            }
            // 版本号之间可能夹着别的字符，凑够三段就够了
            if nums.len() >= 3 {
                break;
            }
            if ch != '.' && !nums.is_empty() && nums.len() < 3 {
                // 形如 "v2.1.258 (build x)" —— 遇到非分隔符就重新开始找
                if ch.is_whitespace() || ch == '(' {
                    break;
                }
            }
        }
    }
    if !cur.is_empty() && nums.len() < 3 {
        nums.push(cur.parse::<u32>().ok()?);
    }
    if nums.len() >= 3 {
        Some((nums[0], nums[1], nums[2]))
    } else {
        None
    }
}

/// 只比前三段，决定该不该装。纯函数，可单测。
pub fn decide(installed: Option<(u32, u32, u32)>, available: Option<(u32, u32, u32)>) -> Action {
    match (installed, available) {
        (None, _) => Action::FreshInstall,
        (Some(_), None) => Action::Unknown,
        (Some(i), Some(a)) if a == i => Action::UpToDate,
        (Some(i), Some(a)) if a < i => Action::WouldDowngrade,
        _ => Action::Upgrade,
    }
}

/// 读本机版本。**从文件属性读，不运行它。**
#[cfg(windows)]
async fn installed_version(path: &std::path::Path) -> Option<String> {
    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-Item -LiteralPath '{}').VersionInfo.ProductVersion",
                path.display()
            ),
        ])
        .output()
        .await
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(not(windows))]
async fn installed_version(_path: &std::path::Path) -> Option<String> {
    None
}

async fn channel_version(ch: Channel) -> Option<String> {
    let c = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .ok()?;
    let text = c
        .get(format!("{RELEASES_URL}/{}", ch.as_str()))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    Some(text.trim().to_string())
}

/// 只看不装：报告本机版本、渠道版本与建议动作。
pub async fn plan(ch: Channel) -> UpgradePlan {
    let path = crate::plugins::sillytavern::find_official_claude().ok();
    let installed = match &path {
        Some(p) => installed_version(p).await,
        None => None,
    };
    let available = channel_version(ch).await;

    let action = decide(
        installed.as_deref().and_then(parse_version),
        available.as_deref().and_then(parse_version),
    );

    let detail = match action {
        Action::FreshInstall => "本机尚未安装官方 Claude Code，将执行全新安装。".into(),
        Action::UpToDate => format!(
            "已经是 {} 渠道的最新版本（{}），无需升级。",
            ch.as_str(),
            installed.clone().unwrap_or_default()
        ),
        Action::WouldDowngrade => format!(
            "已停止：{} 渠道的 {} 比本机的 {} 更旧，装下去是降级。\
             本机版本跑到渠道前面属于正常现象（早期自动升级留下的）。{}",
            ch.as_str(),
            available.clone().unwrap_or_default(),
            installed.clone().unwrap_or_default(),
            if matches!(ch, Channel::Stable) {
                "要升级请改用 latest 渠道。"
            } else {
                ""
            }
        ),
        Action::Upgrade => format!(
            "可升级：{} → {}",
            installed.clone().unwrap_or_default(),
            available.clone().unwrap_or_default()
        ),
        Action::Unknown => "查不到渠道版本，跳过版本比较，交给官方安装器自己判断。".into(),
    };

    UpgradePlan {
        installed,
        available,
        action,
        detail,
    }
}

/// 执行升级。`force` 为真时允许降级。
pub async fn execute(ch: Channel, force: bool, rep: &Reporter) -> Result<String> {
    rep.phase(1, "比对本机版本与渠道版本");
    let p = plan(ch).await;
    if !force && matches!(p.action, Action::UpToDate | Action::WouldDowngrade) {
        rep.done(p.detail.clone());
        return Ok(p.detail);
    }

    // 先清上一次留下的残留 —— 见 gate::clean_stale_copies 的说明，
    // 本次安装新产生的那一份很可能正被占用，留到下次开头再清。
    rep.phase(2, "清理上一次升级留下的残留副本");
    let cleaned = crate::gate::clean_stale_copies();
    for (path, ok) in &cleaned {
        rep.log(2, format!("{} {}", if *ok { "已删除" } else { "删不掉（多半正被占用）" }, path.display()));
    }

    // 官方安装器自己做校验和验证，这里不重复实现。
    rep.phase(3, "下载官方安装脚本");
    let script = std::env::temp_dir().join(format!("claude-installer-{}.ps1", std::process::id()));
    let body = reqwest::Client::new()
        .get(INSTALLER_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    std::fs::write(&script, body)?;

    rep.phase(4, format!("运行官方安装器（{} 渠道）", ch.as_str()));
    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.display().to_string(),
            ch.as_str(),
        ])
        .output()
        .await;
    let _ = std::fs::remove_file(&script);

    let out = out?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if !line.trim().is_empty() {
            rep.log(4, line.trim().to_string());
        }
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        rep.fail(format!("官方 Claude 安装程序失败：{err}"));
        return Err(GateError::Other(format!("官方 Claude 安装程序失败：{err}")));
    }

    // 装完必须重新上锁 —— 新 exe 继承的是干净 ACL，门禁那条 Deny 没了。
    // 这里失败要报错，不能默默放过，否则会留下一个没有执行锁的新二进制。
    rep.phase(5, "重新上锁");
    let relocked = crate::gate::lock_all().map_err(|e| {
        GateError::Other(format!(
            "安装成功，但执行锁重建失败：{e}。新的 claude.exe 目前没有锁，\
             请到「IP 锁」页手动点一次「立即全部上锁」。"
        ))
    })?;

    let after = plan(ch).await;
    crate::gate::log::write(&format!(
        "升级完成，版本 {}，重新上锁 {} 个",
        after.installed.clone().unwrap_or_default(),
        relocked
    ));

    let detail = format!(
        "已安装 {}。重新上锁 {} 个可执行文件。{}",
        after.installed.unwrap_or_default(),
        relocked,
        if cleaned.is_empty() {
            String::new()
        } else {
            format!("顺带清理了 {} 个旧残留副本。", cleaned.len())
        }
    );
    rep.done(detail.clone());
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_and_four_segment_versions_the_same() {
        // 这条是本模块存在的理由：两边位数不一样，归一化后必须相等。
        assert_eq!(parse_version("2.1.258.0"), Some((2, 1, 258)));
        assert_eq!(parse_version("2.1.258"), Some((2, 1, 258)));
        assert_eq!(parse_version("2.1.258.0"), parse_version("2.1.258"));
    }

    #[test]
    fn tolerates_surrounding_noise() {
        assert_eq!(parse_version("  v2.1.236\n"), Some((2, 1, 236)));
    }

    #[test]
    fn rejects_unparseable() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("not a version"), None);
        assert_eq!(parse_version("2.1"), None);
    }

    #[test]
    fn four_vs_three_segments_is_not_a_downgrade() {
        // 回归测试：naive [version] 比较会把这个判成 WouldDowngrade。
        let installed = parse_version("2.1.258.0");
        let available = parse_version("2.1.258");
        assert_eq!(decide(installed, available), Action::UpToDate);
    }

    #[test]
    fn older_channel_is_refused() {
        let installed = parse_version("2.1.258.0");
        let available = parse_version("2.1.236");
        assert_eq!(decide(installed, available), Action::WouldDowngrade);
    }

    #[test]
    fn newer_channel_upgrades() {
        assert_eq!(
            decide(parse_version("2.1.236"), parse_version("2.1.258")),
            Action::Upgrade
        );
    }

    #[test]
    fn missing_install_is_fresh() {
        assert_eq!(decide(None, parse_version("2.1.258")), Action::FreshInstall);
    }

    #[test]
    fn unknown_channel_version_does_not_block() {
        assert_eq!(decide(parse_version("2.1.258"), None), Action::Unknown);
    }

    #[test]
    fn default_channel_is_latest_not_stable() {
        // 写死 stable 会导致降级，见文件头第 1 条。
        assert_eq!(Channel::default(), Channel::Latest);
    }
}
