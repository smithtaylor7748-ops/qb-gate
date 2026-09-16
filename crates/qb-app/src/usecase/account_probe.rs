//! 「这个账户现在还能用吗」—— 拿槽位里的令牌向官方发一次最小认证请求。
//!
//! # 为什么非得联网问一次
//!
//! `accounts::EXPIRY_CAVEAT` 自己写着：剩余天数只读本地那个时间戳，
//! **查不出「被风控下线」**，「唯一能确认的办法是实际发一次认证请求」。
//! 这个模块就是那句话缺的那一半 —— 在此之前面板从来没发过。
//!
//! # ⛔ 这不是「联网查额度」
//!
//! CLAUDE.md 禁的是调 OAuth 内部接口、抓 `/usage` 背后的端点去拿用量。
//! 这里问的是另一件事，而且刻意选了最小的问法：
//!
//! | | 这里做的 | 明确不做的 |
//! |---|---|---|
//! | 打哪个端点 | `GET /v1/models?limit=1`，官方公开文档里的 | 任何未公开接口、`/usage` 背后那些 |
//! | 问什么 | 「你还认这个令牌吗」 | 还剩多少额度、什么时候重置 |
//! | 花不花钱 | 不打模型，不产生 token | —— |
//! | 写不写东西 | 一个字节都不写回槽位 | 不刷新、不续期、不存任何凭证 |
//!
//! 用量仍然只读本地文件（`usage.rs` / `tokens.rs`），一条网络请求都没有。
//!
//! # ⛔ 令牌过期时**不发请求**，也不去刷新
//!
//! `accessToken` 只活 8–12 小时，客户端自己静默换新。本地那份过期是**常态**
//! （实测两个槽位存的都是几天前的）。这时候：
//!
//! - **不发**：拿一个已知过期的令牌去问，401 是必然的，而界面会把它显示成
//!   「你的账户被拒了」—— 那是彻头彻尾的假警报，比不测更糟；
//! - **不刷新**：拿 refreshToken 去换新令牌 = 面板开始经手凭证，
//!   而「面板不经手你的账号密码」是这个项目对外说了一路的话。
//!
//! 所以那一档如实回 [`ProbeState::LocallyExpired`]，并告诉使用者
//! 「先用这个账户跑一次，令牌会自己换新，再回来测」。
//!
//! # ⚠ 一个还没验到的点
//!
//! `/v1/models` 对 Claude Code 那种 OAuth 令牌认不认，**没有用真令牌验过**
//! （手上两个槽位的令牌都过期了，测出来的 401 说明不了问题）。
//! 所以 [`ProbeState::Rejected`] 的文案里必须把第二种可能也写出来。
//! 哪天拿一个刚刷新过的令牌测出 200，就可以把那半句删掉 —— 在那之前不许删。

use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

/// 官方公开的模型列表端点。**不打模型，不产生 token。**
const MODELS_URL: &str = "https://api.anthropic.com/v1/models?limit=1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "AccountProbeState")]
#[serde(rename_all = "snake_case")]
pub enum ProbeState {
    /// 服务端认这个令牌。
    Accepted,
    /// 服务端拒了这个令牌。
    Rejected,
    /// 本地这份 `accessToken` 已经过期 —— **没有发请求**。
    LocallyExpired,
    /// 这个槽位还没登录过，或者凭证读不出来。
    NoCredential,
    /// 问不到（网断了、被代理拦了、服务端回了别的状态码）。
    Unreachable,
}

// ⛔ `rename` 不是洁癖：`qb-station` 那边已经有一个 `ProbeResult`（探中转站的），
// 两个 Rust 类型生成同一个 `.ts` 文件名时 **ts-rs 会静默覆盖**，
// 而覆盖掉的那一方要到别的页面编译不过才看得出来。这一条踩过一次。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "AccountProbe")]
pub struct ProbeResult {
    pub state: ProbeState,
    /// 一句人话。**纯文本渲染**，别写 Markdown 的星号。
    pub detail: String,
    /// 本地 `YYYY-MM-DD HH:MM`。
    pub checked_at: String,
}

fn out(state: ProbeState, detail: impl Into<String>) -> ProbeResult {
    ProbeResult {
        state,
        detail: detail.into(),
        checked_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
    }
}

