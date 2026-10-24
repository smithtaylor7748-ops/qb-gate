//! 测活判读：这条线是死是活、多快、什么时候再测、一次测活大概多少钱。**全部是纯函数。**
//!
//! 改写自 sub2guard v2 的 `scheduler/probe.py`（使用者自己的调度外挂，2026-10-12 并入群守望；
//! 中转站重构阶段 2，ATTRIBUTION 的 sub2guard 一节）。规则一条不改，换掉的只有 Sub2API 专有的那几处：
//!
//! | sub2guard | 这里 |
//! |---|---|
//! | 账户 | 线路（`Route`） |
//! | 测活走 Sub2API 的 admin 测试接口 | 两种测活：**免费**的（空请求体，只证明「线通、Key 认」，测不出快慢）、**花钱**的（真打一发很小的请求，默认关） |
//! | 错误分类表 `classify` | 在 `qb_proxy::classify`（同一张表，已经搬过去了）；这里只认它给的键（`Fault::key` 的写法） |
//! | 速度 = 测活总耗时中位数 | 同样，**只看花钱的测活**（免费的那种回的是一个 400，耗时不是生成速度） |
//!
//! # 时间都是毫秒
//!
//! sub2guard 用秒（Python 的 `time.time()`），这里跟 QB Gate 别处一样用毫秒时间戳；常量照换算。
//!
//! # 判读的口径（sub2guard 的规则，原样）
//!
//! - **确定性故障**（余额 / 凭据 / 退役）：一次就判死，重测没意义，一小时后再试。
//! - **暂时性故障**（5xx / 超时 / 过载 / 认不出）：连续两次才判死；之前成功过的话一次只算「等复测确认」。
//!   退避 5 → 10 → 20 → 40 → 60 分钟。
//! - **忙**（限流 / 并发满）：不算故障，只扣速度分 —— 被限流恰恰说明它在服务。
//! - **缺模型**：只记「这条线上的这个模型 6 小时内别测」，不算这条线的故障。
//! - **本机这一侧 / 请求自己的错**：判死活、算失败率时当它不存在。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 一次测活。按时间存在 `probe_attempts` 表里（迁移 7）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct ProbeAttempt {
    pub id: String,
    pub route_id: String,
    pub at_ms: i64,
    /// 测的哪个模型（客户端那头的名字，没按线路映射换过）。免费的那种是空串：空请求体里不带模型，
    /// 而「测通」不能拿来冲掉这个模型「上游没有」的记忆（[`missing_models`]）。
    pub model: String,
    pub ok: bool,
    /// 失败了是哪一类（`qb_proxy::classify::Fault::key` 的写法）。成功是 `None`。
    pub fault: Option<String>,
    /// 总耗时（毫秒）。
    pub total_ms: Option<u64>,
    /// 首字（毫秒）。只有花钱的那种有。
    pub first_ms: Option<u64>,
    /// 花钱的那种（真打一发模型请求）。免费的是 `false`。
    pub paid: bool,
    /// 这一次大概花了多少（元，已按这家站的充值比例折过）。免费的、失败的是 `None`（失败秒回不扣钱）。
    pub cost_lo: Option<f64>,
    pub cost_hi: Option<f64>,
    /// 失败的一句话（不含请求内容、不含 Key）。
    pub error: Option<String>,
}

// ------------------------------------------------------------------ 设置

/// 测活的设置（三个软件共用一份，存在 metadata 的 `probe_settings` 键下）。
///
/// ⛔ **花钱的那种默认关**（交接文档 §1.2，使用者选的）：开了也按每日预算封顶，预算按元算
/// （已乘各站的充值比例）。免费的那种在调度开着时一直跑。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct ProbeSettings {
    /// 花钱测活开没开。
    pub paid: bool,
    /// 每天最多花多少元（花钱测活的申报上界加起来不超过它）。
    pub daily_budget_cny: f64,
    /// 免费探测：调度开着时，多久没走过真实流量的线探一次（分钟）。
    pub free_interval_min: u32,
    /// 花钱测活的复测间隔夹在这两个数之间（分钟），中间按预算摊（[`probe_interval_min`]）。
    pub paid_min_interval_min: u32,
    pub paid_max_interval_min: u32,
}

impl Default for ProbeSettings {
    fn default() -> Self {
        Self {
            paid: false,
            // sub2guard 默认每天 $0.5；这里按元给一个同量级的数。
            daily_budget_cny: 3.0,
            free_interval_min: 10,
            paid_min_interval_min: 10,
            paid_max_interval_min: 120,
        }
    }
}

