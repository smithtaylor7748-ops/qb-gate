//! 开发工具：真跑一次 DNS 泄露的回显那一半，把 bash.ws **回复的形状**打出来（2026-09-25）。
//!
//! ```powershell
//! cargo run -p qb-probe --example dns-leak
//! ```
//!
//! **会联网**，跟界面上点「开始检测」发的是同一批请求：向 bash.ws 取一个测试 id、用系统解析器
//! 解析 10 个探针域名、取回解析器清单（`probe::dns::fetch_echo`）。不读网卡、不改任何东西。
//!
//! **只打印形状、个数与国家码** —— 回显里有你的出口 IP 与解析器地址，一个地址都不打印。
//!
//! 为什么需要它：2026-09-25 有使用者那边 bash.ws 整份回的是一个 JSON 对象，原来的代码只认数组，
//! 报「JSON 解析失败: invalid type: map, expected a sequence」。现在对象也认（`dns::read_echo`），
//! 认不出来时界面上会说「回的不是解析器清单（…形状…）」—— 那时候**先跑这个**，看它到底回了什么。

use qb_probe::probe::dns;
use qb_probe::sink::Silent;

#[tokio::main]
async fn main() {
    let (text, lookups) = match dns::fetch_echo(&Silent).await {
        Ok(r) => r,
        Err(e) => {
            println!("没问到：{e}");
            return;
        }
    };
    println!(
        "探针：{} 个有回答，其中 {} 个是代理的 fake-ip（198.18.x.x 这类）",
        lookups.answered, lookups.fake_ip
    );
    println!();
    println!("bash.ws 回了 {} 字节：", text.len());
    match serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}').trim()) {
        Ok(v) => {
            println!("  顶层：{}", kind_of(&v));
            walk(&v, 0);
        }
        Err(e) => println!("  不是 JSON（{e}）"),
    }

    println!();
    match dns::read_echo(&text) {
        Ok(echo) => {
            let domestic = echo.resolvers.iter().filter(|r| r.is_domestic).count();
            let mut countries: Vec<String> = echo
                .resolvers
                .iter()
                .map(|r| r.country_code.clone().unwrap_or_else(|| "？".into()))
                .collect();
            countries.sort();
            countries.dedup();
            println!(
                "认出来了：{} 台解析器（国内 {domestic} 台；国家码 {}）· 出口 ASN {} · 结论行 {}",
                echo.resolvers.len(),
                if countries.is_empty() {
                    "（没有）".to_string()
                } else {
                    countries.join(" ")
                },
                if echo.egress_asn.is_some() {
                    "有"
                } else {
                    "没有"
                },
                if echo.conclusion.is_some() {
                    "有"
                } else {
                    "没有"
                },
            );
            if echo.resolvers.is_empty() {
                println!(
                    "  没有回显：{}",
                    if lookups.all_fake_ip() {
                        "探针全被代理的 fake-ip 接住了，查询没从本机发出去"
                    } else {
                        "界面上会说「不能判定」"
                    }
                );
            }
        }
        Err(e) => println!("⚠ 没认出来：{e}"),
    }
}

/// 只描述形状：键名、类型、长度。**不打印任何值。**
fn walk(v: &serde_json::Value, depth: usize) {
    if depth > 3 {
        return;
    }
    let pad = "  ".repeat(depth + 2);
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                // 键名也可能是地址（有的服务按 IP 分组），像 IP 的打码。
                let key = if k.parse::<std::net::IpAddr>().is_ok() {
                    "<IP>"
                } else {
                    k.as_str()
                };
                println!("{pad}{key}: {}", kind_of(val));
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