/// 读槽位里的 `accessToken` 与它的到期毫秒数。
///
/// ⛔ 返回值里带着真令牌 —— 调用方只许把它放进 `Authorization` 头，
/// **不许写日志、不许放进任何 `detail`、不许返回给界面**。
fn credential(slot_dir: &Path) -> Option<(String, i64)> {
    let text = std::fs::read_to_string(slot_dir.join(".credentials.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let o = v.get("claudeAiOauth")?;
    let token = o.get("accessToken")?.as_str()?.to_string();
    let expires = o.get("expiresAt")?.as_i64()?;
    (!token.is_empty()).then_some((token, expires))
}

/// 问一次「你还认这个令牌吗」。
///
/// 只由使用者当次点击触发 —— 没有定时器、没有启动时触发，
/// 跟这个项目里所有会对外发请求的东西同一条规矩。
pub async fn probe(slot_dir: &Path, client: &reqwest::Client) -> ProbeResult {
    let Some((token, expires_at)) = credential(slot_dir) else {
        return out(
            ProbeState::NoCredential,
            "这个槽位还没登录过（读不到 .credentials.json 里的 OAuth 凭证），没什么可测的。",
        );
    };

    let now = chrono::Utc::now().timestamp_millis();
    if expires_at <= now {
        let hours = (now - expires_at) / 3_600_000;
        return out(
            ProbeState::LocallyExpired,
            format!(
                "本地这份访问令牌 {hours} 小时前就过期了，没有发请求 —— \
                 拿过期令牌去问，被拒是必然的，报给你只会是假警报。\
                 访问令牌 8–12 小时一换，是正常轮换：用这个账户跑一次 Claude Code，\
                 它会自己换新，再回来测。"
            ),
        );
    }

    let sent = client
        .get(MODELS_URL)
        .header("anthropic-version", "2023-06-01")
        .bearer_auth(&token)
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await;

    match sent {
        Ok(r) if r.status().is_success() => out(
            ProbeState::Accepted,
            "服务端认这个令牌：这个账户现在发得出请求。这一测不查额度、不打模型，\
             也没有往槽位里写任何东西。",
        ),
        Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => out(
            ProbeState::Rejected,
            format!(
                "服务端拒了这个令牌（HTTP {}）。两种可能，面板分不开：\
                 一是这个账户的凭证确实已经失效（改过密码、网页端撤销过、或者被风控下线），\
                 二是这个端点本来就不接受 Claude Code 这类 OAuth 令牌。\
                 先用这个账户跑一次 Claude Code —— 它要是也登不上，那就是第一种。",
                r.status().as_u16()
            ),
        ),
        Ok(r) => out(
            ProbeState::Unreachable,
            format!(
                "问不出结论：服务端回了 HTTP {}。这既不代表令牌好，也不代表它坏。",
                r.status().as_u16()
            ),
        ),
        // ⛔ 错误原文里可能带着 URL，但绝不会带令牌（它只在头里）。
        Err(e) => out(
            ProbeState::Unreachable,
            format!("请求没发出去或没回来：{e}。网络、代理或防火墙的问题，跟令牌无关。"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        // ⛔ 单测不许碰 `%LOCALAPPDATA%\ClaudeIpGate\`。
        let d = std::env::temp_dir().join(format!(
            "qbgate-probe-{tag}-{}-{}",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_cred(dir: &Path, token: &str, expires_at: i64) {
        std::fs::write(
            dir.join(".credentials.json"),
            serde_json::json!({
                "claudeAiOauth": { "accessToken": token, "expiresAt": expires_at }
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn a_slot_that_never_logged_in_has_nothing_to_probe() {
        let d = scratch("nocred");
        assert!(credential(&d).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_empty_token_counts_as_no_credential() {
        let d = scratch("empty");
        write_cred(&d, "", i64::MAX);
        assert!(credential(&d).is_none(), "空串不是凭证");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_live_token_is_read_back_with_its_expiry() {
        let d = scratch("live");
        write_cred(&d, "tok-abc", 1_788_000_000_000);
        let (t, e) = credential(&d).expect("读得出来");
        assert_eq!(t, "tok-abc");
        assert_eq!(e, 1_788_000_000_000);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ⛔ 过期时**不发请求**：拿过期令牌去问，401 是必然的，
    /// 而界面会把它显示成「你的账户被拒了」—— 假警报比不测更糟。
    #[tokio::test]
    async fn an_expired_token_is_reported_without_sending_anything() {
        let d = scratch("expired");
        // 客户端指向一个不可能连上的地址：真发出去了这条测试就会超时失败。
        let dead = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
            .timeout(std::time::Duration::from_millis(400))
            .build()
            .unwrap();
        write_cred(
            &d,
            "tok-stale",
            chrono::Utc::now().timestamp_millis() - 7_200_000,
        );
        let r = probe(&d, &dead).await;
        assert_eq!(r.state, ProbeState::LocallyExpired);
        assert!(r.detail.contains("没有发请求"), "{}", r.detail);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 令牌一个字都不许出现在给界面的东西里。
    #[tokio::test]
    async fn the_token_never_reaches_the_ui() {
        let d = scratch("leak");
        let secret = "sk-ant-oat-do-not-leak-me";
        write_cred(&d, secret, chrono::Utc::now().timestamp_millis() - 1);
        let dead = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(400))
            .build()
            .unwrap();
        let r = probe(&d, &dead).await;
        assert!(!r.detail.contains(secret), "令牌漏进 detail 了");
        assert!(!format!("{r:?}").contains(secret));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 传给界面的是纯文本，写 Markdown 的星号会显示成两个星号。
    #[test]
    fn nothing_we_hand_the_ui_carries_markdown_bold() {
        for r in [
            out(ProbeState::Accepted, "a"),
            out(ProbeState::NoCredential, "b"),
        ] {
            assert!(!r.detail.contains("**"));
        }
    }
}