impl ProbeSettings {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if !(self.daily_budget_cny.is_finite() && self.daily_budget_cny >= 0.0) {
            return Err("每日预算要是个不小于 0 的数".into());
        }
        if self.free_interval_min == 0 || self.paid_min_interval_min == 0 {
            return Err("间隔不能是 0 分钟".into());
        }
        if self.paid_min_interval_min > self.paid_max_interval_min {
            return Err("花钱测活的最短间隔不能比最长的还长".into());
        }
        Ok(())
    }
}

/// 调度做过的一件事（界面「动作日志」）。存在 `scheduler_actions` 表里。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct SchedulerAction {
    pub id: String,
    pub at_ms: i64,
    /// 哪个软件（`Client` 的 kebab-case 写法）。
    pub client: String,
    /// `switch`（换了第一条）· `reorder`（热备改了顺序）· `pin` / `unpin` · `probe-burst`（p1 连着失败，全队列紧急测一遍）。
    pub kind: String,
    pub route_id: Option<String>,
    /// 一句话（纯文本）。
    pub note: String,
}

// ------------------------------------------------------------------ 分类

/// 确定性故障：整把 Key 的状态（sub2guard 实测：重试三次秒回三次同一个错）。一次即判死。
pub const DETERMINISTIC: &[&str] = &["balance", "auth", "retired"];
/// 暂时性故障：可能是号池抽样或上游抖动。连续两次才判死。
pub const TRANSIENT: &[&str] = &["upstream-down", "timeout", "overloaded", "unknown"];
/// 不算故障：被限流恰恰说明它在服务，只扣速度分。
pub const BUSY: &[&str] = &["rate-limit", "concurrency"];
/// 上游不提供这个模型：换下一个模型测，这个模型 6 小时内跳过。
pub const MODEL_MISSING: &str = "model-missing";
/// 判死活、算失败率时都跳过的：缺模型、本机这一侧、请求自己的错、客户端断开、这条线不接这一发。
pub const SKIP: &[&str] = &[
    MODEL_MISSING,
    "infra",
    "client-request",
    "client-abort",
    "not-served",
];

fn is(set: &[&str], key: Option<&str>) -> bool {
    key.is_some_and(|k| set.contains(&k))
}

impl ProbeAttempt {
    /// 这次失败算不算在这条线头上。
    pub fn is_fault(&self) -> bool {
        !self.ok && !is(BUSY, self.fault.as_deref()) && !is(SKIP, self.fault.as_deref())
    }
}

// ------------------------------------------------------------------ 死活

/// 测活出来的状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeState {
    /// 还没测出结果（没测过，或者只被限流过）。
    #[default]
    Untested,
    Up,
    Down,
}

/// 一条线的死活判读。TS 那边叫 `ProbeHealth`（`Health` 太泛，跟健康度窗口那一套分不开）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "ProbeHealth")]
pub struct Health {
    pub state: ProbeState,
    /// 给界面的一句话。纯文本，不写 `**`。
    pub reason: String,
    /// 最近一次算在这条线头上的失败。
    pub fault: Option<String>,
    /// 连续失败次数（跳过限流 / 缺模型）。
    pub faults: u32,
    pub last_ok_ms: Option<i64>,
    /// 最近被限流过。
    pub busy: bool,
    /// [`MISSING_TTL_MS`] 内回过「没有这个模型」、之后也没在它上面成功过的模型（排好序）。
    pub missing_models: Vec<String>,
}

/// 「上游没有这个模型」的结论保留多久。中转站的渠道会临时下线某个模型，6 小时后再试一次。
pub const MISSING_TTL_MS: i64 = 6 * 3600 * 1000;

fn label(key: &str) -> &'static str {
    match key {
        "balance" => "上游余额不足，要充值",
        "auth" => "上游凭据失效 / 无权访问 / 分组被删",
        "retired" => "上游域名已退役",
        "upstream-down" => "上游 5xx 或号池空",
        "timeout" => "超时",
        "overloaded" => "上游过载",
        "rate-limit" => "被限流",
        "concurrency" => "并发已满",
        "model-missing" => "上游不提供这个模型",
        _ => "未知错误",
    }
}

