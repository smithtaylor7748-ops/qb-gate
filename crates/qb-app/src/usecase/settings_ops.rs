//! 改设置。**跨域：写盘 + 作废门禁判定缓存，两件事必须一起发生。**
//!
//! # 为什么不留在 `settings` 里
//!
//! 原来 `settings::save` 自己调 `gate::invalidate_verdict()` —— 一个本该在
//! 门禁**下面**的模块反过来依赖门禁，`gate ↔ settings` 这对环就是这么来的。
//!
//! # 为什么也不能直接把那句挪给调用方
//!
//! 挪给调用方看着更简单，但那是在赌「以后每个新调用点都记得加上」。
//! 赌输的症状是**改完设置门禁没反应**：它还拿着 `gate-verdict.json` 里那份
//! 陈旧判定继续放行 —— 这是安全相关的，而且极难查。
//!
//! 所以做法是：`settings` 那边只留 `write_without_invalidating_the_gate_verdict`（名字就在说它不完整），
//! 唯一的公开保存路径是这里。想绕过去得先把一个明说「只写盘」的函数拿来用，
//! 那是有意为之，不是手滑。

use crate::error::Result;
use crate::settings::Settings;

/// 保存设置并作废门禁判定缓存。
pub fn save(s: &Settings) -> Result<()> {
    crate::gate::invalidate_verdict()?;
    crate::settings::write_without_invalidating_the_gate_verdict(s)
}
