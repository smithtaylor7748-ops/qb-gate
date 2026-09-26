//! 槽位的用量与恢复时间。
//!
//! # 数据从哪来：**官方客户端自己写在本机的文件**，两层来源三条路径
//!
//! 零网络请求、零接口调用。面板只是把官方客户端已经落盘的东西读出来显示。
//!
//! | 层 | 文件 | 内容 |
//! |---|---|---|
//! | 实时用量 · 当前槽位 | `%APPDATA%\Claude\plan-usage-history.json` | 桌面端每约 15 分钟追加一条 `{t, org, u:{fh, sd}}` |
//! | 实时用量 · 其余槽位 | `%APPDATA%\Claude-<标签>\plan-usage-history.json` | 同上，那个槽位自己那一份 |
//! | 恢复时刻 | 槽位的 `.claude.json` → `cachedUsageUtilization` | Claude Code 写下的 `resets_at` + `fetchedAtMs` |
//!
//! `fh` = 五小时窗口已用百分比，`sd` = 七天已用百分比。
//! 归属靠 `samples[].org` 对上槽位 `oauthAccount.organizationUuid` ——
//! 实测两个槽位的 org 不同，不会串。
//!
//! ## ⛔ 实时用量是两条路径，不是一条
//!
//! `%APPDATA%\Claude` 是个**联结点**，只指向当前槽位的桌面端资料目录；
//! 别的槽位的历史在它自己的 `Claude-<标签>` 里。0.19.2 之前这里只读前者，
//! 于是非当前槽位按 org 一条实时样本都匹配不上，退回十几天前的
//! `cachedUsageUtilization`，界面打出「快照已过期」—— 而那个槽位的
//! `Claude-<标签>\plan-usage-history.json` 十二分钟前刚写过。
//! 症状是「数据明明是新的，面板说它过期」，根因是没去那个目录读。
//!
//! 两条路径对**当前**槽位会解析到同一个文件，所以合并之后必须按
//! `(t, org)` 去重，见 [`dedupe`]。
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
use std::path::{Path, PathBuf};
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
    /// 这份读数之后窗口已经重置过了：已知的某个重置时刻夹在「读数时刻」与「现在」之间（2026-09-25）。
    ///
    /// 那时 `used` 是重置**之前**的数，界面不许把它当当前值显示。原来只按读数的年龄判过期 ——
    /// 缓存 09:00 记下「五小时用了 92%、09:40 重置」，10:30 再看，读数才 90 分钟旧，
    /// 界面照样画「剩 8%」而且标红、重置时刻写「未知」，其实 50 分钟前就重置了。
    pub reset_passed: bool,
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
/// `histories` 是这个槽位的桌面端历史可能在的几份文件，由
/// `AccountRoots::history_files` 给出。**不是一份** —— 理由见文件头。
///
/// `org` 是本槽位 `oauthAccount.organizationUuid`；没有就取不到桌面端那一层
/// （匹配不上 = 不敢认，宁可只用缓存）。org 过滤这道仍然留着：
/// `Claude-<标签>` 里也可能混着别的账户在那个目录下用过时留下的样本，
/// 实测 `Claude-main` 里就有 121 条属于另一个 org 的。
pub fn for_slot(histories: &[PathBuf], slot_dir: &Path, org: Option<&str>) -> Option<SlotUsage> {
    let now = chrono::Utc::now().timestamp_millis();
    let samples = read_histories(histories)
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
        let cached_five = cache
            .as_ref()
            .and_then(|c| c.five_hour_resets_at.as_deref());
        let cached_week = cache
            .as_ref()
            .and_then(|c| c.seven_day_resets_at.as_deref());
        Some(SlotUsage {
            source: UsageSource::Desktop,
            measured_at: rfc3339(l.t),
            age_minutes: (now - l.t) / 60_000,
            five_hour: Some(UsageWindow {
                used: l.fh.clamp(0, 100) as u8,
                // 桌面端那份只有用量，没有重置时刻。先拿缓存里没过期的实测值，
                // 拿不到再从样本的断崖下跌反推。
                resets_at: fresh(cached_five, now).or_else(|| infer_five_hour_reset(&samples, now)),
                estimated: fresh(cached_five, now).is_none(),
                // 缓存记下的、或样本推出来的重置时刻，只要有一个落在这条样本之后、现在之前，
                // 这条样本就是重置前的数。
                reset_passed: passed_between(cached_five, l.t, now)
                    || inferred_reset_ms(&samples).is_some_and(|r| r > l.t && r <= now),
            }),
            // 七天窗口**不推算**。样本里根本见不到它重置（实测一路 76→79），
            // 没有断崖就没有可靠的起点。拿不到就如实说不知道。
            // 样本里没有 `sd` 就不画这一条 —— 原来按 0 算，画成一条实心的「剩 100%」。
            seven_day: l.sd.map(|sd| UsageWindow {
                used: sd.clamp(0, 100) as u8,
                resets_at: fresh(cached_week, now),
                estimated: false,
                reset_passed: passed_between(cached_week, l.t, now),
            }),
        })
    } else {
        let c = cache?;
        let at = c.fetched_at_ms;
        Some(SlotUsage {
            source: UsageSource::Cache,
            measured_at: rfc3339(at),
            age_minutes: (now - at) / 60_000,
            five_hour: c.five_hour.map(|used| UsageWindow {
                used,
                resets_at: fresh(c.five_hour_resets_at.as_deref(), now),
                estimated: false,
                reset_passed: passed_between(c.five_hour_resets_at.as_deref(), at, now),
            }),
            seven_day: c.seven_day.map(|used| UsageWindow {
                used,
                resets_at: fresh(c.seven_day_resets_at.as_deref(), now),
                estimated: false,
                reset_passed: passed_between(c.seven_day_resets_at.as_deref(), at, now),
            }),
        })
    }
}

