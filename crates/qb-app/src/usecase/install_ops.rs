//! 装机编排：装一个目标、升级官方 Claude Code。
//!
//! # 为什么这两段流程不留在 `install` 里
//!
//! 它们在动文件之前要**把门禁的执行锁摘掉**、动完再锁回去，升级那条还要开一个
//! 结束时能把租约还回去的维护窗口。那是「门禁 + 安装」两件事的编排，
//! 不是安装自己的事 —— 留在 `install` 里的代价是 `gate ↔ install` 这一对
//! 循环依赖，而**互相依赖的两个模块拆不进两个 crate**。
//!
//! 留在 `install` 的是能单独说清楚的那部分：版本号怎么比（`parse_version` /
//! `decide`）、winget 怎么调（`winget_args` / `fallback_for`）、换没换成怎么判
//! （`matches_target` / `looks_unchanged`）—— 它们全都有单测。搬上来的是那两段
//! 「按顺序把这些拼起来、顺便往界面报进度」的流程。
//!
//! 搬家的过程中**一个字的语义都没改**：六段/七段的顺序、每一段的文案、
//! 失败分支重锁的时机，全部照抄。

use crate::error::{GateError, Result};
use crate::gate::GateState;
use crate::install::upgrade::{Action, Channel};
use crate::install::winget::{self, Fallback, InstallResult, InstallTarget, Method};
use crate::sink::ProgressSink;

