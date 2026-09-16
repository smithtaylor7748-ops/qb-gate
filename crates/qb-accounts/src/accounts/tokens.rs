//! 槽位用掉的 token：从官方客户端自己写在本机的会话转写里数出来。
//!
//! # 数据从哪来
//!
//! `<槽位目录>\projects\<项目>\<会话>.jsonl`。每一行是一条 JSON 记录，
//! `type` 为 `assistant` 的那些带着 `message.model` 与 `message.usage`：
//!
//! ```text
//! input_tokens · output_tokens · cache_creation_input_tokens · cache_read_input_tokens
//! ```
//!
//! 零网络请求。跟 [`super::usage`] 同一条口径 —— 面板只是把官方客户端
//! 已经落盘的东西读出来显示，不调任何接口（CLAUDE.md「不许加的功能」第 4 行）。
//!
//! # ⛔ 不去重的话，数出来的是真实用量的两倍
//!
//! 续接会话、自动压缩、`--continue`，都会新开一个转写文件并把之前的消息
//! 原样重放进去，包括 assistant 那几条和它们的 `usage`。
//! 实机 2026-09-16 在 `main` 槽位上数过：
//!
//! | | 条数 | 输出 token |
//! |---|--:|--:|
//! | 不去重 | 6,049 | 6,377,231 |
//! | 去重后 | 3,189 | 2,960,298 |
//!
//! **2.15 倍。** 而且多出来的那一倍长得完全像真数据 —— 没有任何地方会报错，
//! 界面上只会显示一个偏大的数字。去重键是 `message.id` + `requestId`：
//! 同一批数据里 `message.id` 从没对应过两个 `requestId`，同一对键也从没
//! 带过不同的 token 数（两条都实测过），所以这次去重是无损的；
//! 带上 `requestId` 是为了万一上游哪天真的为同一条消息发两次请求 ——
//! 那是两次真实开销，不该被合成一条。
//!
//! # 范围：槽位目录 + 默认目录里**认得出归属**的那部分
//!
//! 0.20.0 第一版只数槽位目录。实测那是错的：这台机器上两个槽位的
//! `projects\` **两周没人写过**（最后一条 9-02 / 9-04），而同一天
//! `~\.claude\projects` 里有 780 条回复、2.9 亿 token。原因是
//! 桌面端 Code 页里跑的会话不吃 `CLAUDE_CONFIG_DIR`（档案 §7.26），
//! 转写全落在默认目录。只数槽位目录的结果是：**界面上永远是零**。
//!
//! 所以现在两处都数，默认目录那部分靠 `ownerAccountUuid` 归属：
//!
//! | 来源 | 怎么归 |
//! |---|---|
//! | `<槽位目录>\projects` | 整个目录就是这个槽位的，不用判 |
//! | `~\.claude\projects` | 文件里 `bridge-session` 行上的 `ownerAccountUuid` 对上槽位的 `accountUuid` |
//!
//! ⛔ **`ownerAccountUuid` 不在 assistant 行上**，它在 `bridge-session` 行上，
//! 一个文件一个（实测 60 个文件里没有一个带两个 owner），所以按**文件**归属。
//! 去 assistant 行上找它只会得到零命中 —— 那是实测踩过的。
//!
//! ⛔ **归不出来的那部分不许摊给任何账户。** 实测默认目录里有 16% 的记录
//! （811M token）没有 owner 标记 —— 今天这一整天的记录**全都没有**。
//! 它们单独报在 `unattributed` 里，界面照实说「归不到槽位」。
//! 按「默认目录当前登录的是谁」去摊派是很诱人的，但那是拿一个此刻的快照
//! 去认领几个月的历史，换账户过一次就全错了。
//!
//! # 为什么没有增量缓存
//!
//! 想过按文件偏移做增量。实测否掉了：两个槽位合计 90 MB、9,634 条带用量的
//! 记录，一遍扫完 0.46 秒（还是 Python 量的，Rust 只会更快）。
//! 为这点开销换一份要自己维护失效逻辑的磁盘缓存不划算 ——
//! 而缓存算错的样子，正是「显示一个看起来很正常的错数字」。

use serde::Serialize;
use std::io::BufRead;
use std::path::Path;
use ts_rs::TS;

/// 一条 assistant 回复的用量。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 去重键：`message.id` + `requestId`。见文件头。
    pub key: String,
    /// 这条回复的时刻，毫秒。`i64::MIN` = 这条记录没带时刻。
    pub at: i64,
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
}

