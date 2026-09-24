//! GPT（Codex）账户槽位的启动、关闭与切换 —— 显式动作，与中转调度隔离。
//!
//! # 0.25.0：起桌面端走 Claude 页那条门禁链，不再裸 spawn
//!
//! 0.24 之前 `launch` 是 `codex_desktop::launch()` 一句裸 `std::process::Command`：
//! 不验 IP、不持租约、不进 `sessions::start`，看门狗看不见 —— 账户页那句
//! 「桌面端独立启动，不沿用 Codex CLI 的 IP 执行锁」说的就是它。使用者要的是
//! 「Claude 怎么管，GPT 就怎么管」，所以现在走 [`workspace::launch`]：
//! `gated()` 验 IP 解锁 → `sessions::start`（Job Object）→ 租约 → 命令层挂看门狗。
//! 目录还是那一对 `<槽位>\home` + `<槽位>\desktop`（`workspace::codex_slot_dirs`），
//! 桌面端认不出自己换了启动方式。
//!
//! 关闭分两种（2026-09-23）：
//!
//! - [`close_ours`]：**起槽位 / 切槽位之前**。只收面板起的那些 —— 托管会话按会话停、
//!   交回租约；面板上一次运行留下的（资料目录在面板状态目录下）按 exe 路径 + 创建时间核验后关。
//!   开始菜单、`codex://` 链接、别的多开工具起的**默认资料那一份不碰**：Electron 的单实例锁
//!   按 `--user-data-dir` 算，它挡不住面板给槽位的那份；而把它收掉，起它的那个程序会再把它
//!   拉起来 —— 使用者看到的就是「突然又弹出一个 Codex 窗口」。
//! - [`close`]：使用者在 GPT 页点「一键关闭」、装桌面端之前。**全关**，确认框里写着「所有」。
use crate::error::Result;
use crate::gate::GateState;
use qb_accounts::codex;
use qb_contract::domain::{Client, IdentityKind};
use qb_install::install::codex_desktop;

/// 槽位列表，叠上联网额度那条路问出来的「服务端不认这份登录了」（2026-09-23）。
///
/// `codex::list` 只看 `auth.json` 里两张令牌在不在；服务端早把登录作废了，账户行上照样是
/// 「已登录」、没有「登录」可点，点刷新才冒出一句报错。[`super::login_health`] 记着那一次
/// 401，`auth.json` 一变（官方 Codex 重新登录或自己换新了令牌）就作废。
pub fn list() -> Result<codex::CodexAccounts> {
    let root = codex::root();
    let mut accounts = codex::list(&root)?;
    for s in &mut accounts.slots {
        // id 已经在 `codex::list` 里核过（只有字母数字和 `-`），拼路径是安全的。
        let auth = root.join(&s.id).join("home").join("auth.json");
        let key = super::login_health::codex_key(&s.id);
        if let Some(reason) = super::login_health::check(&key, &auth) {
            s.logged_in = false;
            s.auth_state = reason;
        }
    }
    Ok(accounts)
}

/// 切换 = 先把面板起的关干净，再换指向。不启动任何东西（跟 Claude 一样，「后面启动什么自己点」）。
pub async fn switch(id: &str, gate: &GateState) -> Result<()> {
    let root = codex::root();
    codex::directory(&root, id)?; // Validate before stopping any task.
    close_ours(gate).await?;
    codex::select(&root, id)
}

/// 起这个槽位的 Codex 桌面端。
///
/// 起之前先关掉**面板起的**那份（同一份资料开着再起，只会把旧窗口拉到前面，
/// 新给的 `CODEX_HOME` 一个都不生效；面板一次只管一个桌面端）。别处起的不碰，见模块头。
/// 门禁不过就一个进程都不起 —— 那是 `workspace::launch` 里 `open_authorized_with_mode` 的事。
pub async fn launch(id: &str, gate: &GateState) -> Result<()> {
    let root = codex::root();
    codex::directory(&root, id)?;
    close_ours(gate).await?;
    let session =
        crate::workspace::launch(Client::Codex, IdentityKind::Official, id, None, gate).await?;
    // 账户页靠 `index.json` 里的 pid + 创建时间认「当前启动的是哪个槽位」，
    // 创建时间要 WMI 那种格式，所以按 pid 从 detect() 里找回来。
    let started = tokio::task::spawn_blocking(codex_desktop::detect)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))??
        .processes
        .into_iter()
        .find(|p| p.pid == session.pid)
        .map(|p| p.started)
        .unwrap_or_else(|| session.started_at.clone());
    codex::mark_launched(&root, id, session.pid, &started)
}

/// 关掉所有 Codex 桌面端（使用者点的「一键关闭」、装桌面端之前）。
///
/// 两步：面板托管的会话按会话停（Job Object 收整棵树）并交回租约；
/// 剩下的（使用者自己从开始菜单起的）交给 `codex_desktop::close()`。
/// 任何一步失败都报出来，不许「关了一半接着切」。
pub async fn close(gate: &GateState) -> Result<()> {
    stop_managed(gate)?;
    tokio::task::spawn_blocking(codex_desktop::close)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))??;
    codex::mark_closed(&codex::root())
}

/// 只关面板起的 Codex 桌面端（起槽位 / 切槽位之前），别处起的不碰。见模块头。
pub async fn close_ours(gate: &GateState) -> Result<()> {
    stop_managed(gate)?;
    tokio::task::spawn_blocking(codex_desktop::close_ours)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))??;
    codex::mark_closed(&codex::root())
}

/// 面板托管的 Codex 会话按会话停（Job Object 收整棵树），交回租约。
fn stop_managed(gate: &GateState) -> Result<()> {
    let stopped = crate::sessions::stop_matching(|s| s.context.client == Client::Codex)?;
    for id in &stopped {
        crate::gate::release_holder(gate, id)?;
    }
    Ok(())
}
