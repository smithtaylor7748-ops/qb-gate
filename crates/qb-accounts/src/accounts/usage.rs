//! 槽位的用量与恢复时间。
//!
//! # 数据从哪来：两份**官方客户端自己写在本机的文件**
//!
//! 零网络请求、零接口调用。面板只是把官方客户端已经落盘的东西读出来显示。
//!
//! | 层 | 文件 | 内容 |
//! |---|---|---|
//! | 实时用量 | `%APPDATA%\Claude\plan-usage-history.json` | 桌面端每约 15 分钟追加一条 `{t, org, u:{fh, sd}}` |
//! | 恢复时刻 | 槽位的 `.claude.json` → `cachedUsageUtilization` | Claude Code 写下的 `resets_at` + `fetchedAtMs` |
//!
//! `fh` = 五小时窗口已用百分比，`sd` = 七天已用百分比。
//! 归属靠 `samples[].org` 对上槽位 `oauthAccount.organizationUuid` ——
//! 实测两个槽位的 org 不同，不会串。
//!
//! # 三条物理限制，界面必须如实呈现
//!
//! 1. **只有桌面端当前登录的那个账户有实时数据。** 没登录的账户不可能知道
//!    它此刻的用量 —— 这是物理限制，不是实现缺陷。别的槽位只有缓存。
//! 2. **样本只在桌面端运行时才写。** 实测采样间隔中位 15 分，最大 195 分
//!    （那段是桌面端没开）。关掉之后读数就冻在最后一条，
//!    期间窗口到点重置**没有任何样本能确认**。
//! 3. **缓存经常旧甚至没有。** 实测两个槽位的 `cachedUsageUtilization` 都停在
//!    九月初，`resets_at` 早就过去了。过期的一律标出来，**不当成当前值**。
//!
//! 两源都没有就什么都不显示 —— 沿用 `EXPIRY_CAVEAT` 的口径：
//! 宁可说不知道，也不给假读数。
//!
//! # 采样节奏不是整点网格
//!
//! 曾经以为是固定落在 `:07 :22 :37 :52`。实测推翻了：
//! `13:07:38 → 13:25:04` 隔了 17.4 分钟，`16:55:04` 也不在那个网格上。
//! 它是「桌面端启动之后每约 15 分钟一次」，相位会漂。
//! **所以不许预测「下次更新 12:52」**，只能说「约 15 分钟一次」。

use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

/// 五小时窗口的长度。
const FIVE_HOUR_MS: i64 = 5 * 60 * 60 * 1000;

/// 判定「窗口重置了」的跌幅门槛（百分点）。
///
/// 窗口内用量只增不减，所以任何明显的下跌都只能是重置。取 20 是为了
/// 避开服务端口径微调造成的一两个点的抖动。实测那次重置是 100 → 12。
const RESET_DROP: i64 = 20;

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct UsageWindow {
    /// 已用百分比 0–100。剩余额度 = 100 − 这个数。
    pub used: u8,
    /// 窗口重置时刻，RFC3339。`None` = 既没实测也推算不出来。
    pub resets_at: Option<String>,
    /// `resets_at` 是从样本里推出来的，不是官方客户端写下的。
    /// 界面上**必须**标出来 —— 把推算值伪装成实测值是这套显示最容易犯的错。
    pub estimated: bool,
}

/// 这份读数是从哪儿来的。界面上要标，两个槽位的来源经常不一样。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum UsageSource {
    /// `plan-usage-history.json`，桌面端写的。
    Desktop,
    /// 槽位 `.claude.json` 里的 `cachedUsageUtilization`，Claude Code 写的。
    Cache,
}

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct SlotUsage {
    pub source: UsageSource,
    /// 这份读数是什么时候测的，RFC3339。
    pub measured_at: String,
    /// 落后了多少分钟。界面拿它决定要不要提醒「这不是当前值」。
    ///
    /// 同 `Slot::cli_days_left`：JSON 上是普通数字，不是 bigint。
    #[ts(type = "number")]
    pub age_minutes: i64,
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
}

// ------------------------------------------------------------------ 入口

