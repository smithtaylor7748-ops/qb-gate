//! Claude 桌面端中文界面的编排（2026-09-25，使用者定的：只接上游安全模式 + 事后核验）。
//!
//! 插件本身（取上游、挑文件、核许可证、拼参数、读状态）在 `plugins::claude_zh`；这里是要同时碰
//! 门禁、进程、安装目录、账户资料的那一串：
//!
//! 1. 位置表认得的 Squirrel 装法才做（MSIX 如实说不做）；
//! 2. 上游副本：没有就下，点了「更新」就换最新版（被停用的那一版不用）；
//! 3. **门禁先判**（`gate::judge_now`，只判不解锁）：上游脚本最后会用 `app-*\claude.exe` 自己
//!    重启 Claude —— 那份 exe 按规矩不能加 Deny（开新窗口就崩），出口 IP 不合格时就不许它起；
//! 4. 关桌面端：`workspace::close_desktop_for_relay`（按证据收，关不干净就停）；
//! 5. 基线：`app.asar`、`claude.exe` 的 SHA-256 + Authenticode **状态**；读不出来就不做 ——
//!    做完没法核验的事不做；
//! 6. 跑上游：参数只由 `claude_zh::script_args` 拼（永远是安全模式）；
//! 7. 收回它自己重启的那份（不归面板管、没有租约、看门狗不认它），再核验：
//!    **哈希或签名状态变了 = 越线**（上游以后把安全模式改成会动 app.asar / Claude.exe）→
//!    立刻跑上游 `uninstall` 还原、把那一版记进黑名单、界面标红。这一步不信上游，只信文件。
//!
//! 核验过了才把别的账户资料的 `locale` 也设成 zh-CN —— 上游只改当前那份（`%APPDATA%\Claude`
//! 经联结点指着的那个），而面板的每个 Claude 账户槽位各有一份资料，换个账户就又是英文了。
//! 改之前的值记下来，「恢复英文」时写回。
use crate::error::{GateError, Result};
use crate::gate::GateState;
use crate::plugins::claude_zh::{self, Action, ClaudeZhStatus, Package};
use crate::sink::ProgressSink;
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 进度一共几段（一键汉化与恢复英文共用，恢复英文跳过门禁那一段）。
pub const TOTAL: u32 = 7;

/// 点了之后发生了什么。每一句都是界面原样显示的纯文本。
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhOutcome {
    /// 核验全过。
    pub ok: bool,
    /// 逐条核验结果与做了什么。
    pub lines: Vec<String>,
    /// 核验发现越线，已自动还原并停用了那一版上游。
    pub restored: bool,
    /// 用的是上游哪一版。
    pub tag: Option<String>,
}

/// 弹窗 / 插件页要的现状。**不联网**。
pub async fn status() -> ClaudeZhStatus {
    let msix = if claude_zh::target().is_none() {
        let sw = crate::install::detect::claude_desktop().await;
        // 没有 Squirrel 装法、检测却说装了 = MSIX（`detect` 只在 MSIX 那一档给 advisory）。
        (sw.installed && sw.advisory.is_some())
            .then(|| sw.version.unwrap_or_else(|| "版本未知".into()))
    } else {
        None
    };
    let mut s = claude_zh::status(msix);
    s.desktop_running = tokio::task::spawn_blocking(crate::killswitch::desktop_processes)
        .await
        .ok()
        .and_then(|r| r.ok())
        .map(|v| v.len() as u32);
    s
}

/// 问一次上游最新版（**会联网**；只在打开汉化弹窗 / 插件页、或点「检查更新」时调）。
pub async fn check() -> Result<ClaudeZhStatus> {
    claude_zh::fetch_latest_tag().await?;
    Ok(status().await)
}

/// 准备上游副本：没有就取最新版；`upgrade` 就换成最新版。**不拿独占锁**（只动插件自己的目录），
/// 所以命令层在它之后才去拿锁 —— 下载的那一两分钟里看门狗照常巡检。
pub async fn prepare(upgrade: bool, rep: &dyn ProgressSink) -> Result<Package> {
    rep.phase(2, "准备上游副本");
    let state = claude_zh::load_state();
    let current = claude_zh::current_package(&state);
    let package = match current {
        Some(p) if !upgrade => p,
        current => {
            let latest = claude_zh::fetch_latest_tag().await?;
            match current {
                Some(p) if p.tag == latest => p,
                _ => {
                    if state.blocked.contains(&latest) {
                        return Err(blocked(&latest));
                    }
                    rep.log(2, &format!("取上游 {} {latest}", claude_zh::UPSTREAM_REPO));
                    claude_zh::download(&latest, rep, 2).await?
                }
            }
        }
    };
    if claude_zh::load_state().blocked.contains(&package.tag) {
        return Err(blocked(&package.tag));
    }
    rep.log(2, &format!("用上游 {}", package.tag));
    Ok(package)
}

