//! 开发工具：Claude 桌面端中文界面插件（claude-desktop-zh-cn）的「只查、只下、只核」（2026-09-25）。
//!
//! ```powershell
//! cargo run -p qb-app --example claude-zh -- check
//! ```
//!
//! 做的事跟界面上「一键汉化」的前半段一样，**到跑上游脚本之前为止**：
//!
//! 1. 本机：位置表认出的 Claude 桌面端（版本、`app-*` 目录、中文文件在不在）、各份资料的界面语言；
//! 2. 上游：问 `releases/latest` 的跳转拿最新标签（不走 api.github.com）；
//! 3. 下载那个标签的源码归档**到内存**，挑文件、核许可证、读脚本参数，把「一键汉化」会交给
//!    PowerShell 的参数原样打出来。
//!
//! ⛔ **不跑上游脚本、不关 Claude、不写本机任何东西**（插件自己的记录也不写）。
//! 上游改了归档结构、换了许可证、改了脚本参数时，界面会报「这一版不用」——先跑这个看清楚是哪一样。
use qb_app::plugins::claude_zh::{self, Action};
use qb_app::sink::Silent;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("check") {
        println!("用法：cargo run -p qb-app --example claude-zh -- check");
        return;
    }

    println!("== 本机");
    match claude_zh::target() {
        Some((version, dir)) => {
            println!("Claude 桌面端 {version}（Squirrel）：{}", dir.display());
            println!(
                "中文文件（resources\\zh-CN.json 与 ion-dist\\i18n\\zh-CN.json）：{}",
                if claude_zh::zh_files_present(&dir) {
                    "在"
                } else {
                    "不在"
                }
            );
        }
        None => println!("位置表没认出 Squirrel 装法的 Claude 桌面端（没装，或者是 MSIX 装的）"),
    }
    for dir in claude_zh::profile_dirs() {
        println!(
            "资料 {}：界面语言 {}",
            dir.display(),
            claude_zh::profile_locale(&dir)
                .as_deref()
                .unwrap_or("读不出来")
        );
    }

    println!("== 上游 {}", claude_zh::UPSTREAM_REPO);
    let tag = match claude_zh::latest_tag_online().await {
        Ok(t) => {
            println!("最新 Release：{t}");
            t
        }
        Err(e) => {
            println!("没问到：{e}");
            return;
        }
    };
    match claude_zh::fetch_verified(&tag, &Silent, 0).await {
        Ok(v) => {
            println!(
                "归档 SHA-256 {}，提交 {}",
                v.archive_sha256,
                v.commit.as_deref().unwrap_or("（归档注释里没有）")
            );
            println!("许可证：MIT（三句原文都在）");
            println!(
                "脚本参数：认得 install / uninstall / 安全模式{}",
                if v.caps.skip_asar_patch {
                    "，有 -SkipAsarPatch"
                } else {
                    "，没有 -SkipAsarPatch"
                }
            );
            println!("会落盘的 {} 个文件：", v.files.len());
            for (rel, body) in &v.files {
                println!("  {rel}（{} 字节）", body.len());
            }
            let script = claude_zh::packages_root()
                .join(&v.tag)
                .join(claude_zh::SCRIPT.replace('/', "\\"));
            for action in [Action::Install, Action::Uninstall] {
                println!(
                    "{action:?} 会交给 powershell 的参数：{:?}",
                    claude_zh::script_args(action, v.caps, &script)
                );
            }
        }
        Err(e) => println!("这一版面板不会用：{e}"),
    }
}