/// 缺模型的那几个（`attempts` 新 → 旧）。
pub fn missing_models(attempts: &[ProbeAttempt], now_ms: i64) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut missing = Vec::new();
    for a in attempts {
        let relevant = a.ok || a.fault.as_deref() == Some(MODEL_MISSING);
        if !relevant || seen.contains(&a.model.as_str()) {
            continue;
        }
        seen.push(&a.model);
        if !a.ok && now_ms - a.at_ms <= MISSING_TTL_MS {
            missing.push(a.model.clone());
        }
    }
    missing.sort();
    missing
}

/// 判死活。`attempts` 按时间新 → 旧。
pub fn assess(attempts: &[ProbeAttempt], now_ms: i64) -> Health {
    let missing = missing_models(attempts, now_ms);
    if attempts.is_empty() {
        return Health {
            reason: "还没测过".into(),
            missing_models: missing,
            ..Default::default()
        };
    }
    let any_ok = attempts.iter().find(|a| a.ok).map(|a| a.at_ms);
    let (mut faults, mut fault, mut busy, mut newest_ok) = (0u32, None::<String>, false, None);
    for a in attempts {
        if a.ok {
            newest_ok = Some(a.at_ms);
            break;
        }
        if is(BUSY, a.fault.as_deref()) {
            busy = busy || faults == 0;
            continue;
        }
        if is(SKIP, a.fault.as_deref()) {
            continue;
        }
        faults += 1;
        if fault.is_none() {
            fault = Some(a.fault.clone().unwrap_or_else(|| "unknown".into()));
        }
    }
    let base = Health {
        fault: fault.clone(),
        faults,
        last_ok_ms: any_ok,
        busy,
        missing_models: missing,
        ..Default::default()
    };
    if faults == 0 {
        return if newest_ok.is_some() {
            Health {
                state: ProbeState::Up,
                reason: if busy {
                    "测活通过，但最近被限流".into()
                } else {
                    "测活通过".into()
                },
                ..base
            }
        } else {
            Health {
                state: ProbeState::Untested,
                reason: if busy {
                    "被限流，还没测出结果".into()
                } else {
                    "还没测出结果".into()
                },
                ..base
            }
        };
    }
    let key = fault.unwrap_or_default();
    let what = label(&key);
    let (state, reason) = if DETERMINISTIC.contains(&key.as_str()) {
        (ProbeState::Down, format!("测活失败：{what}"))
    } else if faults >= 2 {
        (
            ProbeState::Down,
            format!("连续 {faults} 次测活失败：{what}"),
        )
    } else if any_ok.is_some() {
        (
            ProbeState::Up,
            format!("最近一次测活失败（{what}），等复测确认"),
        )
    } else {
        (ProbeState::Down, format!("测活失败：{what}"))
    };
    Health {
        state,
        reason,
        ..base
    }
}

/// 最近 `n` 次（不含跳过的那几类）里没测通的比例。**限流也算** —— 使用者同样要白等。
pub fn fail_rate(attempts: &[ProbeAttempt], n: usize) -> f64 {
    let relevant: Vec<&ProbeAttempt> = attempts
        .iter()
        .filter(|a| !is(SKIP, a.fault.as_deref()))
        .take(n)
        .collect();
    if relevant.is_empty() {
        return 0.0;
    }
    relevant.iter().filter(|a| !a.ok).count() as f64 / relevant.len() as f64
}

/// 速度：这个模型最近 `n` 次**花钱的**成功测活（`window_ms` 内）的 `(总耗时中位数, 首字中位数, 样本数)`。
///
/// 免费的那种不算：它回的是一个 400，耗时是网络往返，不是生成速度。
pub fn speed(
    attempts: &[ProbeAttempt],
    model: Option<&str>,
    now_ms: i64,
    n: usize,
    window_ms: i64,
) -> (Option<u64>, Option<u64>, u32) {
    let oks: Vec<&ProbeAttempt> = attempts
        .iter()
        .filter(|a| {
            a.ok && a.paid
                && model.is_none_or(|m| a.model == m)
                && now_ms - a.at_ms <= window_ms
                && a.total_ms.is_some_and(|t| t > 0)
        })
        .take(n)
        .collect();
    if oks.is_empty() {
        return (None, None, 0);
    }
    let total = median(oks.iter().filter_map(|a| a.total_ms).collect());
    let first = median(oks.iter().filter_map(|a| a.first_ms).collect());
    (total, first, oks.len() as u32)
}

