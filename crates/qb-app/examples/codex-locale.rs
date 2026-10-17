//! 开发工具：看 GPT（Codex 桌面端）的界面语言设置读不读得到（2026-09-27）。
//!
//! ```powershell
//! cargo run -p qb-app --example codex-locale -- check
//! ```
//!
//! 逐份列出面板会改的 `config.toml`（每个 GPT 槽位 + 默认那份）：GPT 读不读得了这份（它用严格解析器，
//! 同一张表重复定义一次整份桌面设置就作废）、能不能自动修、现在设的界面语言、GPT 上次运行时用的语言；
//! 再加上开着几个 GPT 桌面端。GPT 界面还是英文时先跑它：
//!
//! - 这一份「GPT 读不了」→ 点「设为中文」会先把重复的表并掉；
//! - 设的是 zh-CN、GPT 上次运行也是 zh-CN，界面仍是英文 → OpenAI 没给这个账户放开中文（`enable_i18n`），
//!   面板管不了。
//!
//! ⛔ **只读：不写任何文件、不关不起 GPT、不联网；配置里别的内容一个字都不打印。**
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("check") {
        println!("用法：cargo run -p qb-app --example codex-locale -- check");
        return;
    }
    let status = match qb_app::usecase::codex_locale::status().await {
        Ok(s) => s,
        Err(e) => {
            println!("读不出来：{e}");
            return;
        }
    };
    if status.homes.is_empty() {
        println!("没有 GPT 槽位，也没有默认的 ~\\.codex。");
    }
    for h in &status.homes {
        println!("{}：{}", h.label, h.config);
        match (&h.error, &h.repair) {
            (Some(e), _) => println!("  GPT 读不了这份，也没法自动修：{e}"),
            (None, Some(tables)) => println!(
                "  GPT 读不了这份：{tables} 重复定义了。点「设为中文」会先并掉重复段（不改任何值）"
            ),
            (None, None) => println!(
                "  GPT 读得了；界面语言：{}",
                h.current
                    .as_deref()
                    .unwrap_or("没设（跟随 GPT 自己的默认）")
            ),
        }
        println!(
            "  GPT 上次运行时用的：{}",
            h.adopted
                .as_deref()
                .unwrap_or("没记录（这份资料里没有 computer-use\\config.json）")
        );
    }
    println!(
        "面板起的 GPT 开着 {} 个，别处起的开着 {} 个{}{}",
        status.ours_running,
        status.foreign_running,
        status
            .processes_error
            .as_deref()
            .map(|e| format!("（查进程出错：{e}）"))
            .unwrap_or_default(),
        if status.egress_running {
            "；Codex 出站插件在跑（这时不改）"
        } else {
            ""
        }
    );
    println!(
        "上次点的是：{}",
        if status.enabled {
            "设为中文"
        } else {
            "没点过 / 恢复默认"
        }
    );
}