/// 装一个目标。
///
/// 顺序是定死的，六段每一段都有理由：
///
///   1. **先解锁**。目标 exe 上挂着 Deny ExecuteFile，虽然实测它不挡写入，
///      但安装器可能要执行旧版本做卸载/迁移。先摘掉最稳妥；
///      整个过程在面板控制下，结尾必然重锁。
///   2. winget 安装，输出逐行推给界面。
///   3. 失败就走兜底（Claude Code 有官方脚本；桌面端只能给下载页）。
///   4. 重新枚举副本 —— 新装的可能落在版本化目录里。
///   5. 核对 Authenticode 主体（只警告，不阻断）。
///   6. **重新上锁，失败必须报错**。新 exe 继承的是干净 ACL，
///      门禁那条 Deny 不会自己跟过去。
pub async fn winget_install(
    target: InstallTarget,
    rep: &dyn ProgressSink,
) -> Result<InstallResult> {
    let mut log: Vec<String> = Vec::new();
    let mut method = Method::Winget;

    // ---- 1 ----
    // Codex 不归 Claude 门禁管，对它解锁再重锁只是白白把 Claude 的门
    // 开一遍。等 Codex 也纳入门禁后，`under_qb_gate` 会变成 true。
    if target.under_qb_gate() {
        rep.phase(1, "摘掉执行锁");
        match crate::gate::unlock_all() {
            Ok(n) => log.push(format!("已解锁 {n} 个副本")),
            Err(e) => log.push(format!("解锁未完全成功（继续）：{e}")),
        }
    } else {
        rep.phase(1, "跳过执行锁（该目标不归 Claude 门禁管）");
        log.push("Codex 不在 IP 锁的管辖范围内，未改动任何 ACL".into());
    }

    // ---- 2 ----
    rep.phase(2, &format!("用 winget 安装 {}", target.label()));
    let mut ok = winget::run_streaming("winget", &winget::winget_args(target), rep, 2, &mut log)
        .await
        .unwrap_or(false);

    // ---- 3 ----
    if !ok {
        match winget::fallback_for(target) {
            Fallback::OfficialScript => {
                rep.phase(3, "winget 没成功，改用官方安装脚本");
                log.push("winget 未成功，回退到官方安装脚本".into());
                method = Method::OfficialScript;
                let args: Vec<String> = [
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    "irm https://claude.ai/install.ps1 | iex",
                ]
                .into_iter()
                .map(String::from)
                .collect();
                ok = winget::run_streaming("powershell", &args, rep, 3, &mut log)
                    .await
                    .unwrap_or(false);
            }
            Fallback::NpmGlobal => {
                rep.phase(3, "winget 没成功，改用 npm 全局安装");
                log.push("winget 未成功，回退到 npm install -g @openai/codex".into());
                method = Method::NpmGlobal;
                // npm 在 Windows 上是 npm.cmd，得走 cmd /c。
                let args: Vec<String> = ["/c", "npm", "install", "-g", "@openai/codex"]
                    .into_iter()
                    .map(String::from)
                    .collect();
                ok = winget::run_streaming("cmd", &args, rep, 3, &mut log)
                    .await
                    .unwrap_or(false);
            }
            Fallback::ManualDownload => {
                rep.phase(3, "winget 没成功");
                method = Method::ManualDownload;
                // 这里**不重锁就返回是不行的** —— 第 1 步已经解锁了。
                let relocked = relock_if_gated(target);
                let detail = format!(
                    "{} 没能通过 winget 装上，请到官方下载页手动安装。\
                     执行锁已恢复（{relocked} 个）。",
                    target.label()
                );
                rep.fail(&detail);
                return Ok(InstallResult {
                    target,
                    ok: false,
                    method,
                    relocked,
                    signature_ok: None,
                    detail,
                    log,
                });
            }
        }
    }

    // 兜底也没成。**必须在这里停住** —— 继续往下走会重新枚举、验签、重锁，
    // 然后返回 ok: true，把「根本没装上」报告成「装好了」。
    if !ok {
        let relocked = relock_if_gated(target);
        let detail = format!(
            "{} 没能装上：winget 与兜底方式都失败了。执行锁已恢复（{relocked} 个）。\
             下面的日志里通常能看出是网络问题还是权限问题。",
            target.label()
        );
        rep.fail(&detail);
        return Ok(InstallResult {
            target,
            ok: false,
            method,
            relocked,
            signature_ok: None,
            detail,
            log,
        });
    }

    // ---- 4 ----
    rep.phase(4, "重新枚举可执行副本");
    let targets = crate::gate::collect_targets();
    log.push(format!("枚举到 {} 个 claude.exe 副本", targets.len()));

    // 桌面端的位置是官方安装器定的（Squirrel 写死在 %LOCALAPPDATA%\AnthropicClaude），
    // 面板接管不了它的目录 —— 那就把实际装在哪从卸载登记里读出来记下，不靠猜。
    if target == InstallTarget::ClaudeDesktop {
        match winget::desktop_install_location().await {
            Some(loc) => {
                let line = format!("桌面端装在 {loc}（官方安装器定的位置，面板接管不了，已记下）");
                crate::audit::write(&line);
                log.push(line);
            }
            None => log.push(
                "注册表里没读到桌面端的卸载登记，按默认位置 %LOCALAPPDATA%\\AnthropicClaude 认"
                    .into(),
            ),
        }
    }

    // ---- 5 ----
    rep.phase(5, "核对 Authenticode 签名");
    // 只把桌面端存根那一条挑出来传进去 —— `verify_signature` 不该认识
    // `gate::targets` 那套类型（见它自己的说明）。
    let desktop_stub = targets
        .iter()
        .find(|t| matches!(t.kind, crate::gate::targets::TargetKind::DesktopStub))
        .map(|t| t.path.as_path());
    let signature_ok = winget::verify_signature(target, desktop_stub).await;
    match signature_ok {
        Some(true) => log.push(format!("签名主体含 {}", target.expected_signer())),
        Some(false) => log.push(format!(
            "⚠ 签名主体里没有 {} —— 只是警告，不阻断",
            target.expected_signer()
        )),
        None => log.push("拿不到签名信息（不等于没签名）".into()),
    }

    // ---- 6 ----
    let relocked = if target.under_qb_gate() {
        rep.phase(6, "重新上锁");
        crate::gate::lock_all().map_err(|e| {
            rep.fail(&format!("安装成功，但执行锁重建失败：{e}"));
            GateError::Other(format!(
                "{} 安装成功，但执行锁重建失败：{e}。\
                 新的 claude.exe 目前没有锁，请到「IP 锁」页手动点一次「立即全部上锁」。",
                target.label()
            ))
        })?
    } else {
        rep.phase(6, "无需重新上锁");
        0
    };

    let detail = format!(
        "{} 已安装（{}），重新上锁 {} 个可执行文件。",
        target.label(),
        match method {
            Method::Winget => "winget",
            Method::OfficialScript => "官方安装脚本",
            Method::NpmGlobal => "npm 全局",
            Method::ManualDownload => "手动",
            Method::Managed => "面板托管",
        },
        relocked
    );
    crate::audit::write(&detail);
    rep.done(&detail);

    Ok(InstallResult {
        target,
        ok: true,
        method,
        relocked,
        signature_ok,
        detail,
        log,
    })
}

