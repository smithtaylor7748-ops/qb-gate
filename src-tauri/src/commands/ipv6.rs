//! IPv6 的显式用户例外：默认在启动时应用；其他网络修改仍只由点击触发。
use crate::{
    error::{GateError, Result},
    operations,
    sysenv::ipv6,
    usecase, AppState,
};

#[tauri::command]
pub async fn ipv6_status() -> Result<ipv6::Ipv6Status> {
    tauri::async_runtime::spawn_blocking(ipv6::status)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn ipv6_set(
    disable: bool,
    state: tauri::State<'_, AppState>,
) -> Result<ipv6::Ipv6Status> {
    let _guard = operations::exclusive().await?;
    let result = usecase::ipv6_ops::apply(&state.gate, Some(disable)).await;
    let mut status = ipv6_status().await?;
    if let Err(e) = result {
        status.error = Some(e.to_string());
    }
    Ok(status)
}

pub async fn apply_on_start(state: &AppState) {
    let result = async {
        let _guard = operations::exclusive_soon().await?;
        usecase::ipv6_ops::apply(&state.gate, None).await
    }
    .await;
    if let Err(e) = result {
        crate::audit::write(&format!("启动 IPv6 设置未完成：{e}"));
    }
}
