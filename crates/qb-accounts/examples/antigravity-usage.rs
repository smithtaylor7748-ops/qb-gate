//! 开发工具：在这台机器上跑一遍反重力用量扫描与 IDE 账户状态读取，把结果打出来。
//!
//! ```powershell
//! cargo run -p qb-accounts --example antigravity-usage
//! cargo run -p qb-accounts --example antigravity-usage -- "C:\Users\<你>\AppData\Roaming\Antigravity IDE"
//! ```
//!
//! 单测**不许**碰真实的运行期状态，所以「解析器对不对得上这台机器的库」只能在这里看。
//! Google 改了记录形状时先跑这个，看清楚再改 `usage.rs` / `status.rs` 里的字段号。
//! 只读；不打印邮箱以外的任何身份字段，令牌一个字节都不读。

use qb_accounts::antigravity::{status, usage};
use std::collections::BTreeMap;

fn main() {
    let home = dirs::home_dir().unwrap_or_default();
    let dirs = usage::conversation_dirs(&home);
    println!("对话目录：{dirs:#?}");
    let scan = usage::scan(&dirs);
    println!(
        "读成功 {} 个库 · 失败 {} · 旧归档 {} · 解不出 {} 条",
        scan.files_read, scan.files_failed, scan.legacy_skipped, scan.incomplete
    );
    let mut per_model: BTreeMap<&str, (i64, i64, i64, i64)> = BTreeMap::new();
    let mut days: BTreeMap<&str, i64> = BTreeMap::new();
    for b in &scan.buckets {
        let e = per_model.entry(&b.model).or_default();
        e.0 += b.input;
        e.1 += b.output;
        e.2 += b.cache_read;
        e.3 += b.messages;
        *days.entry(&b.day).or_default() += b.messages;
    }
    for (m, (i, o, c, n)) in &per_model {
        println!("  {m}: 输入 {i} · 输出 {o} · 缓存读 {c} · {n} 条");
    }
    if let (Some(first), Some(last)) = (days.keys().next(), days.keys().next_back()) {
        println!("  日期范围 {first} … {last}，{} 天有记录", days.len());
    }

    let user_data = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_default()
                .join("Antigravity IDE")
        });
    println!("\nIDE 资料目录：{}", user_data.display());
    let login = status::login_state(&user_data);
    println!("登录态：{} · {}", login.logged_in, login.detail);
    match status::identity(&user_data) {
        Ok(Some(id)) => {
            println!(
                "身份：{} · 档位 {} ({}) · 写入 {}",
                id.email.as_deref().unwrap_or("—"),
                id.tier_name.as_deref().unwrap_or("—"),
                id.tier_id.as_deref().unwrap_or("—"),
                id.written_at
            );
            for m in &id.models {
                println!(
                    "  {:<32} 剩余 {:>5} · 重置 {} · {}",
                    m.label,
                    m.remaining
                        .map(|f| format!("{:.0}%", f * 100.0))
                        .unwrap_or_else(|| "—".into()),
                    m.reset_at.as_deref().unwrap_or("—"),
                    m.tags.join(" / ")
                );
            }
        }
        Ok(None) => println!("身份：状态库里没有 userStatus（还没登录过）"),
        Err(e) => println!("身份：读不出来：{e}"),
    }
}
