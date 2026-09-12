//! 中转站目录：多条供应商的持久化与增删改查。
//!
//! 存在 `%LOCALAPPDATA%\ClaudeIpGate\relay.json`（复用 `gate::state_dir()`）。
//!
//! # 三个结构体，不是一个
//!
//! Key 绝不能随响应回到前端。旧版靠 `#[serde(skip_serializing)]` 一个属性
//! 来保证，那是「记得加」型的约束 —— 加一个新的响应结构体就可能忘掉。
//! 这里改成**结构上不可能**：
//!
//! | 结构体 | 方向 | 带 Key 吗 |
//! |---|---|---|
//! | [`ProviderInput`]  | 前端 → 后端 | 明文 `api_key`，留空表示不改 |
//! | [`StoredProvider`] | 存盘        | DPAPI 密文 `key_sealed` |
//! | [`ProviderView`]   | 后端 → 前端 | **没有任何 Key 字段**，只有掩码与「有没有」 |
//!
//! `ProviderView` 里根本不存在能装下 Key 的字段，所以「Key 泄回前端」
//! 这件事不是靠自觉，是编译期就做不到。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{secret, AuthStyle, RelayTarget, WireApi};
use crate::error::Result;

/// 一条供应商的公开信息 —— 三个结构体共用这一份，免得字段各写三遍还对不齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    /// 本项目内部的稳定 id，跟目标工具的配置无关。
    pub id: String,
    pub target: RelayTarget,
    /// 写进 `config.toml` 的 `model_providers.<slug>`。Codex 用，Claude 侧忽略。
    #[serde(default)]
    pub slug: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub model: Option<String>,
    /// 小模型 / 快模型那一档（落到 `ANTHROPIC_SMALL_FAST_MODEL`）。
    ///
    /// Claude Code 会拿它跑后台的轻活（补全会话标题之类）。**很多中转站的
    /// 小模型跟主模型不是同一个名字**，只配主模型的话那些后台请求会直接报错，
    /// 而报错信息跟你正在做的事完全无关，极难联想。
    ///
    /// `None` = 不设这个变量，交给 Claude Code 用它自己的默认值。
    /// 形态参考 z-switch（MIT）的多档模型配置。
    #[serde(default)]
    pub small_fast_model: Option<String>,
    #[serde(default)]
    pub wire_api: WireApi,
    #[serde(default)]
    pub auth_style: AuthStyle,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub sort: u32,
    #[serde(default)]
    pub created_at: String,
}

/// 前端发过来的一条。`api_key` 是**明文**，留空表示「不改动现有 Key」。
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderInput {
    #[serde(flatten)]
    pub meta: ProviderMeta,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// 存进 `relay.json` 的一条。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProvider {
    #[serde(flatten)]
    pub meta: ProviderMeta,
    /// DPAPI 密文。空串 = 这条没配 Key。
    #[serde(default)]
    pub key_sealed: String,
}

/// 回给前端的一条。**结构上装不下 Key。**
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub meta: ProviderMeta,
    /// 配过 Key 吗。
    pub has_key: bool,
    /// 掩码，例如 `sk-a••••••••wxyz`。只够认出是哪一把，不够用。
    pub key_masked: Option<String>,
    /// 落盘时是加密存的吗。裸明文（早期版本/手改）要在界面上提示。
    pub key_encrypted: bool,
    /// 这条是所属 target 当前启用的那条吗。
    pub active: bool,
}

impl StoredProvider {
    pub fn view(&self, active: bool) -> ProviderView {
        let plain = secret::open(&self.key_sealed);
        ProviderView {
            meta: self.meta.clone(),
            has_key: !self.key_sealed.is_empty(),
            key_masked: plain.as_deref().map(super::mask),
            key_encrypted: secret::is_sealed(&self.key_sealed),
            active,
        }
    }

    /// 取出明文 Key，用于写进目标工具的配置。**只在后端调。**
    pub fn plain_key(&self) -> Option<String> {
        secret::open(&self.key_sealed)
    }
}

/// `relay.json` 的整体形状。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelayStore {
    #[serde(default)]
    pub providers: Vec<StoredProvider>,
    /// target（kebab-case 字符串） → 当前启用的 provider id。
    #[serde(default)]
    pub active: BTreeMap<String, String>,
}

