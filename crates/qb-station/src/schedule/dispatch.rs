//! 派单：一个软件的线路排成一个队列 —— 第一条是当前上游（p1），后面是失败时依次改用的热备。
//!
//! 改写自 sub2guard v2 的 `scheduler/rank.py` 的 `decide`（中转站重构阶段 2；ATTRIBUTION 的 sub2guard 一节），
//! 接到 QB Gate 自己的排序上（使用者定的「分档 + 档内按勾选项」，交接文档 §1.2）：
//!
//! | 档 | 是谁 | sub2guard 里对应 |
//! |---|---|---|
//! | 钉住 | 使用者手动选的那条，4 小时内（除非它挂了） | `override = "pin"` + 人工保护 4 小时 |
//! | 合格 | 没熔断、测活没判死、过了全部底线 | 不亏钱 |
//! | 兜底 | 没过某条底线（**不拉黑**，照样留在队列里接失败切换） | 亏钱的「照样开着兜底」 |
//! | 故障 | 熔断中，或测活判死 | `state = down` |
//!
//! 档内按 [`rank`](super::rank) 的最弱项总分排（勾的「便宜 / 快 / 稳」）。
//!
//! # ⛔ 防抖只有这一道
//!
//! 换第一条用 sub2guard 的防抖：挑战者要领先现任 [`DispatchParams::margin`]（默认沿用 QB Gate 实测的 8%，
//! [`HYSTERESIS`](super::HYSTERESIS)）、自己至少有 [`min_samples`](DispatchParams::min_samples) 个样本、
//! 并且**连续 [`confirm`](DispatchParams::confirm) 轮有新证据**都成立。所以调度这条路上 `rank` 一律不给
//! `incumbent`（它自己那道迟滞不生效）—— 两道叠在一起就是永远不换，而且没人说得清为什么。
//!
//! # 换第一条的规矩（照 sub2guard，加上 QB Gate 自己的那条）
//!
//! - 钉住的那条：直接当第一（它挂了就不算钉住）。
//! - 现任掉档了（熔断 / 判死 / 没过底线）而有**合格**的：立刻换，不谈防抖。
//! - ⛔ **兜底和故障档的线永远不会被换上来当第一**（CLAUDE.md「熔断中或不合格的线不当第一」）。
//!   没有合格的就留着现任 —— 本机路由每一发自己会跳过熔断着的线、改用队列里的下一条，这里不必冒险。
//! - 同档：比总分；现任「有证据却排不出分」（比如请求全失败、量不到首字）时立刻让位；
//!   现任只是还没证据时先等（sub2guard 2026-10-11 上线时踩过：测活模型一换，现任被提前换掉）。
//!
//! # 热备
//!
//! 保持上一轮的相对顺序，除非有一对明显排反了（档次反了，或后面的领先 margin 以上；热备之间 1 个样本就够）——
//! 省掉无意义的来回改。**永不全关**：所有线都在队列里，故障的排在最后；全都熔断着时本机路由仍打第一条。

use super::rank::Ranking;
use super::HYSTERESIS;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// 防抖的几个数。存在 `Schedule` 里，每个软件一份；缺字段按默认补。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct DispatchParams {
    /// 挑战者要领先现任多少（比的是最弱项总分）。默认沿用实测的 8%。
    pub margin: f64,
    /// 挑战者自己至少要有几个样本。
    pub min_samples: u32,
    /// 要连续几轮有新证据都领先才换。
    pub confirm: u32,
}

impl DispatchParams {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if !(self.margin.is_finite() && (0.0..=1.0).contains(&self.margin)) {
            return Err("领先多少要在 0% 到 100% 之间".into());
        }
        if self.min_samples == 0 || self.confirm == 0 {
            return Err("样本数和确认轮数至少是 1".into());
        }
        if self.min_samples > 1_000 || self.confirm > 100 {
            return Err("样本数或确认轮数大得离谱，那等于永远不换".into());
        }
        Ok(())
    }
}

