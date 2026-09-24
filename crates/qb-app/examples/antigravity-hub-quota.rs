//! 开发工具：真的问一次反重力的联网额度，把**回复的形状**打出来（0.32.0 起；2026-09-23 改走新流程）。
//!
//! ```powershell
//! cargo run -p qb-app --example antigravity-hub-quota                     # 问 Hub
//! cargo run -p qb-app --example antigravity-hub-quota -- --account <槽位 id>   # 问一条账户槽位
//! ```
//!
//! # ⛔ 这是会联网的诊断例子
//!
//! 它跟界面上那颗刷新图标走的是同一条路（`usecase::antigravity_quota::exchange`）：
//! 读官方客户端存在本机的令牌、过期了在内存里换新、发三个只读调用
//! （`loadCodeAssist` / `retrieveUserQuotaSummary`，汇总拿不到时再加 `fetchAvailableModels`）。
//! **只打印键名、类型、长度**，以及解析器认出了什么 —— 一个值都不打印，令牌更不会出现在任何地方。
//!
//! 为什么需要它：这几个接口没有公开文档，形状随时会变，而「解析器认出来了吗」只有拿真实
//! 回复跑一遍才知道 —— 坑 7.62 的教训：位置与形状不许猜。
//!
//! 换机器、上游改版、界面上开始显示「读不出来」时，**先跑这个**。
//! 用的是你自己账户的令牌 —— 跑之前想清楚。

use qb_app::usecase::antigravity_quota::{self, Source};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = match args.iter().position(|a| a == "--account") {
        Some(i) => match args.get(i + 1) {
            Some(id) => Source::Account(id.clone()),
            None => {
                println!("--account 后面要跟槽位 id（账户页那一行的 id，在 antigravity-accounts\\index.json 里）");
                return;
            }
        },
        None => Source::Hub,
    };

    let (who, replies) = match antigravity_quota::exchange(&source).await {
        Ok(r) => r,
        Err(e) => {
            println!("没问到：{e}");
            return;
        }
    };
    println!("问的是：{who}");

    shape("loadCodeAssist", 200, &replies.load);
    shape(
        "retrieveUserQuotaSummary",
        replies.summary_status,
        &replies.summary,
    );
    if let Some((status, body)) = &replies.models {
        shape("fetchAvailableModels", *status, body);
    }

    println!();
    match antigravity_quota::parse_load(&replies.load) {
        Ok(l) => println!(
            "档位认出来了：{} / {} · GCP 条款 {} · 有 project {} · 积分字段 {}",
            l.tier_id.as_deref().unwrap_or("（没有）"),
            l.tier_name.as_deref().unwrap_or("（没有）"),
            l.gcp_tos,
            l.project.is_some(),
            if l.credits.is_some() { "有" } else { "没有" }
        ),
        Err(e) => println!("⚠ loadCodeAssist 没认出来：{e}"),
    }
    match antigravity_quota::parse_summary(&replies.summary) {
        Ok(w) => {
            println!("汇总认出来了 {} 格：", w.len());
            for x in &w {
                println!(
                    "  {:?} {:?} · 剩 {}%{} · 重置 {}",
                    x.group,
                    x.span,
                    (x.remaining * 100.0).round(),
                    if x.remaining_implied {
                        "（没给比例，按用光算）"
                    } else {
                        ""
                    },
                    x.reset_at.as_deref().unwrap_or("（没读到）")
                );
            }
        }
        Err(e) => println!("汇总没认出来：{e}"),
    }
    if let Some((_, body)) = &replies.models {
        match antigravity_quota::parse_models(body) {
            Ok(m) => println!("按模型认出来 {} 条", m.len()),
            Err(e) => println!("按模型也没认出来：{e}"),
        }
    }
}

fn shape(name: &str, status: u16, body: &str) {
    println!();
    println!("{name}：HTTP {status}，{} 字节", body.len());
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => walk(&v, 0),
        Err(e) => println!("  不是 JSON（{e}）"),
    }
}

/// 只描述形状：键名、类型、长度。**不打印任何值。**
fn walk(v: &serde_json::Value, depth: usize) {
    if depth > 4 {
        return;
    }
    let pad = "  ".repeat(depth + 1);
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                println!("{pad}{k}: {}", kind_of(val));
                walk(val, depth + 1);
            }
        }
        serde_json::Value::Array(items) => {
            // 数组只看第一项：同构数组打一百遍没有新信息。
            if let Some(first) = items.first() {
                println!("{pad}[0]: {}", kind_of(first));
                walk(first, depth + 1);
            }
        }
        _ => {}
    }
}

fn kind_of(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Bool(_) => "bool".into(),
        serde_json::Value::Number(_) => "number".into(),
        serde_json::Value::String(s) => format!("string，{} 个字符", s.chars().count()),
        serde_json::Value::Array(a) => format!("array，{} 项", a.len()),
        serde_json::Value::Object(o) => format!("object，{} 个键", o.len()),
    }
}