/// 这个重置时刻落在「读数时刻」之后、「现在」之前吗 —— 是的话那份读数是重置前的数。
///
/// 早于读数时刻的重置时刻说明不了什么（那是上一个窗口的，读数本来就是新窗口里的）。
fn passed_between(iso: Option<&str>, measured_ms: i64, now: i64) -> bool {
    iso.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.timestamp_millis())
        .is_some_and(|r| r > measured_ms && r <= now)
}

// ------------------------------------------------------------ 桌面端历史

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub t: i64,
    pub org: String,
    /// 五小时窗口已用百分比。
    pub fh: i64,
    /// 七天窗口已用百分比。**可以没有** —— 没有就是没有，不当成 0（0 在界面上是「剩 100%」）。
    pub sd: Option<i64>,
}

/// 百分比读成整数：小数（`12.5`）四舍五入，不是数就是 `None`。
///
/// 原来用 `as_i64()`，JSON 里一个 `12.5` 就读不出来 —— 整条样本被丢掉，缓存那一格变成没有。
fn percent_of(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| {
        v.as_f64()
            .filter(|x| x.is_finite())
            .map(|x| x.round() as i64)
    })
}

/// 读若干份历史文件并合并成一条时间序列。
///
/// 为什么是「若干份」而不是一份：见文件头「实时用量是两条路径」。
/// 路径由 `AccountRoots::history_files` 拼好传进来 ——
/// 「Claude 的目录名怎么拼」全项目只有那一处。
///
/// 读不出来的文件直接跳过（不存在、没权限、正在写）：这里的每一份都是
/// **可选**的来源，少一份不是错误，只是少一层数据。
fn read_histories(paths: &[PathBuf]) -> Vec<Sample> {
    let mut all: Vec<Sample> = Vec::new();
    for p in paths {
        if let Ok(text) = std::fs::read_to_string(p) {
            all.extend(parse_history(&text));
        }
    }
    dedupe(all)
}

/// 按 `(t, org)` 去重，并重新按时间升序排好。
///
/// 当前槽位的两条路径落在同一个文件上，不去重就是每条样本来两份。
/// 调用方靠 `last()` 拿最新一条，`infer_five_hour_reset` 按相邻样本的
/// 差值找断崖 —— 成对的重复样本会在序列里插进一堆差值为 0 的邻居，
/// 虽然造不出假的重置时刻，却让这条序列不再是「一条样本一个时刻」，
/// 后面任何按相邻关系算的东西都得先怀疑它。
pub fn dedupe(mut all: Vec<Sample>) -> Vec<Sample> {
    all.sort_by(|a, b| a.t.cmp(&b.t).then_with(|| a.org.cmp(&b.org)));
    all.dedup_by(|a, b| a.t == b.t && a.org == b.org);
    all
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
                fh: percent_of(u.get("fh")?)?,
                sd: u.get("sd").and_then(percent_of),
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
    let reset = inferred_reset_ms(samples)?;
    // 推出来的重置时刻已经过去 = 之后又翻过篇了，只是没有样本记下来
    // （桌面端那段时间没开）。这种情况不猜，如实回 None。
    (reset > now).then(|| rfc3339(reset))
}

