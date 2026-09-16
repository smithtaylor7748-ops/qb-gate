//! Chrome 的隐私面审计：策略、WebRTC、扩展的高风险权限（0.19.0）。
//!
//! # 只读优先，改只改一处
//!
//! 需求文档把这件事分成 A（只读审计）和 B（批准后修复）两段，这个模块照此分：
//! [`audit`] 只读，[`set_webrtc_policy`] / [`clear_webrtc_policy`] 是唯一会写的两个。
//!
//! # ⛔ 扩展只报告，绝不动手
//!
//! 文档里写死了「只报告风险，不擅自删除」「等待我确认后才禁用或删除」。
//! 所以这里连「禁用扩展」的函数都不提供 —— 没有那个函数，就不会有人误调它。
//! 使用者看完风险自己去 Chrome 里处理。
//!
//! # ⛔ 不判断「扩展有没有启用」
//!
//! 那要去解析 Chrome 的 `Secure Preferences`（内部结构，还带完整性校验）。
//! 判错的代价是一句说谎的结论：把启用着的报成停用的，使用者就放过了它。
//! 所以这里只回答「装着哪些」，启用与否进 [`Audit::unchecked`]。
//! 跟 `chrome::TraceReport::chrome_scanned` 那条「没扫 ≠ 没有」是同一条。
//!
//! # 策略写 HKCU，但两处都读
//!
//! Chrome 的策略 HKLM 和 HKCU 都认。写哪一处是有取舍的：
//!
//! | | HKLM | HKCU |
//! |---|---|---|
//! | 影响范围 | 整机所有用户 | 只当前用户 |
//! | 要管理员 | 要 | 不要 |
//!
//! 需求文档要求「所有修改都应尽量只影响我指定的软件」，所以写 **HKCU**。
//! 但**读的时候两处都读、并报出来源** —— 只写 HKCU 却只读 HKLM，会把自己
//! 刚写进去的策略报成「没设」。`checkup::check_doh` 目前只读 HKLM，
//! 那是它的既有口径，这里不改它，只在自己这一侧读全。
//!
//! # ⛔「注册表里有值」不等于「策略已生效」
//!
//! 需求文档专门点了这一条：要验证 `chrome://policy` 真的显示生效，
//! 而不只是注册表里有值。面板读不到那个页面，所以**不许把前者说成后者** ——
//! [`set_webrtc_policy`] 返回的话里必须带着「去 chrome://policy 核对」这一句。
//! 跟升级那条「不看安装器退出码，看磁盘」是同一条规矩，只是这里够不到磁盘，
//! 那就如实说够不到。
//!
//! # 为什么不复用 `sysenv::parse_reg_values`
//!
//! 它在 `qb-sysenv`，跟本 crate 同属 L2。横向依赖要往 `ALLOWED_SIDEWAYS`
//! 里加行，而那张表只许变短。跨 crate 重复这十来行读注册表的代码，
//! 比多一条横向依赖便宜得多。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

use crate::error::{GateError, Result};

/// Chrome 策略键。HKLM 与 HKCU 下路径相同。
const POLICY_SUBKEY: &str = r"SOFTWARE\Policies\Google\Chrome";
/// 官方策略名。
const WEBRTC_VALUE: &str = "WebRtcIPHandling";
/// 文档点名的那个取值：保留 WebRTC 功能，但不许走非代理的 UDP。
pub const WEBRTC_HARDENED: &str = "disable_non_proxied_udp";
/// DoH 策略名。
const DOH_VALUE: &str = "DnsOverHttpsMode";
/// 面板会写的那一档。
///
/// 不写 `secure`：`secure` 要求同时给出 `DnsOverHttpsTemplates`
/// （一个具体的 DoH 解析器地址），没给的话 Chrome 什么都解析不出来 ——
/// 「收紧隐私」的按钮把浏览器打断网，是这一类功能最糟的失败形态。
/// 而那个地址该填谁，只有使用者自己知道。
/// `automatic` 是有 DoH 就走 DoH、没有就退回系统解析，不会把人锁在门外。
pub const DOH_HARDENED: &str = "automatic";

