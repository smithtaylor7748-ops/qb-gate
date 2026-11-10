//! 官方原生安装器的引导程序：每次下载到独立文件，再由官方二进制执行安装。
//!
//! 官方 PowerShell 脚本按版本复用下载文件；旧面板退出后留下的下载进程会占用
//! 同一个文件。这里只替代下载引导，不接管官方的安装布局、账户或升级逻辑。

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
#[cfg(any(windows, test))]
use std::path::Path;
use std::path::PathBuf;
use std::process::Output;
#[cfg(any(windows, test))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(windows)]
use std::time::Duration;
#[cfg(any(windows, test))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(any(windows, test))]
use super::{inventory, managed};

#[cfg(windows)]
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
#[cfg(windows)]
const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);
#[cfg(any(windows, test))]
static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

/// 持有本次操作唯一的引导程序。正常结束、错误和取消都清理同一个文件。
///
/// 文件放在 inventory 已枚举的下载目录内；删除失败时补执行锁，随后维护窗口
/// 的重锁也会覆盖它。上层应在维护窗口收尾前释放此值。
#[derive(Debug)]
pub struct PreparedInstaller {
    path: PathBuf,
}

impl PreparedInstaller {
    #[cfg(any(windows, test))]
    fn create_in(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..32 {
            let serial = NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!(
                "claude-qbgate-{}-{stamp}-{serial}.exe",
                std::process::id()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(GateError::Other(
            "无法创建独立的 Claude 安装下载文件".into(),
        ))
    }

    /// 与官方引导脚本相同：执行 `claude install <渠道或明确版本>`。
    /// 运行前由调用方打开维护窗口，以解开下载副本和既有安装的执行锁。
    #[cfg(windows)]
    pub async fn run(&self, target: &str) -> Result<Output> {
        if !valid_target(target) {
            return Err(GateError::Other(
                "Claude 安装目标不是有效渠道或版本号".into(),
            ));
        }
        let path = self.path.display().to_string().replace('\'', "''");
        let target = target.replace('\'', "''");
        let script = format!("& '{path}' install '{target}'\nexit $LASTEXITCODE");
        run_script(&script, INSTALL_TIMEOUT).await
    }

    #[cfg(not(windows))]
    pub async fn run(&self, _target: &str) -> Result<Output> {
        Err(GateError::Other(
            "官方原生安装功能只在 Windows 上可用".into(),
        ))
    }
}

impl Drop for PreparedInstaller {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                #[cfg(windows)]
                let protection = match crate::acl::current_user_sid()
                    .and_then(|sid| crate::acl::lock(&self.path, &sid))
                {
                    Ok(()) => "已补上执行锁".to_string(),
                    Err(lock) => format!("执行锁也未能恢复：{lock}"),
                };
                #[cfg(not(windows))]
                let protection = "保留供下次清理";
                crate::audit::write(&format!(
                    "清理 Claude 安装下载文件 {} 失败：{e}；{protection}",
                    self.path.display()
                ));
            }
        }
    }
}

/// 动态获取 latest 引导程序（官方脚本对 stable 安装也用 latest 引导），
/// 校验官方 manifest 的 SHA-256，再核对签名主体。不会改变用户目录环境变量。
#[cfg(windows)]
pub async fn prepare(rep: &dyn ProgressSink, step: u32) -> Result<PreparedInstaller> {
    let dir = inventory::native_downloads(&inventory::Roots::current())
        .ok_or_else(|| GateError::Other("找不到当前用户的 Claude 下载目录".into()))?;
    tokio::time::timeout(DOWNLOAD_TIMEOUT, async {
        let release = managed::claude_release("latest").await?;
        if !valid_target(&release.version) {
            return Err(GateError::Other("官方发布源返回了无效版本号".into()));
        }
        rep.log(step, &format!("下载官方安装引导程序 {}，使用独立下载文件", release.version));
        let installer = download_verified(&dir, &release, rep, step).await?;
        // 使用同一个受控 runner，签名探针取消或超时也不留下 PowerShell。
        let quoted = installer.path.display().to_string().replace('\'', "''");
        let script = format!(
            "try {{ (Get-AuthenticodeSignature -LiteralPath '{quoted}' -ErrorAction Stop).SignerCertificate.Subject }} catch {{ exit 1 }}"
        );
        let output = run_script(&script, Duration::from_secs(30)).await?;
        let signer = String::from_utf8_lossy(&output.stdout);
        if !output.status.success()
            || super::winget::signature_has_anthropic(Some(signer.trim())) != Some(true)
        {
            return Err(GateError::Other("未能确认官方 Claude 安装引导程序的 Anthropic 签名，已停止".into()));
        }
        rep.log(step, "官方 SHA-256 与 Anthropic 签名主体核对通过");
        Ok(installer)
    })
    .await
    .map_err(|_| GateError::Other("下载 Claude 安装引导程序超过 15 分钟，已停止并清理本次下载".into()))?
}

