use crate::sink::EventSink;
use crate::{
    domain::Operation,
    error::{GateError, Result},
    repository::Repository,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock,
};

static COORDINATOR: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static REVISION: AtomicU64 = AtomicU64::new(0);
pub async fn exclusive_unchecked() -> tokio::sync::MutexGuard<'static, ()> {
    COORDINATOR.get_or_init(Default::default).lock().await
}
pub async fn exclusive() -> Result<tokio::sync::MutexGuard<'static, ()>> {
    let guard = exclusive_unchecked().await;
    crate::readiness::ensure_ready()?;
    Ok(guard)
}
/// 用户点出来的命令用这个：**排一小会儿队，而不是立刻失败**。
///
/// 看门狗每 5 秒就要拿同一把锁去巡检，巡检里要跑 PowerShell 数进程
/// 加查签名（几十个进程能到好几秒）。原来用户侧一律 `try_lock`、占着就直接报
/// 「另一个配置或安装操作正在提交」—— 表现是点「保存」偶发失败、再点一次又好了。
/// 这种偶发错误使用者描述不清楚，也复现不出来，是最难查的一类。
///
/// 仍然有上限：真有一个长任务（比如正在下载扩展）占着锁时，等到点就如实报错，
/// 而不是让界面无限转圈。
pub async fn exclusive_soon() -> Result<tokio::sync::MutexGuard<'static, ()>> {
    let guard = tokio::time::timeout(std::time::Duration::from_secs(20), exclusive_unchecked())
        .await
        .map_err(|_| GateError::Other("另一个配置或安装操作正在进行，请稍后重试".into()))?;
    crate::readiness::ensure_ready()?;
    Ok(guard)
}
/// 告诉界面「这类数据变了」。
///
/// 收 `&dyn EventSink` 而不是 `&AppHandle`：这个模块是平台层，
/// 一旦签名里出现 Tauri 类型，它就再也搬不出 `src-tauri` —— 而全项目
/// 有三十多处调它，等于把 UI 框架顺着调用链撒了一地。
pub fn changed(events: &dyn EventSink, kind: &str, id: &str) {
    events.emit(qb_contract::channels::WORKSPACE_CHANGED,serde_json::json!({"kind":kind,"id":id,"revision":REVISION.fetch_add(1,Ordering::SeqCst)+1}));
}

pub struct Run {
    pub operation: Operation,
    /// 长任务期间一直留着，所以是 `Arc` 而不是借用。
    events: Arc<dyn EventSink>,
}
impl Run {
    pub fn start(events: Arc<dyn EventSink>, kind: &str, target: &str) -> Result<Self> {
        crate::readiness::ensure_ready()?;
        let operation = Operation {
            id: crate::config_io::id(),
            kind: kind.into(),
            target_id: target.into(),
            phase: "准备".into(),
            progress: 0,
            status: "running".into(),
            detail: String::new(),
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        let run = Self { operation, events };
        run.persist()?;
        Ok(run)
    }
    fn persist(&self) -> Result<()> {
        let db = Repository::open()?;
        db.put("operations", &self.operation.id, &self.operation)?;
        // 只增不减的三张表在这里收口，见 Repository::prune_history。
        if self.operation.status != "running" {
            db.prune_history()?;
        }
        changed(self.events.as_ref(), "operation", &self.operation.id);
        Ok(())
    }
    pub fn phase(&mut self, phase: &str, progress: u32) -> Result<()> {
        self.operation.phase = phase.into();
        self.operation.progress = progress;
        self.persist()
    }
    pub fn finish<T>(&mut self, result: Result<T>) -> Result<T> {
        self.operation.status = if result.is_ok() {
            "completed"
        } else {
            "failed"
        }
        .into();
        self.operation.progress = 100;
        self.operation.phase = if result.is_ok() { "完成" } else { "失败" }.into();
        if let Err(e) = &result {
            self.operation.detail = e.to_string();
        }
        self.persist()?;
        result
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        if self.operation.status == "running" {
            self.operation.status = "interrupted".into();
            self.operation.detail = "操作中断；恢复记录将在下次启动时检查".into();
            let _ = self.persist();
        }
    }
}

// 原来这里还有一套「取消操作」：`Run::cancellable` 发一个 watch 通道、`interruptible`
// 把工作和取消信号 select 在一起、`cancel(id)` 给命令层的 `operation_cancel` 用。
// 界面上的「取消」按钮早就没了，命令一直没人调；2026-09-27 连同 `GateError::Cancelled`
// 一起删掉。要加回来，先有一个看得见的按钮。
