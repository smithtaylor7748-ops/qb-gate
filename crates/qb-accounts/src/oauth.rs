//! 一份 OAuth 令牌的形状 —— 反重力（IDE / Hub）与 Gemini CLI 共用（2026-09-23）。
//!
//! 原来它长在 `antigravity::token` 里。Gemini CLI 那一半也要读它自己的令牌之后，
//! `gemini` 就得引用它，而 `antigravity::account` 早就在引用 `gemini::login_state` ——
//! 两个模块互相到得了，`architecture.rs` 的 `module_cycles_only_ever_shrink` 当场抓住。
//! 共用的这一小块提到一个谁都不依赖的叶子模块里（同 A0 把 `paths` / `audit` 提下去）。
//!
//! ⛔ **没有 `Debug` / `Clone` / `Serialize`**：它永远不许进账户列表、日志、事件、审计、
//! IPC 返回值。令牌本身装在 [`Secret`] 里，`Drop` 时抹零。

use qb_platform::credentials::Secret;

/// 一份 OAuth 令牌。
pub struct OAuthToken {
    /// 访问令牌。可能已经过期。
    pub access: Option<Secret>,
    /// 刷新令牌。换新访问令牌要它。
    pub refresh: Option<Secret>,
    /// 访问令牌过期的 Unix 秒。读不到就是 `None` —— 调用方按「已过期」处理。
    pub expires_at: Option<i64>,
}

impl OAuthToken {
    /// 访问令牌在 `margin` 秒之后还有效吗。
    ///
    /// 留余量是因为「发出去的路上过期」跟「本来就过期」一样是 401；
    /// 读不到过期时刻一律当成不能用 —— 宁可多换一次，也不拿一张可能作废的票去问。
    pub fn access_usable(&self, now: i64, margin: i64) -> bool {
        self.access.as_ref().is_some_and(|a| !a.is_empty())
            && self.expires_at.is_some_and(|e| e - margin > now)
    }
}
