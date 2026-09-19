//! 插件。
//!
//! 目前两个插件（酒馆、Codex 出站与换出口），形状按「日后能从远程 index 拉清单」来设计：
//! 每个插件自报 id / 名称 / 检测 / 启停，界面只认这个形状，不认具体插件。

pub mod codex_egress;
pub mod sillytavern;
pub mod tavern_assets;
pub mod tavern_locate;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum PluginState {
    /// 依赖没装齐（找不到目录、没有 Python、没有 Node）
    Missing,
    /// 装好了但没在跑
    Ready,
    /// 正在运行
    Running,
    /// 装了但有问题，`detail` 说明是什么
    Broken,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PluginStatus {
    pub id: &'static str,
    pub name: &'static str,
    pub state: PluginState,
    pub detail: String,
    /// 逐项依赖检查结果，界面上摊开显示，便于用户自己看缺什么。
    pub checks: Vec<DependencyCheck>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DependencyCheck {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

/// 官方插件清单状态。仓库尚未创建时明确保持停用，插件页也不会接受任意 URL。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OfficialCatalogStatus {
    pub configured: bool,
    pub source: Option<String>,
    pub signed: bool,
    pub detail: String,
}

pub fn official_catalog_status() -> OfficialCatalogStatus {
    OfficialCatalogStatus {
        configured: false,
        source: None,
        signed: false,
        detail: "官方插件清单仓库尚未配置；仅可使用内置插件。".into(),
    }
}

impl DependencyCheck {
    pub fn new(label: impl Into<String>, ok: bool, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ok,
            detail: detail.into(),
        }
    }
}