/// 读一个槽位的用量。两源取时间戳大的那个。
///
/// `org` 是本槽位 `oauthAccount.organizationUuid`；没有就取不到桌面端那一层
/// （匹配不上 = 不敢认，宁可只用缓存）。
pub fn for_slot(appdata: Option<&Path>, slot_dir: &Path, org: Option<&str>) -> Option<SlotUsage> {
    let now = chrono::Utc::now().timestamp_millis();
    let samples = appdata
        .map(read_history)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| org.is_some_and(|o| o == s.org))
        .collect::<Vec<_>>();

    let cache = read_cache(slot_dir);
    let live = samples.last();

    // 两源都没有 → 什么都不显示。给一个「0%」比不给更糟：
    // 那是在替一个根本没读到的数字打包票。
    let use_live = match (live, &cache) {
        (None, None) => return None,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(l), Some(c)) => l.t >= c.fetched_at_ms,
    };

    if use_live {
        let l = live?;
        Some(SlotUsage {
            source: UsageSource::Desktop,
            measured_at: rfc3339(l.t),
            age_minutes: (now - l.t) / 60_000,
            five_hour: Some(UsageWindow {
                used: l.fh.clamp(0, 100) as u8,
                // 桌面端那份只有用量，没有重置时刻。先拿缓存里没过期的实测值，
                // 拿不到再从样本的断崖下跌反推。
                resets_at: cache
                    .as_ref()
                    .and_then(|c| fresh(c.five_hour_resets_at.as_deref(), now))
                    .or_else(|| infer_five_hour_reset(&samples, now)),
                estimated: cache
                    .as_ref()
                    .and_then(|c| fresh(c.five_hour_resets_at.as_deref(), now))
                    .is_none(),
            }),
            seven_day: Some(UsageWindow {
                used: l.sd.clamp(0, 100) as u8,
                // 七天窗口**不推算**。样本里根本见不到它重置（实测一路 76→79），
                // 没有断崖就没有可靠的起点。拿不到就如实说不知道。
                resets_at: cache
                    .as_ref()
                    .and_then(|c| fresh(c.seven_day_resets_at.as_deref(), now)),
                estimated: false,
            }),
        })
    } else {
        let c = cache?;
        Some(SlotUsage {
            source: UsageSource::Cache,
            measured_at: rfc3339(c.fetched_at_ms),
            age_minutes: (now - c.fetched_at_ms) / 60_000,
            five_hour: c.five_hour.map(|used| UsageWindow {
                used,
                resets_at: fresh(c.five_hour_resets_at.as_deref(), now),
                estimated: false,
            }),
            seven_day: c.seven_day.map(|used| UsageWindow {
                used,
                resets_at: fresh(c.seven_day_resets_at.as_deref(), now),
                estimated: false,
            }),
        })
    }
}

// ------------------------------------------------------------ 桌面端历史

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub t: i64,
    pub org: String,
    /// 五小时窗口已用百分比。
    pub fh: i64,
    /// 七天窗口已用百分比。
    pub sd: i64,
}

fn read_history(appdata: &Path) -> Vec<Sample> {
    let text = match std::fs::read_to_string(appdata.join("Claude/plan-usage-history.json")) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    parse_history(&text)
}

/// 解析出来的样本**按时间升序**，调用方靠 `last()` 拿最新的一条。
///
/// 文件本身是追加写的，正常就是有序的；但这份显示的正确性不该建立在
/// 「写文件的人一直有序」这个假设上 —— 乱序一次就会把一条旧读数当成最新的。
pub fn parse_history(text: &str) -> Vec<Sample> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(arr) = v.get("samples").and_then(|s| s.as_array()) else {
        return Vec::new();
    };
    let mut out: Vec<Sample> = arr
        .iter()
        .filter_map(|s| {
            let u = s.get("u")?;
            Some(Sample {
                t: s.get("t")?.as_i64()?,
                org: s.get("org")?.as_str()?.to_string(),
                fh: u.get("fh")?.as_i64()?,
                sd: u.get("sd").and_then(|v| v.as_i64()).unwrap_or(0),
            })
        })
        .collect();
    out.sort_by_key(|s| s.t);
    out
}

