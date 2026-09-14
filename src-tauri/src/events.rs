//! 长任务的进度事件。
//!
//! 在这个文件出现之前，后端**一条 event 都没发过** —— 全项目 `app.emit`
//! 零命中，两个接收 `AppHandle` 的命令拿到之后写的是 `let _ = app;`。
//! 于是「启动酒馆」这种最长 80 秒的操作，界面上只有一个「启动中…」，
//! 卡死和正常在视觉上完全一样。
//!
//! 前端在 `src/lib/tasks.ts` 里接 `gate://task`，字段名两边必须一致。

use crate::sink::{EventSink, ProgressSink};
use serde::Serialize;
use tauri::Emitter;

/// 事件通道名。**定义在 `qb-contract::channels`，两边共用一份。**
///
/// 这里只是个别名，免得本模块里到处写全路径。
pub const CHANNEL: &str = qb_contract::channels::TASK;

/// 任务名。前端 `TaskName` 与这四个字符串一一对应。
pub const TASK_INSTALL: &str = "install";
pub const TASK_UPGRADE: &str = "upgrade";
pub const TASK_TAVERN_START: &str = "tavern-start";
pub const TASK_DNS_PROBE: &str = "dns-probe";
pub const TASK_LAUNCH_CODE: &str = "launch-claude-code";
pub const TASK_LAUNCH_DESKTOP: &str = "launch-claude-desktop";
pub const TASK_LAUNCH_CODEX: &str = "launch-codex";
pub const TASK_KILL_PREVIEW: &str = "killswitch-preview";
pub const TASK_KILL_EXECUTE: &str = "killswitch-execute";
pub const TASK_CHROME: &str = "chrome-reinstall";

#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task: String,
    /// **中文短句，直接显示**。前端不再翻一遍，免得两边的措辞对不上。
    pub phase: String,
    pub step: u32,
    pub total: u32,
    /// 追加到日志窗的一行。没有就不追加。
    pub log: Option<String>,
    pub done: bool,
    pub error: Option<String>,
}

/// 借来的事件出口。用 [`ui`] 造它。
///
/// # 为什么要这个壳
///
/// `EventSink` 在 `qb-foundation`、`AppHandle` 在 `tauri`，两个都不是本 crate
/// 的类型 —— 孤儿规则不允许直接给 `AppHandle` impl 这个 trait。壳是本地类型，
/// 所以可以。
///
/// **没有它的话**，`operations::changed` 这种「告诉界面数据变了」的小函数
/// 就得收 `&tauri::AppHandle`，于是 UI 框架顺着三十多处调用链撒进 `usecase`、
/// `gate`、`operations` —— 那几个模块也就永远搬不出 `src-tauri`。
pub struct Ui<'a, R: tauri::Runtime = tauri::Wry>(pub &'a tauri::AppHandle<R>);

impl<R: tauri::Runtime> EventSink for Ui<'_, R> {
    fn emit(&self, topic: &str, payload: serde_json::Value) {
        // 发不出去不算错误 —— 窗口可能已经关了。
        let _ = Emitter::emit(self.0, topic, payload);
    }
}

/// 借一个事件出口：`operations::changed(&events::ui(&app), …)`。
pub fn ui<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Ui<'_, R> {
    Ui(app)
}

/// 自己持有 `AppHandle` 的事件出口。
struct Owned(tauri::AppHandle);

impl EventSink for Owned {
    fn emit(&self, topic: &str, payload: serde_json::Value) {
        let _ = Emitter::emit(&self.0, topic, payload);
    }
}

/// 拿一个**能长期持有**的事件出口。
///
/// `operations::Run` 要在整个长任务期间留着它，借用的活不了那么久。
pub fn sink(app: &tauri::AppHandle) -> std::sync::Arc<dyn EventSink> {
    std::sync::Arc::new(Owned(app.clone()))
}

/// 基于 `AppHandle` 的 [`ProgressSink`] 实现。**这是接口层的东西。**
///
/// 领域代码不许认识它 —— 它们收的是 `&dyn ProgressSink`。理由写在
/// `qb-foundation` 的 `sink` 模块里，一句话版本：签名里一出现 Tauri 类型，
/// 那个函数就再也测不了，而链接器还会把整套 GUI 运行时拖进测试二进制。
///
/// 不发事件的那一份不再由这里提供，用 `sink::Silent`。
#[derive(Clone)]
pub struct Reporter {
    app: tauri::AppHandle,
    task: &'static str,
    total: u32,
}

