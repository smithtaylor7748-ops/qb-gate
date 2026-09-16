//! 网卡修改期间产生的旧门禁检测不能在修改完成后继续下判定。
use crate::{
    error::{GateError, Result},
    gate,
    sysenv::ipv6,
};
use std::sync::atomic::{AtomicU64, Ordering};

struct NetworkChange<'a>(&'a AtomicU64);
impl<'a> NetworkChange<'a> {
    fn begin(generation: &'a AtomicU64) -> Self {
        generation.fetch_add(1, Ordering::SeqCst);
        Self(generation)
    }
}
impl Drop for NetworkChange<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// 调用方持有 operations 独占锁。前后更新代次，覆盖修改前及修改中的慢查询。
pub async fn apply(state: &gate::GateState, preference: Option<bool>) -> Result<()> {
    let _change = NetworkChange::begin(&state.generation);
    gate::invalidate_verdict()?;
    let result = tokio::task::spawn_blocking(move || ipv6::apply(preference))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?;
    // judge_now 在拿操作锁之前探测；修改期间生成的磁盘缓存同样必须失效。
    gate::invalidate_verdict()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_old_and_during_change_verdicts_are_invalidated_even_on_error() {
        let generation = AtomicU64::new(10);
        let result = (|| -> Result<()> {
            let _change = NetworkChange::begin(&generation);
            assert_eq!(generation.load(Ordering::SeqCst), 11);
            Err(GateError::Other("UAC rejected".into()))
        })();
        assert!(result.is_err());
        assert_eq!(generation.load(Ordering::SeqCst), 12);
    }
}
