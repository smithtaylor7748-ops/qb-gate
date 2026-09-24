//! Local Codex rollout usage. token_count contains cumulative snapshots, not
//! individual bills: sum positive differences once per session, never snapshots.
//!
//! 2026-09-24 起每一段差分还记到**样本时刻的本地日**、**文件里最近一条
//! `turn_context.payload.model`** 上（cc-switch 读 Codex 用量也是这么认模型的，MIT），
//! 装成跟 Claude 一样的 [`TokenBucket`] —— 用量明细页的 GPT 一侧因此也有
//! 按天、按模型与按官方 API 价折算的美元。原来界面上写的「Codex 的会话记录里没有
//! 按模型 / 按天的分项」是错的：时刻每条样本都有，模型在 `turn_context` 里。
//!
//! 装桶时 `input_tokens` 里**含**缓存命中的那部分（`cached_input_tokens` 是它的子集），
//! 所以桶里的 `input` = 输入 − 缓存，`cache_read` = 缓存；推理 token 是输出的子集，不另算。
//!
//! 长上下文：OpenAI 对超长输入另有一档价（`pricing::LongContext`）。单次请求的输入超过
//! [`LONG_CONTEXT_FROM`] 的（`last_token_usage.input_tokens`）按天数出来，界面据此把美元写成
//! 「至少」—— 这里不替它挑档位。
use crate::accounts::tokens::TokenBucket;
use crate::error::Result;
use chrono::{DateTime, Local};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    io::BufRead,
    path::Path,
};
use ts_rs::TS;

/// OpenAI 定价表里长上下文那一档从多少 token 起（表头原文「<272K context length」）。
pub const LONG_CONTEXT_FROM: u64 = 272_000;

/// 没读到 `turn_context` 时的模型名。价目里认不出，老老实实进「没有官方价」。
const UNKNOWN_MODEL: &str = "未知模型";

#[derive(Debug, Default, Clone, Serialize, TS)]
#[ts(export)]
pub struct CodexUsage {
    pub input: f64,
    pub output: f64,
    pub cached: f64,
    pub reasoning: f64,
    pub total: f64,
    pub sessions: u32,
    pub files_read: u32,
    pub files_failed: u32,
    pub duplicates: u32,
    pub incomplete: u32,
    pub checked_at: String,
    /// 按本地日 × 模型的差分（只含截止时刻之后的）。`input` 已去掉缓存那部分。
    pub buckets: Vec<TokenBucket>,
    /// 按天：单次输入超过 [`LONG_CONTEXT_FROM`] 的请求数。
    pub long_context: Vec<CodexDayCount>,
}

/// 某一天的请求数。
#[derive(Debug, Default, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CodexDayCount {
    /// `YYYY-MM-DD`，本地时区。
    pub day: String,
    pub requests: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Counts {
    input: u64,
    output: u64,
    cached: u64,
    reasoning: u64,
    total: u64,
}
#[derive(Clone, Debug)]
struct Sample {
    at: i64,
    counts: Counts,
    /// 这条样本之前最近一条 `turn_context` 写的模型。
    model: String,
    /// 这一次请求自己的输入（`last_token_usage.input_tokens`），长上下文判定用。
    last_input: Option<u64>,
}

fn sample(v: &Value, model: &str) -> Option<Sample> {
    let p = &v["payload"];
    if v["type"] != "event_msg" || p["type"] != "token_count" {
        return None;
    }
    let info = p.get("info")?;
    let u = info.get("total_token_usage")?.as_object()?;
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    let input = n("input_tokens");
    let output = n("output_tokens");
    Some(Sample {
        at: DateTime::parse_from_rfc3339(v["timestamp"].as_str()?)
            .ok()?
            .timestamp_millis(),
        counts: Counts {
            input,
            output,
            cached: n("cached_input_tokens").min(input),
            reasoning: n("reasoning_output_tokens").min(output),
            total: u
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(input.saturating_add(output)),
        },
        model: model.to_string(),
        last_input: info
            .get("last_token_usage")
            .and_then(|l| l.get("input_tokens"))
            .and_then(Value::as_u64),
    })
}

fn local_day(ms: i64) -> Option<String> {
    use chrono::TimeZone;
    match Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => Some(t.format("%Y-%m-%d").to_string()),
        _ => None,
    }
}

