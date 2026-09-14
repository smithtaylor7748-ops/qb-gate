//! 事件通道名。**后端 emit 与前端 listen 用的是同一份常量。**
//!
//! # 为什么它们值得一个模块
//!
//! 通道名原来是六处裸字符串：Rust 侧三处 `emit("…")`、前端三处 `listen("…")`。
//! 打错一个字母不会有任何报错 —— 事件照样发得出去，只是永远没人收。
//! 症状是「进度条不动」「切完账户托盘没刷新」这类**看起来像别的 bug** 的现象。
//!
//! 现在名字只有一处定义，由 ts-rs 导给前端。改名要改这一处，两边一起变。
//!
//! # 为什么不用枚举
//!
//! 它们是 Tauri 事件系统的字符串键，不是一个可以穷尽匹配的域。
//! 做成枚举反而要在 emit 处再转回字符串，多一层没有收益的转换。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 长任务的进度。前端 `src/lib/tasks.ts` 接它。
pub const TASK: &str = "gate://task";

/// 工作区数据变了（供应商 / 凭证 / 环境 / 会话 / 操作 …）。
/// 前端收到就重新拉一遍受影响的资源。
pub const WORKSPACE_CHANGED: &str = "workspace://changed";

/// 托盘菜单要求跳到某一页。载荷是路由路径，例如 `/relays`。
pub const NAVIGATE: &str = "workspace://navigate";

/// 三个通道名，一次性导给前端。
///
/// 做成一个结构体而不是三个独立常量，是因为 ts-rs 导的是**类型**不是值 ——
/// 有了这个结构体，前端那边才拿得到一份带着字面量类型的对象。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Channels {
    #[ts(type = "\"gate://task\"")]
    pub task: &'static str,
    #[ts(type = "\"workspace://changed\"")]
    pub workspace_changed: &'static str,
    #[ts(type = "\"workspace://navigate\"")]
    pub navigate: &'static str,
}

impl Channels {
    pub const fn all() -> Self {
        Self {
            task: TASK,
            workspace_changed: WORKSPACE_CHANGED,
            navigate: NAVIGATE,
        }
    }
}

impl Default for Channels {
    fn default() -> Self {
        Self::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exported_object_carries_the_same_strings_as_the_constants() {
        // 结构体是给 ts-rs 用的门面，常量是给 Rust 代码用的。
        // 两者分叉的话，前端 listen 的和后端 emit 的就不是同一个通道了，
        // 而那件事**不会报任何错** —— 事件照发，只是没人收。
        let c = Channels::all();
        assert_eq!(c.task, TASK);
        assert_eq!(c.workspace_changed, WORKSPACE_CHANGED);
        assert_eq!(c.navigate, NAVIGATE);
    }

    #[test]
    fn channel_names_keep_their_scheme_prefix() {
        // `gate://` 与 `workspace://` 是两个命名空间，别混。
        // 改前缀等于改协议，得两边一起改 —— 这条让改动至少经手一次。
        assert!(TASK.starts_with("gate://"));
        assert!(WORKSPACE_CHANGED.starts_with("workspace://"));
        assert!(NAVIGATE.starts_with("workspace://"));
    }
}
