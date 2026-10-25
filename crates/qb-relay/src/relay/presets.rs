//! 预设：填个 Key 就能用的常见端点。
//!
//! # 这里**只放官方端点**
//!
//! 界面上写着「面板不会提供虚构地址」。第三方中转站的地址各家自己在变，
//! 而且很多要登录后台才看得到 —— 把一个记错的地址预置进来，用户会以为是
//! 官方推荐的，排查时先怀疑自己的 Key 而不是怀疑地址。
//!
//! 所以这张表只收**厂商自己文档里公开的官方端点**。第三方中转站请用户
//! 自己填，界面上也是这么说的。
//!
//! # 两个来源（2026-10-25 起，中转站重构阶段 4，cc-switch 的 P1）
//!
//! 1. **cc-switch v4.0.14 的预设**（`presets/*.json`，由 `scripts/import-ccswitch-presets.mjs` 静态解析它的
//!    `src/config/*ProviderPresets.ts` 生成，MIT）。使用者 2026-10-25 定的筛法：只要官方类（厂商官方、国内官方、
//!    云厂商、本地）；聚合平台、第三方中转一律不搬；标了商业合作伙伴的整条不搬；网址里的推广参数全剥掉。
//!    这一份带着各档模型名、拿 Key 的地址、备用端点 —— 线路弹窗拿它预填模型映射。
//! 2. **QB Gate 原来手写的那几条**（[`own`]）：地址跟上一份重复的不再列（那份信息更全），不重复的照留。
//!
//! 形态参考 cc-switch（MIT）的预设目录 —— 它把 Codex 侧按 `wire_api`
//! 分成 responses / chat 两类，这个分法是对的，照搬。

use super::{AuthStyle, RelayTarget, WireApi};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use ts_rs::TS;

/// 预设属于哪一类（cc-switch 的 `ProviderCategory`，只留搬进来的那四类；`own` 是 QB Gate 手写的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PresetCategory {
    Official,
    CnOfficial,
    CloudProvider,
    Local,
    Own,
}

/// 预设声明的上游协议（跟 `qb_station::station::route::UpstreamFormat` 同一组写法；两边同在第 3 层、互不依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PresetApiFormat {
    Anthropic,
    OpenaiChat,
    OpenaiResponses,
    GeminiNative,
}

/// 预设里各档的模型名（发往上游的名字，`[1M]` 已经剥掉 —— 那是给客户端看的窗口标记，见 `supports_1m`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct PresetModels {
    pub default: Option<String>,
    pub haiku: Option<String>,
    pub sonnet: Option<String>,
    pub opus: Option<String>,
    pub fable: Option<String>,
    pub subagent: Option<String>,
}

/// Codex 模型目录里的一项（cc-switch 的 `modelCatalog`，只留名字和窗口）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PresetModel {
    pub model: String,
    pub display_name: Option<String>,
    #[ts(type = "number | null")]
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub target: RelayTarget,
    pub base_url: String,
    pub wire_api: WireApi,
    pub auth_style: AuthStyle,
    pub model: Option<String>,
    pub website: Option<String>,
    /// 一句话说明，界面上显示在名字下面。
    pub note: String,
    // ---- 阶段 4 起（cc-switch 的预设带着的）
    pub category: PresetCategory,
    /// 去哪拿 Key（推广参数已剥）。
    pub api_key_url: Option<String>,
    /// 厂商文档写的上游协议。⛔ 线路弹窗**不拿它直接填「上游协议」**：预设知道的是「官方端点长什么样」，
    /// 不是「你这条线通不通」，协议照旧由探测定。没写是 `None`。
    pub api_format: Option<PresetApiFormat>,
    pub models: PresetModels,
    /// 这家的模型声明了 1M 窗口（cc-switch 预设里模型名带 `[1M]`）。
    pub supports_1m: bool,
    pub catalog: Vec<PresetModel>,
    /// 备用地址（多端点测速那一步用）。
    pub endpoint_candidates: Vec<String>,
    /// 拉模型列表的地址跟基地址不在一处时写在这里。
    pub models_url: Option<String>,
    /// 地址里留着要换成自己的占位（`YOUR_RESOURCE_NAME` 之类，Azure、Cloudflare 的官方端点）。
    pub needs_edit: bool,
}