/// 中位数（偶数个取中间两个的平均，跟 Python 的 `statistics.median` 一样）。
fn median(mut v: Vec<u64>) -> Option<u64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2
    })
}

// ------------------------------------------------------------------ 测哪个模型

/// 测活接口只能测文本对话，这些模型测不了。
pub const NON_TEXT_MODEL_MARKERS: &[&str] = &[
    "image",
    "imagine",
    "video",
    "embedding",
    "tts",
    "whisper",
    "audio",
];

/// 这个软件的测活模型优先序：**真实流量里用得最多的排前面**（各条线尽量测同一个模型，速度才能比），
/// 不是文本模型的去掉；使用者指定了就放第一个。
///
/// sub2guard 还看「支持它的账户最多」—— 那靠 Sub2API 的模型映射；QB Gate 的线路没有「支持哪些模型」
/// 这张表（模型映射是改名，不是白名单），所以只按流量排，缺模型的线由 [`pick_model`] 顺延。
pub fn probe_model_order(traffic_models: &[String], override_model: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(m) = override_model.map(str::trim).filter(|m| !m.is_empty()) {
        out.push(m.to_string());
    }
    for m in traffic_models {
        let lower = m.to_ascii_lowercase();
        if m.trim().is_empty() || NON_TEXT_MODEL_MARKERS.iter().any(|x| lower.contains(x)) {
            continue;
        }
        if !out.contains(m) {
            out.push(m.clone());
        }
    }
    out
}

/// 这条线用哪个模型测：按优先序取第一个上游没说没有的。
pub fn pick_model<'a>(order: &'a [String], missing: &[String]) -> Option<&'a str> {
    order
        .iter()
        .find(|m| !missing.contains(m))
        .map(String::as_str)
}

// ------------------------------------------------------------------ 什么时候再测

/// 暂时性故障的退避（分钟）：一轮测活最多两次尝试，第一轮判死 5 分钟后再测，之后逐轮翻倍，封顶一小时。
pub const BACKOFF_MIN: [i64; 5] = [5, 10, 20, 40, 60];
/// 确定性故障 / 缺模型：一小时后再试（充了值可以点「立刻测」）。
pub const DETERMINISTIC_RETRY_MIN: i64 = 60;
/// 还没测出结果（只被限流过）：10 分钟后再试。
pub const BUSY_RETRY_MIN: i64 = 10;

/// 下次该测的时刻（毫秒）。`0` = 现在就测。
pub fn next_due(health: &Health, last_at_ms: Option<i64>, interval_min: f64) -> i64 {
    let Some(last) = last_at_ms else {
        return 0;
    };
    let wait_min = match health.state {
        ProbeState::Down => {
            let key = health.fault.as_deref().unwrap_or("");
            if DETERMINISTIC.contains(&key) || key == MODEL_MISSING {
                DETERMINISTIC_RETRY_MIN as f64
            } else {
                let round = (health.faults / 2).saturating_sub(1) as usize;
                BACKOFF_MIN[round.min(BACKOFF_MIN.len() - 1)] as f64
            }
        }
        ProbeState::Untested => BUSY_RETRY_MIN as f64,
        ProbeState::Up => interval_min,
    };
    last + (wait_min * 60_000.0) as i64
}

/// 按预算算多候选软件里每条活线的复测间隔（分钟）。
///
/// 预算是钱的天花板：每天花钱的成功测活次数 ≤ `daily_cap`。只有一条线的软件先扣掉它们的份额
/// （它们没得挑，只每 `single_interval_min` 分钟确认一次死活），剩下的平摊给多候选软件的线；
/// 夹在 `[min_interval, max_interval]` 里。
pub fn probe_interval_min(
    n_multi: u32,
    n_single: u32,
    daily_cap: u32,
    single_interval_min: f64,
    min_interval: f64,
    max_interval: f64,
) -> f64 {
    if n_multi == 0 {
        return max_interval;
    }
    let single_per_day = f64::from(n_single) * 1440.0 / single_interval_min.max(1.0);
    let cap = f64::from(daily_cap);
    let available = (cap - single_per_day).max(cap * 0.3);
    (1440.0 * f64::from(n_multi) / available).clamp(min_interval, max_interval)
}

// ------------------------------------------------------------------ 花多少钱

/// 一次花钱测活大概要多少 token：输入是那一句 `hi`（加上协议自己的几个 token），输出按请求里设的上限。
pub const PROBE_INPUT_TOKENS: u64 = 12;

