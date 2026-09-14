//! 会话内门禁与账户槽位的对接。**跨域编排。**
//!
//! # 为什么单独一层
//!
//! `gate::hook` 负责「怎么装、怎么摘、装没装」，`accounts` 负责「有哪些槽位、
//! 当前是哪个」。两边本来各管各的，但 hook 要知道该装到哪个槽位目录去，
//! 于是它直接去问了 `accounts` —— 而 `accounts::switch` 切完之后又要回头叫
//! hook 跟过去。`accounts ↔ gate` 这对循环依赖就是这么来的。
//!
//! 现在由这一层去问 `accounts`，把结果装进 [`hook::Scope`] 交给 `gate::hook`。
//! 两个领域模块谁也不认识谁。
//!
//! # 一条安全相关的不变量
//!
//! **切完账户必须让 hook 跟过去**（[`follow_active_slot`]）。不跟的话 hook 在新槽位里
//! 静默失效：界面还写着「已启用」，实际一次都不会跑 —— 这是最危险的一种失效，
//! 看起来有、其实没有。所以 `accounts::switch*` 一律经 [`crate::usecase::account_ops`]
//! 调用，那边把这两步绑在一起。

use std::path::PathBuf;

use crate::error::Result;
use crate::gate::hook::{self, HookStatus, Scope};

/// 问 `accounts` 要当前的槽位布局，拼成 hook 认得的形状。
///
/// 没有激活槽位时落到 Claude Code 自己的默认目录 `~\.claude` ——
/// 这跟原来 `hook::settings_target` 的行为一致，没有改语义。
pub fn scope() -> Scope {
    let roots = crate::accounts::AccountRoots::current();
    let target = crate::accounts::active_slot_dir(&roots)
        .map(|dir| dir.join("settings.json"))
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude").join("settings.json")));
    let slot_settings: Vec<PathBuf> = crate::accounts::slots()
        .into_iter()
        .map(|s| roots.slot_dir(&s.label).join("settings.json"))
        .collect();
    Scope {
        target,
        slot: crate::accounts::active_label(&roots),
        slot_settings,
    }
}

pub fn status() -> HookStatus {
    hook::status(&scope())
}

pub fn install() -> Result<HookStatus> {
    hook::install(&scope())
}

pub fn uninstall() -> Result<HookStatus> {
    hook::uninstall(&scope())
}

/// 切完账户之后把 hook 对齐到新槽位。见本模块开头那条不变量。
pub fn follow_active_slot() {
    hook::follow_active_slot(&scope());
}
