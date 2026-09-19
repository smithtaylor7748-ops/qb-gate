//! 把站点的消费日志(New API 的 `/api/log/self`)解析成 [`UsageRow`]。
//!
//! **纯函数:只吃一段 JSON 文本,不联网、不落盘。** 拉取那一步在编排层 ——
//! 这个 crate 连 `reqwest` 都没有依赖,物理上发不出请求。
//!
//! # ⚠ 这张字段表是推断,不是核对过的
//!
//! 仓库里唯一有据可查的只有两件事:端点是 `/api/log/self`,首字延迟记在
//! `other.frt` 里(见 `health::window` 模块头)。**其余字段名是按 New API
//! 的常见写法推断的,还没有拿真实响应对过。**
//!
//! 所以这里的设计是「错了也不会撒谎」:
//!
//! - 认不出来的字段一律 `None`,**绝不拿 0 顶上**;
//! - 成败要有明确的字段才算数,认不出来就 [`status_reported`] = false,
//!   聚合那边会因此不给成功率,而不是给一个 100%。
//!
//! 对着真实响应核对之后,要改的只有下面 [`Keys`] 那一张表 ——
//! **别把字段名散进解析逻辑里**,那样下次核对就要翻遍整个文件。
//!
//! [`status_reported`]: UsageRow::status_reported

use super::model::UsageRow;
use serde_json::Value;

/// 各字段可能的名字。**改这里,不要改解析逻辑。**
///
/// 每一项都是「按顺序试,第一个命中的算数」。多写几个别名是便宜的,
/// 写死一个名字然后在别处兜底才是贵的。
pub struct Keys;

impl Keys {
    /// 时间戳。秒或毫秒都收,见 [`normalise_ts`]。
    pub const AT: &'static [&'static str] = &["created_at", "created_time", "timestamp", "time"];
    pub const GROUP: &'static [&'static str] = &["group", "token_group", "channel_group"];
    pub const MODEL: &'static [&'static str] = &["model_name", "model"];
    pub const PROMPT: &'static [&'static str] = &["prompt_tokens", "input_tokens"];
    pub const COMPLETION: &'static [&'static str] = &["completion_tokens", "output_tokens"];
    /// 实扣。New API 记在 `quota`。
    pub const COST: &'static [&'static str] = &["quota", "cost", "used_quota"];
    /// 缓存读。
    pub const CACHE_READ: &'static [&'static str] = &[
        "cache_read_tokens",
        "cache_read_input_tokens",
        "cached_tokens",
        "cache_tokens",
        "cache_read",
    ];
    /// 缓存写。
    pub const CACHE_WRITE: &'static [&'static str] = &[
        "cache_creation_tokens",
        "cache_creation_input_tokens",
        "cache_write_tokens",
        "cache_write",
    ];
    /// 首字延迟。**这一个是有据可查的**:New API 记在 `other.frt`。
    pub const FRT: &'static [&'static str] = &["frt", "first_token_time", "first_token_ms"];
    /// 总耗时。
    pub const TOTAL_MS: &'static [&'static str] = &["total_time", "use_time", "elapsed"];
    /// 成败。**认不出来就不算数** —— 见模块头。
    pub const STATUS: &'static [&'static str] = &["code", "status", "http_status", "status_code"];
}

/// 秒和毫秒都收。
///
/// 现在的 Unix 秒约 1.7e9、毫秒约 1.7e12,`1e11` 把两者干净地分开
/// (它当秒是公元 5138 年,当毫秒是 1973 年 —— 两边都不可能是真账单)。
fn normalise_ts(v: i64) -> i64 {
    if v.abs() < 100_000_000_000 {
        v.saturating_mul(1000)
    } else {
        v
    }
}

/// 取一个数。JSON 数字和数字字符串都收 —— 有些站点会把数字包成字符串。
fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub fn number(v: &Value) -> Option<f64> {
    num(v)
}

/// 按别名表找第一个命中的字段。
fn pick<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| obj.get(*k))
        .filter(|v| !v.is_null())
}

fn pick_f64(obj: &Value, keys: &[&str]) -> Option<f64> {
    pick(obj, keys).and_then(num)
}

/// 取一个非负整数。负数是坏数据,当作没有 —— 不是当作 0。
fn pick_u64(obj: &Value, keys: &[&str]) -> Option<u64> {
    pick_f64(obj, keys)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v as u64)
}