impl Default for DispatchParams {
    fn default() -> Self {
        Self {
            margin: HYSTERESIS,
            min_samples: 3,
            confirm: 2,
        }
    }
}

/// 档位。**数字小的排前面。** TS 那边叫 `DispatchTier`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, rename = "DispatchTier")]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Pinned,
    Qualified,
    Fallback,
    Faulty,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Pinned => "钉住",
            Tier::Qualified => "合格",
            Tier::Fallback => "兜底",
            Tier::Faulty => "故障",
        }
    }
}

/// 一条线除了排序结果以外，派单还要知道的几件事（由 `qb-app` 从测活记录、本机日志里装）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineFacts {
    /// 比快慢用的样本数（开了花钱测活时是测活成功次数，否则是本机路由的请求数）。
    pub samples: u32,
    /// 最新一条证据的时刻。防抖要靠它判断「这一轮有没有新证据」。
    pub evidence_at: Option<i64>,
    /// 测活判死了的话，为什么（`liveness::assess` 的那句）。
    pub probe_down: Option<String>,
    /// 线路的模型映射把请求换成了这些模型、上游却说没有（告警用）。
    pub mapping_missing: Vec<String>,
    /// 给界面的显示名（站点 · 分组）。
    pub label: String,
}

/// 钉住：使用者手动选的那条，到什么时候为止。TS 那边叫 `DispatchPin`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "DispatchPin")]
pub struct Pin {
    pub route_id: String,
    pub until_ms: i64,
}

/// 防抖的记忆（存在派单队列里，跨轮带着）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct DispatchMemory {
    pub challenger: Option<String>,
    pub streak: u32,
    /// 上一轮确认时两边的最新证据时刻：`[挑战者, 现任]`。一样 = 这一轮没有新证据，不算又确认了一次。
    pub signature: Option<[Option<i64>; 2]>,
}

/// 队列里的一行（界面「派单队列」那张表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DispatchRow {
    pub route_id: String,
    pub tier: Tier,
    /// 在队列里第几（1 = 当前上游）。
    pub position: u32,
    /// 最弱项总分（已乘降权）。`None` = 排不出分（缺证据）。
    pub score: Option<f64>,
    pub samples: u32,
    /// 为什么在这一档、这一位。纯文本。
    pub reason: String,
}

/// 这一轮派单的结论。
/// 不过 IPC（界面读的是存进派单队列的那几项），所以不导出 TS。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dispatch {
    /// 第一条是当前上游，后面是热备。**所有线都在里面**（永不全关）。
    pub order: Vec<String>,
    pub rows: Vec<DispatchRow>,
    pub memory: DispatchMemory,
    /// 告警（界面上醒目地列出来）。`P1` 开头的是「客户端现在大概率在报错」那一级。
    pub alerts: Vec<String>,
    /// 一句话结论（写进 `Schedule.last_note`）。
    pub note: String,
    /// 这一轮第一条换了没有。
    pub switched: bool,
}

struct Line<'a> {
    id: &'a str,
    tier: Tier,
    score: Option<f64>,
    samples: u32,
    evidence_at: Option<i64>,
    tested: bool,
    reason: String,
}

#[derive(PartialEq)]
enum Verdict {
    Immediate,
    Challenge,
    No,
}

/// 同档时挑战者能不能换掉现任（sub2guard 的 `_speed_verdict`；QB Gate 的分越大越好）。
fn verdict(
    challenger: &Line,
    incumbent: &Line,
    params: &DispatchParams,
    min_samples: u32,
) -> Verdict {
    let Some(c) = challenger.score else {
        return Verdict::No;
    };
    let Some(i) = incumbent.score else {
        // 现任有证据却排不出分 → 没什么可比的，立刻让位；只是还没证据 → 先等它。
        return if incumbent.tested {
            Verdict::Immediate
        } else {
            Verdict::No
        };
    };
    if challenger.samples >= min_samples && c > i * (1.0 + params.margin) {
        Verdict::Challenge
    } else {
        Verdict::No
    }
}