/// 一个「日 × 模型」桶。按本机本地时区切天 —— 使用者看的是自己的日历。
#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenBucket {
    /// `YYYY-MM-DD`，本地时区。
    pub day: String,
    /// 官方写下的模型名，原样给出，不做任何美化或归并。
    pub model: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    /// 这个桶里有几条回复。
    #[ts(type = "number")]
    pub messages: i64,
}

/// 一个槽位的 token 统计。
///
/// 后面那几个计数不是装饰，它们是「这份统计覆盖了多少」的证据。
/// 只给总数的话，「读了 41 个文件」和「读了 3 个、38 个打不开」
/// 在界面上长得一模一样。
#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenUsage {
    /// 日 × 模型，按 `day` 再按 `model` 升序。
    pub buckets: Vec<TokenBucket>,
    /// 有用量记录的会话（转写文件）数。
    #[ts(type = "number")]
    pub sessions: i64,
    /// 读成功的转写文件数。
    #[ts(type = "number")]
    pub files_read: i64,
    /// 打不开或读坏了的转写文件数。不为零就要在界面上说。
    #[ts(type = "number")]
    pub files_failed: i64,
    /// 被去重丢掉的条数。见文件头 —— 这个数通常和留下的一样大。
    #[ts(type = "number")]
    pub duplicates: i64,
    /// 没有可用时刻、落不进任何一天的条数。
    #[ts(type = "number")]
    pub undated: i64,
    /// 默认目录里**归不到任何槽位**的用量，同样按日 × 模型分桶。
    ///
    /// 单独一列而不是并进 `buckets`：它不属于这个账户，也不属于别的账户 ——
    /// 摊给谁都是编。界面把它当成一行旁注显示。
    pub unattributed: Vec<TokenBucket>,
}

// ------------------------------------------------------------------ 解析

/// 解析一行转写。不是带用量的 assistant 记录就回 `None`。
///
/// 调用方应当先粗筛掉不含 `usage` 的行 —— 转写里绝大多数行是用户输入和
/// 工具结果，对每一行都建一棵 `serde_json::Value` 纯属浪费。
pub fn parse_line(line: &str) -> Option<Entry> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let m = v.get("message")?;
    let u = m.get("usage")?.as_object()?;
    let n = |k: &str| u.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
    // `message.id` 没有时退回这一行自己的 `uuid`：那是每行唯一的，
    // 退回它等于「这一条不参与去重」，而不是把它悄悄丢掉。
    let id = m
        .get("id")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("uuid").and_then(|x| x.as_str()))?;
    let req = v.get("requestId").and_then(|x| x.as_str()).unwrap_or("");
    Some(Entry {
        key: format!("{id}:{req}"),
        at: v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.timestamp_millis())
            .unwrap_or(i64::MIN),
        model: m
            .get("model")
            .and_then(|x| x.as_str())
            .unwrap_or("未知模型")
            .to_string(),
        input: n("input_tokens"),
        output: n("output_tokens"),
        cache_write: n("cache_creation_input_tokens"),
        cache_read: n("cache_read_input_tokens"),
    })
}

/// 按 `key` 去重，保留第一次见到的那条。返回留下的与丢掉的条数。
///
/// 为什么留第一条而不是最后一条：同一对键从没带过不同的 token 数
/// （实测），所以留哪条都一样；留第一条让结果不依赖文件的遍历顺序。
pub fn dedupe(entries: Vec<Entry>) -> (Vec<Entry>, i64) {
    let mut seen = std::collections::HashSet::new();
    let mut dropped = 0;
    let kept = entries
        .into_iter()
        .filter(|e| {
            if seen.insert(e.key.clone()) {
                true
            } else {
                dropped += 1;
                false
            }
        })
        .collect();
    (kept, dropped)
}

/// 聚合成「日 × 模型」。返回桶与没有时刻、落不进任何一天的条数。
pub fn buckets_of(entries: &[Entry]) -> (Vec<TokenBucket>, i64) {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, String), TokenBucket> = BTreeMap::new();
    let mut undated = 0;
    for e in entries {
        let Some(day) = local_day(e.at) else {
            undated += 1;
            continue;
        };
        let b = map
            .entry((day.clone(), e.model.clone()))
            .or_insert_with(|| TokenBucket {
                day,
                model: e.model.clone(),
                input: 0,
                output: 0,
                cache_write: 0,
                cache_read: 0,
                messages: 0,
            });
        b.input += e.input;
        b.output += e.output;
        b.cache_write += e.cache_write;
        b.cache_read += e.cache_read;
        b.messages += 1;
    }
    // BTreeMap 的键就是 (day, model)，迭代出来已经是那个顺序。
    (map.into_values().collect(), undated)
}