/// 一个装着的扩展，以及它要到的高风险权限。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename = "BrowserExtension")]
pub struct Extension {
    pub id: String,
    /// 清单里的名字。用了 `__MSG_xxx__` 国际化占位时留空 —— **不猜**。
    pub name: Option<String>,
    pub version: String,
    /// 它属于哪个 Profile 目录（`Default` / `Profile 1`…）。
    pub profile: String,
    /// 命中的高风险权限，中文人话。空 = 一条都没命中。
    pub risky: Vec<String>,
}

/// 策略读到了，是从哪儿读到的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "PolicyScope")]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// HKCU —— 只影响当前用户。面板写的是这一处。
    CurrentUser,
    /// HKLM —— 整机策略，多半是别人（或企业 IT）设的。
    Machine,
}

/// 一条策略的现状。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename = "PolicyValue")]
pub struct Policy {
    pub value: String,
    pub scope: Scope,
}

/// Chrome 隐私面的只读审计结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, rename = "BrowserAudit")]
pub struct Audit {
    pub chrome_installed: bool,
    pub chrome_path: Option<String>,
    /// `WebRtcIPHandling` 现在是什么。`None` = 没设过策略（**不等于安全**，
    /// 那表示用的是 Chrome 自己的默认值）。
    pub webrtc: Option<Policy>,
    /// `DnsOverHttpsMode`。
    pub doh: Option<Policy>,
    pub extensions: Vec<Extension>,
    /// **没能检查的项，以及为什么。**
    ///
    /// 「没查」绝不能显示成「没问题」—— 这一档跟 `chrome_scanned` 同源。
    pub unchecked: Vec<String>,
}

impl Audit {
    /// WebRTC 策略是不是已经收紧到文档点名的那一档。
    pub fn webrtc_hardened(&self) -> bool {
        self.webrtc
            .as_ref()
            .is_some_and(|p| p.value == WEBRTC_HARDENED)
    }
}

// ------------------------------------------------------------------ 注册表