pub fn store_path() -> std::path::PathBuf {
    crate::gate::state_dir().join("relay.json")
}

/// 读。文件不在、或者内容坏了，都退回空目录而不是报错 ——
/// 一个读不出来的配置文件不该让整个中转站页面打不开。
pub fn load() -> RelayStore {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str::<RelayStore>(&t).ok())
        .unwrap_or_default()
}

pub fn save(s: &RelayStore) -> Result<()> {
    super::atomic_write(&store_path(), &serde_json::to_string_pretty(s)?)
}

impl RelayStore {
    pub fn get(&self, id: &str) -> Option<&StoredProvider> {
        self.providers.iter().find(|p| p.meta.id == id)
    }

    /// 某个 target 当前启用的那条。
    pub fn active_id(&self, t: RelayTarget) -> Option<&str> {
        self.active.get(t.as_str()).map(String::as_str)
    }

    /// 按 target 过滤并按 `sort` 排好序，供界面直接渲染。
    pub fn view_of(&self, t: RelayTarget) -> Vec<ProviderView> {
        let active = self.active_id(t).map(String::from);
        let mut list: Vec<&StoredProvider> =
            self.providers.iter().filter(|p| p.meta.target == t).collect();
        list.sort_by_key(|p| p.meta.sort);
        list.into_iter()
            .map(|p| p.view(active.as_deref() == Some(p.meta.id.as_str())))
            .collect()
    }

    pub fn all_views(&self) -> Vec<ProviderView> {
        RelayTarget::ALL
            .into_iter()
            .flat_map(|t| self.view_of(t))
            .collect()
    }

    /// 新增或更新一条。
    ///
    /// **`api_key` 留空 = 保留原有的那把**，不是清空。这是界面上
    /// 「留空则不改动现有 Key」那句话的实现 —— 弄反了就会出现
    /// 「改个显示名把 Key 改没了」。
    pub fn upsert(&mut self, input: ProviderInput) -> Result<String> {
        let mut meta = input.meta;
        // 地址归一放在这里，不在前端 —— 手填、预设、从当前配置导入三条路都经过
        // upsert，只在表单里做的话另外两条就漏了。见 `probe::normalize_base_url`。
        meta.base_url = super::probe::normalize_base_url(&meta.base_url);
        if meta.slug.trim().is_empty() {
            meta.slug = slugify(&meta.name);
        }
        if meta.id.trim().is_empty() {
            meta.id = new_id(&meta.slug);
            meta.created_at = now_stamp();
        }

        let sealed = match input.api_key.as_deref() {
            // 给了新 Key：加密存。
            Some(k) if !k.trim().is_empty() => secret::seal(k.trim())?,
            // 没给：保留旧的。
            _ => self
                .get(&meta.id)
                .map(|p| p.key_sealed.clone())
                .unwrap_or_default(),
        };

        let id = meta.id.clone();
        match self.providers.iter_mut().find(|p| p.meta.id == id) {
            Some(existing) => {
                // created_at 不让前端覆盖。
                meta.created_at = existing.meta.created_at.clone();
                existing.meta = meta;
                existing.key_sealed = sealed;
            }
            None => {
                if meta.sort == 0 {
                    meta.sort = self.next_sort(meta.target);
                }
                self.providers.push(StoredProvider {
                    meta,
                    key_sealed: sealed,
                });
            }
        }
        Ok(id)
    }

    fn next_sort(&self, t: RelayTarget) -> u32 {
        self.providers
            .iter()
            .filter(|p| p.meta.target == t)
            .map(|p| p.meta.sort)
            .max()
            .unwrap_or(0)
            + 1
    }

