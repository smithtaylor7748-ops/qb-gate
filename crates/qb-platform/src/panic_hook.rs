//! 崩溃时把现场留下来。
//!
//! # 为什么以前什么都拿不到
//!
//! `[profile.release]` 里是 `panic = "abort"` —— 进程在 panic 那一刻直接 abort，
//! **`set_hook` 装的钩子根本没机会跑**。而面板是 GUI 程序、没有控制台，
//! 默认的 panic 输出写到 stderr 等于写进黑洞。
//!
//! 所以症状一直是：使用者说「它闪退了」，而本机一个字都没留下，
//! 只能靠口述复现。改成 `unwind` 加上这个钩子之后，崩溃会在
//! `%LOCALAPPDATA%\ClaudeIpGate\crashes\panic-<时间戳>.log` 留下线程名、
//! 出事位置、panic 信息和调用栈。
//!
//! # 钩子里不许再 panic
//!
//! 下面每一步的错误都被 `let _ =` 吞掉：目录建不了、文件写不了，都不该在
//! 崩溃处理里再炸一次 —— 那会把原始现场彻底盖掉，比没有钩子更糟。
//!
//! # 关于调用栈的符号
//!
//! release 档开着 `strip = true`（等价于 `strip = "symbols"`），所以栈里只有
//! 模块名与偏移，没有函数名。**这是刻意的取舍**：符号表会让安装包明显变大，
//! 而定位一次崩溃，`message` 加 `location`（文件:行:列）已经覆盖绝大多数情况。
//! 真需要带符号的栈时，把 `strip` 改成 `"debuginfo"` 重出一版即可。

use std::io::Write;
use std::path::PathBuf;

/// 崩溃现场目录。
pub fn dir() -> PathBuf {
    crate::paths::state_dir().join("crashes")
}

/// 保留最近多少份现场。崩溃日志不该无限长大 —— 它跟快照不一样，
/// 旧的那些对排查当前问题没有价值。
const KEEP: usize = 20;

/// 装上钩子。**在 `run()` 的最开头调用**，晚一步就可能漏掉启动期的崩溃。
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let message = payload_of(info);
        let location = info.location().map(|l| format!("{l}"));
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let text = report(&thread, &message, location.as_deref(), &backtrace, &at);
        let _ = persist(&text);
        // 仍然把原来的钩子跑一遍：`cargo test` 下那份会打印到测试输出，
        // 吞掉它会让本来看得见的测试失败变成一片沉默。
        previous(info);
    }));
}

/// 从 panic 载荷里取出人能读的那句话。
///
/// `panic!("...")` 的载荷可能是 `&str` 也可能是 `String`，两种都要认；
/// 都不是时不硬猜，如实写「非字符串载荷」。
fn payload_of(info: &std::panic::PanicHookInfo<'_>) -> String {
    let p = info.payload();
    if let Some(s) = p.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = p.downcast_ref::<String>() {
        return s.clone();
    }
    "非字符串载荷（无法读取 panic 信息）".into()
}

/// 拼报告正文。**纯函数**，不碰文件系统，方便单测。
pub(crate) fn report(
    thread: &str,
    message: &str,
    location: Option<&str>,
    backtrace: &str,
    at: &str,
) -> String {
    let mut out = String::with_capacity(backtrace.len() + 512);
    out.push_str("QB Gate 崩溃现场\n");
    out.push_str(&format!("时间   {at}\n"));
    out.push_str(&format!("版本   {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("线程   {thread}\n"));
    // 位置可能拿不到（比如 panic 来自外部库的非 `#[track_caller]` 路径）。
    // 拿不到就如实写「未知」，不要留空让人以为是格式坏了。
    out.push_str(&format!("位置   {}\n", location.unwrap_or("未知")));
    out.push_str(&format!("信息   {message}\n"));
    out.push_str("\n调用栈\n");
    out.push_str(backtrace);
    if !backtrace.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn persist(text: &str) -> std::io::Result<()> {
    persist_in(&dir(), text)
}

/// 落盘。目录由调用方给 —— **单测不许碰真实的运行期目录**，
/// 所以这里不自己去问 `state_dir()`。
pub(crate) fn persist_in(dir: &std::path::Path, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let name = format!(
        "panic-{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
    );
    let mut f = std::fs::File::create(dir.join(name))?;
    f.write_all(text.as_bytes())?;
    rotate(dir, KEEP);
    Ok(())
}

/// 只留最近 `keep` 份。**按文件名排序而不是按 mtime** ——
/// 文件名里就带着秒级时间戳加毫秒，比 mtime 稳（复制、还原都会动 mtime）。
pub(crate) fn rotate(dir: &std::path::Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("panic-") && n.ends_with(".log"))
        })
        .collect();
    if names.len() <= keep {
        return;
    }
    names.sort();
    let cut = names.len() - keep;
    for p in names.into_iter().take(cut) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_every_field_even_when_the_location_is_missing() {
        // 位置拿不到时要写「未知」而不是留空 —— 留空会让人以为报告格式坏了，
        // 转而怀疑崩溃日志本身，而不是去看那条 message。
        let r = report(
            "main",
            "index out of bounds",
            None,
            "0: foo\n",
            "2026-09-13 08:00:00",
        );
        assert!(r.contains("线程   main"));
        assert!(r.contains("位置   未知"));
        assert!(r.contains("信息   index out of bounds"));
        assert!(r.contains("0: foo"));
        assert!(r.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn a_backtrace_without_a_trailing_newline_still_ends_cleanly() {
        let r = report("worker", "boom", Some("src/x.rs:1:2"), "0: bar", "t");
        assert!(r.ends_with('\n'));
        assert!(r.contains("位置   src/x.rs:1:2"));
    }

    #[test]
    fn rotation_keeps_the_newest_and_ignores_unrelated_files() {
        // 单测不碰真实运行期目录：自己建一个临时目录，用完删掉。
        let d = std::env::temp_dir().join(format!("qb-panic-{}", crate::config_io::id()));
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..5 {
            std::fs::write(d.join(format!("panic-2026010{i}-000000-000.log")), "x").unwrap();
        }
        std::fs::write(d.join("readme.txt"), "keep me").unwrap();

        rotate(&d, 2);

        let mut left: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "panic-20260103-000000-000.log".to_string(),
                "panic-20260104-000000-000.log".to_string(),
                "readme.txt".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn persisting_creates_the_directory_and_writes_the_whole_report() {
        // 崩溃时 crashes/ 多半还不存在 —— 钩子得自己把目录建出来，
        // 不能指望别处先建好。
        let d = std::env::temp_dir().join(format!("qb-panic-{}", crate::config_io::id()));
        assert!(!d.exists());
        let text = report("main", "boom", Some("src/a.rs:3:4"), "0: frame", "t");

        persist_in(&d, &text).unwrap();

        let files: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().to_string_lossy().to_string();
        assert!(
            name.starts_with("panic-") && name.ends_with(".log"),
            "{name}"
        );
        assert_eq!(std::fs::read_to_string(files[0].path()).unwrap(), text);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rotation_is_a_no_op_when_under_the_cap() {
        let d = std::env::temp_dir().join(format!("qb-panic-{}", crate::config_io::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("panic-20260101-000000-000.log"), "x").unwrap();
        rotate(&d, 20);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