/// 这个顺序里有没有一对明显排反了。
fn inverted(seq: &[&Line], params: &DispatchParams) -> bool {
    for (i, a) in seq.iter().enumerate() {
        for b in &seq[i + 1..] {
            if b.tier < a.tier {
                return true;
            }
            if b.tier == a.tier && verdict(b, a, params, 1) != Verdict::No {
                return true;
            }
        }
    }
    false
}

/// 排一轮派单。
///
/// - `ranking`：这个软件这一轮 [`rank`](super::rank) 的结果（**不带 incumbent**，见模块头）。
/// - `incumbent`：当前上游；`previous`：上一轮的队列（热备的相对顺序要照它）。
/// - `pin`：钉住的那条（过期了的调用方先去掉也行，这里也查）。
#[allow(clippy::too_many_arguments)]
pub fn dispatch(
    ranking: &Ranking,
    facts: &HashMap<String, LineFacts>,
    incumbent: Option<&str>,
    previous: &[String],
    pin: Option<&Pin>,
    memory: &DispatchMemory,
    params: &DispatchParams,
    now_ms: i64,
) -> Dispatch {
    let pinned_id = pin
        .filter(|p| now_ms < p.until_ms)
        .map(|p| p.route_id.as_str());
    let label = |id: &str| {
        facts
            .get(id)
            .map(|f| f.label.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| id.to_string())
    };

    // ---- 一、每条线进哪一档
    let mut lines: Vec<Line> = ranking
        .rows
        .iter()
        .map(|row| {
            let f = facts.get(&row.route_id).cloned().unwrap_or_default();
            let (tier, reason) = if row.tripped {
                (Tier::Faulty, "熔断中".to_string())
            } else if let Some(why) = &f.probe_down {
                (Tier::Faulty, why.clone())
            } else if pinned_id == Some(row.route_id.as_str()) {
                (Tier::Pinned, "你手动选的，钉住".to_string())
            } else if !row.failed_floors.is_empty() {
                let misses: Vec<String> = row
                    .failed_floors
                    .iter()
                    .map(|m| format!("{}没过线", m.axis.label()))
                    .collect();
                (Tier::Fallback, format!("{}，兜底", misses.join("、")))
            } else {
                (Tier::Qualified, "合格".to_string())
            };
            Line {
                id: &row.route_id,
                tier,
                score: row.score,
                samples: f.samples,
                evidence_at: f.evidence_at,
                tested: f.samples > 0 || f.evidence_at.is_some(),
                reason,
            }
        })
        .collect();
    // 档 → 总分（大的在前，没分的在后）→ id。同样的输入永远同样的输出。
    lines.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then_with(|| match (a.score, b.score) {
                (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.id.cmp(b.id))
    });

    let mut alerts = Vec::new();
    let mut new_memory = DispatchMemory::default();
    if lines.is_empty() {
        return Dispatch {
            order: Vec::new(),
            rows: Vec::new(),
            memory: new_memory,
            alerts: vec!["这个软件底下还没有能调度的线路".into()],
            note: "没有线路可排".into(),
            switched: false,
        };
    }

    // ---- 二、第一条
    let inc = incumbent.and_then(|id| lines.iter().position(|l| l.id == id));
    let best = 0usize;
    let note;
    let first: usize = match inc {
        _ if lines[best].tier == Tier::Pinned => {
            note = format!("钉住的「{}」", label(lines[best].id));
            best
        }
        None => {
            // 还没有当前上游：挑最好的那条（哪一档都行 —— 没有第一条，客户端每一发都是「还没选上游」）。
            note = format!("选了「{}」", label(lines[best].id));
            best
        }
        Some(i) if lines[best].tier < lines[i].tier => {
            if lines[best].tier <= Tier::Qualified {
                note = format!(
                    "「{}」{}，立刻换到「{}」",
                    label(lines[i].id),
                    lines[i].reason,
                    label(lines[best].id)
                );
                best
            } else {
                // ⛔ 兜底 / 故障档的不换上来当第一。
                note = format!(
                    "「{}」{}，但没有合格的线可换（本机路由会自己跳过熔断着的线）",
                    label(lines[i].id),
                    lines[i].reason
                );
                i
            }
        }
        Some(i) => {
            // 同档：找排在现任前面、第一个有资格挑战的（最好的那条样本不够时，轮到第二好的）。
            let mut pick = i;
            let mut v = Verdict::No;
            for (k, cand) in lines.iter().enumerate().take(i) {
                if cand.tier != lines[i].tier {
                    continue;
                }
                v = verdict(cand, &lines[i], params, params.min_samples);
                if v != Verdict::No {
                    pick = k;
                    break;
                }
            }
            match v {
                Verdict::Immediate => {
                    note = format!(
                        "现任「{}」有证据却排不出分，换到「{}」",
                        label(lines[i].id),
                        label(lines[pick].id)
                    );
                    pick
                }
                Verdict::Challenge => {
                    let sig = [lines[pick].evidence_at, lines[i].evidence_at];
                    let streak = if memory.challenger.as_deref() == Some(lines[pick].id) {
                        memory.streak + u32::from(memory.signature != Some(sig))
                    } else {
                        1
                    };
                    let gain = lines[pick].score.unwrap_or(0.0)
                        / lines[i].score.unwrap_or(f64::INFINITY)
                        - 1.0;
                    if streak < params.confirm {
                        new_memory = DispatchMemory {
                            challenger: Some(lines[pick].id.to_string()),
                            streak,
                            signature: Some(sig),
                        };
                        note = format!(
                            "「{}」比现任领先 {:.0}%，再确认 {} 轮新证据就换",
                            label(lines[pick].id),
                            gain * 100.0,
                            params.confirm - streak
                        );
                        i
                    } else {
                        note = format!(
                            "「{}」连续 {streak} 轮都领先现任 {:.0}%，换它派单",
                            label(lines[pick].id),
                            gain * 100.0
                        );
                        pick
                    }
                }
                Verdict::No => {
                    note = "现任仍然是最好的那条".into();
                    i
                }
            }
        }
    };
    let switched = incumbent.is_some_and(|id| id != lines[first].id);

    // ---- 三、热备：保持上一轮的相对顺序，除非明显排反了
    let strict: Vec<&Line> = lines
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != first)
        .map(|(_, l)| l)
        .collect();
    let pos = |id: &str| previous.iter().position(|p| p == id).unwrap_or(usize::MAX);
    let mut kept = strict.clone();
    kept.sort_by_key(|l| pos(l.id));
    let rest =
        if kept.iter().map(|l| l.id).ne(strict.iter().map(|l| l.id)) && !inverted(&kept, params) {
            kept
        } else {
            strict
        };
    let mut order_lines: Vec<&Line> = vec![&lines[first]];
    order_lines.extend(rest);

    // ---- 四、告警（sub2guard §4.5 的 QB Gate 版本）
    let p1 = order_lines[0];
    if lines.iter().all(|l| l.tier == Tier::Faulty) {
        alerts.push(
            "P1 所有线路都熔断着或测活没通过 —— 客户端现在大概率在报错：充值、换 Key，或者加线路"
                .into(),
        );
    } else if !lines.iter().any(|l| l.tier <= Tier::Qualified) {
        alerts.push(format!(
            "没有一条线过得了你设的底线，派单第一的「{}」是兜底",
            label(p1.id)
        ));
    }
    if lines.len() == 1 {
        alerts.push("只有 1 条线，没有备份：它一坏这个软件就停。多加几条线，调度才有得挑".into());
    }
    if let Some(id) = pin
        .filter(|p| now_ms < p.until_ms)
        .map(|p| p.route_id.as_str())
    {
        if let Some(l) = lines.iter().find(|l| l.id == id && l.tier == Tier::Faulty) {
            alerts.push(format!(
                "钉住的「{}」{}，这一轮没按钉住派",
                label(id),
                l.reason
            ));
        }
    }
    for (id, f) in facts {
        if !f.mapping_missing.is_empty() && lines.iter().any(|l| l.id == id) {
            let models = f.mapping_missing.join("、");
            alerts.push(format!(
                "「{}」的模型映射把请求换成了 {models}，但上游说没有 —— 到线路弹窗的「模型映射」里改掉",
                label(id)
            ));
        }
    }
    alerts.sort();

    let rows = order_lines
        .iter()
        .enumerate()
        .map(|(k, l)| DispatchRow {
            route_id: l.id.to_string(),
            tier: l.tier,
            position: k as u32 + 1,
            score: l.score,
            samples: l.samples,
            reason: l.reason.clone(),
        })
        .collect();
    Dispatch {
        order: order_lines.iter().map(|l| l.id.to_string()).collect(),
        rows,
        memory: new_memory,
        alerts,
        note,
        switched,
    }
}

