//! 长任务的进度事件。
//!
//! 在这个文件出现之前，后端**一条 event 都没发过** —— 全项目 `app.emit`
//! 零命中，两个接收 `AppHandle` 的命令拿到之后写的是 `let _ = app;`。
//! 于是「启动酒馆」这种最长 80 秒的操作，界面上只有一个「启动中…」，
//! 卡死和正常在视觉上完全一样。
//!
//! 前端在 `src/lib/tasks.ts` 里接 `gate://task`，字段名两边必须一致。

use serde::Serialize;
use tauri::Emitter;

/// 事件通道名。前端 `listen('gate://task')` 与这里对应。
pub const CHANNEL: &str = "gate://task";

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

/// 进度上报器。
///
/// 拿不到 `AppHandle` 时（单元测试）用 [`Reporter::silent`]，
/// 所有方法都变成空操作 —— 这样业务代码里不必到处写 `if let Some(app)`。
#[derive(Clone)]
pub struct Reporter {
    app: Option<tauri::AppHandle>,
    task: &'static str,
    total: u32,
}

impl Reporter {
    pub fn new(app: tauri::AppHandle, task: &'static str, total: u32) -> Self {
        Self {
            app: Some(app),
            task,
            total,
        }
    }

    /// 不发事件的上报器。单测与非 Tauri 调用路径用它。
    pub fn silent(task: &'static str, total: u32) -> Self {
        Self {
            app: None,
            task,
            total,
        }
    }

    fn emit(&self, p: TaskProgress) {
        if let Some(app) = &self.app {
            // 发不出去不算错误 —— 窗口可能已经关了，任务本身还得跑完。
            let _ = app.emit(CHANNEL, p);
        }
    }

    /// 进入第 `step` 段。
    pub fn phase(&self, step: u32, phase: impl Into<String>) {
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

    /// 追加一行日志，不改变段号。
    pub fn log(&self, step: u32, line: impl Into<String>) {
        let line = line.into();
        self.emit(TaskProgress {
            task: self.task.into(),
            phase: String::new(),
            step,
            total: self.total,
            log: Some(line),
            done: false,
            error: None,
        });
    }

    pub fn done(&self, phase: impl Into<String>) {
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

    pub fn fail(&self, error: impl Into<String>) {
        let error = error.into();
        self.emit(TaskProgress {
            task: self.task.into(),
            phase: "已中止".into(),
            step: 0,
            total: self.total,
            log: None,
            done: true,
            error: Some(error),
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
    // `Reporter::silent` 仍然保留，给非 Tauri 的调用路径用。

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
