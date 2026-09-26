//! Codex 自己写在本机的额度窗口（0.32.0）。
//!
//! # 数据从哪来：**官方客户端自己写在本机的文件**，零网络请求
//!
//! Codex 每发完一轮就往会话记录里追加一条 `token_count` 事件，那条事件里除了
//! `info.total_token_usage`（[`super::usage`] 读的那部分）还挂着一个 `rate_limits`：
//!
//! ```json
//! "rate_limits":{"limit_id":"codex",
//!   "primary":{"used_percent":2.0,"window_minutes":10080,"resets_at":1790247800},
//!   "secondary":null,
//!   "credits":{"has_credits":false,"unlimited":false,"balance":null},
//!   "plan_type":null}
//! ```
//!
//! 跟 Claude 读 `plan-usage-history.json` 是同一条口径：只读官方客户端落盘的东西，
//! 不调任何接口。`window_minutes` 决定这是哪个窗口（`300` = 五小时、`10080` = 七天）。
//!
//! # ⛔ 三条踩出来的规矩
//!
//! 1. **要的是「最新一条 `rate_limits` 非空的」，不是「最新一条记录」。** 本机实测
//!    2026-09-18 那两份 rollout 里 `primary` / `secondary` 整块都是 `null`（走中转或
//!    API Key 的会话不带额度），翻到 09-17 才有数。取最新一条的话永远读出 `null`，
//!    而症状是「这个功能好像没做」。
//! 2. **一条都没有就是 [`None`]，不是 0%。** 跟 [`crate::accounts::usage::for_slot`]
//!    两源都缺时返回 `None` 同一个理由：0% 是一句断言，「没读到」不是。
//! 3. **它是上一次请求时的快照，不是此刻。** 界面必须标「Codex 写入 hh:mm」并在
//!    [`CodexRateLimits::age_minutes`] 大的时候提醒 —— 跟反重力那句「IDE 写入 hh:mm」
//!    同源。Codex 关着的这几个小时里额度早就在恢复，面板不知道。
//!
//! 归属只认这个槽位自己的 `home\sessions` 与 `home\archived_sessions`，默认目录的
//! 不认领 —— 跟 [`super::usage`] 一条口径。

use crate::accounts::usage::UsageWindow;
use crate::error::Result;
use chrono::{DateTime, Local};
use serde::Serialize;
use serde_json::Value;
use std::{io::BufRead, path::Path};
use ts_rs::TS;

/// 一个额度窗口，外加它是多长的窗口。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CodexWindow {
    /// 人话名字：`5 小时` / `7 天` / 认不出来时按分钟如实写。
    pub name: String,
    /// 窗口长度（分钟），原样。
    #[ts(type = "number")]
    pub window_minutes: i64,
    pub window: UsageWindow,
}

/// 点数余额。Codex 写什么就报什么，读不到就是 `None`。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CodexCredits {
    pub has_credits: bool,
    pub unlimited: bool,
    /// 余额。`None` = 这条记录没带。
    pub balance: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CodexRateLimits {
    /// 这份读数出自哪个会话记录文件（只给「统计说明」看）。
    pub source_file: String,
    /// 这份读数是什么时候写的，RFC3339。
    pub measured_at: String,
    /// 落后了多少分钟。界面拿它决定要不要提醒「这不是当前值」。
    #[ts(type = "number")]
    pub age_minutes: i64,
    /// 短窗口（实测 `window_minutes: 300` = 五小时）。`None` = 这条记录没带。
    pub primary: Option<CodexWindow>,
    /// 长窗口（实测 `window_minutes: 10080` = 七天）。
    pub secondary: Option<CodexWindow>,
    /// 档位。Codex 常写 `null`，那就如实是 `None`，别猜成免费档。
    pub plan_type: Option<String>,
    pub credits: Option<CodexCredits>,
}