#[cfg(windows)]
fn reg_read(root: &str, name: &str) -> Option<String> {
    let out = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(["query", &format!(r"{root}\{POLICY_SUBKEY}"), "/v", name])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // 形如：`    WebRtcIPHandling    REG_SZ    disable_non_proxied_udp`
    // 值里可能有空格，所以按类型段切，不取「最后一个空白分段」。
    let line = text.lines().find(|l| l.contains(name))?;
    let ti = line.find("    REG_")?;
    let after = &line[ti + 4..];
    let vi = after.find("    ")?;
    let v = after[vi..].trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

#[cfg(not(windows))]
fn reg_read(_root: &str, _name: &str) -> Option<String> {
    None
}

/// Chrome machine policy overrides user policy; report the effective source.
pub fn read_policy(name: &str) -> Option<Policy> {
    effective_policy(reg_read("HKCU", name), reg_read("HKLM", name))
}

fn effective_policy(user: Option<String>, machine: Option<String>) -> Option<Policy> {
    if let Some(value) = machine {
        return Some(Policy {
            value,
            scope: Scope::Machine,
        });
    }
    user.map(|value| Policy {
        value,
        scope: Scope::CurrentUser,
    })
}

// ------------------------------------------------------------------ 扩展权限

/// 这个 host 匹配模式是不是「所有网站」那一档。
///
/// `<all_urls>` 是显式写法；`*://*/*`、`http://*/*` 这些则是通配到全站。
/// 判据放宽一点没关系（这一项的产出是「你自己去看一眼」），漏报才是问题。
fn is_broad_host(p: &str) -> bool {
    p == "<all_urls>" || p == "*://*/*" || p.contains("://*/")
}

fn strings(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 这个扩展清单要到了哪些高风险权限。**纯函数，单测钉着它。**
///
/// ⚠ MV2 与 MV3 的形状不一样，两种都得认：MV2 把 host 模式混在
/// `permissions` 里，MV3 拆出了独立的 `host_permissions`。
/// 只认其中一种，是这类扫描最典型的漏报 —— 而漏报在这里等于
/// 「面板说这个扩展没问题」。
pub fn risky_permissions(manifest: &serde_json::Value) -> Vec<String> {
    let perms = strings(manifest.get("permissions"));
    let optional = strings(manifest.get("optional_permissions"));
    let hosts_mv3 = strings(manifest.get("host_permissions"));
    let optional_hosts = strings(manifest.get("optional_host_permissions"));

    let has = |n: &str| perms.iter().chain(optional.iter()).any(|p| p == n);

    // MV2：host 模式混在 permissions 里；MV3：独立字段。两边一起看。
    let broad_host = perms
        .iter()
        .chain(hosts_mv3.iter())
        .chain(optional_hosts.iter())
        .any(|p| is_broad_host(p));

    // 内容脚本注入到全站，效果等同于「读取和更改所有网站数据」。
    let broad_content_script = manifest
        .get("content_scripts")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .any(|cs| strings(cs.get("matches")).iter().any(|m| is_broad_host(m)))
        })
        .unwrap_or(false);

    let mut out = Vec::new();
    if broad_host {
        out.push("读取和更改所有网站的数据".to_string());
    }
    if has("webRequest") || has("webRequestBlocking") {
        out.push("拦截或修改网络请求（webRequest）".to_string());
    }
    if has("proxy") {
        out.push("更改浏览器的代理设置（proxy）".to_string());
    }
    if has("nativeMessaging") {
        out.push("与本机程序通信（nativeMessaging）".to_string());
    }
    if has("cookies") {
        out.push("读取 Cookie".to_string());
    }
    if broad_content_script || has("scripting") {
        out.push("向页面注入脚本".to_string());
    }
    out
}

// ------------------------------------------------------------------ 扫描

fn read_manifest(dir: &std::path::Path) -> Option<(String, serde_json::Value)> {
    let mut versions: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect();
    versions.sort();
    // 同一个扩展可能留着多个版本目录，取排序最后那个 —— 也就是最新装的。
    let v = versions.pop()?;
    let name = v.file_name()?.to_str()?.to_string();
    let text = std::fs::read_to_string(v.join("manifest.json")).ok()?;
    Some((name, serde_json::from_str(&text).ok()?))
}

