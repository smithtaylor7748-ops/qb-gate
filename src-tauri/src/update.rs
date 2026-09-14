//! 安全的应用更新状态。
//!
//! 发布仓库尚未创建，因此默认返回未配置；不会访问任意 URL、下载未签名
//! 安装包或执行更新。正式发布时只需在构建环境提供仓库与公钥，并接入
//! Tauri updater 的签名校验。

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UpdateStatus {
    pub current_version: String,
    pub repository: Option<String>,
    pub configured: bool,
    pub update_available: bool,
    pub latest_version: Option<String>,
    pub detail: String,
}

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 当前仅提供安全默认状态。没有仓库配置时启动检查是静默 no-op。
pub fn status() -> UpdateStatus {
    UpdateStatus {
        current_version: CURRENT_VERSION.into(),
        repository: None,
        configured: false,
        update_available: false,
        latest_version: None,
        detail: "GitHub Releases 仓库尚未配置；更新检查已停用。".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_updates_never_claim_an_update() {
        let s = status();
        assert!(!s.configured);
        assert!(!s.update_available);
        assert!(s.repository.is_none());
        assert_eq!(s.current_version, CURRENT_VERSION);
    }
}
