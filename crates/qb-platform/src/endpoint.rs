//! 端点地址的规整与校验。**平台层：零 crate 依赖。**
//!
//! # 为什么单独成一个模块
//!
//! `endpoint_base` 原来在 `workspace`、`normalize_base_url` 原来在 `relay::probe`，
//! 而 `diagnostics` / `extensions` / `repository` / `workspace` 四处都要用前者。
//! 「把用户填的地址规整成一个能用的 base」跟工作空间、跟中转站都没关系，
//! 它只是一段 URL 处理 —— 放在任何一个领域模块里都会把那个模块变成
//! 别人不得不依赖的东西。

use crate::error::{GateError, Result};

/// 把使用者粘进来的地址归一成 base_url。
///
/// 中转站给的地址五花八门：有的发完整端点 `https://x.com/v1/messages`，
/// 有的发 OpenAI 风格的 `https://x.com/v1/chat/completions`，有的就发个域名。
/// 直接存进去，`join` 拼出来就是 `https://x.com/v1/messages/models` —— 404，
/// 而使用者会先去怀疑 Key，不会怀疑地址。
///
/// 形态参考 z-switch（MIT）的「Base URL 智能推断」。纯函数，可单测。
///
/// **认不出的一律原样返回**，不猜：地址里少一段总比多一段容易发现。
pub fn normalize_base_url(raw: &str) -> String {
    let v = raw.trim().trim_end_matches('/');
    if v.is_empty() {
        return String::new();
    }
    // 已知的端点后缀，从长到短剥 —— `/v1/chat/completions` 要先于 `/completions` 命中。
    const ENDPOINTS: &[&str] = &[
        "/chat/completions",
        "/messages",
        "/completions",
        "/responses",
        "/models",
    ];
    for e in ENDPOINTS {
        if let Some(base) = v.strip_suffix(e) {
            return base.trim_end_matches('/').to_string();
        }
    }
    v.to_string()
}

pub fn endpoint_base(raw: &str) -> Result<String> {
    let normalized = crate::endpoint::normalize_base_url(raw.trim());
    let url = reqwest::Url::parse(&normalized)
        .map_err(|_| GateError::Other("请输入完整的 HTTP 或 HTTPS 地址".into()))?;
    if !["https", "http"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(GateError::Other(
            "端点地址不能包含凭证、查询参数或片段".into(),
        ));
    }
    Ok(url.as_str().trim_end_matches('/').into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_known_endpoints() {
        assert_eq!(
            normalize_base_url("https://x.com/v1/messages"),
            "https://x.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://x.com/v1/chat/completions"),
            "https://x.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://x.com/v1/models"),
            "https://x.com/v1"
        );
    }

    #[test]
    fn trailing_slash_and_spaces_are_harmless() {
        assert_eq!(
            normalize_base_url("  https://x.com/v1/  "),
            "https://x.com/v1"
        );
        assert_eq!(normalize_base_url("https://x.com/v1"), "https://x.com/v1");
    }

    /// 认不出的原样返回 —— 不猜。地址少一段比多一段容易发现。
    #[test]
    fn unknown_shapes_are_left_alone() {
        assert_eq!(
            normalize_base_url("https://x.com/custom/path"),
            "https://x.com/custom/path"
        );
        assert_eq!(normalize_base_url("https://x.com"), "https://x.com");
        assert_eq!(normalize_base_url("   "), "");
    }
}