/// 窗口长度 → 人话。
///
/// ⛔ 认不出来的长度**按分钟如实写**，别硬塞进「五小时 / 七天」两个格子里 ——
/// OpenAI 换了窗口长度时，硬塞的结果是界面上一句自信的错话。
fn window_name(minutes: i64) -> String {
    match minutes {
        300 => "5 小时".into(),
        10080 => "7 天".into(),
        m if m > 0 && m % 1440 == 0 => format!("{} 天", m / 1440),
        m if m > 0 && m % 60 == 0 => format!("{} 小时", m / 60),
        m if m > 0 => format!("{m} 分钟"),
        _ => "窗口长度未知".into(),
    }
}

/// 解一个 `primary` / `secondary` 子对象。字段缺一不可少 `used_percent`。
fn parse_window(v: &Value, now_ms: i64) -> Option<CodexWindow> {
    let used = v.get("used_percent")?.as_f64()?;
    if !used.is_finite() {
        return None;
    }
    let minutes = v
        .get("window_minutes")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let reset_secs = v
        .get("resets_at")
        .and_then(Value::as_i64)
        .filter(|s| *s > 0);
    let resets_at = reset_secs
        .and_then(|s| DateTime::from_timestamp(s, 0))
        .map(|d| d.to_rfc3339());
    Some(CodexWindow {
        name: window_name(minutes),
        window_minutes: minutes,
        window: UsageWindow {
            used: used.round().clamp(0.0, 100.0) as u8,
            resets_at,
            // Codex 自己写下来的，不是我们推的。
            estimated: false,
            // 快照里的重置时刻已经过去：这是重置之前记的数（2026-09-25）。
            reset_passed: reset_secs.is_some_and(|s| s.saturating_mul(1000) <= now_ms),
        },
    })
}

fn parse_credits(v: &Value) -> Option<CodexCredits> {
    let o = v.as_object()?;
    Some(CodexCredits {
        has_credits: o.get("has_credits").and_then(Value::as_bool)?,
        unlimited: o.get("unlimited").and_then(Value::as_bool).unwrap_or(false),
        balance: o.get("balance").and_then(Value::as_f64),
    })
}

/// 从一行 JSONL 里取额度。**两个窗口都空就当没有这条**（规矩 1）。
pub fn parse_line(line: &str, source_file: &str, now_ms: i64) -> Option<CodexRateLimits> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v["type"] != "event_msg" || v["payload"]["type"] != "token_count" {
        return None;
    }
    let r = v["payload"].get("rate_limits")?;
    let primary = r.get("primary").and_then(|w| parse_window(w, now_ms));
    let secondary = r.get("secondary").and_then(|w| parse_window(w, now_ms));
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    let at = DateTime::parse_from_rfc3339(v["timestamp"].as_str()?)
        .ok()?
        .timestamp_millis();
    Some(CodexRateLimits {
        source_file: source_file.to_string(),
        measured_at: DateTime::from_timestamp_millis(at)
            .unwrap_or_default()
            .to_rfc3339(),
        age_minutes: (now_ms - at) / 60_000,
        primary,
        secondary,
        plan_type: r
            .get("plan_type")
            .and_then(Value::as_str)
            .map(str::to_owned),
        credits: r.get("credits").and_then(parse_credits),
    })
}

/// 扫描过程中攒下来的东西，给「统计说明」用。
#[derive(Debug, Default, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CodexRateLimitScan {
    /// 翻过几个会话记录文件（**找到就停**，所以这个数通常很小）。
    #[ts(type = "number")]
    pub files_examined: u32,
    /// 打不开 / 读到一半出错的文件或目录数。
    #[ts(type = "number")]
    pub files_failed: u32,
    /// 第一条失败的原因，原话。
    ///
    /// ⛔ **只报一个计数是不够的。** 本机实测：`~\.codex\sessions` 是个联结点
    /// （多半是别的工具建的），[`crate::config_io::ensure_plain_path`] 按规矩拒掉整棵树，
    /// 于是探针打出「翻了 0 个文件 · 失败 258」—— 一句能查的话都没有，
    /// 跟坑 7.62「找不到时把找过的路径列进报错」是同一个教训。
    pub first_error: Option<String>,
    /// 找到的那一份。`None` = 这个槽位还没有带额度信息的会话记录。
    pub found: Option<CodexRateLimits>,
}