fn aggregate(sessions: BTreeMap<String, Vec<Sample>>, cutoff: i64, out: &mut CodexUsage) {
    let mut buckets: BTreeMap<(String, String), TokenBucket> = BTreeMap::new();
    let mut long: BTreeMap<String, u32> = BTreeMap::new();
    for (_, mut rows) in sessions {
        rows.sort_by_key(|s| s.at);
        let mut prev = Counts::default();
        let mut seen = HashSet::new();
        let mut counted = false;
        for row in rows {
            let c = row.counts;
            if !seen.insert((row.at, c.clone())) || c == prev {
                out.duplicates += 1;
                continue;
            }
            if c.total < prev.total || c.input < prev.input || c.output < prev.output {
                // A reset cannot be distinguished from replayed older context.
                // Report incomplete coverage instead of inventing another charge.
                out.incomplete += 1;
                continue;
            }
            if row.at >= cutoff {
                let d_input = c.input.saturating_sub(prev.input);
                let d_output = c.output.saturating_sub(prev.output);
                let d_cached = c.cached.saturating_sub(prev.cached).min(d_input);
                out.input += d_input as f64;
                out.output += d_output as f64;
                out.cached += c.cached.saturating_sub(prev.cached) as f64;
                out.reasoning += c.reasoning.saturating_sub(prev.reasoning) as f64;
                out.total += c.total.saturating_sub(prev.total) as f64;
                counted = true;
                if let Some(day) = local_day(row.at) {
                    let b = buckets
                        .entry((day.clone(), row.model.clone()))
                        .or_insert_with(|| TokenBucket {
                            day: day.clone(),
                            model: row.model.clone(),
                            input: 0,
                            output: 0,
                            cache_write: 0,
                            cache_write_1h: 0,
                            cache_read: 0,
                            messages: 0,
                        });
                    b.input += (d_input - d_cached) as i64;
                    b.cache_read += d_cached as i64;
                    b.output += d_output as i64;
                    b.messages += 1;
                    if row.last_input.is_some_and(|n| n > LONG_CONTEXT_FROM) {
                        *long.entry(day).or_default() += 1;
                    }
                }
            }
            prev = c;
        }
        if counted {
            out.sessions += 1;
        }
    }
    out.buckets = buckets.into_values().collect();
    out.long_context = long
        .into_iter()
        .map(|(day, requests)| CodexDayCount { day, requests })
        .collect();
}

fn scan(dir: &Path, sessions: &mut BTreeMap<String, Vec<Sample>>, out: &mut CodexUsage) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => {
            out.files_failed += 1;
            return;
        }
    };
    for entry in rd {
        let Ok(entry) = entry else {
            out.files_failed += 1;
            continue;
        };
        let path = entry.path();
        if crate::config_io::ensure_plain_path(&path).is_err() {
            out.files_failed += 1;
            continue;
        }
        if path.is_dir() {
            scan(&path, sessions, out);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            out.files_failed += 1;
            continue;
        };
        let mut id = None;
        let mut rows = Vec::new();
        let mut model = UNKNOWN_MODEL.to_string();
        for line in std::io::BufReader::new(file).lines() {
            let Ok(line) = line else {
                out.files_failed += 1;
                break;
            };
            if !line.contains("session_meta")
                && !line.contains("token_count")
                && !line.contains("turn_context")
            {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                out.incomplete += 1;
                continue;
            };
            if v["type"] == "session_meta" {
                id = v["payload"]["id"]
                    .as_str()
                    .or_else(|| v["payload"]["session_id"].as_str())
                    .map(str::to_owned);
            } else if v["type"] == "turn_context" {
                if let Some(m) = v["payload"]["model"].as_str().filter(|m| !m.is_empty()) {
                    model = m.to_string();
                }
            } else if let Some(s) = sample(&v, &model) {
                rows.push(s);
            } else if v["payload"]["type"] == "token_count" && !v["payload"]["info"].is_null() {
                out.incomplete += 1;
            }
        }
        out.files_read += 1;
        if let Some(id) = id {
            sessions.entry(id).or_default().extend(rows);
        } else if !rows.is_empty() {
            out.incomplete += rows.len() as u32;
        }
    }
}

