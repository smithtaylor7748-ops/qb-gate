//! 一键关闭所有 Claude。
//!
//! 沿用现有 ClaudeKillSwitch.ps1 的**双重证据**判定，这是这个文件的全部要点：
//!
//!   一个进程只有满足下面之一才会被收：
//!     ① 可执行文件由 Anthropic 签名（Authenticode 主体含 Anthropic）
//!     ② 命令行**同时**命中 `bridge.py` 与本项目的数据目录
//!
//! **绝不能按进程名杀。** 叫 `claude.exe` 的东西可能是别人的；叫 `python.exe`
//! 的更是满地都是。按名字杀会误伤到用户正在干的别的活，而这个按钮的语义是
//! 「关掉我的 Claude」，不是「清理系统」。
//!
//! 收完之后要重新上锁——否则下一次双击 exe 就真的能起来了。

use crate::error::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Evidence {
    /// 可执行文件由 Anthropic 签名。
    AnthropicSigned,
    /// 命令行同时命中 bridge.py 与数据目录。
    BridgeAndDataDir,
}

#[derive(Debug, Clone, Serialize)]
pub struct KillTarget {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct KillReport {
    pub targets: Vec<KillTarget>,
    pub killed: Vec<u32>,
    pub failed: Vec<(u32, String)>,
    pub relocked: usize,
    /// 枚举到但**故意没动**的进程，连同放过的理由。界面上要显示出来。
    pub spared: Vec<String>,
}

/// PowerShell 枚举出来的一行原始进程信息。
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct RawProcess {
    #[serde(default, rename = "ProcessId")]
    pub pid: u32,
    #[serde(default, rename = "Name")]
    pub name: String,
    #[serde(default, rename = "ExecutablePath")]
    pub path: Option<String>,
    #[serde(default, rename = "CommandLine")]
    pub cmdline: Option<String>,
    /// Authenticode 签名主体。取不到就是 None，**取不到不等于没签名**，
    /// 但我们宁可放过也不误杀。
    #[serde(default, rename = "Signer")]
    pub signer: Option<String>,
}

/// 判定单个进程是否该收。纯函数，全部分支可单测。
///
/// `data_dir` 是本项目的桥接数据目录，比对时统一转小写做包含匹配 ——
/// Windows 路径大小写不敏感，但 `Win32_Process` 报回来的大小写并不稳定。
pub fn classify(p: &RawProcess, data_dir: &str) -> Option<Evidence> {
    // ① Anthropic 签名
    if let Some(signer) = &p.signer {
        if signer.to_lowercase().contains("anthropic") {
            return Some(Evidence::AnthropicSigned);
        }
    }

    // ② 命令行双重命中：bridge.py 与数据目录，缺一不可
    if let Some(cmd) = &p.cmdline {
        let c = cmd.to_lowercase();
        let d = data_dir.to_lowercase();
        if c.contains("bridge.py") && !d.is_empty() && c.contains(&d) {
            return Some(Evidence::BridgeAndDataDir);
        }
    }

    None
}

fn data_dir() -> String {
    dirs::data_local_dir()
        .map(|p| p.join("ClaudeTavernBridge").display().to_string())
        .unwrap_or_default()
}

/// 枚举候选进程并附带签名信息。
///
/// 只枚举 `claude.exe` / `Update.exe` / `python.exe` / `node.exe` 这几类，
/// 是为了少调几次 `Get-AuthenticodeSignature`（那个很慢）——
/// **缩小的是枚举范围，不是判定标准**。判定仍然只认上面那两条证据。
#[cfg(windows)]
async fn enumerate() -> Vec<RawProcess> {
    const SCRIPT: &str = r#"
$names = @('claude.exe','Update.exe','python.exe','pythonw.exe')
Get-CimInstance Win32_Process |
  Where-Object { $names -contains $_.Name } |
  ForEach-Object {
    $signer = $null
    if ($_.ExecutablePath) {
      try {
        $sig = Get-AuthenticodeSignature -FilePath $_.ExecutablePath -ErrorAction Stop
        if ($sig -and $sig.SignerCertificate) { $signer = $sig.SignerCertificate.Subject }
      } catch { }
    }
    [pscustomobject]@{
      ProcessId      = [int]$_.ProcessId
      Name           = [string]$_.Name
      ExecutablePath = $_.ExecutablePath
      CommandLine    = $_.CommandLine
      Signer         = $signer
    }
  } | ConvertTo-Json -Compress -AsArray
"#;

    let Ok(out) = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .output()
        .await
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<Vec<RawProcess>>(text.trim()).unwrap_or_default()
}

#[cfg(not(windows))]
async fn enumerate() -> Vec<RawProcess> {
    Vec::new()
}

/// 只看不动：列出会被收的进程，交给界面让用户确认。
pub async fn preview() -> KillReport {
    let dd = data_dir();
    let all = enumerate().await;
    let mut targets = Vec::new();
    let mut spared = Vec::new();

    for p in all {
        match classify(&p, &dd) {
            Some(evidence) => targets.push(KillTarget {
                pid: p.pid,
                name: p.name,
                path: p.path,
                evidence,
            }),
            None => spared.push(format!(
                "PID {} {} —— 两条证据都不满足，放过",
                p.pid, p.name
            )),
        }
    }

    KillReport {
        targets,
        killed: Vec::new(),
        failed: Vec::new(),
        relocked: 0,
        spared,
    }
}

/// 真正执行。收完重新上锁。
pub async fn execute() -> Result<KillReport> {
    let mut report = preview().await;

    for t in &report.targets {
        // /T 连子进程一起收：Squirrel 存根会拉起 app-* 下的真身，
        // 只收父进程会留下一堆孤儿。
        let out = tokio::process::Command::new("taskkill")
            .args(["/PID", &t.pid.to_string(), "/T", "/F"])
            .output()
            .await;
        match out {
            Ok(o) if o.status.success() => report.killed.push(t.pid),
            Ok(o) => report.failed.push((
                t.pid,
                String::from_utf8_lossy(&o.stderr).trim().to_string(),
            )),
            Err(e) => report.failed.push((t.pid, e.to_string())),
        }
    }

    // 收完必须重新上锁，否则下一次双击 exe 就真能起来了。
    report.relocked = crate::gate::lock_all().unwrap_or(0);
    crate::gate::log::write(&format!(
        "一键关闭：收掉 {} 个进程，重新上锁 {} 个可执行文件",
        report.killed.len(),
        report.relocked
    ));

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DD: &str = r"C:\Users\me\AppData\Local\ClaudeTavernBridge";

    fn proc(name: &str, cmd: Option<&str>, signer: Option<&str>) -> RawProcess {
        RawProcess {
            pid: 1234,
            name: name.into(),
            path: Some(format!(r"C:\x\{name}")),
            cmdline: cmd.map(String::from),
            signer: signer.map(String::from),
        }
    }

    #[test]
    fn anthropic_signature_is_enough() {
        let p = proc("claude.exe", None, Some("CN=Anthropic PBC, O=Anthropic PBC"));
        assert_eq!(classify(&p, DD), Some(Evidence::AnthropicSigned));
    }

    #[test]
    fn bridge_plus_data_dir_is_enough() {
        let cmd = format!(r"python.exe -u C:\proj\bridge.py --data-dir {DD} --port 5001");
        let p = proc("python.exe", Some(&cmd), None);
        assert_eq!(classify(&p, DD), Some(Evidence::BridgeAndDataDir));
    }

    #[test]
    fn bridge_without_data_dir_is_spared() {
        // 别人的 bridge.py —— 只有一条证据，不能动。
        let p = proc("python.exe", Some(r"python.exe -u D:\other\bridge.py"), None);
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn data_dir_without_bridge_is_spared() {
        let cmd = format!(r"python.exe -u other.py --data-dir {DD}");
        let p = proc("python.exe", Some(&cmd), None);
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn name_alone_never_qualifies() {
        // 这条是本文件存在的理由：叫 claude.exe 不构成任何证据。
        let p = proc("claude.exe", Some("claude.exe --help"), None);
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn unrelated_signer_is_spared() {
        let p = proc("python.exe", None, Some("CN=Python Software Foundation"));
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn path_case_differences_still_match() {
        let cmd = r"python.exe -u bridge.py --data-dir c:\users\me\appdata\local\claudetavernbridge";
        let p = proc("python.exe", Some(cmd), None);
        assert_eq!(classify(&p, DD), Some(Evidence::BridgeAndDataDir));
    }

    #[test]
    fn empty_data_dir_does_not_match_everything() {
        // data_dir 取不到时不能退化成「命令行里有 bridge.py 就杀」。
        let p = proc("python.exe", Some("python.exe -u bridge.py"), None);
        assert_eq!(classify(&p, ""), None);
    }
}