#[cfg(test)]
mod tests {
    //! 改写自 sub2guard v2 的 `tests/scheduler/test_rank.py`：换人防抖、掉档立刻换、兜底不当第一、
    //! 钉住、热备不来回改、永不全关、告警。粘性会话回收那一组是 Sub2API 专属的，不搬（交接文档 §4.2）。
    use super::super::rank::Row;
    use super::*;

    const NOW: i64 = 1_000_000_000;

    fn row(id: &str, score: Option<f64>) -> Row {
        Row {
            route_id: id.into(),
            score,
            per_axis: Vec::new(),
            weakest: None,
            missing: Vec::new(),
            failed_floors: Vec::new(),
            eligible: true,
            tripped: false,
        }
    }
    fn ranking(rows: Vec<Row>) -> Ranking {
        Ranking {
            rows,
            winner: None,
            switched: false,
            held_by_hysteresis: false,
            indistinguishable: Vec::new(),
            floors_relaxed: false,
            breakers_relaxed: false,
        }
    }
    fn facts(entries: &[(&str, u32, i64)]) -> HashMap<String, LineFacts> {
        entries
            .iter()
            .map(|(id, samples, at)| {
                (
                    id.to_string(),
                    LineFacts {
                        samples: *samples,
                        evidence_at: Some(*at),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }
    fn run(
        r: &Ranking,
        f: &HashMap<String, LineFacts>,
        inc: Option<&str>,
        mem: &DispatchMemory,
    ) -> Dispatch {
        dispatch(r, f, inc, &[], None, mem, &DispatchParams::default(), NOW)
    }

    #[test]
    fn debounce_params_out_of_range_are_refused() {
        assert!(DispatchParams::default().validate().is_ok());
        for bad in [
            DispatchParams {
                margin: -0.1,
                ..Default::default()
            },
            DispatchParams {
                margin: f64::NAN,
                ..Default::default()
            },
            DispatchParams {
                confirm: 0,
                ..Default::default()
            },
            DispatchParams {
                min_samples: 5_000,
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        // 老数据缺字段按默认补。
        let old: DispatchParams = serde_json::from_str(r#"{"margin":0.2}"#).unwrap();
        assert_eq!((old.min_samples, old.confirm), (3, 2));
    }

    #[test]
    fn a_challenger_needs_the_margin_and_two_rounds_of_new_evidence() {
        let r = ranking(vec![row("a", Some(0.5)), row("b", Some(1.0))]);
        let mut f = facts(&[("a", 5, 100), ("b", 5, 100)]);
        let first = run(&r, &f, Some("a"), &DispatchMemory::default());
        assert_eq!(first.order[0], "a", "第一次发现更好的，先不换");
        assert!(first.note.contains("再确认 1 轮"), "{}", first.note);
        let again = run(&r, &f, Some("a"), &first.memory);
        assert_eq!(again.order[0], "a", "没有新证据，不算第二次确认");
        f.get_mut("b").unwrap().evidence_at = Some(200);
        let third = run(&r, &f, Some("a"), &again.memory);
        assert_eq!(third.order, ["b", "a"]);
        assert!(third.switched);
        assert_eq!(third.memory, DispatchMemory::default(), "换完了记忆要清掉");
    }

    #[test]
    fn a_small_lead_never_switches() {
        // 只领先 5%（门槛 8%）：怎么确认都不换。
        let r = ranking(vec![row("a", Some(0.95)), row("b", Some(1.0))]);
        let mut mem = DispatchMemory::default();
        for step in 0..4 {
            let f = facts(&[("a", 5, 100), ("b", 5, 100 + step)]);
            let out = run(&r, &f, Some("a"), &mem);
            assert_eq!(out.order[0], "a");
            mem = out.memory;
        }
    }

    #[test]
    fn a_challenger_with_too_few_samples_waits_and_the_second_best_may_challenge() {
        let r = ranking(vec![
            row("a", Some(0.4)),
            row("b", Some(0.8)),
            row("c", Some(1.0)),
        ]);
        let f = facts(&[("a", 5, 1), ("b", 5, 1), ("c", 1, 1)]);
        let out = run(&r, &f, Some("a"), &DispatchMemory::default());
        assert_eq!(out.order[0], "a");
        assert_eq!(
            out.memory.challenger.as_deref(),
            Some("b"),
            "最好的那条样本不够，轮到第二好的去挑战"
        );
    }

    #[test]
    fn a_broken_incumbent_is_replaced_immediately_but_never_by_a_fallback() {
        let mut broken = row("a", Some(1.0));
        broken.tripped = true;
        let r = ranking(vec![broken.clone(), row("b", Some(0.3))]);
        let f = facts(&[("a", 5, 1), ("b", 5, 1)]);
        let out = run(&r, &f, Some("a"), &DispatchMemory::default());
        assert_eq!(
            out.order,
            ["b", "a"],
            "现任熔断了、有合格的：立刻换，故障的排最后"
        );
        assert!(out.switched);

        // 唯一的另一条没过底线（兜底）：⛔ 不换上来当第一，留着现任，让路由自己跳过熔断的那条。
        let mut slow = row("b", Some(0.3));
        slow.failed_floors = vec![super::super::rank::FloorMiss {
            axis: super::super::Axis::Fast,
            actual: 9000.0,
            limit: 3000.0,
        }];
        let r = ranking(vec![broken, slow]);
        let out = run(&r, &f, Some("a"), &DispatchMemory::default());
        assert_eq!(out.order, ["a", "b"]);
        assert!(!out.switched);
        assert!(out.alerts.iter().any(|a| a.starts_with("P1")) || out.note.contains("没有合格的"));
    }

    #[test]
    fn a_probe_that_says_dead_makes_the_line_faulty() {
        let r = ranking(vec![row("a", Some(1.0)), row("b", Some(0.5))]);
        let mut f = facts(&[("a", 5, 1), ("b", 5, 1)]);
        f.get_mut("a").unwrap().probe_down = Some("测活失败：上游余额不足，要充值".into());
        let out = run(&r, &f, Some("a"), &DispatchMemory::default());
        assert_eq!(out.order[0], "b");
        assert_eq!(out.rows[1].tier, Tier::Faulty);
        assert!(out.rows[1].reason.contains("余额不足"));
    }

    #[test]
    fn an_incumbent_with_evidence_but_no_score_yields_while_one_without_evidence_is_kept() {
        let r = ranking(vec![row("a", None), row("b", Some(0.5))]);
        let with = facts(&[("a", 4, 1), ("b", 5, 1)]);
        assert_eq!(
            run(&r, &with, Some("a"), &DispatchMemory::default()).order[0],
            "b"
        );
        let mut without = facts(&[("b", 5, 1)]);
        without.insert("a".into(), LineFacts::default());
        assert_eq!(
            run(&r, &without, Some("a"), &DispatchMemory::default()).order[0],
            "a",
            "现任只是还没证据（刚接上）：先等，别提前换掉"
        );
    }

    #[test]
    fn a_pin_goes_first_unless_the_pinned_line_is_dead() {
        let r = ranking(vec![row("a", Some(1.0)), row("b", Some(0.2))]);
        let f = facts(&[("a", 5, 1), ("b", 5, 1)]);
        let pin = Pin {
            route_id: "b".into(),
            until_ms: NOW + 1,
        };
        let p = DispatchParams::default();
        let out = dispatch(
            &r,
            &f,
            Some("a"),
            &[],
            Some(&pin),
            &DispatchMemory::default(),
            &p,
            NOW,
        );
        assert_eq!(out.order[0], "b");
        assert_eq!(out.rows[0].tier, Tier::Pinned);
        // 过期了就不算。
        let out = dispatch(
            &r,
            &f,
            Some("b"),
            &[],
            Some(&pin),
            &DispatchMemory::default(),
            &p,
            NOW + 2,
        );
        assert_ne!(out.rows[0].tier, Tier::Pinned);
        // 钉住的那条熔断了：这一轮不按钉住派，并且告警。
        let mut dead = row("b", Some(0.2));
        dead.tripped = true;
        let r2 = ranking(vec![row("a", Some(1.0)), dead]);
        let out = dispatch(
            &r2,
            &f,
            Some("b"),
            &[],
            Some(&pin),
            &DispatchMemory::default(),
            &p,
            NOW,
        );
        assert_eq!(out.order[0], "a");
        assert!(out.alerts.iter().any(|a| a.contains("钉住的")));
    }

    #[test]
    fn standbys_keep_their_order_unless_clearly_inverted() {
        let p = DispatchParams::default();
        let f = facts(&[("a", 5, 1), ("b", 5, 1), ("c", 1, 1)]);
        // b、c 只差 3%：不来回改，照上一轮的 c 在前。
        let close = ranking(vec![
            row("a", Some(1.0)),
            row("b", Some(0.62)),
            row("c", Some(0.60)),
        ]);
        let prev = ["c".to_string(), "b".to_string()];
        let out = dispatch(
            &close,
            &f,
            Some("a"),
            &prev,
            None,
            &DispatchMemory::default(),
            &p,
            NOW,
        );
        assert_eq!(out.order, ["a", "c", "b"]);
        // 差得多（1 个样本就够）：纠正。
        let far = ranking(vec![
            row("a", Some(1.0)),
            row("b", Some(0.9)),
            row("c", Some(0.3)),
        ]);
        let out = dispatch(
            &far,
            &f,
            Some("a"),
            &prev,
            None,
            &DispatchMemory::default(),
            &p,
            NOW,
        );
        assert_eq!(out.order, ["a", "b", "c"]);
    }

    #[test]
    fn nobody_is_ever_dropped_and_the_alerts_say_what_is_wrong() {
        let mut x = row("x", Some(1.0));
        x.tripped = true;
        let mut y = row("y", Some(0.5));
        y.tripped = true;
        let r = ranking(vec![x, y]);
        let f = facts(&[("x", 5, 1), ("y", 5, 1)]);
        let out = run(&r, &f, Some("x"), &DispatchMemory::default());
        assert_eq!(
            out.order.len(),
            2,
            "全挂了也一条不少（本机路由全熔断时仍打第一条）"
        );
        assert!(out.alerts.iter().any(|a| a.starts_with("P1")));
        let one = run(
            &ranking(vec![row("a", Some(1.0))]),
            &facts(&[("a", 1, 1)]),
            Some("a"),
            &DispatchMemory::default(),
        );
        assert!(one.alerts.iter().any(|a| a.contains("只有 1 条线")));
        let mut mf = facts(&[("a", 1, 1)]);
        mf.get_mut("a").unwrap().mapping_missing = vec!["relay-x".into()];
        let mapped = run(
            &ranking(vec![row("a", Some(1.0))]),
            &mf,
            Some("a"),
            &DispatchMemory::default(),
        );
        assert!(mapped.alerts.iter().any(|a| a.contains("relay-x")));
        let empty = run(
            &ranking(vec![]),
            &HashMap::new(),
            None,
            &DispatchMemory::default(),
        );
        assert!(empty.order.is_empty());
    }
}
