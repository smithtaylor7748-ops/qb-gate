//! 反重力用掉的 token：从它的语言服务器写在本机的对话记录库里数出来。
//!
//! # 数据从哪来
//!
//! `~\.gemini\antigravity*\conversations\<对话 id>.db`（Hub 是 `antigravity`，IDE 是
//! `antigravity-ide`，两个目录都扫）。每个库一段对话，SQLite：
//!
//! | 表 | 用到的列 | 内容 |
//! |---|---|---|
//! | `gen_metadata` | `idx`, `data` | 每一次模型生成一条，`data` 是 protobuf |
//! | `steps` | `idx`, `metadata` | 每一步一条，`metadata` 的 1 号字段是创建时间（Timestamp） |
//!
//! `gen_metadata.data` 的形状（2026-09-21 对着本机 49 个库、12,410 条记录数出来的，
//! 用 [`super::proto`] 逐 tag 走，**不是抄的**）：
//!
//! ```text
//! 1  生成记录
//!    4     用量
//!          1  系统提示 token（同一模型下是常数：1298 / 1318 / 1026）
//!          2  输入 token（不含缓存那部分 —— 实测 5 号常常比它大好几倍）
//!          3  输出 token（= 9 + 10 两项之和，实测三条都对得上）
//!          5  缓存读 token（只在真命中时出现）
//!    9.4   时间戳（Timestamp）—— 12,410 条里 10,643 条有，其余用 steps 里那一步的创建时间兜底
//!    19    模型 id（"gemini-3.7-flash"）
//!    21    显示名（"Gemini 3.7 Flash (High)"）
//! ```
//!
//! 字段含义是按数值关系**推断**的，跟 Antigravity-Tools-Lite 的读法一致（它也把 1–5 读作
//! 系统提示 / 输入 / 输出 / 缓存），但两边都没有 Google 的 schema。界面上要写「按实测推断」。
//!
//! # 口径
//!
//! - **输入** = 系统提示 + 输入（都是没走缓存、按整价计的那部分）；
//! - **缓存读** = 5 号；**缓存写** 恒为 0 —— Gemini 没有按 token 的缓存写价，记录里也没有这一项；
//! - 一条生成记录算一条「回复」。
//!
//! 装进 [`TokenBucket`]（按本地日期 × 模型分桶），然后走 Claude 那套 `token_summary::summarize`——
//! 命中率、缓存省下的钱、时间档都跟 Claude 页同一个算法，不另起一套。
//!
//! # ⛔ 旧 `.pb` 归档不计入
//!
//! `conversations\` 里还有一批 `.pb`（2026-08-14 之前的旧格式，本机 16 份、9.7 MB），
//! 里面一个模型名都找不到（大概率压缩过或另有封装）。**不猜**：数出来报「N 份旧归档未计入」。
//!
//! # 不分账户
//!
//! 记录里没有账户标识，而且两个产品共用同一个 Google 登录。这里数的是**这台机器上**反重力的
//! 全部用量，界面上要写明「不分账户」—— 别按「此刻登录的是谁」去摊派历史。
//!
//! # 只读
//!
//! `SQLITE_OPEN_READ_ONLY`。语言服务器正在写的那个库（带 `-wal`）也照读，WAL 允许并发读；
//! 打不开的记进 `files_failed`，**不许把打不开当成 0**。

use super::proto::{timestamp_secs, Message};
use crate::accounts::tokens::TokenBucket;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 一次扫描的结果。计数不是装饰：它们是「这份统计覆盖了多少」的证据。
#[derive(Debug, Default, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityUsageScan {
    pub buckets: Vec<TokenBucket>,
    /// 读成功的 `.db` 数。
    #[ts(type = "number")]
    pub files_read: u32,
    /// 打不开 / 表不对 / 读到一半出错的 `.db` 数。
    #[ts(type = "number")]
    pub files_failed: u32,
    /// 旧 `.pb` 归档数（没读）。
    #[ts(type = "number")]
    pub legacy_skipped: u32,
    /// 解不出用量或时间的生成记录数。
    #[ts(type = "number")]
    pub incomplete: u32,
    /// 同一个对话 id 在第二个目录里又出现一次（`antigravity-backup` 之类），只数第一份。
    #[ts(type = "number")]
    pub duplicates: u32,
    /// 扫过的目录（给「统计说明」看）。
    pub scanned_dirs: Vec<String>,
}

