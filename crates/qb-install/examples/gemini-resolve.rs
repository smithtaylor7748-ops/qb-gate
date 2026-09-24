//! 一次性核对工具：在这台机器上，面板到底去哪儿找 Gemini CLI、找没找到。
//! **只读盘，不装、不起进程。**
//!
//! 用法：
//!   cargo run -p qb-install --example gemini-resolve
//!
//! 为什么要有这个工具：入口路径写死过一次（`dist\index.js`，而官方包的入口是
//! `bundle\gemini.js`），症状是 **npm 明明装成功了，软件页还显示未安装、
//! 酒馆的 Gemini 桥接一直说找不到 CLI**。那种错在代码里看不出来 ——
//! 只有把「找过哪些路径、每条在不在」打出来，跟盘上的真实位置对一眼才看得见。
//! 换了机器（npm 前缀不同）或包换了布局，先跑这个。
use qb_install::install::detect;

fn main() {
    println!("== 候选包目录（顺序 = 优先级）==");
    for root in detect::gemini_cli_roots() {
        let mark = if root.is_dir() { "在" } else { "无" };
        println!("  [{mark}] {}", root.display());
        if !root.is_dir() {
            continue;
        }
        for entry in detect::gemini_cli_entries(&root) {
            let mark = if entry.is_file() { "✓" } else { "×" };
            println!("        {mark} {}", entry.display());
        }
    }

    println!("\n== node.exe ==");
    match detect::node_exe() {
        Some(p) => println!("  {}", p.display()),
        None => println!("  没找到（Gemini CLI 是 Node 包，桥接要用 node 起它）"),
    }

    println!("\n== 检测结论（软件页与酒馆插件看的就是这个）==");
    let s = detect::gemini_cli();
    println!("  installed = {}", s.installed);
    println!("  version   = {}", s.version.as_deref().unwrap_or("—"));
    println!(
        "  path      = {}",
        s.path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "—".into())
    );
    if let Some(a) = &s.advisory {
        println!("  advisory  = {a}");
    }
    if !s.installed {
        println!("  {}", detect::gemini_cli_searched());
    }
}
