//! 出口探测与环境体检相关的命令。

use crate::sink::ProgressSink;
use crate::{
    error::Result,
    events::{self, Reporter},
    probe, sysenv,
};

/// 运行环境体检：代理形态、出口一致性、Anthropic 服务可达、claude.ai 的解析、IPv6 出口、
/// 浏览器 DoH、环境变量残留、MCP 配置里的明文密钥。
///
/// 读注册表和本地配置文件，**不改任何东西**。联网的那几项（两轮出口、服务可达、IPv6 出口）
/// 只在这一次点击时发，**都不带任何账户凭据**（`sysenv::checkup` 文件头）。
#[tauri::command]
pub async fn checkup_scan() -> sysenv::checkup::Checkup {
    sysenv::checkup::scan().await
}

// ------------------------------------------------------------ 真实浏览器采集

/// 采集页从哪来：Tauri 的内嵌前端资源（开发模式下 Tauri 自己退回读 `dist/`）。
struct TauriAssets(tauri::AppHandle);

impl qb_app::browser_probe::AssetSource for TauriAssets {
    fn get(&self, path: &str) -> Option<(String, Vec<u8>)> {
        let a = self.0.asset_resolver().get(path.to_string())?;
        Some((a.mime_type, a.bytes))
    }
}

/// 用系统默认浏览器做一次采集（2026-09-24）：在 127.0.0.1 上开一个一次性小服务，
/// 打开默认浏览器。边界见 `qb_app::browser_probe` 文件头。**只由界面上的那一次点击触发。**
///
/// 打不开浏览器不算失败：返回的地址界面上给「复制链接」，换哪个浏览器打开就测哪个。
#[tauri::command]
pub async fn browser_probe_start(
    app: tauri::AppHandle,
) -> Result<qb_app::browser_probe::ProbeStart> {
    use tauri_plugin_opener::OpenerExt;
    let mut started =
        qb_app::browser_probe::start(std::sync::Arc::new(TauriAssets(app.clone()))).await?;
    started.opened = app
        .opener()
        .open_url(started.url.clone(), None::<&str>)
        .is_ok();
    Ok(started)
}

/// 等那一次采集交回结果。两分钟没人交回是 `null`。
#[tauri::command]
pub async fn browser_probe_wait(token: String) -> Option<qb_app::browser_probe::BrowserReport> {
    qb_app::browser_probe::wait(&token).await
}

/// 最近一次交回来的报告（只在内存里，面板重启就没了）。界面自己按时刻判断新不新。
#[tauri::command]
pub fn browser_probe_last() -> Option<qb_app::browser_probe::BrowserReport> {
    qb_app::browser_probe::last_report()
}

// ------------------------------------------------------------------ 探测

#[tauri::command]
pub async fn probe_ip() -> Result<probe::ip::IpInfo> {
    probe::ip::ip_info().await
}

#[tauri::command]
pub async fn probe_ip_lookup(ip: String) -> Result<probe::ip_lookup::IpLookupReport> {
    probe::ip_lookup::lookup(&ip).await
}

#[tauri::command]
pub async fn probe_purity() -> Result<probe::verdict::PanelVerdict> {
    let info = probe::ip::ip_info().await?;
    Ok(probe::verdict::evaluate(&info))
}

#[tauri::command]
pub async fn probe_dns(app: tauri::AppHandle) -> Result<probe::dns::DnsReport> {
    // 10 个探针域名各报一次，界面上那条进度条才动得起来。
    let rep = Reporter::new(app, events::TASK_DNS_PROBE, 10);
    let r = probe::dns::check(&rep).await;
    match &r {
        Ok(_) => rep.done("检测完成"),
        Err(e) => rep.fail(&e.to_string()),
    }
    r
}

/// 权威站点地址与通过标准。前端原样展示，不要在前端硬编码第二份。
#[tauri::command]
pub fn purity_criteria() -> probe::verdict::PurityCriteria {
    probe::verdict::criteria()
}
