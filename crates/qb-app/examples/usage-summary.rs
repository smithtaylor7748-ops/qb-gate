//! 开发工具：拿真实数据核一遍用量明细页的数字（2026-09-24 起）。
//!
//! ```powershell
//! cargo run -p qb-app --example usage-summary -- --label main --days 7
//! ```
//!
//! **零网络。** 跟界面上「用量明细」走同一条路（`usecase::token_summary::overview`）：
//! 读槽位与默认目录里的会话转写、库里抓回来的官方价（没有就用内置快照）。
//! **只打印合计、条数与美元** —— 会话转写里全是对话原文，这里一个字都不打印，
//! 项目目录名也不打印。
//!
//! 什么时候跑：怀疑界面上的美元不对时，拿它跟一份独立的重算比（按 `message.id:requestId`
//! 去重、四类 token 各乘各的价、缓存写分 5 分钟 / 1 小时两档）。两边对不上，先怀疑价目，
//! 再怀疑归属（`tokens.rs` 文件头那三级判定）。

use qb_app::repository::Repository;
use qb_app::usecase::token_summary;
use qb_station::station::pricing;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(label) = arg("--label") else {
        println!("用法：--label <槽位名> [--days 1|7|30|0]（0 = 全部）");
        return;
    };
    let days: i64 = arg("--days").and_then(|d| d.parse().ok()).unwrap_or(1);

    let fetched: Vec<pricing::FetchedPrice> = Repository::open()
        .ok()
        .and_then(|db| db.meta("station_prices").ok().flatten())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    println!("抓回来的官方价 {} 条（0 条 = 全按内置快照）", fetched.len());

    let o = token_summary::overview(&label, days, &pricing::Catalog::new(fetched));
    let s = &o.summary;
    println!();
    println!("== {label} · days={days}（1 = 今天，0 = 全部）");
    println!(
        "回复 {} 条 · 另有 {} 条四类全 0 的报错没计入 · 最后一条 {}",
        s.messages,
        s.empty_replies,
        s.last_at.as_deref().unwrap_or("（没有）")
    );
    println!(
        "输入 {} · 输出 {} · 缓存写 {}（其中 1 小时档 {}）· 缓存读 {}",
        s.input, s.output, s.cache_write, s.cache_write_1h, s.cache_read
    );
    println!("按 API 价折算：{}", usd(s.cost_usd));
    if let Some(p) = &s.cost_parts {
        println!(
            "  输入 ${:.4} · 输出 ${:.4} · 缓存写 ${:.4} · 缓存读 ${:.4}",
            p.input, p.output, p.cache_write, p.cache_read
        );
    }
    println!(
        "今天 {} · 近 7 天 {} · 近 30 天 {}",
        usd(s.spend.today.usd),
        usd(s.spend.last_7d.usd),
        usd(s.spend.last_30d.usd)
    );
    println!();
    println!("按模型：");
    for m in &s.models {
        println!("  {} · {} 条 · {}", m.model, m.messages, usd(m.cost_usd));
    }
    for u in &s.unpriced {
        println!(
            "  没有官方价：{} · {} 条 · {} token",
            u.model, u.messages, u.tokens
        );
    }
    println!("按天：");
    for d in &s.daily {
        println!("  {} · {} 条 · {}", d.day, d.messages, usd(d.cost_usd));
    }
    println!("按账户：");
    for a in &o.by_account {
        println!(
            "  {:?} {} · {} 条 · {}",
            a.kind,
            a.label,
            a.messages,
            usd(a.cost_usd)
        );
    }
    if let Some(c) = &s.coverage {
        println!(
            "覆盖：读了 {} 个转写、{} 个没读成、去重丢掉 {} 条、{} 条没有时刻",
            c.files_read, c.files_failed, c.duplicates, c.undated
        );
    }
}

fn usd(v: Option<f64>) -> String {
    v.map_or("—".to_string(), |x| format!("${x:.2}"))
}
