//! 执行租约。
//!
//! 「租约」= 门禁临时摘掉 Deny ACE，放某个进程起来。租约在外时目标是解锁的；
//! 收回租约就是重新上锁。
//!
//! G1 的教训写在这里：租约退出时必须重锁**全部**副本，而不只是租出去的那一个。
//! 因为 Claude 自动更新会写一个全新的 exe，新文件继承干净 ACL，
//! 那条 Deny 跟着旧文件一起没了 —— 只重锁「本次租出去的那条路径」会漏掉它。

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Lease {
    pub holder: Option<String>,
    pub granted: BTreeSet<PathBuf>,
}

impl Lease {
    pub fn is_held(&self) -> bool {
        self.holder.is_some()
    }

    pub fn grant(&mut self, holder: &str, paths: Vec<PathBuf>) {
        self.holder = Some(holder.to_string());
        self.granted = paths.into_iter().collect();
    }

    pub fn release(&mut self) {
        self.holder = None;
        self.granted.clear();
    }
}