    /// 删一条。**当前启用的那条也允许删** —— 但要把 active 指向清掉，
    /// 否则会留下一个指向不存在记录的悬空 id。
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|p| p.meta.id != id);
        self.active.retain(|_, v| v != id);
        self.providers.len() != before
    }

    /// 复制一条，名字加 `copy` 后缀，排在原条目下面。**Key 一起复制** ——
    /// 复制出来的副本通常就是要换端点不换账号。
    pub fn duplicate(&mut self, id: &str) -> Option<String> {
        let src = self.get(id)?.clone();
        let mut copy = src.clone();
        copy.meta.id = new_id(&src.meta.slug);
        copy.meta.name = format!("{} copy", src.meta.name);
        copy.meta.slug = format!("{}_copy", src.meta.slug);
        copy.meta.sort = src.meta.sort + 1;
        copy.meta.created_at = now_stamp();
        let new = copy.meta.id.clone();

        // 给后面的腾位置，免得两条 sort 撞在一起顺序随机。
        for p in self.providers.iter_mut() {
            if p.meta.target == src.meta.target && p.meta.sort > src.meta.sort {
                p.meta.sort += 1;
            }
        }
        self.providers.push(copy);
        Some(new)
    }

    /// 按前端给的 id 顺序重排某个 target。列表里没提到的保持在后面。
    pub fn reorder(&mut self, t: RelayTarget, ids: &[String]) {
        for (i, id) in ids.iter().enumerate() {
            if let Some(p) = self
                .providers
                .iter_mut()
                .find(|p| p.meta.id == *id && p.meta.target == t)
            {
                p.meta.sort = i as u32;
            }
        }
    }
}

/// 把显示名压成一个能当 TOML 键用的 slug。
///
/// `model_providers.<slug>` 是 TOML 的裸键，带空格或中文就得加引号，
/// 而 Codex 那边按裸键查。这里只留 ASCII 字母数字和下划线。
pub fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "provider".into()
    } else {
        s
    }
}