/// `other` 可能是对象,也可能是一段 JSON 字符串。两种都收。
///
/// 解不开时返回 `Null` 而不是报错:`other` 解不开只该让首字延迟变成「没有」,
/// 不该让整条账单记录作废 —— 那一行的 token 和花费仍然是有效证据。
fn other_of(row: &Value) -> Value {
    match row.get("other") {
        Some(Value::Object(m)) => Value::Object(m.clone()),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// 从一堆可能的外壳里把行数组挖出来。
///
/// 站点的分页外壳各写各的,这里把见过的几种都收下 ——
/// 认不出来时返回空数组,而不是报错:一个没见过的外壳不该让整页数据消失成一条报错。
fn rows_of(root: &Value) -> Vec<Value> {
    let candidates = [
        root.pointer("/data/items"),
        root.pointer("/data/records"),
        root.pointer("/data/logs"),
        root.pointer("/data/list"),
        root.get("data"),
        root.get("items"),
        root.get("records"),
        root.get("logs"),
    ];
    for c in candidates.into_iter().flatten() {
        if let Value::Array(a) = c {
            return a.clone();
        }
    }
    match root {
        Value::Array(a) => a.clone(),
        _ => Vec::new(),
    }
}

/// 解析一行。时间戳取不到就整行作废 —— 没有时间的账单行没法进任何时间窗。
fn parse_row(row: &Value) -> Option<UsageRow> {
    let at_ms = normalise_ts(pick_f64(row, Keys::AT)? as i64);
    let other = other_of(row);

    // 缓存两项:先在行上找,找不到再去 `other` 里找。
    let cache_read = pick_u64(row, Keys::CACHE_READ).or_else(|| pick_u64(&other, Keys::CACHE_READ));
    let cache_write =
        pick_u64(row, Keys::CACHE_WRITE).or_else(|| pick_u64(&other, Keys::CACHE_WRITE));

    // `input_uncached` 按约定是「已经减掉缓存读写」的那一部分。
    // 减出负数说明这三个数互相矛盾(站点报错了,或者名字对不上)——
    // 这时给 `None`,**不给 0**:0 会让缓存命中率变成 100%,
    // 而那正好是造假站点希望你看到的方向。
    let prompt = pick_u64(row, Keys::PROMPT);
    let input_uncached = match (prompt, cache_read, cache_write) {
        (Some(p), Some(r), Some(w)) => p.checked_sub(r).and_then(|x| x.checked_sub(w)),
        // 没有缓存字段时,prompt 本身就是未缓存输入。
        (Some(p), None, None) => Some(p),
        _ => None,
    };

    // 成败:有明确字段才算数。New API 的 `code` 用 0 表示正常,
    // HTTP 风格的 `status` 用 2xx 表示正常 —— 两种都收,认不出来就不算数。
    let status = pick_f64(row, Keys::STATUS).or_else(|| pick_f64(&other, Keys::STATUS));
    let (ok, status_reported) = match status {
        Some(0.0) => (true, true),
        Some(v) if (200.0..300.0).contains(&v) => (true, true),
        Some(_) => (false, true),
        None => (true, false),
    };

    Some(UsageRow {
        request_id: row
            .get("request_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        at_ms,
        group: pick(row, Keys::GROUP)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model: pick(row, Keys::MODEL)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        input_uncached,
        cache_read,
        cache_write,
        output: pick_u64(row, Keys::COMPLETION),
        first_token_ms: pick_u64(&other, Keys::FRT).or_else(|| pick_u64(row, Keys::FRT)),
        total_ms: pick_u64(row, Keys::TOTAL_MS).or_else(|| pick_u64(&other, Keys::TOTAL_MS)),
        cost: pick_f64(row, Keys::COST).filter(|c| c.is_finite()),
        ok,
        status_reported,
    })
}

/// 把一段 `/api/log/self` 的响应解析成账单行。
///
/// **认不出来的行会被跳过,不会让整批作废** —— 一行畸形数据不该
/// 把另外九十九行有效证据一起丢掉。整段 JSON 都解不开时返回空。
pub fn parse_self_log(body: &str) -> Vec<UsageRow> {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    rows_of(&root).iter().filter_map(parse_row).collect()
}

fn checked_rows(root: &Value) -> Result<Vec<Value>, String> {
    if root.is_array()
        || [
            "/data/items",
            "/data/records",
            "/data/logs",
            "/data/list",
            "/data",
            "/items",
            "/records",
            "/logs",
        ]
        .iter()
        .any(|path| root.pointer(path).is_some_and(Value::is_array))
    {
        Ok(rows_of(root))
    } else {
        Err("账单响应不包含可识别的明细列表，不能当成零消费".into())
    }
}

pub fn parse_self_log_checked(root: &Value) -> Result<Vec<UsageRow>, String> {
    let raw = checked_rows(root)?;
    let rows: Vec<_> = raw.iter().filter_map(parse_row).collect();
    if !raw.is_empty() && rows.is_empty() {
        return Err("账单记录无法解析时间戳".into());
    }
    Ok(rows)
}

/// Sub2API reports uncached input and dollar costs directly; do not subtract cache again.
pub fn parse_sub2_log(root: &Value) -> Result<Vec<UsageRow>, String> {
    let raw = checked_rows(root)?;
    let rows: Vec<_> = raw
        .iter()
        .filter_map(|row| {
            let at = pick(row, Keys::AT)?;
            let at_ms = num(at).map(|v| normalise_ts(v as i64)).or_else(|| {
                chrono::DateTime::parse_from_rfc3339(at.as_str()?)
                    .ok()
                    .map(|d| d.timestamp_millis())
            })?;
            let mut parsed = parse_row(&serde_json::json!({"created_at": at_ms}))?;
            parsed.model = pick(row, Keys::MODEL)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();
            parsed.request_id = row
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();
            parsed.group = row
                .pointer("/group/name")
                .and_then(Value::as_str)
                .or_else(|| row.get("group_name").and_then(Value::as_str))
                .or_else(|| row.get("group").and_then(Value::as_str))
                .unwrap_or_default()
                .into();
            parsed.input_uncached = pick_u64(row, &["input_tokens"]);
            parsed.cache_read = pick_u64(row, Keys::CACHE_READ);
            parsed.cache_write = pick_u64(row, Keys::CACHE_WRITE);
            parsed.output = pick_u64(row, &["output_tokens"]);
            parsed.cost = pick_f64(row, &["actual_cost"]).filter(|v| v.is_finite() && *v >= 0.0);
            parsed.first_token_ms = pick_u64(row, &["first_token_ms", "ttft_ms"]);
            parsed.total_ms = pick_u64(row, &["duration_ms"]);
            // A consumption-only ledger cannot establish the overall success rate.
            parsed.status_reported = false;
            Some(parsed)
        })
        .collect();
    if !raw.is_empty() && rows.is_empty() {
        return Err("Sub2API 账单记录无法解析时间戳".into());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub2_keeps_uncached_input_and_dollar_cost_separate() {
        let rows = parse_sub2_log(&serde_json::json!({"data":{"items":[{
            "created_at":"2026-09-18T12:00:00Z", "model":"gpt-5.6-sol", "group":{"name":"team"},
            "input_tokens":100, "cache_read_tokens":900, "cache_creation_tokens":0,
            "output_tokens":20, "actual_cost":0.02, "first_token_ms":321
        }]}}))
        .unwrap();
        assert_eq!(rows[0].input_uncached, Some(100));
        assert_eq!(rows[0].input_total(), Some(1000));
        assert_eq!(rows[0].cost, Some(0.02));
        assert_eq!(rows[0].group, "team");
        assert!(!rows[0].status_reported);
    }

    #[test]
    fn a_login_response_is_not_an_empty_ledger() {
        assert!(
            parse_self_log_checked(&serde_json::json!({"success":false,"message":"login"}))
                .is_err()
        );
        assert!(parse_sub2_log(&serde_json::json!({"data":{"items":[]}}))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_new_api_shaped_payload_parses() {
        let body = r#"{"success":true,"data":{"items":[
            {"created_at":1700000000,"model_name":"claude-sonnet-4.5","group":"default",
             "prompt_tokens":1000,"completion_tokens":200,"quota":1234,
             "other":"{\"frt\":432,\"cache_tokens\":600}"}
        ]}}"#;
        let rows = parse_self_log(body);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.at_ms, 1_700_000_000_000);
        assert_eq!(r.model, "claude-sonnet-4.5");
        assert_eq!(r.group, "default");
        assert_eq!(r.first_token_ms, Some(432));
        assert_eq!(r.cache_read, Some(600));
        assert_eq!(r.output, Some(200));
        assert_eq!(r.cost, Some(1234.0));
    }

    #[test]
    fn seconds_and_milliseconds_both_land_on_milliseconds() {
        for (raw, want) in [
            (1_700_000_000i64, 1_700_000_000_000i64),
            (1_700_000_000_000, 1_700_000_000_000),
        ] {
            let body = format!(r#"[{{"created_at":{raw}}}]"#);
            assert_eq!(parse_self_log(&body)[0].at_ms, want);
        }
    }

    #[test]
    fn a_row_without_a_timestamp_is_dropped_but_the_others_survive() {
        // 一行畸形不该把另外几行有效证据一起丢掉。
        let body = r#"[{"model_name":"a"},{"created_at":1700000000,"model_name":"b"}]"#;
        let rows = parse_self_log(body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "b");
    }

    #[test]
    fn an_unreported_status_is_not_treated_as_success() {
        // 这是整个模块的立身之本:认不出成败就不作数,
        // 否则每一家站点的成功率都是 100%。
        let body = r#"[{"created_at":1700000000}]"#;
        let r = &parse_self_log(body)[0];
        assert!(!r.status_reported);
    }

    #[test]
    fn explicit_status_codes_are_honoured_in_both_conventions() {
        let cases = [
            (r#"{"created_at":1,"code":0}"#, true),
            (r#"{"created_at":1,"code":500}"#, false),
            (r#"{"created_at":1,"status":200}"#, true),
            (r#"{"created_at":1,"status":429}"#, false),
        ];
        for (row, want_ok) in cases {
            let r = &parse_self_log(&format!("[{row}]"))[0];
            assert!(r.status_reported, "{row} 应当算作报了成败");
            assert_eq!(r.ok, want_ok, "{row}");
        }
    }

    #[test]
    fn inconsistent_token_fields_yield_none_rather_than_a_flattering_zero() {
        // 缓存读写加起来比 prompt 还大 —— 三个数互相矛盾。
        // 给 0 会让命中率算成 100%,正好是造假站点想要的方向。
        let body = r#"[{"created_at":1,"prompt_tokens":100,
            "other":"{\"cache_tokens\":900,\"cache_creation_input_tokens\":0}"}]"#;
        let r = &parse_self_log(body)[0];
        assert_eq!(r.input_uncached, None);
        assert_eq!(r.input_total(), None);
    }

    #[test]
    fn prompt_tokens_alone_count_as_uncached_input() {
        let body = r#"[{"created_at":1,"prompt_tokens":100}]"#;
        let r = &parse_self_log(body)[0];
        assert_eq!(r.input_uncached, Some(100));
    }

    #[test]
    fn other_can_be_an_object_as_well_as_a_string() {
        let body = r#"[{"created_at":1,"other":{"frt":77}}]"#;
        assert_eq!(parse_self_log(body)[0].first_token_ms, Some(77));
    }

    #[test]
    fn an_unparseable_other_only_costs_that_one_field() {
        // `other` 解不开不该让整行作废 —— token 和花费仍然是有效证据。
        let body = r#"[{"created_at":1,"quota":9,"prompt_tokens":5,"other":"not json"}]"#;
        let r = &parse_self_log(body)[0];
        assert_eq!(r.first_token_ms, None);
        assert_eq!(r.cost, Some(9.0));
        assert_eq!(r.input_uncached, Some(5));
    }

    #[test]
    fn numbers_wrapped_in_strings_are_accepted() {
        // 有些站点把数字包成字符串。认不出来的话花费会整列变成「—」。
        let body = r#"[{"created_at":"1700000000","quota":"12.5"}]"#;
        let r = &parse_self_log(body)[0];
        assert_eq!(r.at_ms, 1_700_000_000_000);
        assert_eq!(r.cost, Some(12.5));
    }

    #[test]
    fn several_envelope_shapes_are_understood() {
        for body in [
            r#"{"data":{"items":[{"created_at":1}]}}"#,
            r#"{"data":{"records":[{"created_at":1}]}}"#,
            r#"{"data":[{"created_at":1}]}"#,
            r#"{"items":[{"created_at":1}]}"#,
            r#"[{"created_at":1}]"#,
        ] {
            assert_eq!(parse_self_log(body).len(), 1, "外壳没认出来: {body}");
        }
    }

    #[test]
    fn garbage_yields_an_empty_batch_rather_than_a_panic() {
        for body in ["", "null", "{}", "not json at all", r#"{"data":{}}"#] {
            assert!(parse_self_log(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn negative_token_counts_are_treated_as_absent_not_as_zero() {
        let body = r#"[{"created_at":1,"completion_tokens":-5}]"#;
        assert_eq!(parse_self_log(body)[0].output, None);
    }
}
