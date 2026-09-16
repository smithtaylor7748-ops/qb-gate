//! 可执行文件的 Authenticode 签名。**平台层：只依赖 `process`。**
//!
//! # 为什么从 `killswitch` 里搬出来
//!
//! 「这个 exe 是谁签的」跟「把进程杀掉」是两件事。但 `signer_of` 一直住在
//! `killswitch` 里，于是 `install` 的五个地方（装完核对签名、升级前核对、
//! winget 验签）为了问一句签名而依赖了杀进程模块 —— 而 `killswitch` 反过来
//! 又要用 `install::inventory` 认「哪个进程是桌面端」。`install ↔ killswitch`
//! 这对环就是这么来的。

/// 单个文件的 Authenticode 主体。
///
/// 与上面 `enumerate` 里那段用的是同一个 `Get-AuthenticodeSignature`，
/// 抽出来给安装流程复用 —— 装完要核对新文件的签名主体含不含 Anthropic。
///
/// **`None` 表示拿不到，不表示没签名。** 这个区别在调用方那里要保住：
/// 拿不到只能说「没验成」，不能说成「验证失败」。
#[cfg(windows)]
pub async fn signer_of(path: &std::path::Path) -> Option<String> {
    // 单引号在 Windows 文件名里是合法字符，塞进 PowerShell 的单引号串之前
    // 必须按它的规矩转义成两个 —— 不转就是一条现成的命令拼接口子。
    let quoted = path.display().to_string().replace('\'', "''");
    let script = format!(
        "try {{ (Get-AuthenticodeSignature -LiteralPath '{quoted}' -ErrorAction Stop).SignerCertificate.Subject }} catch {{ }}"
    );
    let out = crate::process::powershell_tokio(&script)
        .output()
        .await
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(not(windows))]
pub async fn signer_of(_path: &std::path::Path) -> Option<String> {
    None
}