fn new_id(slug: &str) -> String {
    format!("{slug}-{:x}", chrono::Utc::now().timestamp_millis())
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 粘进来的是完整端点时，存下来的必须是 base —— 否则 `join` 会拼出
    /// `…/v1/messages/models`，404，而使用者会先去怀疑 Key 不怀疑地址。
    #[test]
    fn upsert_normalizes_a_pasted_endpoint() {
        let mut st = RelayStore::default();
        let mut i = input("x", RelayTarget::ClaudeCode, Some("k"));
        i.meta.base_url = "https://api.example.com/v1/messages".into();
        let id = st.upsert(i).unwrap();
        assert_eq!(st.get(&id).unwrap().meta.base_url, "https://api.example.com/v1");
    }

    fn input(name: &str, target: RelayTarget, key: Option<&str>) -> ProviderInput {
        ProviderInput {
            meta: ProviderMeta {
                id: String::new(),
                target,
                slug: String::new(),
                name: name.into(),
                base_url: "https://api.example.com".into(),
                model: None,
                small_fast_model: None,
                wire_api: WireApi::Responses,
                auth_style: AuthStyle::EnvKey,
                note: None,
                website: None,
                icon: None,
                sort: 0,
                created_at: String::new(),
            },
            api_key: key.map(String::from),
        }
    }

    #[test]
    fn view_can_not_carry_the_key() {
        // 这条是整个模块存在的理由：ProviderView 结构上装不下 Key。
        let mut s = RelayStore::default();
        s.upsert(input("我的中转", RelayTarget::Codex, Some("sk-secret-value")))
            .unwrap();
        let j = serde_json::to_string(&s.all_views()).unwrap();
        assert!(!j.contains("sk-secret-value"), "Key 泄回前端了：{j}");
        assert!(!j.contains("key_sealed"));
        assert!(j.contains("key_masked"));
        assert!(j.contains("\"has_key\":true"));
    }

    #[test]
    fn stored_file_never_holds_plaintext_on_windows() {
        let mut s = RelayStore::default();
        s.upsert(input("x", RelayTarget::Codex, Some("sk-secret-value")))
            .unwrap();
        let j = serde_json::to_string(&s).unwrap();
        if cfg!(windows) {
            assert!(!j.contains("sk-secret-value"), "明文落盘了：{j}");
        }
    }

    #[test]
    fn empty_key_on_update_keeps_the_existing_one() {
        // 「改个显示名把 Key 改没了」是最容易犯的一个错。
        let mut s = RelayStore::default();
        let id = s
            .upsert(input("原名", RelayTarget::Codex, Some("sk-keep-me")))
            .unwrap();

        let mut again = input("改了名", RelayTarget::Codex, None);
        again.meta.id = id.clone();
        s.upsert(again).unwrap();

        let p = s.get(&id).unwrap();
        assert_eq!(p.meta.name, "改了名");
        assert_eq!(p.plain_key().as_deref(), Some("sk-keep-me"));
    }

    #[test]
    fn blank_key_string_also_keeps_the_existing_one() {
        // 前端传的是 ''（清空输入框）而不是 undefined，也算「不改」。
        let mut s = RelayStore::default();
        let id = s
            .upsert(input("a", RelayTarget::Codex, Some("sk-keep-me")))
            .unwrap();
        let mut again = input("a", RelayTarget::Codex, Some("   "));
        again.meta.id = id.clone();
        s.upsert(again).unwrap();
        assert_eq!(s.get(&id).unwrap().plain_key().as_deref(), Some("sk-keep-me"));
    }

    #[test]
    fn removing_the_active_one_clears_the_pointer() {
        // 留下一个指向不存在记录的 active id，界面会显示「当前启用」但点不开。
        let mut s = RelayStore::default();
        let id = s.upsert(input("a", RelayTarget::Codex, None)).unwrap();
        s.active.insert(RelayTarget::Codex.as_str().into(), id.clone());
        assert!(s.remove(&id));
        assert_eq!(s.active_id(RelayTarget::Codex), None);
    }

    #[test]
    fn duplicate_lands_right_below_the_source() {
        let mut s = RelayStore::default();
        let a = s.upsert(input("A", RelayTarget::Codex, Some("sk-a"))).unwrap();
        s.upsert(input("B", RelayTarget::Codex, None)).unwrap();

        let copy = s.duplicate(&a).unwrap();
        let order: Vec<String> = s
            .view_of(RelayTarget::Codex)
            .into_iter()
            .map(|v| v.meta.name)
            .collect();
        assert_eq!(order, vec!["A", "A copy", "B"]);
        // 副本连 Key 一起带过去 —— 通常是换端点不换账号。
        assert_eq!(s.get(&copy).unwrap().plain_key().as_deref(), Some("sk-a"));
    }

    #[test]
    fn targets_are_isolated_from_each_other() {
        // 切 Codex 的供应商不该影响 Claude 那边的列表。
        let mut s = RelayStore::default();
        s.upsert(input("codex 的", RelayTarget::Codex, None)).unwrap();
        s.upsert(input("claude 的", RelayTarget::ClaudeCode, None))
            .unwrap();
        assert_eq!(s.view_of(RelayTarget::Codex).len(), 1);
        assert_eq!(s.view_of(RelayTarget::ClaudeCode).len(), 1);
        assert_eq!(s.view_of(RelayTarget::ClaudeDesktop).len(), 0);
    }

    #[test]
    fn reorder_only_touches_the_named_target() {
        let mut s = RelayStore::default();
        let a = s.upsert(input("A", RelayTarget::Codex, None)).unwrap();
        let b = s.upsert(input("B", RelayTarget::Codex, None)).unwrap();
        s.reorder(RelayTarget::Codex, &[b.clone(), a.clone()]);
        let order: Vec<String> = s
            .view_of(RelayTarget::Codex)
            .into_iter()
            .map(|v| v.meta.name)
            .collect();
        assert_eq!(order, vec!["B", "A"]);
    }

    #[test]
    fn slug_stays_a_bare_toml_key() {
        // model_providers.<slug> 是 TOML 裸键，带空格或中文就得加引号，
        // 而 Codex 按裸键查。
        assert_eq!(slugify("速联言 Relay"), "relay");
        assert_eq!(slugify("PackyCode"), "packycode");
        assert_eq!(slugify("kimi-for-coding"), "kimi_for_coding");
        assert_eq!(slugify(""), "provider");
        assert_eq!(slugify("中文"), "provider");
        for name in ["A B", "x/y", "全中文名字"] {
            let s = slugify(name);
            assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{s}");
        }
    }

    #[test]
    fn broken_store_file_reads_as_empty_not_error() {
        // 读不出来的配置文件不该让整个中转站页面打不开。
        let s: RelayStore = serde_json::from_str("{ not json").unwrap_or_default();
        assert!(s.providers.is_empty());
    }
}