/// 毫秒 → 本地日期 `YYYY-MM-DD`。没有时刻的回 `None`。
fn local_day(ms: i64) -> Option<String> {
    if ms == i64::MIN {
        return None;
    }
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => Some(t.format("%Y-%m-%d").to_string()),
        _ => None,
    }
}

// ------------------------------------------------------------------ 入口

/// 数一个槽位用掉的 token。
///
/// 槽位目录不存在、没有 `projects\`、一个转写都没有，都回一份全零的报告
/// 而不是错误 —— 「这个槽位还没跑过会话」是正常状态，不是故障。
pub fn for_slot(slot_dir: &Path) -> TokenUsage {
    for_slot_and_default(slot_dir, None, None)
}

/// 数一个槽位用掉的 token，可选地把默认目录里归得上的那部分也算进来。
///
/// `default_dir` 是 `~\.claude`，`account_uuid` 是这个槽位的
/// `oauthAccount.accountUuid`。两个都给齐才会去数默认目录 ——
/// 没有 uuid 就归不了属，那时候去数只会把别人的量算到这个账户头上。
///
/// 槽位目录不存在、没有 `projects\`、一个转写都没有，都回一份全零的报告
/// 而不是错误 —— 「这个槽位还没跑过会话」是正常状态，不是故障。
pub fn for_slot_and_default(
    slot_dir: &Path,
    default_dir: Option<&Path>,
    account_uuid: Option<&str>,
) -> TokenUsage {
    let mut entries: Vec<Entry> = Vec::new();
    let mut stray: Vec<Entry> = Vec::new();
    let mut files_read = 0;
    let mut files_failed = 0;
    let mut sessions = 0;

    for file in transcripts(&slot_dir.join("projects")) {
        match read_entries(&file) {
            Ok(found) => {
                files_read += 1;
                if !found.is_empty() {
                    sessions += 1;
                }
                entries.extend(found);
            }
            Err(_) => files_failed += 1,
        }
    }

    if let Some(home) = default_dir {
        for file in transcripts(&home.join("projects")) {
            let owner = owner_of(&file);
            match read_entries(&file) {
                Ok(found) => {
                    files_read += 1;
                    match (owner.as_deref(), account_uuid) {
                        // 归得上这个槽位。
                        (Some(o), Some(me)) if o.eq_ignore_ascii_case(me) => {
                            if !found.is_empty() {
                                sessions += 1;
                            }
                            entries.extend(found);
                        }
                        // 归到别的账户去了 —— 跟这个槽位无关，一个字不提。
                        (Some(_), _) => {}
                        // 没有归属标记：既不能算给这个账户，也不能当它不存在。
                        (None, _) => stray.extend(found),
                    }
                }
                Err(_) => files_failed += 1,
            }
        }
    }

    let (kept, duplicates) = dedupe(entries);
    let (buckets, undated) = buckets_of(&kept);
    let (stray_kept, _) = dedupe(stray);
    let (unattributed, _) = buckets_of(&stray_kept);
    TokenUsage {
        buckets,
        sessions,
        files_read,
        files_failed,
        duplicates,
        undated,
        unattributed,
    }
}

/// 这份转写属于哪个账户。
///
/// ⛔ 读的是 `bridge-session` 行上的 `ownerAccountUuid`，**不是 assistant 行**
/// —— assistant 行上没有这个字段（实测 13,143 条，零命中）。
/// 一个文件只会有一个 owner（实测 60 个文件里没有例外），所以读到第一个就停。
fn owner_of(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    for line in std::io::BufReader::new(file).lines() {
        let line = line.ok()?;
        if !line.contains("ownerAccountUuid") {
            continue;
        }
        if let Some(v) = serde_json::from_str::<serde_json::Value>(&line)
            .ok()
            .and_then(|j| {
                j.get("ownerAccountUuid")
                    .and_then(|x| x.as_str())
                    .map(str::to_string)
            })
        {
            return Some(v);
        }
    }
    None
}