fn blocked(tag: &str) -> GateError {
    GateError::Other(format!(
        "上游 {tag} 这一版被停用了：上次核验发现它改动了 app.asar 或 Claude.exe（越过了「只用安全模式」那条线）。等上游发新版再点「检查更新」。"
    ))
}

/// 改之前的样子。**纯数据**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub asar: Option<String>,
    pub exe: Option<String>,
    /// Authenticode 状态（`Valid` / `HashMismatch` …）。拿不到是 `None`（不算越线，只看哈希）。
    pub exe_signature: Option<String>,
}

async fn baseline(app_dir: &Path) -> Baseline {
    let asar = app_dir.join("resources").join("app.asar");
    let exe = app_dir.join("claude.exe");
    let exe2 = exe.clone();
    let (asar, exe_hash) = tokio::task::spawn_blocking(move || {
        (
            crate::install::managed::sha256_file(&asar),
            crate::install::managed::sha256_file(&exe2),
        )
    })
    .await
    .unwrap_or((None, None));
    Baseline {
        asar,
        exe: exe_hash,
        exe_signature: crate::signature::status_of(&exe).await,
    }
}

/// **纯函数**：拿改之前与改之后比，越线的每一条都说出来。空 = 没越线。
///
/// - `app.asar` / `claude.exe` 的哈希变了（或者之后读不出来了）= 越线；
/// - 签名状态两次都读到了、却不一样（`Valid` → `HashMismatch`）= 越线。
///   有一次没读到只说「没验成」，不算越线 —— 哈希那一条已经是硬判据。
pub fn breaches(before: &Baseline, after: &Baseline) -> Vec<String> {
    let mut out = Vec::new();
    let mut hash = |what: &str, b: &Option<String>, a: &Option<String>| match (b, a) {
        (Some(b), Some(a)) if a != b => out.push(format!("{what} 被改了（SHA-256 变了）")),
        (Some(_), None) => out.push(format!("改完之后读不出 {what} 了")),
        _ => {}
    };
    hash("app.asar", &before.asar, &after.asar);
    hash("claude.exe", &before.exe, &after.exe);
    if let (Some(b), Some(a)) = (&before.exe_signature, &after.exe_signature) {
        if a != b {
            out.push(format!("claude.exe 的签名状态从 {b} 变成了 {a}"));
        }
    }
    out
}

