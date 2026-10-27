// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 用量脚本的沙箱（中转站重构阶段 6 · U4，cc-switch 的 `usage_script.rs`）。
//!
//! 使用者给某个站点写一小段 JS，求值得到一个对象：
//!
//! ```js
//! ({
//!   request: { url: "{{origin}}/v1/dashboard/billing/subscription", method: "GET",
//!              headers: { Authorization: "Bearer {{apiKey}}" } },
//!   extractor: function (response) { return { remaining: response.hard_limit_usd, unit: "USD" }; }
//! })
//! ```
//!
//! 面板先求出 `request`（[`request_of`]），**由编排层**核对地址、发请求，再把回复交给 `extractor`（[`extract`]）。
//!
//! # 这个 crate 只算，不联网、不碰文件
//!
//! QuickJS 本身没有 `fetch`、没有文件接口；这里也不往里注入任何宿主函数。脚本能做的只有算。
//! 限制照 cc-switch：**内存 16 MiB、栈 256 KiB、5 秒**（解释器每一轮都看一次钟，到点就中断，脚本接不住）。
//! 每一步都是新建的运行时，用完即丢，两次求值之间什么都不留。

use rquickjs::{Context, Function, Object, Runtime};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// 一段脚本最多多大（再大多半不是手写的用量脚本）。
pub const SCRIPT_MAX: usize = 64 << 10;
const MEMORY_LIMIT: usize = 16 << 20;
const STACK_LIMIT: usize = 256 << 10;
const TIMEOUT: Duration = Duration::from_secs(5);

/// 脚本要发的请求（`request` 那一段）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScriptRequest {
    pub url: String,
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

fn get() -> String {
    "GET".into()
}

/// 把 `{{名字}}` 换成真值（cc-switch 的 `build_script_with_vars`）。Key 只在跑的那一刻换进去，存下来的脚本里没有。
pub fn fill(script: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(script.to_string(), |s, (k, v)| {
        s.replace(&format!("{{{{{k}}}}}"), v)
    })
}

fn runtime() -> Result<(Runtime, Context), String> {
    let rt = Runtime::new().map_err(|e| format!("创建脚本运行时失败：{e}"))?;
    rt.set_memory_limit(MEMORY_LIMIT);
    rt.set_max_stack_size(STACK_LIMIT);
    let deadline = Instant::now() + TIMEOUT;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let ctx = Context::full(&rt).map_err(|e| format!("创建脚本上下文失败：{e}"))?;
    Ok((rt, ctx))
}

/// 脚本求值的报错：抛的是 JS 异常就带上它的消息，超时 / 超内存说清楚。
fn js_error(ctx: &rquickjs::Ctx<'_>, e: rquickjs::Error, what: &str) -> String {
    if e.is_exception() {
        let caught = ctx.catch();
        let msg = caught
            .as_exception()
            .and_then(|x| x.message())
            .or_else(|| caught.as_string().and_then(|s| s.to_string().ok()))
            .unwrap_or_else(|| "（没有消息）".into());
        if msg.contains("interrupted") {
            return format!("{what}：脚本跑超过 5 秒，被中断了");
        }
        return format!("{what}：{msg}");
    }
    if matches!(e, rquickjs::Error::Allocation) {
        return format!("{what}：脚本用的内存超过 16 MiB");
    }
    format!("{what}：{e}")
}

fn eval_object<'js>(ctx: &rquickjs::Ctx<'js>, script: &str) -> Result<Object<'js>, String> {
    if script.len() > SCRIPT_MAX {
        return Err("脚本太长了（最多 64 KiB）".into());
    }
    ctx.eval::<Object, _>(script.as_bytes()).map_err(|e| {
        js_error(
            ctx,
            e,
            "脚本求值没成（要求值成一个对象，形如 ({ request: …, extractor: … })）",
        )
    })
}

/// 第一步：求值，取出 `request`。
pub fn request_of(script: &str) -> Result<ScriptRequest, String> {
    let (_rt, ctx) = runtime()?;
    ctx.with(|ctx| {
        let config = eval_object(&ctx, script)?;
        let request: Object = config
            .get("request")
            .map_err(|_| "脚本里没有 request 对象".to_string())?;
        let json = ctx
            .json_stringify(request)
            .map_err(|e| js_error(&ctx, e, "request 转成 JSON 没成"))?
            .ok_or("request 转成 JSON 是空的")?
            .to_string()
            .map_err(|e| format!("request 读不出来：{e}"))?;
        serde_json::from_str::<ScriptRequest>(&json).map_err(|e| {
            format!("request 的形状不对（要有 url，method / headers / body 可选）：{e}")
        })
    })
}