/// 一条解出来的生成记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    /// Unix 秒。
    pub at: i64,
    pub model: String,
    pub system: u64,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

/// `~\.gemini` 下所有 `antigravity*\conversations` 目录（在的才算）。
pub fn conversation_dirs(home: &Path) -> Vec<PathBuf> {
    let gemini = home.join(".gemini");
    let Ok(entries) = std::fs::read_dir(&gemini) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("antigravity"))
        .map(|e| e.path().join("conversations"))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// 解一条 `gen_metadata.data`。时间戳缺时用 `fallback_at`（那一步的创建时间）。
///
/// **纯函数。** 解不出用量（没有 1.4）→ `None`；有用量没模型名 → 模型记作 `unknown`。
pub fn parse_generation(data: &[u8], fallback_at: Option<i64>) -> Option<Generation> {
    let outer = Message::parse(data).ok()?;
    let gen = outer.sub(1)?;
    let usage = gen.sub(4)?;
    let at = gen
        .sub(9)
        .and_then(|t| t.bytes(4))
        .and_then(timestamp_secs)
        .filter(|s| *s > 0)
        .or(fallback_at)?;
    Some(Generation {
        at,
        model: gen
            .str(19)
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string(),
        system: usage.varint(1).unwrap_or(0),
        input: usage.varint(2).unwrap_or(0),
        output: usage.varint(3).unwrap_or(0),
        cached: usage.varint(5).unwrap_or(0),
    })
}

/// `steps.metadata` 的 1 号字段：这一步的创建时间。
pub fn parse_step_created(metadata: &[u8]) -> Option<i64> {
    Message::parse(metadata)
        .ok()?
        .bytes(1)
        .and_then(timestamp_secs)
        .filter(|s| *s > 0)
}

fn local_day(secs: i64) -> Option<String> {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(t) => Some(t.format("%Y-%m-%d").to_string()),
        _ => None,
    }
}

/// 读一个库里的全部生成记录。表不对 / 打不开 → `Err`。
fn read_db(path: &Path, incomplete: &mut u32) -> rusqlite::Result<Vec<Generation>> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // 每一步的创建时间：生成记录自己没带时间戳时拿同一个 idx 的这一步兜底。
    let mut created: BTreeMap<i64, i64> = BTreeMap::new();
    {
        let mut st = conn.prepare("SELECT idx, metadata FROM steps WHERE metadata IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        for row in rows.flatten() {
            if let Some(at) = parse_step_created(&row.1) {
                created.insert(row.0, at);
            }
        }
    }
    let mut out = Vec::new();
    let mut st = conn.prepare("SELECT idx, data FROM gen_metadata WHERE data IS NOT NULL")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
    for row in rows {
        let (idx, data) = row?;
        match parse_generation(&data, created.get(&idx).copied()) {
            Some(g) => out.push(g),
            None => *incomplete += 1,
        }
    }
    Ok(out)
}

/// 把生成记录装进按「本地日期 × 模型」的桶。**纯函数。**
pub fn bucketize(gens: &[Generation]) -> Vec<TokenBucket> {
    let mut map: BTreeMap<(String, String), TokenBucket> = BTreeMap::new();
    for g in gens {
        let Some(day) = local_day(g.at) else { continue };
        let b = map
            .entry((day.clone(), g.model.clone()))
            .or_insert_with(|| TokenBucket {
                day,
                model: g.model.clone(),
                input: 0,
                output: 0,
                cache_write: 0,
                cache_write_1h: 0,
                cache_read: 0,
                messages: 0,
            });
        b.input += (g.system + g.input) as i64;
        b.output += g.output as i64;
        b.cache_read += g.cached as i64;
        b.messages += 1;
    }
    map.into_values().collect()
}