/// 实际的资料目录（联结点解开、去掉 `\\?\` 前缀）。记 `locale` 用它当键：
/// `%APPDATA%\Claude` 是指向当前账户的联结点，换了账户它就指向别处了。
fn real_dir(p: &Path) -> PathBuf {
    let real = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = real.display().to_string();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// 跑上游脚本。失败时把最后几行原话带出来。
async fn run(
    action: Action,
    package: &Package,
    rep: &dyn ProgressSink,
    step: u32,
    log: &mut Vec<String>,
) -> Result<()> {
    let args = claude_zh::script_args(action, package.caps, &package.script);
    let ok = crate::install::winget::run_streaming_env(
        "powershell",
        &args,
        &[claude_zh::SKIP_UPDATE_CHECK],
        rep,
        step,
        log,
    )
    .await?;
    if ok {
        return Ok(());
    }
    let tail: Vec<&str> = log.iter().rev().take(4).rev().map(String::as_str).collect();
    Err(GateError::Other(format!(
        "上游 {} 的脚本没跑成功。最后几行：{}",
        package.tag,
        tail.join(" / ")
    )))
}

/// 上游脚本收尾时会自己用 `explorer.exe` 起一次 Claude（没有开关能关掉它）。那份不归面板管、
/// 没有租约 —— 等它出现再按证据关掉。它是异步起的，脚本退出那一刻多半还没进进程表，所以要等一会儿。
async fn close_restarted(gate: &GateState, rep: &dyn ProgressSink, step: u32) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let found = tokio::task::spawn_blocking(crate::killswitch::desktop_processes)
            .await
            .map_err(|e| GateError::Other(e.to_string()))??;
        if !found.is_empty() {
            // 让它把子进程拉起来再收，免得收完主进程又冒出一个没收到的。
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let n = crate::workspace::close_desktop_for_relay(gate, |_| {}).await?;
            rep.log(
                step,
                &format!("上游脚本自己重启的 Claude 桌面端已关掉（{n} 个进程）"),
            );
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            rep.log(step, "上游脚本没有重启 Claude 桌面端（等了 20 秒没见到）");
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// 位置表认不出 Squirrel 装法时说什么。
async fn unsupported() -> GateError {
    let s = status().await;
    GateError::Other(s.detail)
}

/// 一键汉化。`package` 由 [`prepare`] 先备好（命令层在那之后才拿独占锁）。
pub async fn apply(
    package: Package,
    gate: &GateState,
    rep: &dyn ProgressSink,
) -> Result<ClaudeZhOutcome> {
    rep.phase(1, "确认 Claude 桌面端的位置");
    let Some((version, app_dir)) = claude_zh::target() else {
        return Err(unsupported().await);
    };
    rep.log(1, &format!("{version}：{}", app_dir.display()));

    rep.phase(3, "验出口 IP");
    match crate::gate::judge_now().await {
        crate::gate::judge::Judgement::Allowed { .. } => {}
        other => {
            return Err(GateError::Other(format!(
                "出口 IP 不合格，这次不汉化 —— 上游脚本最后会自己重启 Claude 桌面端。{}",
                other.reason()
            )))
        }
    }

    rep.phase(4, "关闭 Claude 桌面端");
    let closed = crate::workspace::close_desktop_for_relay(gate, |n| {
        rep.log(4, &format!("正在关闭 {n} 个桌面端进程"))
    })
    .await?;
    rep.log(4, &format!("已关闭 {closed} 个桌面端进程"));

    rep.phase(5, "记下改之前的样子");
    let before = baseline(&app_dir).await;
    if before.asar.is_none() || before.exe.is_none() {
        return Err(GateError::Other(
            "读不出 app.asar 或 claude.exe 的哈希，做完之后就没法核验它们没被动过 —— 这次不汉化。"
                .into(),
        ));
    }
    let mut state = claude_zh::load_state();
    // 只在「还没汉化过」时记：重新应用的时候各份资料已经是 zh-CN 了，再记一次就把原来的值冲掉了。
    if state.previous_locales.is_empty() {
        for dir in claude_zh::profile_dirs() {
            state.previous_locales.insert(
                real_dir(&dir).display().to_string(),
                claude_zh::profile_locale(&dir),
            );
        }
        claude_zh::save_state(&state)?;
    }

    rep.phase(6, &format!("运行上游 {} 的安全模式", package.tag));
    let mut log = Vec::new();
    let ran = run(Action::Install, &package, rep, 6, &mut log).await;
    close_restarted(gate, rep, 6).await?;

    rep.phase(7, "核验");
    let after = baseline(&app_dir).await;
    let broke = breaches(&before, &after);
    if !broke.is_empty() {
        return guard_restore(&package, &app_dir, gate, rep, before, broke).await;
    }
    if let Err(e) = ran {
        // 跑到一半失败：前端 bundle 可能已经改了一半。上游自己也建议这时跑一次卸载 —— 替使用者跑。
        let mut undo = Vec::new();
        let undone = run(Action::Uninstall, &package, rep, 7, &mut undo).await;
        let _ = crate::workspace::close_desktop_for_relay(gate, |_| {}).await;
        return Err(GateError::Other(format!(
            "{e}{}",
            if undone.is_ok() {
                "。已用上游自己的卸载撤回了改了一半的部分，Claude 回到了原样。"
            } else {
                "。上游卸载也没跑成功，Claude 可能处于改了一半的状态 —— 到 Claude 官网重新下载安装包覆盖安装最稳。"
            }
        )));
    }

    let mut out = ClaudeZhOutcome {
        tag: Some(package.tag.clone()),
        ..Default::default()
    };
    out.lines.push(format!(
        "app.asar 与 claude.exe 没被动过（哈希一致{}）。",
        match (&before.exe_signature, &after.exe_signature) {
            (Some(b), Some(a)) if a == b => format!("，签名状态仍是 {a}"),
            _ => "；签名状态没读到，只核了哈希".to_string(),
        }
    ));
    if let Some(reported) = claude_zh::reported_app_dir(&log) {
        if !claude_zh::same_dir(&reported, &app_dir) {
            out.lines.push(format!(
                "上游改的是 {reported}，而位置表认的是 {} —— 两边对不上，已还原。",
                app_dir.display()
            ));
            let mut undo = Vec::new();
            let _ = run(Action::Uninstall, &package, rep, 7, &mut undo).await;
            return Ok(out);
        }
    }
    let files = claude_zh::zh_files_present(&app_dir);
    out.lines.push(if files {
        format!("这一版 Claude（{version}）上的中文文件在。")
    } else {
        "上游说跑完了，但这一版 Claude 上没有中文文件 —— 可能上游还没跟上这一版的 Claude。".into()
    });
    for dir in claude_zh::profile_dirs() {
        let shown = real_dir(&dir).display().to_string();
        match claude_zh::profile_locale(&dir).as_deref() {
            Some(claude_zh::LANGUAGE) => {}
            _ => match claude_zh::set_profile_locale(&dir, claude_zh::LANGUAGE) {
                Ok(_) => out
                    .lines
                    .push(format!("{shown}：界面语言设为 {}", claude_zh::LANGUAGE)),
                Err(e) => out.lines.push(format!("{shown}：没能设界面语言（{e}）")),
            },
        }
    }
    let active = dirs::config_dir()
        .map(|a| a.join("Claude"))
        .and_then(|d| claude_zh::profile_locale(&d));
    let locale_ok = active.as_deref() == Some(claude_zh::LANGUAGE);
    out.lines.push(if locale_ok {
        "当前账户资料的界面语言是 zh-CN。".into()
    } else {
        format!(
            "当前账户资料的界面语言是 {}，不是 zh-CN。",
            active.as_deref().unwrap_or("读不出来")
        )
    });
    out.ok = files && locale_ok;

    let mut state = claude_zh::load_state();
    if out.ok {
        state.desired = true;
        state.applied_tag = Some(package.tag.clone());
        state.applied_claude = Some(version.clone());
        state.applied_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
        claude_zh::save_state(&state)?;
    }
    crate::audit::write(&format!(
        "Claude 汉化：上游 {}（安全模式）→ Claude {version}，核验{}",
        package.tag,
        if out.ok { "通过" } else { "没全过" }
    ));
    Ok(out)
}

/// 越线了：立刻用上游自己的 `uninstall` 还原（它从 `.zh-cn-backups` 把原文件拷回来），
/// 再核一次，把那一版记进黑名单。**不管还原成没成都要让界面标红**。
async fn guard_restore(
    package: &Package,
    app_dir: &Path,
    gate: &GateState,
    rep: &dyn ProgressSink,
    before: Baseline,
    broke: Vec<String>,
) -> Result<ClaudeZhOutcome> {
    let mut out = ClaudeZhOutcome {
        tag: Some(package.tag.clone()),
        restored: true,
        ..Default::default()
    };
    out.lines.push(format!(
        "越线：{}。上游 {} 的「安全模式」动了不该动的文件，这是 Anthropic 条款不许的那一类（绕过防篡改）。",
        broke.join("；"),
        package.tag
    ));
    let mut log = Vec::new();
    let undone = run(Action::Uninstall, package, rep, 7, &mut log).await;
    let _ = crate::workspace::close_desktop_for_relay(gate, |_| {}).await;
    let again = breaches(&before, &baseline(app_dir).await);
    out.lines.push(match (&undone, again.is_empty()) {
        (Ok(()), true) => "已用上游自己的卸载还原，app.asar 与 claude.exe 又跟原来一样了。".into(),
        (_, false) => format!(
            "还原之后仍不一致：{}。请到 Claude 官网重新下载安装包覆盖安装。",
            again.join("；")
        ),
        (Err(e), true) => format!("文件已经跟原来一样，但上游卸载报了错：{e}"),
    });
    let mut state = claude_zh::load_state();
    if !state.blocked.contains(&package.tag) {
        state.blocked.push(package.tag.clone());
    }
    state.desired = false;
    claude_zh::save_state(&state)?;
    out.lines.push(format!(
        "上游 {} 已停用，面板不会再用它；等上游发了新版再点「检查更新」。",
        package.tag
    ));
    crate::audit::write(&format!(
        "Claude 汉化：上游 {} 越线（{}），已还原并停用",
        package.tag,
        broke.join("；")
    ));
    Ok(out)
}

/// 恢复英文：跑上游 `uninstall`，再把各份资料的 `locale` 写回汉化之前的值。
pub async fn restore(
    package: Package,
    gate: &GateState,
    rep: &dyn ProgressSink,
) -> Result<ClaudeZhOutcome> {
    rep.phase(1, "确认 Claude 桌面端的位置");
    let Some((version, app_dir)) = claude_zh::target() else {
        return Err(unsupported().await);
    };
    rep.phase(4, "关闭 Claude 桌面端");
    crate::workspace::close_desktop_for_relay(gate, |n| {
        rep.log(4, &format!("正在关闭 {n} 个桌面端进程"))
    })
    .await?;
    rep.phase(5, "记下改之前的样子");
    let before = baseline(&app_dir).await;
    rep.phase(6, &format!("运行上游 {} 的卸载", package.tag));
    let mut log = Vec::new();
    let ran = run(Action::Uninstall, &package, rep, 6, &mut log).await;
    let _ = crate::workspace::close_desktop_for_relay(gate, |_| {}).await;

    rep.phase(7, "核验");
    let mut out = ClaudeZhOutcome {
        tag: Some(package.tag.clone()),
        ..Default::default()
    };
    let broke = breaches(&before, &baseline(&app_dir).await);
    if !broke.is_empty() {
        out.lines.push(format!(
            "上游卸载动了 app.asar / claude.exe：{}。请到 Claude 官网重新下载安装包覆盖安装。",
            broke.join("；")
        ));
    }
    ran?;
    let files = claude_zh::zh_files_present(&app_dir);
    out.lines.push(if files {
        format!("这一版 Claude（{version}）上的中文文件还在 —— 上游卸载没删干净。")
    } else {
        format!("这一版 Claude（{version}）上的中文文件已删掉。")
    });
    let mut state = claude_zh::load_state();
    for (dir, previous) in &state.previous_locales {
        let dir = Path::new(dir);
        if !dir.join("config.json").is_file() {
            continue;
        }
        let back = previous.as_deref().unwrap_or("en-US");
        if claude_zh::profile_locale(dir).as_deref() == Some(back) {
            continue;
        }
        match claude_zh::set_profile_locale(dir, back) {
            Ok(_) => out
                .lines
                .push(format!("{}：界面语言写回 {back}", dir.display())),
            Err(e) => out
                .lines
                .push(format!("{}：没能写回界面语言（{e}）", dir.display())),
        }
    }
    out.ok = broke.is_empty() && !files;
    if out.ok {
        state.desired = false;
        state.applied_tag = None;
        state.applied_claude = None;
        state.applied_at = None;
        state.previous_locales.clear();
        claude_zh::save_state(&state)?;
    }
    crate::audit::write(&format!(
        "Claude 汉化：已恢复英文（上游 {} 的卸载），核验{}",
        package.tag,
        if out.ok { "通过" } else { "没全过" }
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(asar: &str, exe: &str, sig: Option<&str>) -> Baseline {
        Baseline {
            asar: Some(asar.into()),
            exe: Some(exe.into()),
            exe_signature: sig.map(str::to_string),
        }
    }

    #[test]
    fn untouched_files_are_not_a_breach() {
        assert!(breaches(&b("a", "e", Some("Valid")), &b("a", "e", Some("Valid"))).is_empty());
        // 签名状态有一次没读到：只看哈希，不算越线。
        assert!(breaches(&b("a", "e", Some("Valid")), &b("a", "e", None)).is_empty());
    }

    /// 上游 official 模式的样子：app.asar 改了、Claude.exe 内嵌哈希被重写、签名变 HashMismatch。
    /// 三条都要说出来。
    #[test]
    fn the_official_mode_shape_is_caught_on_every_count() {
        let got = breaches(
            &b("a", "e", Some("Valid")),
            &b("a2", "e2", Some("HashMismatch")),
        );
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(got.iter().any(|l| l.contains("app.asar")));
        assert!(got.iter().any(|l| l.contains("HashMismatch")));
    }

    #[test]
    fn a_file_that_became_unreadable_is_a_breach() {
        let after = Baseline {
            asar: None,
            ..b("a", "e", None)
        };
        assert_eq!(breaches(&b("a", "e", None), &after).len(), 1);
    }
}
