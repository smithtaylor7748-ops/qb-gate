// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 整流器与发送前优化器的开关。
//!
//! 只搬了 cc-switch `proxy/types.rs` 里这两份（其余是它的代理服务、接管官方配置的状态，不搬）。
//! 界面上那几个开关在 `qb-station::router::RouterSettings` 里，由 `qb-app` 换成这里的形状 ——
//! 跨 crate 的类型转换放编排层（`ALLOWED_SIDEWAYS` 不加边）。
use serde::{Deserialize, Serialize};

/// 整流器配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectifierConfig {
    /// 总开关：是否启用整流器（默认开启）
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 请求整流：启用 thinking 签名整流器（默认开启）
    ///
    /// 处理错误：Invalid 'signature' in 'thinking' block
    #[serde(default = "default_true")]
    pub request_thinking_signature: bool,
    /// 请求整流：启用 thinking budget 整流器（默认开启）
    ///
    /// 处理错误：budget_tokens + thinking 相关约束
    #[serde(default = "default_true")]
    pub request_thinking_budget: bool,
    /// 请求整流：不支持的图片降级（默认开启）
    ///
    /// 上游拒绝图片输入时，把图片块替换为 [Unsupported Image] 标记，
    /// 让对话不中断。总开关，管辖「显式声明 text-only」与「上游报错后兜底」两条事实驱动路径。
    #[serde(default = "default_true")]
    pub request_media_fallback: bool,
    /// 请求整流：确认纯文本注册表的发送前降级（默认开启）
    ///
    /// 在模型未声明能力时，按内置的确认纯文本注册表预先剥离图片。
    /// 受 request_media_fallback 管辖；单独关闭只停用代理的注册表预判，
    /// 仍保留「显式声明」与「上游兜底」。
    #[serde(default = "default_true")]
    pub request_media_heuristic: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RectifierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            request_thinking_signature: true,
            request_thinking_budget: true,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }
}

/// 请求优化器配置
///
/// 仅对 Bedrock 线路生效（cc-switch 按 `CLAUDE_CODE_USE_BEDROCK = "1"` 认）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerConfig {
    /// 总开关（默认关闭，用户需手动启用）
    #[serde(default)]
    pub enabled: bool,
    /// Thinking 优化子开关（总开关开启后默认生效）
    #[serde(default = "default_true")]
    pub thinking_optimizer: bool,
    /// Cache 注入子开关（总开关开启后默认生效）
    #[serde(default = "default_true")]
    pub cache_injection: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            thinking_optimizer: true,
            cache_injection: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectifiers_default_on_and_the_bedrock_optimizer_defaults_off() {
        let r = RectifierConfig::default();
        assert!(r.enabled && r.request_thinking_signature && r.request_thinking_budget);
        assert!(r.request_media_fallback && r.request_media_heuristic);
        assert!(!OptimizerConfig::default().enabled);
        // 老数据缺字段时按默认补（serde 的 default）。
        let parsed: RectifierConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, RectifierConfig::default());
    }
}
