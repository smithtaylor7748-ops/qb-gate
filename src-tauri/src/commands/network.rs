//! 「两个口子」与浏览器隐私面的命令（0.19.0）。
//!
//! # 这一层只搬参数
//!
//! 三条硬约束（`CLAUDE.md`「两个口子」一节）的落实分别在
//! `platform::firewall`、`sysenv::proxy`、`install::browser_audit` 里，
//! 这里不重复判断，只负责把使用者那一次点击转成一次调用。
//!
//! # ⛔ 这个文件里的每一条会改系统的命令，都只能由点击触发
//!
//! 没有定时器、没有启动钩子、看门狗也不碰它们。**别给这些命令找任何
//! 自动调用点** —— 那三条约束的第三条就是这个意思，而它只能在调用侧保证。
//!
//! # 读和写分开
//!
//! `*_read` / `*_rules` / `browser_audit` 一律只读，不取独占锁 ——
//! 跟 `purge_plan` 不取锁是同一条：只读的东西要随时看得了，
//! 否则界面会在别的操作跑着时莫名其妙地查不动。

use crate::{
    error::{GateError, Result},
    firewall, install, operations, sysenv, usecase,
};

// ------------------------------------------------------------------ 出站锁

/// 当前由面板加的出站规则。**只列面板自己加的。**
#[tauri::command]
pub fn firewall_rules() -> Result<Vec<firewall::Rule>> {
    firewall::list()
}

/// 本机网卡清单。**不替使用者判断哪块是物理网卡、哪块是 TUN。**
#[tauri::command]
pub fn firewall_adapters() -> Result<Vec<firewall::Adapter>> {
    firewall::adapters()
}

/// 给一个 exe 加出站阻止规则，每个点名的接口各一条。
///
/// ⚠ 界面必须已经让使用者选好了**要拦哪几块网卡**，并且看过代价：
/// 规则不随面板退出消失（DISCLAIMER §5.2）。
#[tauri::command]
pub async fn firewall_block(exe: String, interfaces: Vec<String>) -> Result<Vec<firewall::Rule>> {
    let _guard = operations::exclusive().await?;
    if interfaces.is_empty() {
        return Err(GateError::Other(
            "没有选中任何网卡 —— 不选接口就等于把这个程序的所有出口都拦掉，包括你想让它走的那条"
                .into(),
        ));
    }
    let mut out = Vec::new();
    for i in &interfaces {
        out.push(firewall::add_block(&exe, i)?);
    }
    Ok(out)
}

/// 一键撤销：把面板加过的规则全删掉。返回删了几条。
#[tauri::command]
pub async fn firewall_revoke_all() -> Result<usize> {
    let _guard = operations::exclusive().await?;
    firewall::remove_all()
}

// ------------------------------------------------------------------ 系统代理

#[tauri::command]
pub fn proxy_read() -> Result<sysenv::proxy::ProxyState> {
    sysenv::proxy::read()
}

/// 面板动手之前那一份原值。`None` = 没改过，界面不该显示「还原」。
#[tauri::command]
pub fn proxy_backup() -> Option<sysenv::proxy::ProxyState> {
    sysenv::proxy::backup()
}

/// 改系统代理。**改之前先把原值存下来**，否则回滚无从谈起。
///
/// ⚠ 整机设置：所有跟随系统代理的程序都会跟着变，改错会当场断网。
/// 界面上那一次确认必须把这件事说全。
#[tauri::command]
pub async fn proxy_apply(next: sysenv::proxy::ProxyState) -> Result<sysenv::proxy::ProxyState> {
    let _guard = operations::exclusive().await?;
    // 顺序不能反：先存原值再动手。反过来的话，中途失败就留下一个
    // 改了一半、又没有备份的系统。
    let before = sysenv::proxy::read()?;
    sysenv::proxy::save_backup(&before)?;
    sysenv::proxy::apply(&next)
}

/// 回滚到面板动手之前那一份。
#[tauri::command]
pub async fn proxy_rollback() -> Result<String> {
    let _guard = operations::exclusive().await?;
    let Some(original) = sysenv::proxy::backup() else {
        return Err(GateError::Other(
            "没有存下来的原值 —— 面板没改过系统代理，没有可还原的东西".into(),
        ));
    };
    sysenv::proxy::restore(&original)?;
    // 还原完把备份清掉，否则界面上会一直挂着一个没有意义的「还原」按钮。
    sysenv::proxy::clear_backup();
    Ok(format!(
        "系统代理已还原为 {}",
        sysenv::proxy::describe(&original)
    ))
}

// ------------------------------------------------------------------ 浏览器

/// Chrome 隐私面的**只读**审计：策略、WebRTC、扩展的高风险权限。
///
/// 扩展这一项**只报告**，面板不提供禁用或删除的入口。
#[tauri::command]
pub fn browser_audit() -> install::browser_audit::Audit {
    install::browser_audit::audit()
}

/// 把 WebRTC 收紧到 `disable_non_proxied_udp`（只影响当前用户）。
///
/// 返回的话里带着「注册表里有值 ≠ 策略已生效」那一句 —— 面板读不到
/// `chrome://policy`，够不到就如实说够不到，**界面不许把它改写成「已生效」**。
#[tauri::command]
pub async fn browser_webrtc_harden() -> Result<String> {
    let _guard = operations::exclusive().await?;
    install::browser_audit::set_webrtc_policy()
}

/// 撤销上面那条策略。只删面板设的那条（HKCU），整机策略一个字不碰。
#[tauri::command]
pub async fn browser_webrtc_clear() -> Result<String> {
    let _guard = operations::exclusive().await?;
    install::browser_audit::clear_webrtc_policy()
}

// ------------------------------------------------------------ 出口一致性

/// 跑一轮出口一致性检查。**只读**（但会真发两轮探测：绕过代理与跟随代理）。
///
/// `country` 是当前出口国家的两字母码，由界面从它那份 `R.ip` 传进来 ——
/// 出口 IP 全项目只探一处，这里不自己再发一轮。
#[tauri::command]
pub async fn egress_checks_scan(country: Option<String>) -> usecase::egress_checks::EgressChecks {
    usecase::egress_checks::scan(country).await
}

/// 修某一项（WebRTC / DoH 策略）。**只由使用者当次点击触发。**
#[tauri::command]
pub async fn egress_checks_fix(id: String) -> Result<String> {
    let _guard = operations::exclusive().await?;
    usecase::egress_checks::fix(&id)
}

/// 撤销面板刚才改的那一项。
#[tauri::command]
pub async fn egress_checks_undo(id: String) -> Result<String> {
    let _guard = operations::exclusive().await?;
    usecase::egress_checks::undo(&id)
}
