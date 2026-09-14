//! 预设：填个 Key 就能用的常见端点。
//!
//! # 这里**只放官方直连端点**
//!
//! 界面上写着「面板不会提供虚构地址」。第三方中转站的地址各家自己在变，
//! 而且很多要登录后台才看得到 —— 把一个记错的地址预置进来，用户会以为是
//! 官方推荐的，排查时先怀疑自己的 Key 而不是怀疑地址。
//!
//! 所以这张表只收**厂商自己文档里公开的官方端点**。第三方中转站请用户
//! 自己填，界面上也是这么说的。
//!
//! 形态参考 cc-switch（MIT）的预设目录 —— 它把 Codex 侧按 `wire_api`
//! 分成 responses / chat 两类，这个分法是对的，照搬。

use super::{AuthStyle, RelayTarget, WireApi};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub target: RelayTarget,
    pub base_url: &'static str,
    pub wire_api: WireApi,
    pub auth_style: AuthStyle,
    pub model: Option<&'static str>,
    pub website: Option<&'static str>,
    /// 一句话说明，界面上显示在名字下面。
    pub note: &'static str,
}

// Compact literal constructor for the fixed catalog columns.
#[allow(clippy::too_many_arguments)]
const fn p(
    id: &'static str,
    name: &'static str,
    target: RelayTarget,
    base_url: &'static str,
    wire_api: WireApi,
    auth_style: AuthStyle,
    model: Option<&'static str>,
    website: Option<&'static str>,
    note: &'static str,
) -> Preset {
    Preset {
        id,
        name,
        target,
        base_url,
        wire_api,
        auth_style,
        model,
        website,
        note,
    }
}

/// 全部预设。按 target 过滤后给界面。
pub fn all() -> Vec<Preset> {
    vec![
        // ---------------------------------------------- Claude Code
        // 这几家都公布了 Anthropic Messages 兼容端点，可以直连。
        p(
            "deepseek-claude",
            "DeepSeek",
            RelayTarget::ClaudeCode,
            "https://api.deepseek.com/anthropic",
            WireApi::Responses,
            AuthStyle::BearerToken,
            None,
            Some("https://platform.deepseek.com/"),
            "官方 Anthropic 兼容端点",
        ),
        p(
            "zhipu-claude",
            "智谱 GLM",
            RelayTarget::ClaudeCode,
            "https://open.bigmodel.cn/api/anthropic",
            WireApi::Responses,
            AuthStyle::BearerToken,
            None,
            Some("https://open.bigmodel.cn/"),
            "官方 Anthropic 兼容端点",
        ),
        p(
            "kimi-claude",
            "Kimi（Moonshot）",
            RelayTarget::ClaudeCode,
            "https://api.moonshot.cn/anthropic",
            WireApi::Responses,
            AuthStyle::BearerToken,
            None,
            Some("https://platform.moonshot.cn/"),
            "官方 Anthropic 兼容端点",
        ),
        // ---------------------------------------------- Codex（responses）
        p(
            "openai-codex",
            "OpenAI 官方",
            RelayTarget::Codex,
            "https://api.openai.com/v1",
            WireApi::Responses,
            AuthStyle::EnvKey,
            None,
            Some("https://platform.openai.com/"),
            "原生 Responses 协议",
        ),
        // ---------------------------------------------- Codex（chat）
        // 这些只提供 OpenAI Chat Completions，必须选 chat，
        // 选错了 Codex 发出去的请求体对方解析不了。
        p(
            "deepseek-codex",
            "DeepSeek",
            RelayTarget::Codex,
            "https://api.deepseek.com/v1",
            WireApi::Chat,
            AuthStyle::EnvKey,
            Some("deepseek-chat"),
            Some("https://platform.deepseek.com/"),
            "只支持 Chat Completions",
        ),
        p(
            "zhipu-codex",
            "智谱 GLM",
            RelayTarget::Codex,
            "https://open.bigmodel.cn/api/paas/v4",
            WireApi::Chat,
            AuthStyle::EnvKey,
            None,
            Some("https://open.bigmodel.cn/"),
            "只支持 Chat Completions",
        ),
        p(
            "kimi-codex",
            "Kimi（Moonshot）",
            RelayTarget::Codex,
            "https://api.moonshot.cn/v1",
            WireApi::Chat,
            AuthStyle::EnvKey,
            None,
            Some("https://platform.moonshot.cn/"),
            "只支持 Chat Completions",
        ),
        p(
            "modelscope-codex",
            "魔搭 ModelScope",
            RelayTarget::Codex,
            "https://api-inference.modelscope.cn/v1",
            WireApi::Chat,
            AuthStyle::EnvKey,
            None,
            Some("https://modelscope.cn/"),
            "只支持 Chat Completions",
        ),
        p(
            "siliconflow-codex",
            "硅基流动 SiliconFlow",
            RelayTarget::Codex,
            "https://api.siliconflow.cn/v1",
            WireApi::Chat,
            AuthStyle::EnvKey,
            None,
            Some("https://siliconflow.cn/"),
            "只支持 Chat Completions",
        ),
        p(
            "openrouter-codex",
            "OpenRouter",
            RelayTarget::Codex,
            "https://openrouter.ai/api/v1",
            WireApi::Chat,
            AuthStyle::EnvKey,
            None,
            Some("https://openrouter.ai/"),
            "聚合路由，只支持 Chat Completions",
        ),
        p(
            "dashscope-codex",
            "阿里云百炼",
            RelayTarget::Codex,
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            WireApi::Chat,
            AuthStyle::EnvKey,
            None,
            Some("https://bailian.console.aliyun.com/"),
            "只支持 Chat Completions",
        ),
    ]
}

pub fn for_target(t: RelayTarget) -> Vec<Preset> {
    all().into_iter().filter(|p| p.target == t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_url_is_https() {
        // 明文 http 把 Key 送出去，等于没有 Key。
        for p in all() {
            assert!(
                p.base_url.starts_with("https://"),
                "{} 的地址不是 https：{}",
                p.name,
                p.base_url
            );
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        let mut ids: Vec<&str> = all().iter().map(|p| p.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "预设 id 撞了");
    }

    #[test]
    fn codex_presets_declare_their_wire_api_explicitly() {
        // 这张表存在的一半意义就是替用户记住哪家是 chat 哪家是 responses。
        let codex = for_target(RelayTarget::Codex);
        assert!(codex.iter().any(|p| p.wire_api == WireApi::Responses));
        assert!(codex.iter().any(|p| p.wire_api == WireApi::Chat));
    }

    #[test]
    fn claude_presets_exist_and_are_not_codex_endpoints() {
        let claude = for_target(RelayTarget::ClaudeCode);
        assert!(!claude.is_empty());
        for p in claude {
            // Anthropic 兼容端点不会长成 /v1 结尾的 OpenAI 形状。
            assert!(
                p.base_url.contains("anthropic"),
                "{} 看着不像 Anthropic 兼容端点：{}",
                p.name,
                p.base_url
            );
        }
    }
}
