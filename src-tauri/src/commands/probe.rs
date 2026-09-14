//! 出口探测与环境体检相关的命令。

use crate::sink::ProgressSink;
use crate::{
    error::Result,
    events::{self, Reporter},
    probe, sysenv,
};

/// 运行环境体检：系统代理、IPv6、浏览器 DoH、MCP 配置里的明文密钥。
///
/// 读注册表和本地配置文件，**不联网、不改任何东西**。
#[tauri::command]
pub async fn checkup_scan() -> sysenv::checkup::Checkup {
    sysenv::checkup::scan().await
}

// ------------------------------------------------------------------ 探测

#[tauri::command]
pub async fn probe_ip() -> Result<probe::ip::IpInfo> {
    probe::ip::ip_info().await
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
