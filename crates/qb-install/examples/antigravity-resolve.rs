//! 一次性核对工具：真的去读一遍反重力官方下载页，看一键安装那条链认不认得出最新版。
//! **只读元数据，不下载、不安装。**
//!
//! 用法：
//!   cargo run -p qb-install --example antigravity-resolve              # 两个产品各查一次
//!   cargo run -p qb-install --example antigravity-resolve -- locate    # 同上（明确一点）
//!   cargo run -p qb-install --example antigravity-resolve -- dump      # 把页面里所有候选地址列出来
//!
//! 页面改版时先跑 `dump` 看清楚再改 `antigravity_setup::pick_download`，
//! 别拿断言去将就一个已经读错的解析器。
use qb_install::install::antigravity::Product;
use qb_install::install::antigravity_setup::{self as setup, Arch};

#[tokio::main]
async fn main() {
    let dump = std::env::args().any(|a| a == "dump");
    let arch = Arch::current();
    println!(
        "本机架构 {}",
        match arch {
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
        }
    );
    println!("下载页 {}", setup::DOWNLOAD_PAGE);

    if dump {
        // ⚠ 走的是 `setup::fetch_page`，跟面板真正用的**同一条路**。
        // 这个工具自己另写一份抓取，就会出现「工具里好好的、面板里拿不到」——
        // 实际栽过一次：面板的 reqwest 没开解压特性，拿回来的是压缩字节流。
        let body = match setup::fetch_page().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("读不到下载页：{e}");
                std::process::exit(1);
            }
        };
        let head: String = body.chars().take(300).collect();
        println!("页面 {} 字节。开头 300 字：", body.len());
        println!("{head}");
        println!("---- 页面里所有 .exe 候选：");
        let mut seen: Vec<String> = body
            .split(|c: char| c == '"' || c == '\'' || c == '\\' || c.is_whitespace())
            .filter(|u| u.starts_with("https://") && u.to_ascii_lowercase().ends_with(".exe"))
            .map(|u| {
                format!(
                    "{}  [域{}]",
                    u,
                    if setup::allowed_host(u) {
                        "通过"
                    } else {
                        "不通过"
                    }
                )
            })
            .collect();
        seen.sort();
        seen.dedup();
        for line in seen {
            println!("  {line}");
        }
        return;
    }

    let mut bad = false;
    for product in Product::ALL {
        match setup::resolve(product, arch).await {
            Ok(r) => {
                println!("\n{}：", product.label());
                println!("  版本 {}", r.version);
                println!("  地址 {}", r.url);
                println!("  winget 包 id {}", setup::winget_id(product));
                println!(
                    "  本机装了吗：{}",
                    if qb_install::install::antigravity::product_installed(product) {
                        "装了"
                    } else {
                        "没装"
                    }
                );
            }
            Err(e) => {
                bad = true;
                eprintln!("\n{}：失败 —— {e}", product.label());
            }
        }
    }
    if bad {
        std::process::exit(1);
    }
    println!("\n只查到这里为止 —— 没有下载、没有安装。");
}