// ------------------------------------------------------------------ cc-switch 的预设

/// `presets/*.json` 里的一条（生成脚本写的形状）。
#[derive(Debug, Deserialize)]
struct CatalogEntry {
    id: String,
    name: String,
    category: PresetCategory,
    website: Option<String>,
    api_key_url: Option<String>,
    base_url: String,
    api_format: PresetApiFormat,
    auth_field: Option<String>,
    #[serde(default)]
    models: PresetModels,
    #[serde(default)]
    supports_1m: bool,
    #[serde(default)]
    catalog: Vec<PresetModel>,
    #[serde(default)]
    endpoint_candidates: Vec<String>,
    models_url: Option<String>,
    #[serde(default)]
    needs_edit: bool,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    presets: Vec<CatalogEntry>,
}

const CATALOG_FILES: [(RelayTarget, &str); 3] = [
    (
        RelayTarget::ClaudeCode,
        include_str!("presets/claude-code.json"),
    ),
    (
        RelayTarget::ClaudeDesktop,
        include_str!("presets/claude-desktop.json"),
    ),
    (RelayTarget::Codex, include_str!("presets/codex.json")),
];

/// 从 cc-switch 搬来的预设（读一次就缓存）。数据文件是编进程序里的，解析不了是打包错误 —— 测试钉着。
fn catalog() -> &'static [Preset] {
    static ALL: OnceLock<Vec<Preset>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for (target, text) in CATALOG_FILES {
            // 解析不了只可能是打包错误，`every_catalog_file_parses_and_is_not_empty` 先挡住；真漏过去了就少这一份，不让面板崩。
            let Ok(file) = serde_json::from_str::<CatalogFile>(text) else {
                continue;
            };
            out.extend(file.presets.into_iter().map(|e| from_catalog(target, e)));
        }
        out
    })
}

fn from_catalog(target: RelayTarget, e: CatalogEntry) -> Preset {
    let note = match e.category {
        PresetCategory::CnOfficial => "国内官方端点",
        PresetCategory::CloudProvider => "云厂商官方端点",
        PresetCategory::Local => "本机运行的模型服务",
        PresetCategory::Official | PresetCategory::Own => "官方端点",
    };
    Preset {
        id: format!("{}/{}", target.as_str(), e.id),
        name: e.name,
        target,
        base_url: e.base_url,
        wire_api: if e.api_format == PresetApiFormat::OpenaiChat {
            WireApi::Chat
        } else {
            WireApi::Responses
        },
        auth_style: match e.auth_field.as_deref() {
            Some("ANTHROPIC_API_KEY") => AuthStyle::EnvKey,
            Some(_) => AuthStyle::BearerToken,
            None => AuthStyle::EnvKey,
        },
        model: e.models.default.clone(),
        website: e.website,
        note: if e.needs_edit {
            format!("{note}（地址里的 YOUR_… 要换成你自己的）")
        } else {
            note.to_string()
        },
        category: e.category,
        api_key_url: e.api_key_url,
        api_format: Some(e.api_format),
        models: e.models,
        supports_1m: e.supports_1m,
        catalog: e.catalog,
        endpoint_candidates: e.endpoint_candidates,
        models_url: e.models_url,
        needs_edit: e.needs_edit,
    }
}

// ------------------------------------------------------------------ QB Gate 手写的

// Compact literal constructor for the fixed catalog columns.
#[allow(clippy::too_many_arguments)]
fn p(
    id: &str,
    name: &str,
    target: RelayTarget,
    base_url: &str,
    wire_api: WireApi,
    auth_style: AuthStyle,
    model: Option<&str>,
    website: Option<&str>,
    note: &str,
) -> Preset {
    Preset {
        id: id.to_string(),
        name: name.to_string(),
        target,
        base_url: base_url.to_string(),
        wire_api,
        auth_style,
        model: model.map(str::to_string),
        website: website.map(str::to_string),
        note: note.to_string(),
        category: PresetCategory::Own,
        api_key_url: None,
        api_format: None,
        models: PresetModels::default(),
        supports_1m: false,
        catalog: Vec::new(),
        endpoint_candidates: Vec::new(),
        models_url: None,
        needs_edit: false,
    }
}

