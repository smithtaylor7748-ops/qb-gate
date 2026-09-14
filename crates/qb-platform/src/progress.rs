//! 引导进度。
//!
//! 侧栏那五个菜单**本身就是引导步骤**，没有单独的「新手引导」页。
//! 导航是自由的 —— 随便点，只标状态；每行右侧标风险度。
//!
//! 强制跳过必须留痕：总览上要能看见「你跳过了 N 项」，
//! 否则跳过就成了静默的坑，出问题时没人记得哪一步没做。

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

/// 风险度，渲染在侧栏每一行的右侧。
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
    /// 一句话说明，直接显示在侧栏 tooltip 与总览里。
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct Progress {
    pub steps: BTreeMap<String, StepRecord>,
    /// 引导是否已整体走完一轮。走完后侧栏序号换成状态灯。
    #[serde(default)]
    pub completed_once: bool,
}

impl Progress {
    pub fn skipped_count(&self) -> usize {
        self.steps
            .values()
            .filter(|r| r.state == StepState::Skipped)
            .count()
    }

    pub fn done_count(&self) -> usize {
        self.steps
            .values()
            .filter(|r| matches!(r.state, StepState::Passed | StepState::Skipped))
            .count()
    }

    /// 所有步骤都不再是 Pending 时，算走完一轮。
    pub fn recompute_completion(&mut self) {
        self.completed_once = STEP_IDS.iter().all(|id| {
            self.steps
                .get(*id)
                .is_some_and(|r| r.state != StepState::Pending)
        });
    }
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
    p.recompute_completion();
    save(&p);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(state: StepState) -> StepRecord {
        StepRecord {
            state,
            risk: Risk::Low,
            detail: String::new(),
            updated_at: None,
        }
    }

    #[test]
    fn skipped_is_counted_separately_from_pending() {
        let mut p = Progress::default();
        p.steps.insert("purity".into(), rec(StepState::Skipped));
        p.steps.insert("dns".into(), rec(StepState::Pending));
        assert_eq!(p.skipped_count(), 1);
        assert_eq!(p.done_count(), 1);
    }

    #[test]
    fn completion_needs_every_step_touched() {
        let mut p = Progress::default();
        for id in STEP_IDS.iter().take(4) {
            p.steps.insert((*id).into(), rec(StepState::Passed));
        }
        p.recompute_completion();
        assert!(!p.completed_once, "还剩一步没碰就不算走完");

        p.steps.insert(STEP_IDS[4].into(), rec(StepState::Skipped));
        p.recompute_completion();
        assert!(p.completed_once, "跳过也算「碰过」");
    }
}