/// 从断崖下跌反推当前五小时窗口的重置时刻。
///
/// 窗口内用量只增不减，所以 `fh` 明显下跌只可能是窗口翻篇了。翻篇后第一条
/// 样本的时刻落在新窗口**开始之后**，所以拿它 + 5 小时是个偏晚的上界；
/// 取跌前那条与跌后那条的中点，把误差压到半个采样间隔（约 7 分钟）。
///
/// 推出来的值**一律标 `estimated`**。
pub fn infer_five_hour_reset(samples: &[Sample], now: i64) -> Option<String> {
    // 从后往前找最近一次下跌。
    let drop = samples
        .windows(2)
        .rposition(|w| w[0].fh - w[1].fh >= RESET_DROP)?;
    let before = &samples[drop];
    let after = &samples[drop + 1];
    let start = (before.t + after.t) / 2;
    let reset = start + FIVE_HOUR_MS;
    // 推出来的重置时刻已经过去 = 之后又翻过篇了，只是没有样本记下来
    // （桌面端那段时间没开）。这种情况不猜，如实回 None。
    (reset > now).then(|| rfc3339(reset))
}

// -------------------------------------------------------------- 槽位缓存

#[derive(Debug, Clone, PartialEq)]
pub struct Cached {
    pub fetched_at_ms: i64,
    pub five_hour: Option<u8>,
    pub seven_day: Option<u8>,
    pub five_hour_resets_at: Option<String>,
    pub seven_day_resets_at: Option<String>,
}

fn read_cache(slot_dir: &Path) -> Option<Cached> {
    parse_cache(&std::fs::read_to_string(slot_dir.join(".claude.json")).ok()?)
}

pub fn parse_cache(text: &str) -> Option<Cached> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let c = v.get("cachedUsageUtilization")?;
    let u = c.get("utilization")?;
    let win = |name: &str| u.get(name);
    let pct = |name: &str| {
        win(name)
            .and_then(|w| w.get("utilization"))
            .and_then(|x| x.as_i64())
            .map(|x| x.clamp(0, 100) as u8)
    };
    let reset = |name: &str| {
        win(name)
            .and_then(|w| w.get("resets_at"))
            .and_then(|x| x.as_str())
            .map(String::from)
    };
    Some(Cached {
        fetched_at_ms: c.get("fetchedAtMs")?.as_i64()?,
        five_hour: pct("five_hour"),
        seven_day: pct("seven_day"),
        five_hour_resets_at: reset("five_hour"),
        seven_day_resets_at: reset("seven_day"),
    })
}

// ------------------------------------------------------------------ 工具

/// 只留还没到点的重置时刻。
///
/// 已经过去的 `resets_at` 说明这份缓存跨过了至少一个窗口，它记的用量
/// 肯定不是当前值。**把过去的时刻显示成「恢复时间」是彻头彻尾的误导** ——
/// 使用者会以为还要等，其实早就重置了。
fn fresh(iso: Option<&str>, now: i64) -> Option<String> {
    let s = iso?;
    let t = chrono::DateTime::parse_from_rfc3339(s).ok()?;
    (t.timestamp_millis() > now).then(|| s.to_string())
}