/// 第二步：再求值一次，拿 `extractor` 对回复（JSON 文本）跑一遍，返回它给的值（对象或对象数组）。
pub fn extract(script: &str, response_json: &str) -> Result<Value, String> {
    let (_rt, ctx) = runtime()?;
    ctx.with(|ctx| {
        let config = eval_object(&ctx, script)?;
        let extractor: Function = config
            .get("extractor")
            .map_err(|_| "脚本里没有 extractor 函数".to_string())?;
        let response = ctx
            .json_parse(response_json)
            .map_err(|e| js_error(&ctx, e, "回复不是 JSON"))?;
        let out: rquickjs::Value = extractor
            .call((response,))
            .map_err(|e| js_error(&ctx, e, "extractor 跑出错了"))?;
        let json = ctx
            .json_stringify(out)
            .map_err(|e| js_error(&ctx, e, "extractor 的结果转成 JSON 没成"))?
            .ok_or("extractor 什么都没返回")?
            .to_string()
            .map_err(|e| format!("extractor 的结果读不出来：{e}"))?;
        let v: Value =
            serde_json::from_str(&json).map_err(|e| format!("extractor 的结果不是 JSON：{e}"))?;
        validate(&v)?;
        Ok(v)
    })
}

/// extractor 的结果：一个对象或一组对象；字段都可选，有就要是对的类型（cc-switch 的 `validate_single_usage`）。
pub fn validate(result: &Value) -> Result<(), String> {
    let items: Vec<&Value> = match result {
        Value::Array(a) if a.is_empty() => return Err("extractor 返回了空数组".into()),
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    for (i, item) in items.iter().enumerate() {
        let obj = item
            .as_object()
            .ok_or_else(|| format!("第 {} 项不是对象", i + 1))?;
        let check = |key: &str, ok: fn(&Value) -> bool, what: &str| -> Result<(), String> {
            match obj.get(key) {
                Some(v) if !v.is_null() && !ok(v) => {
                    Err(format!("第 {} 项的 {key} 要是{what}", i + 1))
                }
                _ => Ok(()),
            }
        };
        check("isValid", Value::is_boolean, "true / false")?;
        check("invalidMessage", Value::is_string, "字符串")?;
        check("remaining", Value::is_number, "数字")?;
        check("total", Value::is_number, "数字")?;
        check("used", Value::is_number, "数字")?;
        check("unit", Value::is_string, "字符串")?;
        check("planName", Value::is_string, "字符串")?;
        check("extra", Value::is_string, "字符串")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"({
        request: {
            url: "{{origin}}/v1/dashboard/billing/subscription",
            headers: { Authorization: "Bearer {{apiKey}}" }
        },
        extractor: function (r) {
            return { planName: "按 Key", remaining: r.hard_limit_usd, unit: "USD", isValid: r.hard_limit_usd > 0 };
        }
    })"#;

    #[test]
    fn the_request_comes_out_with_variables_filled_in() {
        let s = fill(
            SCRIPT,
            &[("origin", "https://relay.example"), ("apiKey", "sk-1")],
        );
        let r = request_of(&s).unwrap();
        assert_eq!(
            r.url,
            "https://relay.example/v1/dashboard/billing/subscription"
        );
        assert_eq!(r.method, "GET", "没写 method 就是 GET");
        assert_eq!(r.headers["Authorization"], "Bearer sk-1");
        assert_eq!(r.body, None);
        // 没认出来的 `{{…}}` 原样留着，不会被吃掉。
        assert!(fill("{{nope}}", &[("apiKey", "x")]).contains("{{nope}}"));
    }

    #[test]
    fn the_extractor_runs_on_the_reply_and_its_shape_is_checked() {
        let v = extract(SCRIPT, r#"{"hard_limit_usd": 12.5}"#).unwrap();
        assert_eq!(v["remaining"], 12.5);
        assert_eq!(v["isValid"], true);
        let bad = r#"({ request: { url: "x" }, extractor: function (r) { return { remaining: "很多" }; } })"#;
        assert!(extract(bad, "{}")
            .unwrap_err()
            .contains("remaining 要是数字"));
        let empty = r#"({ request: { url: "x" }, extractor: function () { return []; } })"#;
        assert!(extract(empty, "{}").unwrap_err().contains("空数组"));
        assert!(extract(SCRIPT, "not json")
            .unwrap_err()
            .contains("回复不是 JSON"));
    }

    #[test]
    fn a_runaway_script_is_stopped_and_cannot_hog_memory() {
        let spin = r#"(function () { while (true) {} })()"#;
        let t = Instant::now();
        let err = request_of(spin).unwrap_err();
        assert!(t.elapsed() < Duration::from_secs(15), "5 秒就该被中断");
        assert!(err.contains("5 秒") || err.contains("interrupt"), "{err}");
        let hog =
            r#"(function () { var a = []; while (true) { a.push(new Array(1e6).join("x")); } })()"#;
        assert!(request_of(hog).is_err());
    }

    #[test]
    fn the_sandbox_has_no_way_out() {
        // 没有 fetch、没有 require / process —— 脚本只能算。
        for probe in [
            "typeof fetch",
            "typeof require",
            "typeof process",
            "typeof XMLHttpRequest",
        ] {
            let s = format!(r#"({{ request: {{ url: {probe} }} }})"#);
            assert_eq!(request_of(&s).unwrap().url, "undefined", "{probe}");
        }
        let thrown = r#"({ request: (function () { throw new Error("坏了"); })() })"#;
        assert!(request_of(thrown).unwrap_err().contains("坏了"));
        assert!(request_of("42").is_err(), "不是对象");
        assert!(request_of("({})").unwrap_err().contains("没有 request"));
        assert!(request_of(&"x".repeat(SCRIPT_MAX + 1))
            .unwrap_err()
            .contains("太长"));
    }
}