pub fn read(home: &Path, days: u32) -> Result<CodexUsage> {
    crate::config_io::ensure_plain_path(home)?;
    let cutoff = if days == 0 {
        i64::MIN
    } else {
        let date = Local::now().date_naive() - chrono::Duration::days(i64::from(days - 1));
        date.and_hms_opt(0, 0, 0)
            .and_then(|t| t.and_local_timezone(Local).earliest())
            .map(|d| d.timestamp_millis())
            .unwrap_or(i64::MIN)
    };
    let mut out = CodexUsage {
        checked_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
        ..Default::default()
    };
    let mut sessions = BTreeMap::new();
    scan(&home.join("sessions"), &mut sessions, &mut out);
    scan(&home.join("archived_sessions"), &mut sessions, &mut out);
    aggregate(sessions, cutoff, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(at: i64, input: u64, output: u64) -> Sample {
        Sample {
            at,
            counts: Counts {
                input,
                output,
                total: input + output,
                ..Default::default()
            },
            model: "gpt-5.6-sol".into(),
            last_input: None,
        }
    }
    fn run(rows: Vec<Sample>, cutoff: i64) -> CodexUsage {
        let mut out = CodexUsage::default();
        aggregate(BTreeMap::from([("session".into(), rows)]), cutoff, &mut out);
        out
    }
    #[test]
    fn cumulative_snapshots_and_archive_copies_are_not_added_twice() {
        let rows = vec![
            at(1, 100, 10),
            at(2, 200, 20),
            at(1, 100, 10),
            at(2, 200, 20),
            at(3, 200, 20),
        ];
        let r = run(rows, i64::MIN);
        assert_eq!(r.total, 220.0);
        assert_eq!(r.duplicates, 3);
    }
    #[test]
    fn todays_usage_subtracts_yesterdays_cumulative_count() {
        let r = run(vec![at(1, 100, 10), at(2, 150, 15)], 2);
        assert_eq!(r.total, 55.0);
    }
    #[test]
    fn unknown_counter_reset_is_reported_instead_of_double_charged() {
        let r = run(vec![at(1, 100, 10), at(2, 10, 1)], 0);
        assert_eq!(r.total, 110.0);
        assert_eq!(r.incomplete, 1);
    }
    #[test]
    fn cached_and_reasoning_are_subsets_not_extra_tokens() {
        let v = serde_json::json!({"type":"event_msg","timestamp":"2026-09-16T10:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":80,"output_tokens":20,"reasoning_output_tokens":10,"total_tokens":120}}}});
        let s = sample(&v, "gpt-5.6-sol").unwrap();
        let r = run(vec![s], 0);
        assert_eq!(r.total, 120.0);
        assert_eq!(r.cached, 80.0);
        assert_eq!(r.reasoning, 10.0);
        // 桶里的输入去掉了缓存那部分：20 新输入 + 80 缓存读，不是 100 + 80。
        assert_eq!(r.buckets.len(), 1);
        assert_eq!((r.buckets[0].input, r.buckets[0].cache_read), (20, 80));
        assert_eq!(r.buckets[0].output, 20);
    }

    /// 每一段差分记到**它自己那条样本**的模型上：中途换了模型，前后两段分开。
    #[test]
    fn deltas_follow_the_model_in_effect_and_the_local_day() {
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-09-20T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let mut a = at(t0, 100, 10);
        a.model = "gpt-5.6-sol".into();
        let mut b = at(t0 + 60_000, 250, 30);
        b.model = "gpt-6-luna".into();
        b.last_input = Some(LONG_CONTEXT_FROM + 1);
        let r = run(vec![a, b], i64::MIN);
        let sol = r.buckets.iter().find(|x| x.model == "gpt-5.6-sol").unwrap();
        let luna = r.buckets.iter().find(|x| x.model == "gpt-6-luna").unwrap();
        assert_eq!((sol.input, sol.output, sol.messages), (100, 10, 1));
        assert_eq!((luna.input, luna.output, luna.messages), (150, 20, 1));
        assert_eq!(r.long_context.iter().map(|d| d.requests).sum::<u32>(), 1);
    }

    /// 整条读文件的路径：`turn_context` 在前、`token_count` 在后，模型认得出来。
    #[test]
    fn a_rollout_file_is_read_with_its_models() {
        let d = std::env::temp_dir().join(format!(
            "qbgate-codex-usage-{}-{}",
            std::process::id(),
            Local::now().format("%H%M%S%f")
        ));
        let home = d.join("home");
        let day = home.join("sessions/2026/09/20");
        std::fs::create_dir_all(&day).unwrap();
        let lines = [
            r#"{"type":"session_meta","timestamp":"2026-09-20T12:00:00Z","payload":{"id":"s1"}}"#,
            r#"{"type":"turn_context","timestamp":"2026-09-20T12:00:01Z","payload":{"model":"gpt-5.6-sol"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-09-20T12:00:05Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050},"last_token_usage":{"input_tokens":1000}}}}"#,
        ];
        std::fs::write(day.join("rollout-1.jsonl"), lines.join("\n")).unwrap();
        let got = read(&home, 0).unwrap();
        assert_eq!(got.files_read, 1);
        assert_eq!(got.buckets.len(), 1);
        assert_eq!(got.buckets[0].model, "gpt-5.6-sol");
        assert_eq!(got.buckets[0].input, 600);
        assert_eq!(got.buckets[0].cache_read, 400);
        let _ = std::fs::remove_dir_all(&d);
    }
}
