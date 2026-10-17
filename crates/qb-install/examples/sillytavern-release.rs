//! 一次性核对工具：软件页「SillyTavern（酒馆）」取最新版、下源码包那两步，在这台机器上走不走得通。
//! **会联网（只问 SillyTavern 的官方 GitHub），不跑 npm、不写酒馆配置、不碰面板装的那份。**
//!
//! 用法：
//!   cargo run -p qb-install --example sillytavern-release -- latest
//!   cargo run -p qb-install --example sillytavern-release -- releases
//!   cargo run -p qb-install --example sillytavern-release -- download --to <一个空目录>
//!
//! `releases`（2026-09-29）读发布页的 Atom（「指定版本」那张清单），打印认出了哪几个版本、
//! 每个的日期与更新说明的字数 —— 界面开始报「一个版本都认不出来」时先跑这个。
//!
//! `latest` 只打印标签、版本、`engines.node` 和本机 Node.js。
//! `download` 再把那个标签的源码包下到 `<目录>\sillytavern.staging`（跟面板安装走同一段代码：
//! 同样的域名限制、大小上限、解压与布局检查），打印顶层有哪些东西 —— GitHub 或 SillyTavern
//! 改了布局、面板开始报「源码包的布局跟预期不一样」时先跑这个。
use qb_install::install::{sillytavern as st, sillytavern_setup as setup};
use qb_install::sink::Silent;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("latest");

    if mode == "releases" {
        println!("== 读 {} ==", setup::RELEASES_FEED);
        match setup::releases().await {
            Ok(list) => {
                for r in list {
                    println!(
                        "  {:<10} {}  更新说明 {} 字",
                        r.tag,
                        r.date,
                        r.notes.chars().count()
                    );
                }
            }
            Err(e) => {
                println!("  失败：{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    println!("== 问 {} ==", setup::LATEST_URL);
    let rel = match setup::latest().await {
        Ok(r) => r,
        Err(e) => {
            println!("  失败：{e}");
            std::process::exit(1);
        }
    };
    println!("  标签        = {}", rel.tag);
    println!("  版本        = {}", rel.version);
    println!(
        "  engines.node = {}",
        rel.node_engine
            .as_deref()
            .unwrap_or("读不到（raw.githubusercontent.com 连不上？）")
    );
    println!("  源码包      = {}", rel.archive_url);
    let node = setup::node_info();
    println!(
        "\n== 本机 Node.js：{} · {} ==",
        node.path.as_deref().unwrap_or("没找到"),
        node.version.as_deref().unwrap_or("版本读不出")
    );
    if let (Some(e), Some(v)) = (rel.node_engine.as_deref(), node.version.as_deref()) {
        println!(
            "  够不够：{}",
            match setup::node_satisfies(e, v) {
                Some(true) => "够",
                Some(false) => "不够",
                None => "判断不了（认不出 engines 的写法）",
            }
        );
    }

    if mode != "download" {
        return;
    }
    let Some(to) = args
        .iter()
        .position(|a| a == "--to")
        .and_then(|i| args.get(i + 1))
    else {
        println!("\ndownload 要带 --to <一个空目录>");
        std::process::exit(2);
    };
    let target = std::path::Path::new(to).join(st::DIR);
    println!("\n== 下载并解压到 {} ==", st::staging_of(&target).display());
    match setup::download_and_unpack(&rel, &target, &Silent).await {
        Ok((sha, dir)) => {
            println!("  SHA-256（只留痕）= {sha}");
            println!("  像 SillyTavern  = {}", st::looks_like_sillytavern(&dir));
            let pkg = st::read_package(&dir).unwrap_or_default();
            println!(
                "  package.json    = {} {}",
                pkg.name.as_deref().unwrap_or("?"),
                pkg.version.as_deref().unwrap_or("?")
            );
            println!("  Start.bat       = {}", st::launcher_in(&dir).is_file());
            println!("  config.yaml     = {}", dir.join("config.yaml").is_file());
            let mut names: Vec<String> = std::fs::read_dir(&dir)
                .map(|r| {
                    r.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            println!("  顶层 {} 项：{}", names.len(), names.join("  "));
        }
        Err(e) => {
            println!("  失败：{e}");
            std::process::exit(1);
        }
    }
}