/// 最近一次断崖下跌推出来的那个重置时刻（毫秒），**过没过去都给** —— 判「读数之后翻过篇没有」要用它。
fn inferred_reset_ms(samples: &[Sample]) -> Option<i64> {
    // 从后往前找最近一次下跌。
    let drop = samples
        .windows(2)
        .rposition(|w| w[0].fh - w[1].fh >= RESET_DROP)?;
    let before = &samples[drop];
    let after = &samples[drop + 1];
    Some((before.t + after.t) / 2 + FIVE_HOUR_MS)
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
            .and_then(percent_of)
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
            sd: Some(sd),
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
            "accountUuid":"00000000",
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

    // ------------------------------------------------ 两条路径（0.20.0）

    /// 一个临时目录。⛔ 单测不许碰 `%LOCALAPPDATA%\ClaudeIpGate\`。
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let d = std::env::temp_dir().join(format!(
                "qbgate-usage-{tag}-{}-{}",
                std::process::id(),
                chrono::Local::now().format("%H%M%S%f")
            ));
            std::fs::create_dir_all(&d).unwrap();
            Self(d)
        }

        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
            p
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn history_json(org: &str, t: i64, fh: i64, sd: i64) -> String {
        format!(
            r#"{{"version":2,"samples":[{{"t":{t},"org":"{org}","u":{{"fh":{fh},"sd":{sd}}}}}]}}"#
        )
    }

    /// 实机 2026-09-16 的形态：`Claude` 联结点里只有当前槽位那个 org 的样本，
    /// 而 `main` 槽位自己的历史在 `Claude-main` 里，十二分钟前刚写过。
    ///
    /// 只读联结点那一份的话，`main` 一条实时样本都匹配不上，退回九月初的
    /// 缓存，界面打「快照已过期」—— 数据是新的，只是没人去那个目录读。
    #[test]
    fn a_slot_reads_its_own_desktop_history_not_just_the_junction() {
        let d = Dir::new("own-history");
        let now = chrono::Utc::now().timestamp_millis();
        // 联结点那份：别的槽位（org-b）的样本，更新。
        let junction = d.write(
            "Roaming/Claude/plan-usage-history.json",
            &history_json("org-b", now - 5 * MIN, 44, 44),
        );
        // 这个槽位自己那份：org-a，十二分钟前。
        let own = d.write(
            "Roaming/Claude-main/plan-usage-history.json",
            &history_json("org-a", now - 12 * MIN, 7, 58),
        );
        // 槽位缓存停在很久以前 —— 只读联结点时就是它被显示出来的。
        let slot = d.0.join("claude-profile-main");
        std::fs::create_dir_all(&slot).unwrap();
        std::fs::write(
            slot.join(".claude.json"),
            format!(
                r#"{{"cachedUsageUtilization":{{"fetchedAtMs":{},"utilization":{{
                    "five_hour":{{"utilization":0}},"seven_day":{{"utilization":10}}}}}}}}"#,
                now - 14 * 24 * 60 * MIN
            ),
        )
        .unwrap();

        let got = for_slot(&[junction, own], &slot, Some("org-a")).expect("两源都有，不该是 None");
        assert_eq!(
            got.source,
            UsageSource::Desktop,
            "自己那份历史是新的，不该退回十几天前的缓存"
        );
        assert!(got.age_minutes <= 13, "age={} 分钟", got.age_minutes);
        assert_eq!(got.seven_day.as_ref().map(|w| w.used), Some(58));
    }

    /// 当前槽位的两条路径会解析到同一个文件（`Claude` 是指向
    /// `Claude-<标签>` 的联结点），不去重就是每条样本来两份。
    #[test]
    fn the_same_history_read_twice_is_counted_once() {
        let d = Dir::new("dedupe");
        let text = r#"{"version":2,"samples":[
            {"t":100,"org":"a","u":{"fh":10,"sd":1}},
            {"t":200,"org":"a","u":{"fh":20,"sd":2}}
        ]}"#;
        let one = d.write("Roaming/Claude/plan-usage-history.json", text);
        let two = d.write("Roaming/Claude-main/plan-usage-history.json", text);
        let got = read_histories(&[one, two]);
        assert_eq!(
            got.iter().map(|x| x.t).collect::<Vec<_>>(),
            vec![100, 200],
            "同一条样本读两遍只能算一条"
        );
    }

    /// 不同 org 同一时刻是两条不同的样本，不许被去重当成一条。
    #[test]
    fn dedupe_keys_on_org_too() {
        let got = dedupe(vec![
            Sample {
                t: 100,
                org: "a".into(),
                fh: 1,
                sd: Some(1),
            },
            Sample {
                t: 100,
                org: "b".into(),
                fh: 2,
                sd: Some(2),
            },
            Sample {
                t: 100,
                org: "a".into(),
                fh: 1,
                sd: Some(1),
            },
        ]);
        assert_eq!(got.len(), 2);
    }

    /// 2026-09-25：缓存 09:00 记下「五小时 92%、09:40 重置」，10:30 再看 —— 读数才 90 分钟旧，
    /// 原来照样画「剩 8%」。重置时刻夹在读数与现在之间，读数就是重置前的数。
    #[test]
    fn a_cached_reading_from_before_a_known_reset_is_flagged() {
        let d = Dir::new("reset-passed");
        let now = chrono::Utc::now().timestamp_millis();
        let iso = rfc3339;
        let slot = d.0.join("claude-profile-main");
        std::fs::create_dir_all(&slot).unwrap();
        std::fs::write(
            slot.join(".claude.json"),
            format!(
                r#"{{"cachedUsageUtilization":{{"fetchedAtMs":{},"utilization":{{
                    "five_hour":{{"utilization":92,"resets_at":"{}"}},
                    "seven_day":{{"utilization":40,"resets_at":"{}"}}}}}}}}"#,
                now - 90 * MIN,
                iso(now - 50 * MIN),
                iso(now + 3 * 24 * 60 * MIN)
            ),
        )
        .unwrap();
        let got = for_slot(&[], &slot, Some("org-a")).expect("缓存在");
        let five = got.five_hour.unwrap();
        assert!(five.reset_passed, "50 分钟前就重置了，92% 不是当前值");
        assert_eq!(
            five.resets_at, None,
            "过去的重置时刻照旧不显示成「恢复时间」"
        );
        let week = got.seven_day.unwrap();
        assert!(!week.reset_passed, "七天窗口还没到点");
    }

    /// 桌面端样本在前、缓存里的重置时刻在它之后且已过去 —— 同样是重置前的数；
    /// 反过来样本在重置之后，就是新窗口里的读数，不许误标。
    #[test]
    fn a_live_sample_is_stale_only_when_a_reset_came_after_it() {
        let now = chrono::Utc::now().timestamp_millis();
        let reset = rfc3339(now - 30 * MIN);
        let later = rfc3339(now + MIN);
        assert!(passed_between(Some(reset.as_str()), now - 60 * MIN, now));
        assert!(!passed_between(Some(reset.as_str()), now - 10 * MIN, now));
        assert!(!passed_between(Some(later.as_str()), now - 60 * MIN, now));
        assert!(!passed_between(None, now - 60 * MIN, now));
        // 样本里那次断崖推出来的重置时刻也算：推出来已经过去了，就说明最后一条样本之后又翻过篇。
        let samples = vec![s(now - 400 * MIN, 90, 9), s(now - 390 * MIN, 5, 9)];
        assert!(inferred_reset_ms(&samples).is_some_and(|r| r <= now));
    }

    /// 样本里没有 `sd`：七天那条不画（原来按 0 算，画成实心的「剩 100%」）；小数照样读得出来。
    #[test]
    fn a_missing_week_is_absent_not_zero_and_fractions_are_read() {
        let text = r#"{"version":2,"samples":[
            {"t":100,"org":"a","u":{"fh":12.5}},
            {"t":200,"org":"a","u":{"fh":30,"sd":7.4}}
        ]}"#;
        let got = parse_history(text);
        assert_eq!(got.len(), 2, "小数不许让整条样本被丢掉");
        assert_eq!(got[0].fh, 13);
        assert_eq!(got[0].sd, None);
        assert_eq!(got[1].sd, Some(7));

        let d = Dir::new("no-sd");
        let now = chrono::Utc::now().timestamp_millis();
        let hist = d.write(
            "Roaming/Claude-main/plan-usage-history.json",
            &format!(
                r#"{{"version":2,"samples":[{{"t":{},"org":"a","u":{{"fh":20}}}}]}}"#,
                now - MIN
            ),
        );
        let slot = d.0.join("claude-profile-main");
        std::fs::create_dir_all(&slot).unwrap();
        let got = for_slot(&[hist], &slot, Some("a")).unwrap();
        assert!(got.five_hour.is_some());
        assert!(got.seven_day.is_none(), "没有七天的数就不画这一条");
    }

    /// 读不出来的路径是「少一层来源」，不是错误 —— 另一份照样要读出来。
    #[test]
    fn a_missing_history_file_is_skipped_not_fatal() {
        let d = Dir::new("missing");
        let there = d.write(
            "Roaming/Claude-main/plan-usage-history.json",
            &history_json("a", 100, 10, 1),
        );
        let gone = d.0.join("Roaming/Claude/plan-usage-history.json");
        assert_eq!(read_histories(&[gone, there]).len(), 1);
    }
}
