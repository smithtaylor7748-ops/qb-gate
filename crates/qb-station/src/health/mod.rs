//! 线路健康度。
//!
//! 目前只有时间窗聚合（`window`）。测活、测速、两级熔断、三口径评分
//! 会在 P1 / P3 接进来。
//!
//! # 为什么 `window::aggregate` 是 `pub(super)`
//!
//! 它只许从这里调用。一旦有人把那段聚合算法搬进调用方自己写一遍，
//! `aggregate` 就没人调了，`cargo clippy --lib -- -D warnings` 会因为
//! dead_code 直接编译失败 —— 而不是留下两份会各自漂移的实现。
//! （单测不参与 `--lib` 编译，所以「只剩测试在调」＝没人调。）
//!
//! **这个模块曾经是孤儿**，同 `station`：275 行代码加 9 个测试，
//! 因为没有 `mod.rs` 而从没编译过、从没跑过。

pub mod window;

use crate::station::model::UsageRow;

pub use window::{Window, DAY_MS, HOUR_MS, WEEK_MS};

/// 界面上那三个缓存命中率格子对应的三个窗口。
///
/// 一次算齐是有意的：三个窗口共用同一批行、同一个 `now`，
/// 分三次调用的话若中间跨了整点，会出现「1 小时比 7 天还多」这种自相矛盾的显示。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Windows {
    pub hour: Window,
    pub day: Window,
    pub week: Window,
}

/// 把账单日志聚合成 1H / 1D / 7D 三个窗口。
///
/// `now_ms` 由调用方传入而不是这里取系统时间 —— 纯函数才测得了，
/// 也才能保证三个窗口用的是同一个时刻。
pub fn windows(rows: &[UsageRow], now_ms: i64) -> Windows {
    Windows {
        hour: window::aggregate(rows, now_ms, HOUR_MS),
        day: window::aggregate(rows, now_ms, DAY_MS),
        week: window::aggregate(rows, now_ms, WEEK_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_windows_share_one_now_and_never_contradict() {
        // 1H ⊆ 1D ⊆ 7D。分三次调用、中间跨了整点的话，
        // 界面上会出现「1 小时的请求数比 7 天还多」。
        let now = 30 * DAY_MS;
        let rows: Vec<UsageRow> = [10 * 60_000, 5 * HOUR_MS, 3 * DAY_MS, 20 * DAY_MS]
            .iter()
            .map(|back| UsageRow {
                at_ms: now - back,
                ok: true,
                ..Default::default()
            })
            .collect();
        let w = windows(&rows, now);
        assert_eq!(
            (w.hour.requests, w.day.requests, w.week.requests),
            (1, 2, 3)
        );
        assert!(w.hour.requests <= w.day.requests);
        assert!(w.day.requests <= w.week.requests);
    }

    #[test]
    fn no_rows_means_unknown_in_every_window() {
        let w = windows(&[], 1_000_000);
        assert_eq!(w.hour.cache_hit_rate, None);
        assert_eq!(w.day.cache_hit_rate, None);
        assert_eq!(w.week.cache_hit_rate, None);
    }
}