#[cfg(not(windows))]
pub async fn prepare(_rep: &dyn ProgressSink, _step: u32) -> Result<PreparedInstaller> {
    Err(GateError::Other(
        "官方原生安装功能只在 Windows 上可用".into(),
    ))
}

#[cfg(any(windows, test))]
async fn download_verified(
    dir: &Path,
    release: &managed::Release,
    rep: &dyn ProgressSink,
    step: u32,
) -> Result<PreparedInstaller> {
    let installer = PreparedInstaller::create_in(dir)?;
    let hash = managed::download(&release.url, &installer.path, release.size, rep, step).await?;
    if !release
        .sha256
        .as_deref()
        .is_some_and(|sha| sha.eq_ignore_ascii_case(&hash))
    {
        return Err(GateError::Other(
            "Claude 安装引导程序的 SHA-256 与官方清单不一致，已删除本次下载".into(),
        ));
    }
    Ok(installer)
}

#[cfg(any(windows, test))]
fn valid_target(target: &str) -> bool {
    if matches!(target, "latest" | "stable") {
        return true;
    }
    let (core, suffix) = target.split_once('-').unwrap_or((target, ""));
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        && (!target.contains('-') || !suffix.is_empty())
        && suffix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

/// 取消 future 无法 await；同步兜底保证安装子树先退出，外层维护窗口才会重锁。
/// 正常收尾已异步等到 ActiveProcesses=0 时，Drop 不再重复等待。
#[cfg(windows)]
struct InstallerJob {
    job: crate::process::KillOnCloseJob,
    drained: bool,
}

#[cfg(windows)]
impl InstallerJob {
    fn new() -> Result<Self> {
        Ok(Self {
            job: crate::process::KillOnCloseJob::new()?,
            drained: false,
        })
    }

    async fn finish(&mut self) -> Result<()> {
        self.job.terminate_and_wait(Duration::from_secs(5)).await?;
        self.drained = true;
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for InstallerJob {
    fn drop(&mut self) {
        if !self.drained {
            if let Err(e) = self.job.terminate_and_wait_blocking(Duration::from_secs(5)) {
                crate::audit::write(&format!("取消 Claude 安装时未能确认安装进程树已退出：{e}"));
            }
        }
    }
}

#[cfg(windows)]
const START_SIGNAL: &str = "QB_GATE_INSTALL_START";

/// PowerShell 启动后先等 stdin；未分配 Job 的窗口内不能执行正文或派生安装器。
/// 父进程在握手前退出时 stdin 关闭，ReadLine 返回空，正文也不会执行。
#[cfg(windows)]
fn script_command(script: &str) -> tokio::process::Command {
    crate::process::powershell_tokio(&format!(
        "$ErrorActionPreference = 'Stop'\nif ([Console]::ReadLine() -ne '{START_SIGNAL}') {{ exit 1 }}\n{script}"
    ))
}

/// 保留 Child 到回收完成；关闭/超时/取消时，Job 同时结束安装器的后代进程。
#[cfg(windows)]
async fn run_script(script: &str, limit: Duration) -> Result<Output> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut cmd = script_command(script);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // 这个守卫属于 run_script future；取消时先同步回收安装进程树，
    // 才从该 future 返回到外层 PreparedInstaller / Maintenance 的 Drop。
    let mut job = InstallerJob::new()?;
    let mut child = crate::process::hidden_tokio(cmd).spawn()?;
    if let Err(e) = job.job.assign(&child) {
        let _ = child.kill().await;
        return Err(e);
    }
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin.write_all(START_SIGNAL.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    drop(stdin);
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = tokio::time::timeout(limit, async {
        let wait = async {
            let status = child.wait().await?;
            // 后代可能继承 stdout/stderr；先结束它们，读管道才会遇到 EOF。
            job.finish().await.map_err(std::io::Error::other)?;
            Ok::<_, std::io::Error>(status)
        };
        let (status, _, _) = tokio::try_join!(
            wait,
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err)
        )?;
        Ok::<_, std::io::Error>(status)
    })
    .await;
    // 即使主进程已退出，也不留它派生的后台进程。
    let stopped = job.finish().await;
    if !matches!(&result, Ok(Ok(_))) {
        let _ = child.kill().await;
    }
    stopped?;
    match result {
        Ok(Ok(status)) => Ok(Output {
            status,
            stdout: out,
            stderr: err,
        }),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(GateError::Other(format!(
            "Claude 安装程序超过 {} 秒未结束，已停止本次安装进程及其子进程，请重试",
            limit.as_secs()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "qb-native-install-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn separate_attempts_never_reuse_a_download_and_drop_only_their_own_file() {
        let t = Temp::new();
        let existing = t.0.join("claude-83.4.917-win32-x64.exe");
        std::fs::write(&existing, b"another install").unwrap();
        let a = PreparedInstaller::create_in(&t.0).unwrap();
        let b = PreparedInstaller::create_in(&t.0).unwrap();
        assert_ne!(a.path, b.path);
        let path = a.path.clone();
        drop(a);
        assert!(!path.exists());
        assert!(b.path.exists());
        assert_eq!(std::fs::read(existing).unwrap(), b"another install");
    }

    #[test]
    fn target_validation_accepts_both_channels_and_future_versions() {
        for v in ["latest", "stable", "2.1.322", "17.80.9999", "3.0.0-beta.17"] {
            assert!(valid_target(v), "{v}");
        }
        for v in [
            "",
            "2.1",
            "2.1.3-",
            "2.1.3-../../x",
            "latest;exit",
            "3.0.0 x",
        ] {
            assert!(!valid_target(v), "{v}");
        }
    }

    #[test]
    fn staged_installers_remain_visible_to_gate_inventory() {
        let t = Temp::new();
        let roots = inventory::Roots {
            home: Some(t.0.clone()),
            ..Default::default()
        };
        let dir = inventory::native_downloads(&roots).unwrap();
        let staged = PreparedInstaller::create_in(&dir).unwrap();
        let found = inventory::scan(&roots);
        assert!(found
            .iter()
            .any(|i| i.path == staged.path && i.lockable && !i.launchable));
    }

    use crate::sink::Silent;

    async fn serve(body: &'static [u8]) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/claude.exe", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request).await.unwrap();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            stream.write_all(body).await.unwrap();
        });
        (url, task)
    }

    #[tokio::test]
    async fn checksum_failure_removes_only_the_failed_attempt() {
        let t = Temp::new();
        let other = PreparedInstaller::create_in(&t.0).unwrap();
        let (url, server) = serve(b"not the expected binary").await;
        let release = managed::Release {
            version: "7.8.900".into(),
            url,
            sha256: Some("0".repeat(64)),
            size: None,
        };
        assert!(download_verified(&t.0, &release, &Silent, 3).await.is_err());
        server.await.unwrap();
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 1);
        assert!(other.path.exists());
    }

    #[tokio::test]
    async fn cancelling_a_download_removes_its_partial_file() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let t = Temp::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let release = managed::Release {
            version: "83.4.917".into(),
            url: format!("http://{}/claude.exe", listener.local_addr().unwrap()),
            sha256: Some("0".repeat(64)),
            size: None,
        };
        let dir = t.0.clone();
        let download =
            tokio::spawn(async move { download_verified(&dir, &release, &Silent, 3).await });
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\npartial")
            .await
            .unwrap();
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 1);
        download.abort();
        assert!(download.await.unwrap_err().is_cancelled());
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn an_exclusively_locked_shared_cache_does_not_block_a_new_download() {
        use sha2::{Digest, Sha256};
        use std::os::windows::fs::OpenOptionsExt;
        let t = Temp::new();
        let existing = t.0.join("claude-83.4.917-win32-x64.exe");
        std::fs::write(&existing, b"busy").unwrap();
        let _locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&existing)
            .unwrap();
        let (url, server) = serve(b"verified bootstrap").await;
        let release = managed::Release {
            version: "83.4.917".into(),
            url,
            sha256: Some(hex::encode(Sha256::digest(b"verified bootstrap"))),
            size: None,
        };
        let installer = download_verified(&t.0, &release, &Silent, 3).await.unwrap();
        server.await.unwrap();
        assert_ne!(installer.path, existing);
        assert_eq!(
            std::fs::read(&installer.path).unwrap(),
            b"verified bootstrap"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn closing_stdin_without_the_handshake_never_executes_the_target() {
        use std::process::Stdio;
        let t = Temp::new();
        let marker = t.0.join("target-was-run.txt");
        let quoted = marker.display().to_string().replace('\'', "''");
        let script = format!("[IO.File]::WriteAllText('{quoted}', 'ran')");
        let mut cmd = script_command(&script);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // 故意不分配 Job，也不发送握手，模拟父进程在 assign 前退出。
        let mut child = cmd.spawn().unwrap();
        drop(child.stdin.take());
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.code(), Some(1));
        assert!(
            !marker.exists(),
            "target ran before the parent assigned its Job"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn nonzero_installer_exit_and_stderr_are_preserved() {
        let output = run_script(
            "[Console]::Error.WriteLine('test failure'); exit 7",
            Duration::from_secs(20),
        )
        .await
        .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&output.stderr).contains("test failure"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn timeout_terminates_the_installers_process_tree() {
        let t = Temp::new();
        let pid_file = t.0.join("child.pid");
        let quoted = pid_file.display().to_string().replace('\'', "''");
        let script = format!(
            "$c = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile -NonInteractive -Command Start-Sleep -Seconds 60' -WindowStyle Hidden -PassThru; [IO.File]::WriteAllText('{quoted}', [string]$c.Id); Start-Sleep -Seconds 60"
        );
        let result = run_script(&script, Duration::from_secs(8)).await;
        assert!(result.is_err());
        let pid: u32 = std::fs::read_to_string(pid_file)
            .expect("test descendant started")
            .parse()
            .unwrap();
        assert_stopped(pid);
    }

    #[cfg(windows)]
    fn assert_stopped(pid: u32) {
        use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };
        // 立即检查：不能在断言里再等进程退出，遮住取消与维护窗口重锁之间的竞态。
        match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
            Ok(handle) => {
                let state = unsafe { WaitForSingleObject(handle, 0) };
                unsafe {
                    let _ = CloseHandle(handle);
                }
                assert_eq!(state, WAIT_OBJECT_0, "installer descendant still runs");
            }
            Err(e) => assert_eq!(
                e.code(),
                windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0),
                "only an already-reaped process is accepted"
            ),
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cancelling_the_runner_terminates_its_descendants() {
        let t = Temp::new();
        let pid_file = t.0.join("cancel-child.pid");
        let quoted = pid_file.display().to_string().replace('\'', "''");
        let script = format!(
            "$c = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile -NonInteractive -Command Start-Sleep -Seconds 60' -WindowStyle Hidden -PassThru; [IO.File]::WriteAllText('{quoted}', [string]$c.Id); Start-Sleep -Seconds 60"
        );
        let run = tokio::spawn(async move { run_script(&script, Duration::from_secs(60)).await });
        let pid = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = text.parse::<u32>() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("test descendant started");
        run.abort();
        assert!(run.await.unwrap_err().is_cancelled());
        assert_stopped(pid);
    }
}