/// 一次花钱测活的花费区间（跟单价同一种钱）。`input_per_mtok` / `output_per_mtok` 是每百万 token 的单价，
/// `max_output` 是请求里设的输出上限（Anthropic / Chat 是 1，Responses 的下限是 16）。
///
/// 下限按「只吐 1 个 token」、上限按「吐满上限」算；推理模型的思考 token 也算在输出里、也受上限管。
pub fn estimate_cost(input_per_mtok: f64, output_per_mtok: f64, max_output: u64) -> (f64, f64) {
    let input = PROBE_INPUT_TOKENS as f64 * input_per_mtok / 1e6;
    let lo = input + output_per_mtok / 1e6;
    let hi = input + max_output.max(1) as f64 * output_per_mtok / 1e6;
    (lo, hi)
}

#[cfg(test)]
mod tests {
    //! 改写自 sub2guard v2 的 `tests/scheduler/test_probe.py`（判死活、选模型、复测节奏、预算间隔）。
    //! 错误分类那一组在 `qb_proxy::classify` 的测试里。
    use super::*;

    const NOW: i64 = 100_000_000;

    fn att(at_s: i64, ok: bool, fault: &str, model: &str) -> ProbeAttempt {
        ProbeAttempt {
            at_ms: at_s * 1000,
            model: model.into(),
            ok,
            fault: (!ok).then(|| fault.to_string()),
            total_ms: Some(if ok { 1000 } else { 50 }),
            first_ms: ok.then_some(400),
            paid: true,
            ..Default::default()
        }
    }
    fn ok(at: i64) -> ProbeAttempt {
        att(at, true, "", "m")
    }
    fn bad(at: i64, fault: &str) -> ProbeAttempt {
        att(at, false, fault, "m")
    }

    #[test]
    fn untested_up_and_a_deterministic_fault_once_is_down() {
        assert_eq!(assess(&[], NOW).state, ProbeState::Untested);
        assert_eq!(assess(&[ok(10)], NOW).state, ProbeState::Up);
        let h = assess(&[bad(20, "balance"), ok(10)], NOW);
        assert_eq!(
            (h.state, h.fault.as_deref()),
            (ProbeState::Down, Some("balance"))
        );
    }

    #[test]
    fn a_transient_fault_needs_two_in_a_row_unless_it_never_worked() {
        assert_eq!(
            assess(&[bad(20, "upstream-down"), ok(10)], NOW).state,
            ProbeState::Up,
            "成功过的线一次暂时性失败只算等复测"
        );
        let h = assess(&[bad(30, "upstream-down"), bad(20, "timeout"), ok(10)], NOW);
        assert_eq!((h.state, h.faults), (ProbeState::Down, 2));
        assert_eq!(
            assess(&[bad(20, "upstream-down")], NOW).state,
            ProbeState::Down,
            "从没成功过的一次失败就算挂"
        );
    }

    #[test]
    fn rate_limits_are_not_faults_and_infra_is_ignored() {
        let h = assess(&[bad(30, "rate-limit"), bad(20, "rate-limit"), ok(10)], NOW);
        assert_eq!(h.state, ProbeState::Up);
        assert!(h.busy);
        assert_eq!(
            assess(&[bad(20, "rate-limit")], NOW).state,
            ProbeState::Untested
        );
        // 本机这一侧连不上（路由 / 代理）不能把上游判死。
        let attempts = [bad(40, "infra"), bad(30, "infra"), bad(20, "infra"), ok(10)];
        assert_eq!(assess(&attempts, NOW).state, ProbeState::Up);
    }

    #[test]
    fn a_missing_model_is_skipped_remembered_and_expires() {
        let now_s = NOW / 1000;
        let attempts = [
            att(now_s - 10, false, MODEL_MISSING, "a"),
            att(now_s - 20, true, "", "b"),
        ];
        let h = assess(&attempts, NOW);
        assert_eq!(h.state, ProbeState::Up);
        assert_eq!(h.missing_models, ["a"]);
        let old = [att(
            now_s - MISSING_TTL_MS / 1000 - 5,
            false,
            MODEL_MISSING,
            "a",
        )];
        assert!(missing_models(&old, NOW).is_empty());
        // 后来在这个模型上成功过，就不算缺。
        let back = [
            att(now_s - 5, true, "", "a"),
            att(now_s - 10, false, MODEL_MISSING, "a"),
        ];
        assert!(missing_models(&back, NOW).is_empty());
    }

