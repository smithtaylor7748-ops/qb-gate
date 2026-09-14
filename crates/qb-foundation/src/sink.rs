//! 领域代码跟外界说话的三个口子。**地基层：不依赖任何别的模块。**
//!
//! # 为什么需要它们
//!
//! 体检里最贵的一条：`gate/mod.rs` 976 行、`lib.rs` 1544 行，**测试各为 0**。
//! 原因不是没人想写，是写不了 —— 签名长这样：
//!
//! ```text
//! gate::run_watchdog(…, app: Option<tauri::AppHandle>)
//! workspace::launch(…, state: &crate::AppState)
//! ```
//!
//! 要构造它们就得先起一个 Tauri 进程。更糟的是链接器：单测只要**可能**走到
//! `app.emit(...)`，tao / wry 整套 GUI 运行时就会被拖进测试二进制，而 cargo
//! 生成的测试 exe 没有应用程序清单，加载 comctl32 时以 `0xc0000139` 崩在
//! main 之前 —— 一个测试都没跑就整体失败（这段教训原样记在 `events.rs` 里）。
//!
//! 所以领域代码不许认识 `AppHandle`，只认识下面这三个 trait。`src-tauri`
//! 提供基于 `AppHandle` 的实现，测试传 [`Silent`] 或 [`Recording`]。
//!
//! # 为什么方法收 `&str` 而不是 `impl Into<String>`
//!
//! 泛型方法会让 trait 不是对象安全的，`&dyn ProgressSink` 就传不了。
//! 传 `&dyn` 是这套东西的全部意义（一个函数同时能吃真上报器和假上报器），
//! 所以调用方多写一个 `&format!(…)` 是划算的。

use std::sync::Mutex;

/// 往界面报进度。分段、追加日志、收尾、失败。
///
/// 实现必须容忍「窗口已经关了」：发不出去不是错误，任务本身还得跑完。
pub trait ProgressSink: Send + Sync {
    /// 进入第 `step` 段。`phase` 是**中文短句，直接显示** —— 前端不再翻一遍。
    fn phase(&self, step: u32, phase: &str);
    /// 往日志窗追加一行，不改变段号。
    fn log(&self, step: u32, line: &str);
    /// 正常收尾。
    fn done(&self, phase: &str);
    /// 中止。`error` 同样是可以直接显示的人话。
    fn fail(&self, error: &str);
}

/// 什么都不做的上报器。
///
/// 给两类调用方：非 Tauri 路径（命令行、启动早期），以及不关心进度的单测。
/// 有了它，业务代码里就不必到处写 `if let Some(app)`。
#[derive(Debug, Clone, Copy, Default)]
pub struct Silent;

impl ProgressSink for Silent {
    fn phase(&self, _step: u32, _phase: &str) {}
    fn log(&self, _step: u32, _line: &str) {}
    fn done(&self, _phase: &str) {}
    fn fail(&self, _error: &str) {}
}

/// 把上报内容记下来的上报器。**给测试用。**
///
/// 有了它，「装机流程第 1 段一定是解锁、失败分支一定重锁」这类
/// 原来只能靠读代码确认的事，才第一次变得可以断言。
#[derive(Debug, Default)]
pub struct Recording {
    lines: Mutex<Vec<String>>,
}

impl Recording {
    pub fn new() -> Self {
        Self::default()
    }

    /// 至今为止收到的全部上报，按发生顺序。
    ///
    /// 形如 `phase 1 摘掉执行锁` / `log 2 winget 输出一行` / `done …` / `fail …`。
    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }

    /// 有没有哪一条上报含有这段文字。
    pub fn contains(&self, needle: &str) -> bool {
        self.lines().iter().any(|l| l.contains(needle))
    }

    fn push(&self, line: String) {
        self.lines.lock().unwrap().push(line);
    }
}

impl ProgressSink for Recording {
    fn phase(&self, step: u32, phase: &str) {
        self.push(format!("phase {step} {phase}"));
    }
    fn log(&self, step: u32, line: &str) {
        self.push(format!("log {step} {line}"));
    }
    fn done(&self, phase: &str) {
        self.push(format!("done {phase}"));
    }
    fn fail(&self, error: &str) {
        self.push(format!("fail {error}"));
    }
}

/// 往界面发一条一次性事件（不是进度）。
///
/// 跟 [`ProgressSink`] 分开是因为两者的失败语义不同：进度丢了无所谓，
/// 而「门被动关上了」这种事件丢了，使用者就再也看不到那条横幅。
/// 分成两个 trait，实现方才能对它们区别对待。
pub trait EventSink: Send + Sync {
    fn emit(&self, topic: &str, payload: serde_json::Value);
}

/// 什么都不发。
impl EventSink for Silent {
    fn emit(&self, _topic: &str, _payload: serde_json::Value) {}
}

/// 现在几点。
///
/// 存在的唯一理由是**时间窗的测试**：中转站的健康度要按 1H / 1D / 7D 三个窗口
/// 聚合，而「这条记录落在哪个窗里」的边界条件用真实时钟根本测不稳 ——
/// 测试跑得快一点慢一点，结论就变了。
pub trait Clock: Send + Sync {
    /// Unix 毫秒。
    fn now_ms(&self) -> i64;
}

/// 真实时钟。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

/// 钉死在某一刻的时钟。给测试用。
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一段假的「领域代码」：它只认识 trait，不认识任何 UI 框架。
    fn pretend_to_install(p: &dyn ProgressSink) {
        p.phase(1, "摘掉执行锁");
        p.log(2, "winget 输出一行");
        p.done("装好了");
    }

    #[test]
    fn a_recording_sink_lets_a_test_assert_the_order_of_the_steps() {
        // 这条测试本身就是这个模块存在的证明：换成 Reporter 的话，
        // 它要么跑不起来，要么把整个测试二进制拖垮。
        let rec = Recording::new();
        pretend_to_install(&rec);
        assert_eq!(
            rec.lines(),
            vec!["phase 1 摘掉执行锁", "log 2 winget 输出一行", "done 装好了"]
        );
    }

    #[test]
    fn the_silent_sink_swallows_everything_without_panicking() {
        // 非 Tauri 路径（启动早期、命令行）走的就是它。
        pretend_to_install(&Silent);
        Silent.fail("随便什么错");
        Silent.emit("gate://task", serde_json::json!({ "x": 1 }));
    }

    #[test]
    fn contains_finds_text_across_all_kinds_of_reports() {
        let rec = Recording::new();
        rec.fail("装不上：磁盘满了");
        assert!(rec.contains("磁盘满了"));
        assert!(!rec.contains("网络"));
    }

    #[test]
    fn a_fixed_clock_does_not_move() {
        // 时间窗的边界条件靠它才测得稳 —— 真实时钟下，
        // 「这条记录算不算落在 1 小时窗内」会随测试跑多快而变。
        let c = FixedClock(1_700_000_000_000);
        assert_eq!(c.now_ms(), c.now_ms());
        assert_eq!(c.now_ms(), 1_700_000_000_000);
    }

    #[test]
    fn the_system_clock_is_in_milliseconds_not_seconds() {
        // 单位搞错是这类 API 最常见的坑，而且不会报错，
        // 只会让所有时间窗差三个数量级。
        assert!(SystemClock.now_ms() > 1_600_000_000_000);
    }
}
