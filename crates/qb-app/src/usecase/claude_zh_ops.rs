//! 清理 Claude 汉化残留（2026-09-27：Claude 汉化撤掉了，只剩这一件事）。
//!
//! 为什么撤、为什么清理不跑上游的卸载脚本，见 `plugins::claude_zh` 文件头。
//! 文件那一侧（列出、逐字节比、换回原版、删）在 `plugins::claude_zh`；这里是要碰进程与门禁的那一串：
//!
//! 1. 看还剩什么（[`leftovers`]，只读）；
//! 2. 要往 Claude **现在用的那一版**里写（换回原版文件、删中文文件）时，先关桌面端 ——
//!    `workspace::close_desktop_for_relay`：按证据收、关不干净就停（确认框里事先说了）；
//! 3. 逐个 `app-*` 清：换回原版 → 删中文文件 → 删上游的备份；
//! 4. 把汉化留下的中文界面语言写回（汉化之前的值，没记就 en-US）；
//! 5. 删上游建的 `Claude-3p`、面板自己下载过的上游副本与那版插件的记录。
//!
//! 不联网、不起 PowerShell、不跑上游脚本；不重启 Claude（它是使用者自己再开的）。
use crate::error::{GateError, Result};
use crate::gate::GateState;
use crate::plugins::claude_zh::{self, ClaudeZhLeftovers};
use crate::sink::ProgressSink;
use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

/// 进度一共几段。
pub const TOTAL: u32 = 5;

/// 点了之后发生了什么。每一句都是界面原样显示的纯文本。
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhOutcome {
    /// 清完之后复查，一样都没剩。
    pub ok: bool,
    /// 逐条做了什么。
    pub lines: Vec<String>,
    /// 为了往现在用的那一版里写，先关了 Claude 桌面端。
    pub closed_claude: bool,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| GateError::Other(e.to_string()))
}

/// Claude 桌面端开着几个进程。查不到是 `None`（**不是「没有」**）。
async fn desktop_count() -> Option<u32> {
    blocking(crate::killswitch::desktop_processes)
        .await
        .ok()
        .and_then(|r| r.ok())
        .map(|v| v.len() as u32)
}

/// 还剩什么。只读本机，不联网。
pub async fn leftovers() -> ClaudeZhLeftovers {
    let mut l = blocking(claude_zh::leftovers).await.unwrap_or_default();
    l.desktop_running = desktop_count().await;
    l
}

/// 一次性清理。命令层拿着独占锁调它。
pub async fn cleanup(gate: &GateState, rep: &dyn ProgressSink) -> Result<ClaudeZhOutcome> {
    rep.phase(1, "看还剩什么");
    let before = blocking(claude_zh::leftovers).await?;
    let mut out = ClaudeZhOutcome::default();
    if before.empty {
        out.ok = true;
        out.lines.push("没有残留，不用清。".into());
        return Ok(out);
    }

    // 往现在用的那一版里写之前先关 Claude（确认框里写着）。不用写的时候不关 ——
    // 使用者多半正在 Claude 里干活（Code 页的会话也在里面）。
    let mut current_writable = false;
    if before.needs_close {
        rep.phase(2, "关闭 Claude 桌面端");
        let n = crate::workspace::close_desktop_for_relay(gate, |n| {
            rep.log(2, &format!("正在关闭 {n} 个桌面端进程"))
        })
        .await
        .map_err(|e| {
            GateError::Other(format!(
                "清理要先关 Claude 桌面端，没关成，这次什么都没动：{e}"
            ))
        })?;
        out.closed_claude = n > 0;
        current_writable = true;
        if n > 0 {
            out.lines
                .push(format!("先关掉了 Claude 桌面端（{n} 个进程）。"));
        }
    } else if desktop_count().await == Some(0) {
        // 事先按大小没看出要写，可 Claude 反正没开 —— 逐字节比出同样大小的改动也能直接换。
        current_writable = true;
    }

    rep.phase(3, "把上游改过的文件换回原版");
    let apps = before.apps.clone();
    let cleaned = blocking(move || {
        apps.iter()
            .map(|a| {
                claude_zh::clean_app(
                    Path::new(&a.dir),
                    &a.version,
                    !a.current || current_writable,
                )
            })
            .collect::<Vec<_>>()
    })
    .await?;
    for c in cleaned {
        for line in &c.lines {
            rep.log(3, line);
        }
        out.lines.extend(c.lines);
    }

    rep.phase(4, "写回界面语言");
    let state = claude_zh::load_state();
    let mut profiles_ok = true;
    for p in &before.zh_profiles {
        let dir = Path::new(&p.dir);
        let back = state
            .previous_locales
            .get(&claude_zh::real_dir(dir).display().to_string())
            .cloned()
            .flatten()
            .filter(|l| !claude_zh::is_zh(l))
            .unwrap_or_else(|| "en-US".into());
        match claude_zh::set_profile_locale(dir, &back) {
            Ok(_) => out.lines.push(format!("{}：界面语言写回 {back}。", p.dir)),
            Err(e) => {
                profiles_ok = false;
                out.lines
                    .push(format!("{}：没能写回界面语言（{e}）。", p.dir));
            }
        }
    }

    rep.phase(5, "删掉上游与面板留下的东西");
    match blocking(claude_zh::remove_stray_3p).await? {
        Ok(Some(dir)) => out.lines.push(format!(
            "删掉了上游建的 {}（里面只有一个写了界面语言的 config.json）。",
            dir.display()
        )),
        Ok(None) => {}
        Err(e) => out.lines.push(e.to_string()),
    }
    if profiles_ok {
        match blocking(claude_zh::remove_panel_copy).await? {
            Ok(removed) => {
                for r in removed {
                    out.lines.push(format!("删掉了 {r}。"));
                }
            }
            Err(e) => out.lines.push(e.to_string()),
        }
    } else {
        // 那份记录里有各份资料汉化之前的界面语言 —— 还没全写回之前留着。
        out.lines
            .push("有资料的界面语言没写回，面板那份记录（里面有汉化之前的值）先留着。".into());
    }

    let after = blocking(claude_zh::leftovers).await?;
    out.ok = after.empty;
    crate::audit::write(&format!(
        "Claude 汉化残留：清理{}{}",
        if out.ok { "完成" } else { "没全清掉" },
        if out.closed_claude {
            "（先关了 Claude 桌面端）"
        } else {
            ""
        }
    ));
    Ok(out)
}
