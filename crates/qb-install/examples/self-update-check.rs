//! 一次性核对工具：面板的「发现新版本 / 一键更新」那条链，在这台机器上走不走得通。
//! **会联网（只问本项目的 GitHub 发布页），不安装、不退出任何东西。**
//!
//! 用法：
//!   cargo run -p qb-install --example self-update-check
//!   cargo run -p qb-install --example self-update-check -- --as 0.25.2
//!   cargo run -p qb-install --example self-update-check -- --as 0.25.2 --download
//!
//! `--as <版本>` 假装当前装的是那一版（默认是这份源码的版本号）；`--download` 再把安装包
//! 下到 `%LOCALAPPDATA%\ClaudeIpGate\updates`、按 `SHA256SUMS.txt` 核一遍 —— 跟面板点「一键更新」
//! 走的是同一段代码，只是停在「交给安装包」之前。
//!
//! 为什么要有这个工具：一键更新要等**下一版**发出来，装着旧版的机器才第一次真正走得到。
//! 发版之后先跑这个：`update.json` 读不读得出、版本号认不认、哈希对不对得上，
//! 不用等使用者来报「弹窗没出来」。
use qb_install::install::self_update;
use qb_install::sink::Silent;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let current = args
        .iter()
        .position(|a| a == "--as")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let download = args.iter().any(|a| a == "--download");

    println!("== 问 {} ==", self_update::manifest_url());
    let info = match self_update::fetch_latest().await {
        Ok(i) => i,
        Err(e) => {
            println!("  失败：{e}");
            std::process::exit(1);
        }
    };
    println!("  版本     = {}", info.version);
    println!("  标签     = {}", info.tag);
    println!(
        "  发布时刻 = {}",
        info.published_at.as_deref().unwrap_or("—")
    );
    println!("  说明     = {} 个字符", info.notes.chars().count());
    println!("  页面     = {}", info.page_url);
    let newer = self_update::is_newer(&info.version, &current);
    println!(
        "\n== 当前按 {current} 算：{} ==",
        if newer {
            "有新版，面板会弹窗"
        } else {
            "没有新版，面板不弹"
        }
    );

    if !download {
        return;
    }
    println!("\n== 下载并核对（不安装）==");
    match self_update::download(&info, &Silent).await {
        Ok((path, sha)) => {
            println!("  已下载：{}", path.display());
            println!("  SHA-256 与 SHA256SUMS.txt 一致：{sha}");
        }
        Err(e) => {
            println!("  失败：{e}");
            std::process::exit(1);
        }
    }
}
