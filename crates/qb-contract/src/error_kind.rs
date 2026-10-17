//! 错误的机器可读分类。
//!
//! # 为什么它在契约层
//!
//! 因为前端要拿它做判断。原来 `GateError` 过 IPC 时是
//! `s.serialize_str(&self.to_string())` —— **所有错误都变成一个裸中文字符串**，
//! 没有 code、没有 kind。前端想区分「这条记录重复了」和「磁盘满了」，
//! 只剩一条路：`message.includes("重复")`。
//!
//! 那条路会烂：中文文案是会改的（改文案是最常见的改动之一），而改的人
//! 根本不知道某个页面正靠那几个字做分支。症状是某个错误处理**静默失效**。
//!
//! # 怎么用
//!
//! **前端做条件处理只许看 `kind`，不许匹配 `message` 的内容。**
//! `message` 是给人看的，随时可能改；`kind` 是给代码看的，改它要改两边。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 后端错误的分类。与 `qb_foundation::error::GateError` 的变体一一对应。
///
/// 原来第一个是 `Cancelled`（使用者点了取消），连同取消机制 2026-09-27 删了。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ErrorKind {
    /// 读写文件失败。
    Io,
    /// 网络请求失败。
    Http,
    /// JSON 解析失败。
    Json,
    /// Windows API 返回了错误码。
    WinApi,
    /// 路径不存在。
    NotFound,
    /// 出口 IP 查到了，但不在白名单里。
    IpNotAllowed,
    /// 查不到出口 IP。跟上一条分开是有意的：一个该去查网络，一个该去换节点，
    /// 处理方式完全相反。
    IpUnknown,
    /// 门禁判定没过，`message` 已经是一句可以直接显示的人话。
    GateRejected,
    /// 数据库操作失败。
    Database,
    /// 还没分类的。**这个值越少越好** —— 体检时 417 处构造里有 396 处是它。
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_as_kebab_case_strings() {
        // 前端按这些字符串做分支，改任何一个都要同步改两边。
        assert_eq!(
            serde_json::to_string(&ErrorKind::IpNotAllowed).unwrap(),
            "\"ip-not-allowed\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorKind::WinApi).unwrap(),
            "\"win-api\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorKind::Other).unwrap(),
            "\"other\""
        );
    }
}