/// `projects\<项目>\<会话>.jsonl` 的全部路径。
///
/// 只下一层 —— 官方就是这么排的，无限递归只会把别的东西卷进来。
fn transcripts(projects: &Path) -> Vec<std::path::PathBuf> {
    let Ok(rd) = std::fs::read_dir(projects) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for project in rd.flatten() {
        if !project.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(project.path()) else {
            continue;
        };
        out.extend(
            files
                .flatten()
                .map(|f| f.path())
                .filter(|p| p.extension().is_some_and(|e| e == "jsonl")),
        );
    }
    out.sort();
    out
}

fn read_entries(path: &Path) -> std::io::Result<Vec<Entry>> {
    let file = std::fs::File::open(path)?;
    let mut out = Vec::new();
    for line in std::io::BufReader::new(file).lines() {
        // 读坏的那一行跳过，不让它废掉整个文件 —— 转写是追加写的，
        // 正在写的最后一行可能只写了一半。
        let Ok(line) = line else { continue };
        if !line.contains("\"usage\"") {
            continue;
        }
        if let Some(e) = parse_line(&line) {
            out.push(e);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, req: &str, model: &str, out: i64, ts: &str) -> String {
        format!(
            concat!(
                r#"{{"type":"assistant","uuid":"u-{id}","requestId":"{req}","timestamp":"{ts}","#,
                r#""message":{{"id":"{id}","model":"{model}","usage":{{"#,
                r#""input_tokens":3,"output_tokens":{out},"#,
                r#""cache_creation_input_tokens":11,"cache_read_input_tokens":222}}}}}}"#
            ),
            id = id,
            req = req,
            model = model,
            out = out,
            ts = ts
        )
    }

    #[test]
    fn a_plain_assistant_row_parses() {
        let e = parse_line(&line(
            "m1",
            "r1",
            "claude-opus-5",
            40,
            "2026-09-16T10:00:00.000Z",
        ))
        .expect("这是实机上真实的形状");
        assert_eq!(e.key, "m1:r1");
        assert_eq!(e.model, "claude-opus-5");
        assert_eq!(
            (e.input, e.output, e.cache_write, e.cache_read),
            (3, 40, 11, 222)
        );
    }

    #[test]
    fn rows_that_are_not_assistant_usage_are_skipped() {
        assert_eq!(
            parse_line(r#"{"type":"user","message":{"content":"hi"}}"#),
            None
        );
        assert_eq!(
            parse_line(r#"{"type":"assistant","message":{"id":"m"}}"#),
            None
        );
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line(r#"{"type":"queue-operation"}"#), None);
    }

    /// 文件头那条：不去重就是两倍。实机上 6049 条里 2860 条是重放。
    #[test]
    fn replayed_rows_are_counted_once() {
        let rows = vec![
            parse_line(&line(
                "m1",
                "r1",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:00Z",
            ))
            .unwrap(),
            parse_line(&line(
                "m2",
                "r2",
                "claude-opus-5",
                60,
                "2026-09-16T10:01:00Z",
            ))
            .unwrap(),
            // 续接之后重放的同两条
            parse_line(&line(
                "m1",
                "r1",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:00Z",
            ))
            .unwrap(),
            parse_line(&line(
                "m2",
                "r2",
                "claude-opus-5",
                60,
                "2026-09-16T10:01:00Z",
            ))
            .unwrap(),
        ];
        let (kept, dropped) = dedupe(rows);
        assert_eq!(dropped, 2);
        let (buckets, _) = buckets_of(&kept);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].output, 100, "重放的那两条不许再加一遍");
        assert_eq!(buckets[0].messages, 2);
    }

    /// 同一条消息真的发了两次请求 = 两次真实开销，不许合成一条。
    #[test]
    fn the_same_message_with_two_request_ids_stays_two_rows() {
        let rows = vec![
            parse_line(&line(
                "m1",
                "r1",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:00Z",
            ))
            .unwrap(),
            parse_line(&line(
                "m1",
                "r2",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:05Z",
            ))
            .unwrap(),
        ];
        let (kept, dropped) = dedupe(rows);
        assert_eq!((kept.len(), dropped), (2, 0));
    }

    #[test]
    fn buckets_split_by_day_and_by_model() {
        let rows = vec![
            parse_line(&line(
                "a",
                "1",
                "claude-opus-5",
                10,
                "2026-09-15T10:00:00+08:00",
            ))
            .unwrap(),
            parse_line(&line(
                "b",
                "2",
                "claude-sonnet-5",
                20,
                "2026-09-15T11:00:00+08:00",
            ))
            .unwrap(),
            parse_line(&line(
                "c",
                "3",
                "claude-opus-5",
                30,
                "2026-09-16T11:00:00+08:00",
            ))
            .unwrap(),
        ];
        let (buckets, undated) = buckets_of(&rows);
        assert_eq!(undated, 0);
        assert!(buckets.len() >= 2, "至少按模型和天分开了");
        let days: Vec<_> = buckets.iter().map(|b| b.day.as_str()).collect();
        assert!(days.windows(2).all(|w| w[0] <= w[1]), "{days:?} 不是升序");
    }

    /// 没有时刻的条目落不进任何一天。要有个数，不能悄悄消失。
    #[test]
    fn rows_without_a_timestamp_are_counted_not_dropped_silently() {
        let e = parse_line(
            r#"{"type":"assistant","uuid":"u1","message":{"id":"m1","model":"x",
               "usage":{"input_tokens":1,"output_tokens":2}}}"#,
        )
        .expect("没有 timestamp 也要解得出来");
        assert_eq!(e.at, i64::MIN);
        let (buckets, undated) = buckets_of(&[e]);
        assert!(buckets.is_empty());
        assert_eq!(undated, 1);
    }

    /// 缺字段按 0 算，不是整条丢掉 —— 实机上 `<synthetic>` 那几条就没有
    /// 缓存字段。模型名原样留着，界面自己决定要不要筛掉。
    #[test]
    fn missing_token_fields_count_as_zero_and_the_model_name_is_kept_verbatim() {
        let e = parse_line(
            r#"{"type":"assistant","uuid":"u1","timestamp":"2026-09-16T10:00:00Z",
               "message":{"id":"m1","model":"<synthetic>","usage":{"input_tokens":5}}}"#,
        )
        .unwrap();
        assert_eq!((e.output, e.cache_write, e.cache_read), (0, 0, 0));
        assert_eq!(e.model, "<synthetic>");
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        // ⛔ 单测不许碰 `%LOCALAPPDATA%\ClaudeIpGate\`。
        let d = std::env::temp_dir().join(format!(
            "qbgate-tokens-{tag}-{}-{}",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 没跑过会话的槽位是正常状态，不是故障。
    #[test]
    fn a_slot_with_no_transcripts_reports_zeroes_not_an_error() {
        let d = scratch("empty");
        let got = for_slot(&d);
        assert_eq!(got.files_read, 0);
        assert_eq!(got.files_failed, 0);
        assert!(got.buckets.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 两个转写文件、其中一条是重放，走完整条 I/O 路径。
    #[test]
    fn a_slot_is_read_across_project_folders() {
        let d = scratch("read");
        let p1 = d.join("projects/proj-a");
        let p2 = d.join("projects/proj-b");
        std::fs::create_dir_all(&p1).unwrap();
        std::fs::create_dir_all(&p2).unwrap();
        std::fs::write(
            p1.join("s1.jsonl"),
            format!(
                "{}\n{}\n{}\n",
                r#"{"type":"user","message":{"content":"hi"}}"#,
                line("m1", "r1", "claude-opus-5", 40, "2026-09-16T10:00:00Z"),
                line("m2", "r2", "claude-opus-5", 60, "2026-09-16T10:01:00Z"),
            ),
        )
        .unwrap();
        // 续接出来的第二个会话，重放了 m2。
        std::fs::write(
            p2.join("s2.jsonl"),
            format!(
                "{}\n{}\n",
                line("m2", "r2", "claude-opus-5", 60, "2026-09-16T10:01:00Z"),
                line("m3", "r3", "claude-opus-5", 5, "2026-09-16T10:02:00Z"),
            ),
        )
        .unwrap();
        // 不是 .jsonl 的不许扫进来。
        std::fs::write(p2.join("notes.txt"), "\"usage\"").unwrap();

        let got = for_slot(&d);
        assert_eq!(got.files_read, 2);
        assert_eq!(got.sessions, 2);
        assert_eq!(got.duplicates, 1, "m2 被重放了一次");
        let total: i64 = got.buckets.iter().map(|b| b.output).sum();
        assert_eq!(total, 105, "40 + 60 + 5，重放的那条不再加");
        let _ = std::fs::remove_dir_all(&d);
    }
}