    #[test]
    fn the_probe_model_follows_traffic_skips_non_text_and_falls_through_missing() {
        let traffic: Vec<String> = ["gpt-6-luna", "gpt-image-2", "gpt-5.6-sol", "gpt-6-luna"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            probe_model_order(&traffic, None),
            ["gpt-6-luna", "gpt-5.6-sol"]
        );
        assert_eq!(
            probe_model_order(&traffic, Some("gpt-5.6-sol")),
            ["gpt-5.6-sol", "gpt-6-luna"]
        );
        let order = probe_model_order(&traffic, None);
        assert_eq!(
            pick_model(&order, &["gpt-6-luna".to_string()]),
            Some("gpt-5.6-sol")
        );
        assert_eq!(pick_model(&order, &order.clone()), None);
    }

    #[test]
    fn retest_timing_follows_the_backoff_table() {
        assert_eq!(
            next_due(&assess(&[], 0), None, 30.0),
            0,
            "没测过 = 现在就测"
        );
        assert_eq!(
            next_due(&assess(&[ok(1000)], 2_000_000), Some(1_000_000), 30.0),
            1_000_000 + 30 * 60_000
        );
        let auth = assess(&[bad(1000, "auth")], 2_000_000);
        assert_eq!(
            next_due(&auth, Some(1_000_000), 30.0),
            1_000_000 + 3_600_000
        );
        let after = |rounds: usize| {
            let attempts: Vec<ProbeAttempt> = (0..rounds * 2)
                .map(|i| bad(1000 - i as i64, "upstream-down"))
                .collect();
            (next_due(&assess(&attempts, 2_000_000), Some(1_000_000), 30.0) - 1_000_000) / 60_000
        };
        assert_eq!(
            [1, 2, 3, 4, 5, 9].map(after),
            [5, 10, 20, 40, 60, 60],
            "退避 5 → 10 → 20 → 40 → 60，封顶一小时"
        );
    }

    #[test]
    fn speed_counts_only_paid_successes_of_that_model() {
        let mut free = ok(50);
        free.paid = false;
        free.total_ms = Some(30);
        let attempts = [
            free,
            ok(40),
            bad(30, "timeout"),
            ok(20),
            att(10, true, "", "other"),
        ];
        let (total, first, n) = speed(&attempts, Some("m"), NOW, 5, i64::MAX);
        assert_eq!((total, first, n), (Some(1000), Some(400), 2));
        assert!((fail_rate(&attempts, 10) - 0.2).abs() < 1e-9);
        assert_eq!(median(vec![1, 2, 3, 10]), Some(2));
    }

    #[test]
    fn paid_probing_is_off_by_default_and_bad_settings_are_refused() {
        let s = ProbeSettings::default();
        assert!(!s.paid, "花钱测活默认关（使用者选的）");
        assert!(s.validate().is_ok());
        let bad = ProbeSettings {
            paid_min_interval_min: 200,
            ..Default::default()
        };
        assert!(bad.validate().is_err());
        let negative = ProbeSettings {
            daily_budget_cny: -1.0,
            ..Default::default()
        };
        assert!(negative.validate().is_err());
        // 老数据缺字段按默认补。
        let old: ProbeSettings = serde_json::from_str(r#"{"paid":true}"#).unwrap();
        assert!(old.paid);
        assert_eq!(old.free_interval_min, 10);
    }

    #[test]
    fn the_budget_spreads_paid_probes_over_the_lines_that_have_a_choice() {
        // 250 次 / 天；3 个单线软件每小时 1 次 = 72；5 条多候选线平摊剩下的 178 → 约 40 分钟。
        let v = probe_interval_min(5, 3, 250, 60.0, 10.0, 120.0);
        assert!((v - 1440.0 * 5.0 / 178.0).abs() < 1e-9);
        assert_eq!(probe_interval_min(1, 0, 250, 60.0, 10.0, 120.0), 10.0);
        assert_eq!(probe_interval_min(100, 0, 250, 60.0, 10.0, 120.0), 120.0);
        assert_eq!(probe_interval_min(0, 3, 250, 60.0, 10.0, 120.0), 120.0);
        let (lo, hi) = estimate_cost(5.0, 25.0, 16);
        assert!(
            lo < hi && hi < 0.001,
            "一次测活该是千分之一美元以下：{lo}..{hi}"
        );
    }
}
