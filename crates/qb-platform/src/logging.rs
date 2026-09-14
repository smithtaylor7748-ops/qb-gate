//! 诊断日志。
//!
//! # 跟 `ip-gate.log` 是两份，不要合并
//!
//! [`crate::audit`] 写的 `ip-gate.log` 是**给使用者看的审计日志**，
//! 它的约束写在那个模块的文件头上：只记时间、公网 IP、上锁/解锁动作，
//! **不记任何别的东西**。那份保持原样。
//!
//! 这里写的是 `logs/qb-gate.log.<日期>`：带 level、模块名、线程，
//! 给排障用。审计事件会**单向**桥接过来（`audit::write` 顺手发一条
//! `target = "audit"` 的 tracing 事件），反向不成立 —— tracing 的东西
//! 永远不会流进 `ip-gate.log`。
//!
//! # 为什么之前没有
//!
//! 全项目零日志框架。排障时唯一的材料是一份没有 level、没有模块名、
//! 没有 span 的纯文本，出了问题只能从头翻，也没法按模块过滤。
//!
//! # 级别怎么调
//!
//! 默认 `info`。临时要更细时设环境变量：`QB_GATE_LOG=debug`，
//! 或者按模块：`QB_GATE_LOG=qb_gate_lib::gate=trace,info`。

use std::path::{Path, PathBuf};

use tracing_subscriber::EnvFilter;

/// 诊断日志目录。跟审计日志分开放，免得使用者把两者搞混。
pub fn dir() -> PathBuf {
    crate::paths::state_dir().join("logs")
}

/// 保留多少天。诊断日志每天一份，二十天足够覆盖「上周就开始了」这种描述。
const KEEP_DAYS: usize = 20;

const ENV: &str = "QB_GATE_LOG";

/// 装订阅器。**只装一次** —— 重复调用会被 tracing 拒绝，这里吞掉那个错误。
///
/// 在 `run()` 里紧跟 `panic_hook::install()` 之后调用。放这么早是因为
/// 启动恢复那一段（配置解析、迁移、ACL 修复）恰恰是最需要日志的地方。
pub fn install() {
    let d = dir();
    if std::fs::create_dir_all(&d).is_err() {
        // 目录建不出来就安静地不记日志。**绝不因为日志装不上而让面板起不来** ——
        // 那是拿一个诊断功能去换掉整个产品的可用性。
        return;
    }
    retain(&d, KEEP_DAYS);
    let writer = tracing_appender::rolling::daily(&d, "qb-gate.log");
    let _ = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_env_filter(EnvFilter::try_from_env(ENV).unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init();
}

/// 只留最近 `keep` 份。`tracing_appender` 自己不删旧文件 ——
/// 不管的话，一个长期开着的面板会在这里堆几百个文件。
///
/// 按文件名排序：`rolling::daily` 的后缀就是 `.YYYY-MM-DD`，字典序即时间序。
pub(crate) fn retain(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("qb-gate.log"))
        })
        .collect();
    if logs.len() <= keep {
        return;
    }
    logs.sort();
    let cut = logs.len() - keep;
    for p in logs.into_iter().take(cut) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("qb-logs-{}", crate::config_io::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn retention_drops_the_oldest_and_leaves_other_files_alone() {
        let d = temp();
        for day in 1..=5 {
            std::fs::write(d.join(format!("qb-gate.log.2026-01-0{day}")), "x").unwrap();
        }
        // 审计日志不在这个目录，但万一有人手动放了别的东西，不许误删。
        std::fs::write(d.join("notes.txt"), "keep").unwrap();

        retain(&d, 2);

        let mut left: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "notes.txt".to_string(),
                "qb-gate.log.2026-01-04".to_string(),
                "qb-gate.log.2026-01-05".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn retention_is_a_no_op_when_under_the_cap() {
        let d = temp();
        std::fs::write(d.join("qb-gate.log.2026-01-01"), "x").unwrap();
        retain(&d, 20);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_directory_is_not_an_error() {
        // 日志目录不存在时要安静返回，而不是 panic —— 这个函数会在
        // install() 里、面板起来之前被调用。
        retain(Path::new("definitely-not-a-real-directory-9f2a"), 3);
    }
}
