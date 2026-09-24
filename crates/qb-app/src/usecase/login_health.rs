//! 联网额度那条路上**问出来**的「这份登录服务端不认了」—— 记在内存里，账户列表拿它修正登录态
//! （2026-09-23，使用者：「GPT 和 Gemini 的登录态对着那个开源项目修一下」）。
//!
//! # 为什么要有这一层
//!
//! 本机文件只说得出「有没有令牌」，说不出「服务端还认不认」。原来列槽位时只看令牌在不在：
//! GPT 看 `auth.json` 里有没有两张令牌，Gemini CLI 只看 `oauth_creds.json` 在不在 ——
//! 登录早就被服务端作废了，账户行上照样是「已登录」，点刷新才冒出一句「登录令牌已失效」，
//! 而那一行没有「登录」按钮可点。
//!
//! 「还认不认」只有真去问一次才知道 —— 使用者点刷新图标那一刻。问出来不认了就记一笔：
//! 账户行上改说「登录已失效」、露出「登录」。cockpit-tools 也是这么分的（读它的源码核对的事实，
//! 一行没抄）：刷新令牌作废（`invalid_grant`、`refresh_token_reused` / `_expired`）、访问令牌被
//! 服务端撤销（`token_invalidated`）归「要重新登录」，跟「这次没问成」（网络、限流、改版）分开。
//!
//! # 按凭据文件的「版本」记，不读内容
//!
//! 记的是那个凭据文件当时的修改时刻 + 长度。官方客户端重新登录、或者自己把令牌换新了，
//! 文件就变了，这一笔自然作废 —— 不用读令牌，也不用谁来清。只在内存里，面板重启就忘。
//! 使用者在面板里点「登录」、或者移除了槽位时也清掉（[`forget`]）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

/// 凭据文件的版本：`(修改时刻, 长度)`。读不到元数据就是 `None`。
type Stamp = Option<(SystemTime, u64)>;

static REJECTED: Mutex<Option<HashMap<String, (Stamp, String)>>> = Mutex::new(None);

/// 一个 GPT（Codex）槽位的键。
pub fn codex_key(slot_id: &str) -> String {
    format!("codex:{slot_id}")
}

/// 一条反重力账户槽位 CLI 那一半（Gemini CLI）的键。
pub fn gemini_cli_key(account_id: &str) -> String {
    format!("gemini-cli:{account_id}")
}

/// 一条反重力账户槽位 IDE 那一半的键。
pub fn antigravity_ide_key(account_id: &str) -> String {
    format!("antigravity-ide:{account_id}")
}

fn stamp(file: &Path) -> Stamp {
    let m = std::fs::metadata(file).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// 记一笔：`key` 这份登录（凭据文件是 `file`）服务端不认了。`reason` 是给界面的那句话，
/// **纯文本**（界面不渲染 Markdown），而且要能照着做。
pub fn record(key: &str, file: &Path, reason: &str) {
    record_stamp(key, stamp(file), reason);
}

/// 这份登录还记着「不认了」吗 —— 凭据文件从那以后没变过才算；变了就把这一笔删掉。
pub fn check(key: &str, file: &Path) -> Option<String> {
    check_stamp(key, stamp(file))
}

/// 这一笔不再作数（使用者点了「登录」、移除了槽位）。
pub fn forget(key: &str) {
    if let Ok(mut g) = REJECTED.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(key);
        }
    }
}

fn record_stamp(key: &str, stamp: Stamp, reason: &str) {
    if let Ok(mut g) = REJECTED.lock() {
        g.get_or_insert_with(HashMap::new)
            .insert(key.to_string(), (stamp, reason.to_string()));
    }
}

fn check_stamp(key: &str, now: Stamp) -> Option<String> {
    let mut g = REJECTED.lock().ok()?;
    let m = g.as_mut()?;
    let (then, reason) = m.get(key)?;
    if *then == now {
        return Some(reason.clone());
    }
    m.remove(key);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64, len: u64) -> Stamp {
        Some((SystemTime::UNIX_EPOCH + Duration::from_secs(secs), len))
    }

    #[test]
    fn a_rejection_holds_until_the_credential_file_changes() {
        let key = "codex:test-holds-7a1c";
        record_stamp(key, at(100, 50), "登录已失效");
        assert_eq!(check_stamp(key, at(100, 50)).as_deref(), Some("登录已失效"));
        // 官方客户端重新登录 / 自己换新了令牌：文件变了，这一笔就作废，而且删掉。
        assert_eq!(check_stamp(key, at(200, 50)), None);
        assert_eq!(check_stamp(key, at(100, 50)), None, "删掉了就不会再回来");
    }

    #[test]
    fn a_length_change_alone_also_clears_it_and_forget_clears_it_outright() {
        let key = "gemini-cli:test-len-2b9d";
        record_stamp(key, at(100, 50), "x");
        assert_eq!(check_stamp(key, at(100, 51)), None);
        record_stamp(key, at(100, 50), "x");
        forget(key);
        assert_eq!(check_stamp(key, at(100, 50)), None);
    }

    #[test]
    fn keys_for_different_halves_never_collide() {
        assert_ne!(gemini_cli_key("a"), antigravity_ide_key("a"));
        assert_ne!(codex_key("a"), gemini_cli_key("a"));
    }
}