/// QB Gate 原来手写的那几条（阶段 4 之前的全部预设）。
fn own() -> Vec<Preset> {
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

/// 比地址时不看大小写、末尾斜杠和 `/v1`（同一个端点有人写带、有人写不带）。
fn same_endpoint(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let s = s.trim().trim_end_matches('/').to_ascii_lowercase();
        s.strip_suffix("/v1").map(str::to_string).unwrap_or(s)
    };
    norm(a) == norm(b)
}

/// 全部预设：cc-switch 那份在前，手写的里面地址没重复的跟在后面。按 target 过滤后给界面。
pub fn all() -> Vec<Preset> {
    let mut out: Vec<Preset> = catalog().to_vec();
    for mine in own() {
        if !out
            .iter()
            .any(|c| c.target == mine.target && same_endpoint(&c.base_url, &mine.base_url))
        {
            out.push(mine);
        }
    }
    out
}

pub fn for_target(t: RelayTarget) -> Vec<Preset> {
    all().into_iter().filter(|p| p.target == t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_url_is_https_except_local_services_on_this_machine() {
        // 明文 http 把 Key 送出去，等于没有 Key。阶段 4 起多了「本地」一类（Ollama、LM Studio 这些），
        // 它们只在本机回环上，明文不出这台机器 —— 原来那条「全是 https」改写成这样，意思不变。
        for p in all() {
            if p.category == PresetCategory::Local {
                assert!(
                    p.base_url.starts_with("http://localhost")
                        || p.base_url.starts_with("http://127.0.0.1"),
                    "{} 是本地服务却不在回环上：{}",
                    p.name,
                    p.base_url
                );
            } else {
                assert!(
                    p.base_url.starts_with("https://"),
                    "{} 的地址不是 https：{}",
                    p.name,
                    p.base_url
                );
            }
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        let all = all();
        let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
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
            // Anthropic 兼容端点不会长成 /v1 结尾的 OpenAI 形状。原来断言的是「地址里有 anthropic」——
            // cc-switch 的预设里有好几家官方 Anthropic 端点不带这个词（Kimi For Coding 的 /coding/、StepFun 的
            // /step_plan、本地服务），改写成直接断言「不是 OpenAI 的形状」。
            assert!(
                !p.base_url.trim_end_matches('/').ends_with("/v1"),
                "{} 看着不像 Anthropic 兼容端点：{}",
                p.name,
                p.base_url
            );
        }
    }

    #[test]
    fn every_catalog_file_parses_and_is_not_empty() {
        for (target, text) in CATALOG_FILES {
            let file: CatalogFile = serde_json::from_str(text)
                .unwrap_or_else(|e| panic!("{} 的预设数据解析不了：{e}", target.as_str()));
            assert!(
                !file.presets.is_empty(),
                "{} 一条预设都没有",
                target.as_str()
            );
        }
    }

    #[test]
    fn imported_presets_carry_no_referral_parameters_and_only_official_kinds() {
        // 使用者 2026-10-25 定的：只要官方类，推广参数全剥。
        let promo = ["aff=", "ref=", "invite", "utm_", "track_id", "?ch=", "&ch="];
        for p in catalog() {
            for url in [
                Some(&p.base_url),
                p.website.as_ref(),
                p.api_key_url.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                for bad in promo {
                    assert!(!url.contains(bad), "{} 的网址里还有推广参数：{url}", p.name);
                }
            }
            assert_ne!(p.category, PresetCategory::Own);
        }
    }

    #[test]
    fn placeholders_in_addresses_are_flagged() {
        for p in all() {
            assert_eq!(
                p.needs_edit,
                p.base_url.contains("YOUR_"),
                "{}：地址里有没有占位跟标记对不上",
                p.name
            );
        }
    }

    #[test]
    fn hand_written_presets_are_kept_only_where_the_catalog_lacks_their_address() {
        let all = all();
        // DeepSeek 的 Anthropic 端点 cc-switch 那份有（信息更全），手写那条不再列。
        assert!(!all.iter().any(|p| p.id == "deepseek-claude"));
        assert!(all.iter().any(|p| p.id == "claude-code/deepseek"));
        // OpenRouter 不在官方类里，手写那条照留。
        assert!(all.iter().any(|p| p.id == "openrouter-codex"));
    }
}
