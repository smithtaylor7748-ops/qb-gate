//! 开发工具：在这台机器上读一遍 Codex 自己写下的额度窗口，把结果打出来。
//!
//! ```powershell
//! cargo run -p qb-accounts --example codex-ratelimits
//! cargo run -p qb-accounts --example codex-ratelimits -- "C:\Users\<你>\.codex"
//! ```
//!
//! 单测**不许**碰真实的运行期状态，所以「解析器对不对得上这台机器的记录」只能在这里看。
//! OpenAI 改了记录形状、或者换机器读不出来时**先跑这个**，看清楚再改 `ratelimit.rs`——
//! 跟 `gemini-resolve` 是同一个用法（坑 7.62：位置/形状不许猜）。
//!
//! 只读。不打印任何令牌、邮箱或对话内容。

use qb_accounts::codex::ratelimit;
use std::path::PathBuf;

fn main() {
    let home = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))
        .unwrap_or_default();
    println!("Codex home：{}", home.display());

    let scan = match ratelimit::read(&home, 200) {
        Ok(s) => s,
        Err(e) => {
            println!("读不了：{e}");
            return;
        }
    };
    println!(
        "翻了 {} 个会话记录文件 · 失败 {}",
        scan.files_examined, scan.files_failed
    );
    if let Some(e) = &scan.first_error {
        println!("第一条失败的原因：{e}");
    }

    let Some(r) = scan.found else {
        println!(
            "没找到带额度信息的会话记录 —— 界面这时该说「还没有带额度信息的会话记录」，不是 0%。"
        );
        println!("（实测：走中转或 API Key 的会话里 rate_limits 整块是 null，很正常。）");
        return;
    };
    println!("出自：{}", r.source_file);
    println!("写入时间：{} · 落后 {} 分钟", r.measured_at, r.age_minutes);
    for (which, w) in [("primary", &r.primary), ("secondary", &r.secondary)] {
        match w {
            Some(w) => println!(
                "  {which}: {} · 已用 {}% · 还剩 {}% · 重置 {}",
                w.name,
                w.window.used,
                100 - w.window.used,
                w.window.resets_at.as_deref().unwrap_or("（没写）")
            ),
            None => println!("  {which}: 这条记录没带"),
        }
    }
    println!("档位：{}", r.plan_type.as_deref().unwrap_or("（没写）"));
    match &r.credits {
        Some(c) => println!(
            "点数：有点数 {} · 不限量 {} · 余额 {}",
            c.has_credits,
            c.unlimited,
            c.balance
                .map(|b| b.to_string())
                .unwrap_or_else(|| "（没写）".into())
        ),
        None => println!("点数：这条记录没带"),
    }
}
