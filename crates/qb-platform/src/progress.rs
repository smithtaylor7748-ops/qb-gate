//! 五项检查的进度记录（`progress.json`）。
//!
//! 最早侧栏那五个菜单本身就是引导步骤，后来有过一页「新手引导」；两样都没了
//! （新手引导 2026-09-27 删掉）。现在真正被界面读的只有 `purity` —— 综合评分里
//! 纯净度那一项的人工判定；其余几步照常记、照常读得进来。
//!
//! 五个 id 是使用者机器上已有数据的 key，**不许改名**（见 [`STEP_IDS`]）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use ts_rs::TS;

/// 五个步骤的稳定标识，与前端 `steps.ts` 一一对应，改名要两边一起改。
pub const STEP_IDS: [&str; 5] = ["purity", "environment", "dns", "iplock", "accounts"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum StepState {
    #[default]
    Pending,
    Passed,
    Failed,
    /// 用户明确按了「强制跳过」。与 Pending 区分开 —— 一个是还没做，一个是知情放弃。
    Skipped,
}

/// 风险度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// 未检测，还不知道。
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepRecord {
    pub state: StepState,
    pub risk: Risk,
    /// 一句话说明（审计留痕用，界面上纯净度那一项会显示它）。
    pub detail: String,
    pub updated_at: Option<String>,
}

impl Default for StepRecord {
    fn default() -> Self {
        Self {
            state: StepState::Pending,
            risk: Risk::Unknown,
            detail: String::new(),
            updated_at: None,
        }
    }
}

/// 原来还有 `completed_once`（「引导走完一轮，侧栏序号换成状态灯」）和两个计数
/// （「你跳过了 N 项」）。侧栏向导与新手引导都没了，没人读，2026-09-27 删掉；
/// 老文件里的 `completed_once` 读的时候直接忽略。
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct Progress {
    pub steps: BTreeMap<String, StepRecord>,
}

fn path() -> PathBuf {
    crate::paths::state_dir().join("progress.json")
}

pub fn load() -> Progress {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(p: &Progress) {
    let f = path();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(body) = serde_json::to_string_pretty(p) {
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    }
}

pub fn set(id: &str, state: StepState, risk: Risk, detail: impl Into<String>) -> Progress {
    let mut p = load();
    p.steps.insert(
        id.to_string(),
        StepRecord {
            state,
            risk,
            detail: detail.into(),
            updated_at: Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
        },
    );
    save(&p);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 老版本写的 `progress.json` 还带着 `completed_once`，照样读得进来、步骤一条不丢。
    #[test]
    fn an_old_file_with_completed_once_still_loads() {
        let old = r#"{"steps":{"purity":{"state":"passed","risk":"low","detail":"x","updated_at":null}},"completed_once":true}"#;
        let p: Progress = serde_json::from_str(old).unwrap();
        assert_eq!(p.steps["purity"].state, StepState::Passed);
        assert!(STEP_IDS.contains(&"purity"));
    }
}