/// 扫一个 Profile 下装着的扩展。
fn scan_profile(profile: &std::path::Path) -> Vec<Extension> {
    let label = profile
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Profile")
        .to_string();
    let Ok(rd) = std::fs::read_dir(profile.join("Extensions")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.filter_map(|e| e.ok()) {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let Some((version, manifest)) = read_manifest(&dir) else {
            continue;
        };
        let raw_name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or("");
        out.push(Extension {
            id,
            // `__MSG_appName__` 这种要去 `_locales` 里查才知道真名。
            // 查不到就留空 —— 编一个名字比没有名字糟。
            name: if raw_name.starts_with("__MSG_") || raw_name.is_empty() {
                None
            } else {
                Some(raw_name.to_string())
            },
            version,
            profile: label.clone(),
            risky: risky_permissions(&manifest),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 只读审计。**不改任何东西。**
pub fn audit() -> Audit {
    let chrome = super::chrome::chrome_exe();
    let mut unchecked = Vec::new();

    let profiles = super::chrome::chrome_profiles();
    if profiles.is_empty() {
        unchecked.push("没找到 Chrome 的用户资料目录，扩展这一项没查。".into());
    }
    let extensions: Vec<Extension> = profiles.iter().flat_map(|p| scan_profile(p)).collect();

    // 装着 ≠ 启用着。判不了就说判不了。
    if !extensions.is_empty() {
        unchecked.push(
            "只列出「装着的」扩展 —— 是否已启用要解析 Chrome 的内部状态文件，\
             面板不去猜，请在 chrome://extensions 里自行核对。"
                .into(),
        );
    }

    Audit {
        chrome_installed: chrome.is_some(),
        chrome_path: chrome.map(|p| p.display().to_string()),
        webrtc: read_policy(WEBRTC_VALUE),
        doh: read_policy("DnsOverHttpsMode"),
        extensions,
        unchecked,
    }
}

// ------------------------------------------------------------------ 改策略

#[cfg(windows)]
fn reg_write(name: &str, data: &str) -> Result<()> {
    let out = crate::process::hidden_std(std::process::Command::new("reg"))
        .args([
            "add",
            &format!(r"HKCU\{POLICY_SUBKEY}"),
            "/v",
            name,
            "/t",
            "REG_SZ",
            "/d",
            data,
            "/f",
        ])
        .output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "写 Chrome 策略失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn reg_write(_name: &str, _data: &str) -> Result<()> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

#[cfg(windows)]
fn reg_delete(name: &str) -> Result<()> {
    let _ = crate::process::hidden_std(std::process::Command::new("reg"))
        .args([
            "delete",
            &format!(r"HKCU\{POLICY_SUBKEY}"),
            "/v",
            name,
            "/f",
        ])
        .output()?;
    Ok(())
}

#[cfg(not(windows))]
fn reg_delete(_name: &str) -> Result<()> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

#[derive(Serialize, Deserialize)]
struct PolicyBackup {
    value: Option<String>,
}

fn backup_path(name: &str) -> std::path::PathBuf {
    qb_foundation::paths::state_dir()
        .join("browser-policy-backups")
        .join(format!("{name}.json"))
}

/// Only a backup created by this panel enables undo.
pub fn policy_undoable(name: &str) -> bool {
    matches!(name, WEBRTC_VALUE | DOH_VALUE) && backup_path(name).is_file()
}

fn user_policy(name: &str) -> Result<Option<String>> {
    // These names are constants; reject arbitrary registry paths/PS syntax.
    if !matches!(name, WEBRTC_VALUE | DOH_VALUE) {
        return Err(GateError::Other("未知浏览器策略".into()));
    }
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$path = 'HKCU:\{POLICY_SUBKEY}'
$value = $null
if (Test-Path -LiteralPath $path) {{
  $key = Get-Item -LiteralPath $path
  if ($key.GetValueNames() -contains '{name}') {{
    if ($key.GetValueKind('{name}').ToString() -ne 'String') {{ throw '策略类型不是字符串，未修改' }}
    $value = [string]$key.GetValue('{name}')
  }}
}}
@{{value=$value}} | ConvertTo-Json -Compress
"#
    );
    let out = crate::process::powershell_std(&script).output()?;
    if !out.status.success() {
        return Err(GateError::Other(
            "无法完整读取浏览器策略原值，未修改".into(),
        ));
    }
    let saved: PolicyBackup = serde_json::from_slice(&out.stdout)?;
    Ok(saved.value)
}

fn set_backed_up(name: &str, next: &str) -> Result<()> {
    if let Some(p) = read_policy(name) {
        if p.scope == Scope::Machine && p.value != next {
            return Err(GateError::Other(
                "整机策略优先，当前用户修复无法覆盖。请联系策略管理员，原值未修改。".into(),
            ));
        }
    }
    let path = backup_path(name);
    crate::config_io::ensure_plain_path(&path)?;
    let before = user_policy(name)?;
    if !path.exists() {
        crate::config_io::replace(
            &path,
            Some(&serde_json::to_vec(&PolicyBackup {
                value: before.clone(),
            })?),
        )?;
    }
    reg_write(name, next)?;
    if user_policy(name)?.as_deref() != Some(next) {
        return Err(GateError::Other(
            "浏览器策略写入后核验失败，原值已保留，可撤销".into(),
        ));
    }
    Ok(())
}

fn restore_policy(name: &str, written: &str) -> Result<()> {
    let path = backup_path(name);
    let bytes = crate::config_io::read_optional(&path)?.ok_or_else(|| {
        GateError::Other("没有本面板保存的策略原值，不撤销其他程序设置的策略".into())
    })?;
    let original: PolicyBackup = serde_json::from_slice(&bytes)?;
    let current = user_policy(name)?;
    if current.as_deref() != Some(written) && current != original.value {
        return Err(GateError::Other(
            "策略已被其他程序修改，未覆盖新值；原始备份仍保留".into(),
        ));
    }
    match &original.value {
        Some(value) => reg_write(name, value)?,
        None => reg_delete(name)?,
    }
    if user_policy(name)? != original.value {
        return Err(GateError::Other("策略还原后核验失败，备份仍保留".into()));
    }
    crate::config_io::replace(&path, None)
}

/// 把 WebRTC 收紧到 `disable_non_proxied_udp`（当前用户）。
///
/// ⚠ **调用方必须已经拿到使用者当次的点击。**
///
/// 返回的话里**必须**带着「注册表写进去了 ≠ 策略已生效」那一句 ——
/// 面板读不到 `chrome://policy`，够不到就如实说够不到。
pub fn set_webrtc_policy() -> Result<String> {
    set_backed_up(WEBRTC_VALUE, WEBRTC_HARDENED)?;
    let back = read_policy(WEBRTC_VALUE);
    if back.as_ref().map(|p| p.value.as_str()) != Some(WEBRTC_HARDENED) {
        return Err(GateError::Other(
            "策略写进去了，但读回来对不上 —— 没有生效，请手动核对。".into(),
        ));
    }
    let msg = format!(
        "已为当前用户设置 {WEBRTC_VALUE} = {WEBRTC_HARDENED}。\
         这只说明注册表里有了这个值：要确认 Chrome 真的用上了，\
         请重启 Chrome 后打开 chrome://policy 核对。撤销入口在同一张卡片里。"
    );
    crate::audit::write(&msg);
    Ok(msg)
}

/// 把 Chrome 的 DoH 开到 `automatic`（当前用户）。
///
/// ⚠ 调用方必须已经拿到使用者当次的点击。跟 WebRTC 那条同一套：
/// 只写 HKCU、写完读回、返回的话里必须带「去 chrome://policy 核实」。
///
/// 为什么是 `automatic` 而不是 `secure`，见 [`DOH_HARDENED`]。
pub fn set_doh_policy() -> Result<String> {
    set_backed_up(DOH_VALUE, DOH_HARDENED)?;
    let back = read_policy(DOH_VALUE);
    if back.as_ref().map(|p| p.value.as_str()) != Some(DOH_HARDENED) {
        return Err(GateError::Other(
            "策略写进去了，但读回来对不上 —— 没有生效，请手动核对。".into(),
        ));
    }
    // ⛔ 这段话是纯文本渲染的（CLAUDE.md 最后一节），不许写 Markdown 的星号。
    let msg = format!(
        "已为当前用户设置 {DOH_VALUE} = {DOH_HARDENED}（有 DoH 就走 DoH，没有就退回系统解析）。         这只说明注册表里有了这个值：要确认 Chrome 真的用上了，请重启 Chrome 后打开          chrome://policy 核对。想更严的话要自己选一个解析器并配 DnsOverHttpsTemplates，         面板不替你挑解析器。"
    );
    crate::audit::write(&msg);
    Ok(msg)
}

/// 撤销：把面板设的那条 DoH 策略删掉。只删 HKCU 下的那条。
pub fn clear_doh_policy() -> Result<String> {
    restore_policy(DOH_VALUE, DOH_HARDENED)?;
    let msg = format!(
        "已还原当前用户的 {DOH_VALUE} 策略。         如果整机（HKLM）还压着一条同名策略，那不是面板设的，面板不碰它。"
    );
    crate::audit::write(&msg);
    Ok(msg)
}

/// 撤销：把面板设的那条 WebRTC 策略删掉。
///
/// 只删 HKCU 下的那条 —— **整机策略不是面板设的，一个字都不碰。**
pub fn clear_webrtc_policy() -> Result<String> {
    restore_policy(WEBRTC_VALUE, WEBRTC_HARDENED)?;
    let msg = format!(
        "已还原当前用户的 {WEBRTC_VALUE} 策略。\
         如果整机（HKLM）还压着一条同名策略，那不是面板设的，面板不碰它。"
    );
    crate::audit::write(&msg);
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn machine_policy_cannot_be_hidden_by_a_user_fix() {
        let effective = effective_policy(Some("automatic".into()), Some("off".into())).unwrap();
        assert_eq!(effective.scope, Scope::Machine);
        assert_eq!(effective.value, "off");
    }

    /// ⚠ MV3：host 权限在独立字段里。
    #[test]
    fn mv3_host_permissions_are_seen() {
        let m = json!({ "manifest_version": 3, "host_permissions": ["<all_urls>"] });
        assert!(risky_permissions(&m).contains(&"读取和更改所有网站的数据".to_string()));
    }

    /// ⚠ MV2：host 模式混在 `permissions` 里。**只认 MV3 就是漏报。**
    ///
    /// 漏报在这里等于面板说「这个扩展没问题」，比多报一条糟得多。
    #[test]
    fn mv2_hosts_mixed_into_permissions_are_also_seen() {
        let m = json!({ "manifest_version": 2, "permissions": ["tabs", "*://*/*"] });
        assert!(risky_permissions(&m).contains(&"读取和更改所有网站的数据".to_string()));
    }

    /// 文档点名的那几项权限都要认出来。
    #[test]
    fn the_permissions_the_document_names_are_all_flagged() {
        let m = json!({
            "permissions": ["webRequest", "proxy", "nativeMessaging", "cookies", "scripting"]
        });
        let r = risky_permissions(&m);
        for want in [
            "拦截或修改网络请求（webRequest）",
            "更改浏览器的代理设置（proxy）",
            "与本机程序通信（nativeMessaging）",
            "读取 Cookie",
            "向页面注入脚本",
        ] {
            assert!(r.iter().any(|x| x == want), "漏了「{want}」：{r:?}");
        }
    }

    /// 注入到全站的内容脚本，等同于「读取和更改所有网站数据」。
    #[test]
    fn a_content_script_on_every_site_counts_as_injection() {
        let m = json!({ "content_scripts": [{ "matches": ["*://*/*"] }] });
        assert!(risky_permissions(&m).contains(&"向页面注入脚本".to_string()));
    }

    /// ⛔ 老实的扩展不许被报成有风险 —— 误报多了，这一栏就没人看了。
    #[test]
    fn a_harmless_extension_raises_nothing() {
        let m = json!({
            "manifest_version": 3,
            "permissions": ["storage", "alarms"],
            "host_permissions": ["https://example.com/*"]
        });
        assert!(
            risky_permissions(&m).is_empty(),
            "{:?}",
            risky_permissions(&m)
        );
    }

    /// 单站点的 host 权限不算「所有网站」。
    #[test]
    fn a_single_site_host_is_not_broad() {
        assert!(!is_broad_host("https://example.com/*"));
        assert!(is_broad_host("<all_urls>"));
        assert!(is_broad_host("*://*/*"));
        assert!(is_broad_host("http://*/*"));
    }

    /// 收紧判定只认那一个取值，别的都不算数。
    #[test]
    fn only_the_documented_value_counts_as_hardened() {
        let mut a = Audit::default();
        assert!(!a.webrtc_hardened(), "没设策略不等于已收紧");

        a.webrtc = Some(Policy {
            value: "default".into(),
            scope: Scope::CurrentUser,
        });
        assert!(!a.webrtc_hardened());

        a.webrtc = Some(Policy {
            value: WEBRTC_HARDENED.into(),
            scope: Scope::CurrentUser,
        });
        assert!(a.webrtc_hardened());
    }
}