impl CodexRateLimitScan {
    fn fail(&mut self, why: impl FnOnce() -> String) {
        self.files_failed += 1;
        if self.first_error.is_none() {
            self.first_error = Some(why());
        }
    }
}

/// 把一个目录下的 `.jsonl` 按修改时间**从新到旧**列出来。
fn rollouts(
    dir: &Path,
    out: &mut Vec<(std::time::SystemTime, std::path::PathBuf)>,
    scan: &mut CodexRateLimitScan,
) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            scan.fail(|| format!("{} 打不开：{e}", dir.display()));
            return;
        }
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if let Err(e) = crate::config_io::ensure_plain_path(&path) {
            scan.fail(|| format!("{}：{e}", path.display()));
            continue;
        }
        if path.is_dir() {
            rollouts(&path, out, scan);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let when = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        out.push((when, path));
    }
}

/// 在一个文件里找**最后一条**带额度的记录。行内先做子串预筛，避免对 16 MB 的
/// rollout 逐行解 JSON（本机实测最大的一份就是 16 MB）。
fn last_in_file(path: &Path, now_ms: i64) -> std::io::Result<Option<CodexRateLimits>> {
    let file = std::fs::File::open(path)?;
    let name = path.display().to_string();
    let mut found = None;
    for line in std::io::BufReader::new(file).lines() {
        // 一行坏编码（Codex 正在写的最后一行只写了一半）原来会让整份文件作废 —— 前面已经找到的
        // 额度一起丢掉，退回更旧的那份（2026-09-25）。坏编码的行跳过；别的读错误停读，前面读到的照样算。
        let line = match line {
            Ok(line) => line,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(e) if found.is_none() => return Err(e),
            Err(_) => break,
        };
        if !line.contains("\"rate_limits\"") {
            continue;
        }
        if let Some(r) = parse_line(&line, &name, now_ms) {
            found = Some(r);
        }
    }
    Ok(found)
}

