//! turn-state「识别」的落盘 marker：`state_dir/turnstate-takeover.json`。
//!
//! 从 `usecase::turnstate_ops` 里拆出来的叶子模块 —— 0.25.0 起 `workspace::launch`
//! 起 GPT 桌面端时要问一句「这个槽位是不是被识别接管着」（接管期间 `config.toml` 的
//! `model_provider` 是我们自己写的，不能让残留检查把它当中转配置拒掉），
//! 而 `usecase` 又要调 `workspace::launch`。两边互相 import 就是
//! `module_cycles_only_ever_shrink` 抓的那种环；把共用的这一小块提到下面来，
//! 跟 A0 把 `state_dir` / `log` 提成 `paths` / `audit` 是同一个手法。
//!
//! 读写 marker 的**唯一**入口在这里；`turnstate_ops` 里的 `enable` / `disable` 调它。
//! marker 是「识别开着」的唯一真相（面板重启后内存里的 armed 位归零、它不会）。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

/// 写进槽位 `config.toml` 的 provider id。跟中转环境用的 `qb_relay` 分开，
/// 一眼分得清「这是识别改的」还是「这是中转环境」。
pub const PROVIDER_ID: &str = "qb_turnstate";

/// 「识别开着」的落盘记录。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Takeover {
    pub slot_id: String,
    pub slot_label: String,
    /// 被接管的槽位目录（`…\<slot>\home`）。
    pub home: String,
    /// 接管前 `model_provider` 是什么；`None` = 原来没写（官方默认）。关闭时按它恢复。
    pub previous_model_provider: Option<String>,
    pub applied_at: String,
}

fn marker_path() -> PathBuf {
    crate::paths::state_dir().join("turnstate-takeover.json")
}

/// 现在有没有接管中的槽位（= 识别开着没）。
pub fn takeover() -> Option<Takeover> {
    crate::config_io::read_optional(&marker_path())
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

/// 落盘。
pub fn write(take: &Takeover) -> Result<()> {
    crate::config_io::replace(&marker_path(), Some(&serde_json::to_vec_pretty(take)?))
}

/// 删掉 marker（= 识别关了）。
pub fn clear() -> Result<()> {
    crate::config_io::replace(&marker_path(), None)
}