impl Reporter {
    pub fn new(app: tauri::AppHandle, task: &'static str, total: u32) -> Self {
        Self { app, task, total }
    }

    fn emit(&self, p: TaskProgress) {
        // 发不出去不算错误 —— 窗口可能已经关了，任务本身还得跑完。
        let _ = Emitter::emit(&self.app, CHANNEL, p);
    }
}

impl ProgressSink for Reporter {
    fn phase(&self, step: u32, phase: &str) {
        self.emit(TaskProgress {
            task: self.task.into(),
            phase: phase.into(),
            step,
            total: self.total,
            log: None,
            done: false,
            error: None,
        });
    }

    fn log(&self, step: u32, line: &str) {
        self.emit(TaskProgress {
            task: self.task.into(),
            phase: String::new(),
            step,
            total: self.total,
            log: Some(line.into()),
            done: false,
            error: None,
        });
    }

    fn done(&self, phase: &str) {
        self.emit(TaskProgress {
            task: self.task.into(),
            phase: phase.into(),
            step: self.total,
            total: self.total,
            log: None,
            done: true,
            error: None,
        });
    }

    fn fail(&self, error: &str) {
        self.emit(TaskProgress {
            task: self.task.into(),
            phase: "已中止".into(),
            step: 0,
            total: self.total,
            log: None,
            done: true,
            error: Some(error.into()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TaskProgress {
        TaskProgress {
            task: TASK_INSTALL.into(),
            phase: "正在解锁".into(),
            step: 1,
            total: 6,
            log: Some("winget 输出一行".into()),
            done: false,
            error: None,
        }
    }

    #[test]
    fn serializes_with_the_field_names_the_frontend_expects() {
        // 前端 `src/lib/tasks.ts` 的 TaskProgress 就认这七个名字，
        // 改任何一个都会让进度条静默失灵（不报错，只是永远不动）。
        let v: serde_json::Value = serde_json::to_value(sample()).unwrap();
        for key in ["task", "phase", "step", "total", "log", "done", "error"] {
            assert!(v.get(key).is_some(), "缺字段 {key}");
        }
        assert_eq!(v["task"], "install");
        assert_eq!(v["step"], 1);
        assert_eq!(v["done"], false);
    }

    #[test]
    fn absent_log_and_error_serialize_as_null_not_omitted() {
        // 前端读的是 `p.log ?? null`，字段整个消失也不会崩，
        // 但保持稳定的形状能让「没有日志」和「字段名打错了」区分得开。
        let p = TaskProgress {
            log: None,
            error: None,
            ..sample()
        };
        let v: serde_json::Value = serde_json::to_value(p).unwrap();
        assert!(v["log"].is_null());
        assert!(v["error"].is_null());
    }

    // ⚠ **单元测试里不要调用 `Reporter` 的 phase / log / done / fail。**
    //
    // 那几个方法内部走到 `app.emit(...)`，一旦从测试里可达，链接器就会把
    // tao / wry 那整套 GUI 运行时拉进 lib 的测试二进制里。而 cargo 生成的
    // 测试 exe **没有应用程序清单**，拿不到 Common Controls v6，
    // 加载 comctl32 时就会以 `STATUS_ENTRYPOINT_NOT_FOUND`（0xc0000139）
    // 直接崩在 main 之前 —— 表现是「一个测试都没跑就整体失败」，
    // 很难从报错反推到这里。
    //
    // 真正要测的是事件的形状（上面两个测试），那不需要 AppHandle。
    // 不发事件的那一份现在是 `sink::Silent`，它在 qb-foundation 里，
    // 不链接任何 GUI 运行时。

    #[test]
    fn task_names_match_the_frontend_union() {
        // 前端 `TaskName` 是一个字面量联合类型，多一个少一个都对不上。
        assert_eq!(
            [
                TASK_INSTALL,
                TASK_UPGRADE,
                TASK_TAVERN_START,
                TASK_DNS_PROBE,
                TASK_LAUNCH_CODE,
                TASK_LAUNCH_DESKTOP,
                TASK_LAUNCH_CODEX,
                TASK_KILL_PREVIEW,
                TASK_KILL_EXECUTE,
                TASK_CHROME,
            ],
            [
                "install",
                "upgrade",
                "tavern-start",
                "dns-probe",
                "launch-claude-code",
                "launch-claude-desktop",
                "launch-codex",
                "killswitch-preview",
                "killswitch-execute",
                "chrome-reinstall",
            ]
        );
    }
}
