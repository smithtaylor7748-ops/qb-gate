//! 开发工具：看 Claude 汉化还剩什么（2026-09-27：Claude 汉化撤掉了，只剩清理残留）。
//!
//! ```powershell
//! cargo run -p qb-app --example claude-zh -- leftovers
//! ```
//!
//! 列的跟账户页「清理汉化残留」弹窗里一样：各个 `app-*` 里上游放的中文文件、上游的备份
//! （多大、按大小看有几个文件跟原版对不上、能不能自动还原）、界面语言还是中文的资料、
//! 上游建的 `Claude-3p`、面板下载过的上游副本与那版插件的记录。
//!
//! ⛔ **只读：一样都不删、不关 Claude、不联网。**
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("leftovers") {
        println!("用法：cargo run -p qb-app --example claude-zh -- leftovers");
        return;
    }
    let l = qb_app::usecase::claude_zh_ops::leftovers().await;
    if l.empty {
        println!("没有残留。");
        return;
    }
    for a in &l.apps {
        println!(
            "Claude {}{}：{}",
            a.version,
            if a.current {
                "（现在用的这一版）"
            } else {
                "（旧版目录）"
            },
            a.dir
        );
        println!("  上游放的中文文件：{} 个", a.zh_files);
        match a.backup_mb {
            Some(mb) => println!(
                "  上游的备份：{mb} MB；按大小看跟原版对不上的 {} 个；{}",
                a.changed,
                if a.restorable {
                    "能自动还原"
                } else {
                    "不能自动还原（备份里有程序文件，或者读不懂）"
                }
            ),
            None => println!("  上游的备份：没有"),
        }
    }
    for p in &l.zh_profiles {
        println!(
            "资料 {}：界面语言 {}",
            p.dir,
            p.locale.as_deref().unwrap_or("读不出来")
        );
    }
    if let Some(d) = &l.stray_3p {
        println!("上游建的 {d}（里面只有一个写了界面语言的 config.json）");
    }
    if let Some(d) = &l.panel_copy {
        println!("面板下载过的上游副本：{d}");
    }
    if let Some(f) = &l.state_file {
        println!("那版插件的记录：{f}");
    }
    println!(
        "清理要不要先关 Claude 桌面端：{}（它现在开着 {} 个进程）",
        if l.needs_close { "要" } else { "不用" },
        l.desktop_running
            .map(|n| n.to_string())
            .unwrap_or_else(|| "查不到".into())
    );
}
