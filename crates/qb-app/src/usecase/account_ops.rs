//! 切账户。**跨域编排：换指向 + 让会话内门禁跟过去。**
//!
//! # 为什么不能直接调 `accounts::switch`
//!
//! 槽位换了 = `CLAUDE_CONFIG_DIR` 换了目录，会话内门禁必须跟过去，
//! 否则它在新槽位里**静默失效** —— 界面写着「已启用」，实际一次都不跑。
//!
//! 原来这一步写在 `accounts::switch` 里面（它直接调 `gate::hook`），
//! 代价是 `accounts ↔ gate` 这对循环依赖。搬到这一层之后，
//! `accounts` 只管换指向，`gate::hook` 只管装摘，两步由这里绑在一起。
//!
//! **所有切换入口都必须走这里**：命令、托盘、档案应用。绕过去的那一个，
//! 就是下一个「切完账户门禁没了」的 bug。

use crate::accounts::{DesktopMode, SwitchOutcome};
use crate::error::Result;

/// 切之前先确认目标槽位存在，并把 hook 补进去。
pub fn preflight_switch(label: &str) -> Result<()> {
    crate::accounts::preflight_switch(label)?;
    let target = crate::accounts::AccountRoots::current().slot_dir(label);
    crate::gate::hook::ensure_for_dir(&target)
}

pub fn switch(label: &str) -> Result<SwitchOutcome> {
    let out = crate::accounts::switch(label)?;
    super::hook_ops::follow_active_slot();
    Ok(out)
}

/// 面板对话框与托盘走的切换。对话框由使用者勾选（`Follow` / `Keep`）；
/// 托盘没有复选框，用 `Auto`。
pub fn switch_with(label: &str, mode: DesktopMode) -> Result<SwitchOutcome> {
    preflight_switch(label)?;
    let out = crate::accounts::switch_with(label, mode)?;
    super::hook_ops::follow_active_slot();
    Ok(out)
}