fn rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: i64, fh: i64, sd: i64) -> Sample {
        Sample {
            t,
            org: "org-a".into(),
            fh,
            sd,
        }
    }

    const MIN: i64 = 60_000;

    #[test]
    fn history_comes_back_sorted_even_when_the_file_is_not() {
        let text = r#"{"version":2,"samples":[
            {"t":300,"org":"a","u":{"fh":30,"sd":3}},
            {"t":100,"org":"a","u":{"fh":10,"sd":1}},
            {"t":200,"org":"b","u":{"fh":20,"sd":2}}
        ]}"#;
        let got = parse_history(text);
        assert_eq!(
            got.iter().map(|x| x.t).collect::<Vec<_>>(),
            vec![100, 200, 300],
            "乱序一次就会把旧读数当成最新的"
        );
    }

    #[test]
    fn history_skips_entries_missing_the_fields_it_needs() {
        let text = r#"{"version":2,"samples":[
            {"t":100,"org":"a","u":{"fh":10,"sd":1}},
            {"t":200,"org":"a"},
            {"t":300,"u":{"fh":30,"sd":3}},
            {"org":"a","u":{"fh":40,"sd":4}}
        ]}"#;
        assert_eq!(parse_history(text).len(), 1);
    }

    /// 实测那一次：`13:40 fh=100` → `16:55 fh=12`，中间桌面端没开。
    #[test]
    fn reset_is_inferred_from_the_cliff_between_two_samples() {
        let base = 1_788_000_000_000;
        let samples = vec![
            s(base, 76, 76),
            s(base + 15 * MIN, 100, 78),
            s(base + 210 * MIN, 12, 79),
        ];
        // 窗口起点取两条的中点：base + (15 + 210) / 2 = base + 112.5 分钟。
        let want = base + 112 * MIN + 30_000 + FIVE_HOUR_MS;
        let now = base + 220 * MIN;
        let got = infer_five_hour_reset(&samples, now).expect("有断崖就该推得出来");
        assert_eq!(got, rfc3339(want));
    }

    #[test]
    fn no_cliff_means_no_guess() {
        let base = 1_788_000_000_000;
        let samples = vec![s(base, 10, 1), s(base + 15 * MIN, 24, 2)];
        assert_eq!(infer_five_hour_reset(&samples, base + 20 * MIN), None);
    }

    /// 一两个点的抖动不是重置。判成重置的话，恢复时间会被往后推五小时。
    #[test]
    fn small_dips_are_not_treated_as_a_reset() {
        let base = 1_788_000_000_000;
        let samples = vec![s(base, 40, 5), s(base + 15 * MIN, 38, 5)];
        assert_eq!(infer_five_hour_reset(&samples, base + 20 * MIN), None);
    }

    /// 推出来的重置时刻已经过去 = 之后又翻过篇，只是没样本记下来。不猜。
    #[test]
    fn an_inferred_reset_already_in_the_past_is_dropped() {
        let base = 1_788_000_000_000;
        let samples = vec![s(base, 90, 9), s(base + 15 * MIN, 5, 9)];
        let now = base + 6 * 60 * MIN;
        assert_eq!(infer_five_hour_reset(&samples, now), None);
    }

    #[test]
    fn cache_parses_both_windows() {
        let text = r#"{"cachedUsageUtilization":{"fetchedAtMs":1788359225310,
            "accountUuid":"b418b186",
            "utilization":{
              "five_hour":{"utilization":25,"resets_at":"2026-09-02T18:49:59.597651+00:00"},
              "seven_day":{"utilization":13,"resets_at":"2026-09-06T17:59:59.597673+00:00"}}}}"#;
        let c = parse_cache(text).expect("这是实机上真实的形状");
        assert_eq!(c.fetched_at_ms, 1_788_359_225_310);
        assert_eq!(c.five_hour, Some(25));
        assert_eq!(c.seven_day, Some(13));
        assert!(c.five_hour_resets_at.is_some());
    }

    #[test]
    fn cache_without_the_field_is_none() {
        assert_eq!(parse_cache(r#"{"oauthAccount":{}}"#), None);
    }

    /// 已经过去的 `resets_at` 不许当成「恢复时间」显示 ——
    /// 使用者会以为还要等，其实早就重置了。
    #[test]
    fn a_reset_time_in_the_past_is_dropped() {
        let now = chrono::Utc::now().timestamp_millis();
        assert_eq!(fresh(Some("2020-01-01T00:00:00+00:00"), now), None);
        assert!(fresh(Some("2099-01-01T00:00:00+00:00"), now).is_some());
    }

    #[test]
    fn garbage_does_not_panic() {
        assert!(parse_history("not json").is_empty());
        assert!(parse_history(r#"{"samples":"nope"}"#).is_empty());
        assert_eq!(parse_cache("not json"), None);
    }
}
