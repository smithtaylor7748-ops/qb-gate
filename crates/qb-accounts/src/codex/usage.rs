//! Local Codex rollout usage. token_count contains cumulative snapshots, not
//! individual bills: sum positive differences once per session, never snapshots.
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
}

fn sample(v: &Value) -> Option<Sample> {
    let p = &v["payload"];
    if v["type"] != "event_msg" || p["type"] != "token_count" {
        return None;
    }
    let u = p.get("info")?.get("total_token_usage")?.as_object()?;
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
    })
}

fn aggregate(sessions: BTreeMap<String, Vec<Sample>>, cutoff: i64, out: &mut CodexUsage) {
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
                out.input += c.input.saturating_sub(prev.input) as f64;
                out.output += c.output.saturating_sub(prev.output) as f64;
                out.cached += c.cached.saturating_sub(prev.cached) as f64;
                out.reasoning += c.reasoning.saturating_sub(prev.reasoning) as f64;
                out.total += c.total.saturating_sub(prev.total) as f64;
                counted = true;
            }
            prev = c;
        }
        if counted {
            out.sessions += 1;
        }
    }
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
        for line in std::io::BufReader::new(file).lines() {
            let Ok(line) = line else {
                out.files_failed += 1;
                break;
            };
            if !line.contains("session_meta") && !line.contains("token_count") {
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
            } else if let Some(s) = sample(&v) {
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
        let s = sample(&v).unwrap();
        let r = run(vec![s], 0);
        assert_eq!(r.total, 120.0);
        assert_eq!(r.cached, 80.0);
        assert_eq!(r.reasoning, 10.0);
    }
}