/// 只有归 Claude 门禁管的目标才重锁。返回重锁了几个。
///
/// 提出来是因为失败分支有两处要调它，漏掉任何一处都会让面板在
/// 「第 1 步已解锁」之后带着开着的门返回。
fn relock_if_gated(t: InstallTarget) -> usize {
    if t.under_qb_gate() {
        crate::gate::lock_all().unwrap_or(0)
    } else {
        0
    }
}

// ------------------------------------------------------------------ 执行

/// 执行升级。`force` 为真时允许降级。
pub async fn upgrade(
    ch: Channel,
    force: bool,
    state: &GateState,
    rep: &dyn ProgressSink,
) -> Result<String> {
    rep.phase(1, "比对本机版本与渠道版本");
    let p = crate::install::upgrade::plan(ch).await;
    if !force && matches!(p.action, Action::UpToDate | Action::WouldDowngrade) {
        rep.done(&p.detail);
        return Ok(p.detail);
    }

    // 先清上一次留下的残留 —— 见 gate::clean_stale_copies 的说明，
    // 本次安装新产生的那一份很可能正被占用，留到下次开头再清。
    rep.phase(2, "清理上一次升级留下的残留副本");
    let cleaned = crate::gate::clean_stale_copies();
    for (path, ok) in &cleaned {
        rep.log(
            2,
            &format!(
                "{} {}",
                if *ok {
                    "已删除"
                } else {
                    "删不掉（多半正被占用）"
                },
                path.display()
            ),
        );
    }

    // 官方安装器自己做校验和验证，这里不重复实现。
    rep.phase(3, "下载官方安装脚本");
    let script = std::env::temp_dir().join(format!("claude-installer-{}.ps1", std::process::id()));
    let body = reqwest::Client::new()
        .get(crate::install::upgrade::INSTALLER_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    std::fs::write(&script, body)?;

    // ---- 维护窗口开始。
    //
    // 老代码从头到尾**一次都没解过锁**就去跑安装器了。安装器是个黑盒第三方
    // 程序（`install.ps1` 只是引导壳，真正干活的是它下下来的那个二进制），
    // 它要不要执行一下现有的 launcher、要不要读它，我们无从知道 ——
    // 那就别让门禁成为一个未知变量。
    //
    // 守卫的 Drop 里有同步兜底：下面任何一个 `?` 提前返回都仍然会重锁。
    let guard = crate::gate::Maintenance::with_unlock(state);

    rep.phase(4, &format!("运行官方安装器（{} 渠道）", ch.as_str()));
    if !guard.did_unlock() {
        rep.log(4, "注意：这一轮没能先解开执行锁，安装器可能因此写不进去");
    }
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
            rep.log(4, line.trim());
        }
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = format!("官方 Claude 安装程序失败：{err}");
        let done = guard.finish(state).await;
        rep.fail(&format!("{msg} {}", done.detail));
        return Err(GateError::Other(msg));
    }

    // ---- 核对：**按哈希，不是按退出码，也不只是按版本号字符串。**
    //
    // 2026-09-10 实测的一次「成功的失败」：安装器把 2.1.267 好好地放进了
    // `versions\2.1.267`，却**换不动**（其实是压根没去换）
    // `~\.local\bin\claude.exe`，并且照样打印
    // 「✓ successfully installed! Version: 2.1.267」并 exit 0。
    rep.phase(5, "核对磁盘上那份到底换没换");
    let bin = crate::install::upgrade::native_bin();
    let mut supplemented: Option<String> = None;
    // 哈希核对给出结论了吗。`false` 时下面要退回版本号那条兜底判据。
    let mut settled_by_hash = false;

    if let (Some(bin), Some(target)) = (bin.as_ref(), p.available.as_deref()) {
        let bin_hash = crate::install::upgrade::sha256_of(bin);
        let target_hash = crate::install::upgrade::versions_file(target)
            .as_deref()
            .and_then(crate::install::upgrade::sha256_of);

        match crate::install::upgrade::matches_target(bin_hash.as_deref(), target_hash.as_deref()) {
            Some(true) => {
                settled_by_hash = true;
                rep.log(5, &format!("已确认 {} 就是 {target}", bin.display()));
            }
            Some(false) => {
                rep.phase(6, "安装器没换成，由面板自己补完");
                crate::audit::write(&format!(
                    "升级：安装器报成功但 {} 仍是旧版，转由面板补完",
                    bin.display()
                ));
                match crate::install::upgrade::finish_install(bin, target, rep).await {
                    Ok(detail) => {
                        crate::audit::write(&detail);
                        supplemented = Some(detail);
                        settled_by_hash = true;
                    }
                    Err(e) => {
                        let why = crate::install::upgrade::diagnose_blocker(bin).await;
                        let msg = format!("{e} {why}");
                        crate::audit::write(&format!("升级没生效：{msg}"));
                        let done = guard.finish(state).await;
                        rep.fail(&format!("{msg} {}", done.detail));
                        return Err(GateError::Other(msg));
                    }
                }
            }
            // 判断不了就别声称判断出来了。走到这里通常是 versions 下没有
            // 这个版本的文件（渠道版本查不到时 target 也可能是空的）。
            None => rep.log(5, "核对不了（拿不到其中一份的哈希），下面只按版本号给结论"),
        }
    }

    // ---- 维护窗口收尾：重锁；原来有租约且 IP 仍合格就把租约还回去。
    //
    // 「装完必须重新上锁」是文件头第 4 条。v0.7.0 多的那半句是：
    // **重锁不等于把使用者关在门外** —— 他升级之前手里拿着的租约，
    // 只要出口 IP 还合格就该还给他，而不是让他下次开新会话时撞一鼻子灰。
    rep.phase(7, "重新上锁并恢复租约");
    let done = guard.finish(state).await;

    let after = crate::install::upgrade::plan(ch).await;

    // 哈希没能给出结论时退回版本号比较。**不能什么都不查就报成功** ——
    // 那等于退回 v0.5.3 之前「安装器说成功就是成功」的样子，
    // 而 §7.21 那次「成功的失败」正是这么溜过去的。
    if !settled_by_hash
        && crate::install::upgrade::looks_unchanged(
            p.installed.as_deref(),
            after.action,
            after.installed.as_deref(),
        )
    {
        let bin_path = bin
            .as_ref()
            .map(|b| b.display().to_string())
            .unwrap_or_else(|| "claude.exe".into());
        let why = match bin.as_ref() {
            Some(b) => crate::install::upgrade::diagnose_blocker(b).await,
            None => String::new(),
        };
        let msg = format!(
            "官方安装器报成功，但 {bin_path} 还是原来那份（{}），渠道上是 {}，             而 versions 目录下也没有可用来补完的那份二进制。{why}",
            after.installed.clone().unwrap_or_default(),
            after.available.clone().unwrap_or_default(),
        );
        crate::audit::write(&format!("升级没生效：{msg}"));
        rep.fail(&format!("{msg} {}", done.detail));
        return Err(GateError::Other(msg));
    }

    let version = after
        .installed
        .clone()
        .unwrap_or_else(|| "（版本号读不出来，见「环境与安装」页的提示）".into());
    crate::audit::write(&format!(
        "升级完成，版本 {version}，重新上锁 {} 个",
        done.relocked
    ));

    let detail = format!(
        "已安装 {}。{}{}{}",
        version,
        done.detail,
        if cleaned.is_empty() {
            String::new()
        } else {
            format!("顺带清理了 {} 个旧残留副本。", cleaned.len())
        },
        supplemented.map(|s| format!("（{s}）")).unwrap_or_default(),
    );
    rep.done(&detail);
    Ok(detail)
}
