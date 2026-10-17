//! 插件。
//!
//! 目前三个插件（酒馆、Codex 出站与换出口、反重力汉化与审批），
//! 形状按「日后能从远程 index 拉清单」来设计：每个插件自报 id / 名称 / 检测 / 启停，
//! 界面只认这个形状，不认具体插件。
//!
//! 酒馆 2026-09-29 起是面板的**内置功能**：扩展目录里没有它了（`extensions::RETIRED`），
//! 界面在三个账户页（起酒馆的磁贴 + 右上角的「酒馆」弹窗：控制台、角色卡、世界书、扩展、回收站）
//! 与软件页（SillyTavern 卡是酒馆本体；三家的桥接设置在 Claude Code / Codex CLI / Gemini CLI 三张卡的最下面，
//! 2026-09-29 第三轮对调过来的），模块与 `plugin_*` 命令名是历史名字，没改。
//!
//! `claude_zh` 不再是插件（2026-09-27 撤掉了 Claude 汉化），留在这里的只是清理残留那一点。

pub mod antigravity_ui;
pub mod claude_zh;
pub mod codex_egress;
pub mod sillytavern;
pub mod tavern_assets;
pub mod tavern_bridge_api;
pub mod tavern_console;
pub mod tavern_library;
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

// 原来这里有 `OfficialCatalogStatus` / `official_catalog_status()`：远程「官方插件清单」的状态，
// 永远回「尚未配置」。那份清单从来没建过，旧插件页也早没了，2026-09-27 删掉。
// 扩展目录只有内置的 `extension-catalog.json`，不接受任意下载地址 —— 这一条照旧。

impl DependencyCheck {
    pub fn new(label: impl Into<String>, ok: bool, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ok,
            detail: detail.into(),
        }
    }
}