/// 扫这些目录里的全部 `.db`。
pub fn scan(dirs: &[PathBuf]) -> AntigravityUsageScan {
    let mut out = AntigravityUsageScan {
        scanned_dirs: dirs.iter().map(|d| d.display().to_string()).collect(),
        ..Default::default()
    };
    let mut gens: Vec<Generation> = Vec::new();
    // 对话 id 就是文件名。`antigravity-backup\` 这种整目录备份会让同一段对话出现两次 ——
    // 只数第一份（目录按名字排序，正本 `antigravity` 排在备份前面）。
    let mut seen: std::collections::HashSet<std::ffi::OsString> = std::collections::HashSet::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            out.files_failed += 1;
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            match p.extension().and_then(|e| e.to_str()) {
                Some("db") if !seen.insert(p.file_name().unwrap_or_default().to_os_string()) => {
                    out.duplicates += 1;
                }
                Some("db") => match read_db(&p, &mut out.incomplete) {
                    Ok(mut g) => {
                        out.files_read += 1;
                        gens.append(&mut g);
                    }
                    Err(_) => out.files_failed += 1,
                },
                Some("pb") => out.legacy_skipped += 1,
                _ => {}
            }
        }
    }
    out.buckets = bucketize(&gens);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint_bytes(mut v: u64) -> Vec<u8> {
        let mut b = Vec::new();
        while v >= 0x80 {
            b.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        b.push(v as u8);
        b
    }
    fn tag(number: u32, wire: u8) -> Vec<u8> {
        varint_bytes(u64::from(number) << 3 | u64::from(wire))
    }
    fn fb(number: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = tag(number, 2);
        b.extend(varint_bytes(payload.len() as u64));
        b.extend_from_slice(payload);
        b
    }
    fn fv(number: u32, v: u64) -> Vec<u8> {
        let mut b = tag(number, 0);
        b.extend(varint_bytes(v));
        b
    }

    /// 照实机形状拼一条生成记录。`at` 为 None 时不带 9.4。
    fn generation(
        model: &str,
        sys: u64,
        input: u64,
        output: u64,
        cached: u64,
        at: Option<i64>,
    ) -> Vec<u8> {
        let mut usage = fv(1, sys);
        usage.extend(fv(2, input));
        usage.extend(fv(3, output));
        if cached > 0 {
            usage.extend(fv(5, cached));
        }
        let mut gen = fv(3, sys);
        gen.extend(fb(4, &usage));
        if let Some(at) = at {
            let ts = fv(1, at as u64);
            let mut nine = fv(2, u64::MAX);
            nine.extend(fb(4, &ts));
            gen.extend(fb(9, &nine));
        }
        gen.extend(fb(19, model.as_bytes()));
        gen.extend(fb(21, b"Display Name"));
        let mut outer = fb(2, &[0x08, 0x04]);
        outer.extend(fb(4, b"59ca648f-uuid"));
        outer.extend(fb(1, &gen));
        outer
    }

    #[test]
    fn a_generation_record_yields_model_tokens_and_its_own_timestamp() {
        let g = parse_generation(
            &generation("gemini-3.7-flash", 1298, 15249, 285, 0, Some(1_786_800_000)),
            None,
        )
        .unwrap();
        assert_eq!(g.model, "gemini-3.7-flash");
        assert_eq!(
            (g.system, g.input, g.output, g.cached),
            (1298, 15249, 285, 0)
        );
        assert_eq!(g.at, 1_786_800_000);
    }

    /// 没有 9.4 的记录用那一步的创建时间兜底；两个都没有 → 不计（进 incomplete）。
    #[test]
    fn a_record_without_its_own_timestamp_falls_back_to_the_step_time_or_is_dropped() {
        let bytes = generation("gemini-3.8-flash", 1318, 100, 10, 40_000, None);
        assert_eq!(
            parse_generation(&bytes, Some(1_786_900_000)).unwrap().at,
            1_786_900_000
        );
        assert_eq!(parse_generation(&bytes, None), None);
        let step = fb(1, &fv(1, 1_786_793_933));
        assert_eq!(parse_step_created(&step), Some(1_786_793_933));
        assert_eq!(parse_step_created(&[0x10, 0x01]), None);
    }

    /// 没有用量消息的 blob 不是「零 token」，是解不出来。
    #[test]
    fn garbage_and_usage_less_blobs_are_none_not_zero() {
        assert_eq!(parse_generation(b"\xff\xff", Some(1)), None);
        let no_usage = fb(1, &fb(19, b"gemini-3.7-flash"));
        assert_eq!(parse_generation(&no_usage, Some(1)), None);
    }

    /// 桶：系统提示并进输入；缓存读单列；缓存写恒为 0；一条记录一条回复。
    #[test]
    fn buckets_fold_system_prompt_into_input_and_never_write_cache() {
        let at = 1_786_800_000;
        let gens = vec![
            Generation {
                at,
                model: "m".into(),
                system: 1000,
                input: 200,
                output: 30,
                cached: 5000,
            },
            Generation {
                at: at + 60,
                model: "m".into(),
                system: 1000,
                input: 100,
                output: 20,
                cached: 0,
            },
            Generation {
                at: at + 120,
                model: "n".into(),
                system: 1,
                input: 1,
                output: 1,
                cached: 1,
            },
        ];
        let b = bucketize(&gens);
        assert_eq!(b.len(), 2);
        let m = b.iter().find(|x| x.model == "m").unwrap();
        assert_eq!(m.input, 2300);
        assert_eq!(m.output, 50);
        assert_eq!(m.cache_read, 5000);
        assert_eq!(m.cache_write, 0);
        assert_eq!(m.messages, 2);
        assert_eq!(m.day.len(), 10);
    }

    /// 端到端：搭一个真的 SQLite 库（临时目录），扫出来的数对得上；`.pb` 只计数；坏库计失败。
    #[test]
    fn scanning_a_conversation_directory_reads_db_files_and_counts_the_rest() {
        let root = std::env::temp_dir().join(format!("qb-ag-usage-{}", crate::config_io::id()));
        let conv = root
            .join(".gemini")
            .join("antigravity")
            .join("conversations");
        std::fs::create_dir_all(&conv).unwrap();
        std::fs::create_dir_all(
            root.join(".gemini")
                .join("antigravity-ide")
                .join("conversations"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".gemini").join("other")).unwrap();
        let dirs = conversation_dirs(&root);
        assert_eq!(dirs.len(), 2, "两个 antigravity* 目录，other 不算");

        let db = conv.join("a.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER, metadata BLOB);
             CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB, size INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO steps VALUES (3, 15, ?1)",
            [fb(1, &fv(1, 1_786_800_000))],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO gen_metadata VALUES (3, ?1, 0)",
            [generation("gemini-3.7-flash", 1298, 100, 10, 0, None)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO gen_metadata VALUES (4, ?1, 0)",
            [generation(
                "gemini-3.7-flash",
                1298,
                50,
                5,
                700,
                Some(1_786_800_100),
            )],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO gen_metadata VALUES (5, ?1, 0)",
            [b"\xff".to_vec()],
        )
        .unwrap();
        drop(conn);
        std::fs::write(conv.join("old.pb"), b"legacy").unwrap();
        std::fs::write(conv.join("broken.db"), b"not a database").unwrap();
        // 备份目录里同名的那份只计一次。
        std::fs::copy(
            &db,
            root.join(".gemini")
                .join("antigravity-ide")
                .join("conversations")
                .join("a.db"),
        )
        .unwrap();

        let scan = scan(&dirs);
        assert_eq!(scan.files_read, 1);
        assert_eq!(
            scan.duplicates, 1,
            "同一个对话 id 第二次出现只计数、不重复相加"
        );
        assert_eq!(scan.files_failed, 1, "坏库计失败，不当成 0");
        assert_eq!(scan.legacy_skipped, 1);
        assert_eq!(scan.incomplete, 1);
        assert_eq!(scan.buckets.len(), 1);
        let b = &scan.buckets[0];
        assert_eq!(b.model, "gemini-3.7-flash");
        assert_eq!(b.input, 1298 * 2 + 150);
        assert_eq!(b.output, 15);
        assert_eq!(b.cache_read, 700);
        assert_eq!(b.messages, 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