/// 读这个槽位最新的一份额度快照。
///
/// 从最新的会话记录往回翻，**翻到第一份带额度的就停**。翻不到就是
/// [`CodexRateLimitScan::found`] 为 `None` —— 界面照实说「还没有带额度信息的会话记录」。
///
/// `limit` 是最多往回翻几个文件；给一个上界是因为老槽位可能有上千份 rollout，
/// 而「一年前那份的额度」对使用者没有任何意义。
pub fn read(home: &Path, limit: usize) -> Result<CodexRateLimitScan> {
    crate::config_io::ensure_plain_path(home)?;
    let mut out = CodexRateLimitScan::default();
    let mut files = Vec::new();
    rollouts(&home.join("sessions"), &mut files, &mut out);
    rollouts(&home.join("archived_sessions"), &mut files, &mut out);
    // 从新到旧（`Reverse`）—— 最新那份里的额度才是有用的。
    files.sort_by_key(|(when, _)| std::cmp::Reverse(*when));
    let now_ms = Local::now().timestamp_millis();
    for (_, path) in files.into_iter().take(limit) {
        out.files_examined += 1;
        match last_in_file(&path, now_ms) {
            Ok(Some(r)) => {
                out.found = Some(r);
                break;
            }
            Ok(None) => {}
            Err(e) => out.fail(|| format!("{}：{e}", path.display())),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE_WITH: &str = r#"{"timestamp":"2026-09-17T07:53:51.000Z","type":"event_msg","payload":{"type":"token_count","info":{},"rate_limits":{"limit_id":"codex","primary":{"used_percent":2.0,"window_minutes":10080,"resets_at":1790247800},"secondary":null,"credits":{"has_credits":false,"unlimited":false,"balance":null},"plan_type":null}}}"#;
    const LINE_ALL_NULL: &str = r#"{"timestamp":"2026-09-18T20:04:54.998Z","type":"event_msg","payload":{"type":"token_count","info":{},"rate_limits":{"limit_id":"codex","primary":null,"secondary":null,"credits":null,"plan_type":null}}}"#;

    /// Codex 正在写的最后一行只写了一半（坏编码）：前面已经找到的额度照样算，不许整份作废。
    #[test]
    fn a_half_written_last_line_does_not_throw_away_the_file() {
        let path = std::env::temp_dir().join(format!(
            "qbgate-ratelimit-badline-{}-{}.jsonl",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        let mut bytes = LINE_WITH.as_bytes().to_vec();
        bytes.extend_from_slice(b"\n{\"rate_limits\":\xff\xfe");
        std::fs::write(&path, &bytes).unwrap();
        let got = last_in_file(&path, 0);
        let _ = std::fs::remove_file(&path);
        assert!(
            got.expect("坏编码的一行不许让整份文件报错").is_some(),
            "前面那条额度要留住"
        );
    }

    #[test]
    fn a_record_whose_windows_are_all_null_is_not_a_reading() {
        // 本机实测：走中转/API Key 的会话整块是 null。把它当成一份读数，
        // 界面上就会出现一个「0%」——而那是编的。
        assert!(parse_line(LINE_ALL_NULL, "f", 0).is_none());
    }

    #[test]
    fn the_window_length_decides_the_name_and_unknown_lengths_stay_honest() {
        assert_eq!(window_name(300), "5 小时");
        assert_eq!(window_name(10080), "7 天");
        assert_eq!(window_name(180), "3 小时");
        assert_eq!(window_name(90), "90 分钟");
        assert_eq!(window_name(0), "窗口长度未知");
    }

    #[test]
    fn a_populated_record_keeps_codexs_own_numbers_and_is_never_marked_estimated() {
        let r = parse_line(LINE_WITH, "rollout.jsonl", 0).expect("有 primary 就该读得出来");
        let p = r.primary.expect("primary 在");
        assert_eq!(p.window_minutes, 10080);
        assert_eq!(p.name, "7 天");
        assert_eq!(p.window.used, 2);
        assert!(!p.window.estimated, "这是 Codex 自己写的，不是推算的");
        assert!(p.window.resets_at.is_some());
        assert!(r.secondary.is_none());
        assert_eq!(r.plan_type, None);
        assert_eq!(
            r.credits,
            Some(CodexCredits {
                has_credits: false,
                unlimited: false,
                balance: None
            })
        );
    }

    #[test]
    fn used_percent_is_clamped_instead_of_wrapping_around() {
        let over = r#"{"timestamp":"2026-09-17T07:53:51.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":140.0,"window_minutes":300,"resets_at":0}}}}"#;
        let r = parse_line(over, "f", 0).expect("读得出来");
        let p = r.primary.expect("primary 在");
        assert_eq!(p.window.used, 100);
        assert_eq!(p.window.resets_at, None, "resets_at 是 0 就是没写");
    }

    #[test]
    fn events_that_are_not_token_counts_are_ignored() {
        let other = r#"{"timestamp":"2026-09-17T07:53:51.000Z","type":"event_msg","payload":{"type":"agent_message","rate_limits":{"primary":{"used_percent":9.0,"window_minutes":300}}}}"#;
        assert!(parse_line(other, "f", 0).is_none());
    }

    #[test]
    fn age_is_measured_against_the_caller_supplied_clock() {
        let at = DateTime::parse_from_rfc3339("2026-09-17T07:53:51.000Z")
            .unwrap()
            .timestamp_millis();
        let r = parse_line(LINE_WITH, "f", at + 90 * 60_000).expect("读得出来");
        assert_eq!(r.age_minutes, 90);
    }
}
