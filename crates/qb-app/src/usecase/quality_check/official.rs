//! 官方账号只通过公开 codex exec；本模块不读取、复制或导出认证文件。
use super::parse::{Decoder, Protocol};
use super::*;
use crate::gate::GateState;
use crate::process::KillOnCloseJob;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

fn error(text: &str) -> GateError {
    GateError::Other(text.into())
}

struct Runtime(PathBuf);
impl Runtime {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("qb-quality-{}", uuid::Uuid::new_v4()));
        crate::config_io::ensure_plain_path(&path)
            .map_err(|_| error("检测临时目录不满足隔离要求"))?;
        std::fs::create_dir(&path).map_err(|_| error("无法创建检测临时目录"))?;
        Ok(Self(path))
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // 只删除自己创建的空目录；绝不递归清理未知文件或用户的账户目录。
        let _ = std::fs::remove_dir(&self.0);
    }
}

struct GateLease {
    gate: Arc<GateState>,
    holder: Option<String>,
    generation: u64,
}
impl GateLease {
    async fn open(
        gate: &Arc<GateState>,
        request: &QualityCheckRequest,
        start_watchdog: impl Fn(),
    ) -> Result<Self> {
        let mut lease = Self {
            gate: Arc::clone(gate),
            holder: None,
            generation: 0,
        };
        let _lock = crate::operations::exclusive_soon().await?;
        let gated = crate::launch::LaunchTarget::Codex.gated();
        registry::check_gate(&request.run_id, gate, gated)?;
        if !gated {
            return Ok(lease);
        }
        let holder = format!("codex-quality-{}", request.request_id);
        if let Err(error) = crate::gate::open_authorized_with_mode(
            &holder,
            Some(crate::launch::LaunchTarget::Codex.watch_mode()),
            gate,
        )
        .await
        {
            registry::revoke(&request.run_id);
            return Err(error);
        }
        lease.generation = gate.generation.load(Ordering::SeqCst);
        lease.holder = Some(holder);
        start_watchdog();
        Ok(lease)
    }
    async fn revoked(&self) {
        let Some(holder) = self.holder.as_deref() else {
            std::future::pending::<()>().await;
            return;
        };
        loop {
            let valid = self.gate.generation.load(Ordering::SeqCst) == self.generation
                && !self.gate.manual_paused.load(Ordering::SeqCst)
                && self
                    .gate
                    .lease
                    .lock()
                    .is_ok_and(|lease| lease.holders.contains_key(holder));
            if !valid {
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    async fn close(&mut self) -> Result<()> {
        if self.holder.is_some() {
            let _lock = crate::operations::exclusive_unchecked().await;
            if let Some(holder) = self.holder.take() {
                crate::gate::release_holder(&self.gate, &holder)
                    .map_err(|_| error("检测已结束，但检测租约清理未完整完成"))?;
            }
        }
        Ok(())
    }
}
impl Drop for GateLease {
    fn drop(&mut self) {
        let Some(holder) = self.holder.take() else {
            return;
        };
        let gate = Arc::clone(&self.gate);
        // future 被取消时仍回收自己的租约。操作锁防止与另一个放行/手动关闭交错。
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _lock = crate::operations::exclusive_unchecked().await;
                if crate::gate::release_holder(&gate, &holder).is_err() {
                    tracing::error!("质量检测租约清理失败");
                }
            });
        }
    }
}

fn environment(home: &Path) -> Vec<(String, String)> {
    // 官方调用不用继承的 provider/auth/proxy 环境；门禁核验的是本机直连出口。
    crate::sessions::sanitized_environment(
        std::env::vars().filter(|(key, _)| {
            let key = key.to_ascii_uppercase();
            !["CODEX_", "OPENAI_", "ANTHROPIC_", "CLAUDE_"]
                .iter()
                .any(|prefix| key.starts_with(prefix))
                && !["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"].contains(&key.as_str())
        }),
        vec![("CODEX_HOME".into(), home.display().to_string())],
    )
}

fn args(runtime: &Path, request: &QualityCheckRequest) -> Vec<String> {
    let mut args = crate::gpt_bridge::exec_args_isolated(
        runtime,
        &request.model,
        &request.effort,
        false,
        true,
    );
    args.pop();
    for config in [
        "features.shell_tool=false",
        "web_search=\"disabled\"",
        "project_doc_max_bytes=0",
        "history.persistence=\"none\"",
    ] {
        args.push("-c".into());
        args.push(config.into());
    }
    args.push("-".into());
    args
}

fn command(exe: &Path, runtime: &Path, home: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(exe);
    cmd.current_dir(runtime)
        .env_clear()
        .envs(environment(home))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::process::hidden_tokio(cmd)
}

async fn bounded(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    (&mut reader)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| error("读取 Codex 检测输出失败"))?;
    if bytes.len() > limit {
        return Err(error("Codex 检测输出超过安全读取上限"));
    }
    Ok(bytes)
}

async fn supports_isolation(exe: &Path, runtime: &Path, home: &Path) -> Result<()> {
    let job = KillOnCloseJob::new()?;
    let mut child = command(exe, runtime, home)
        .args(["exec", "--help"])
        .spawn()
        .map_err(|_| error("无法启动 Codex CLI 检查隔离参数；请检查安装和门禁"))?;
    job.assign(&child)?;
    drop(child.stdin.take());
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error("无法读取 Codex CLI 帮助"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| error("无法读取 Codex CLI 帮助"))?;
    let read = async {
        let (out, _, status) = tokio::try_join!(
            bounded(stdout, 256 * 1024),
            bounded(stderr, 64 * 1024),
            async {
                child
                    .wait()
                    .await
                    .map_err(|_| error("Codex CLI 帮助检查失败"))
            }
        )?;
        if !status.success()
            || !crate::gpt_bridge::help_supports_ignore_user_config(&String::from_utf8_lossy(&out))
        {
            return Err(error(
                "当前 Codex CLI 不支持 --ignore-user-config，请先升级官方 CLI 后再运行质量检测",
            ));
        }
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(10), read)
        .await
        .map_err(|_| error("Codex CLI 隔离参数检查超时"))?
}

async fn read_events(
    mut stdout: impl AsyncRead + Unpin,
    result: &mut QualityCheckResult,
    started: Instant,
) -> Result<()> {
    let mut decoder = Decoder::new(Protocol::Cli, true);
    let mut buffer = [0u8; 8192];
    loop {
        let count = stdout
            .read(&mut buffer)
            .await
            .map_err(|_| error("Codex 检测输出读取中断"))?;
        if count == 0 {
            break;
        }
        decoder.push(
            &buffer[..count],
            result,
            started.elapsed().as_millis() as u64,
        )?;
    }
    decoder.finish(result, started.elapsed().as_millis() as u64)
}

pub(super) async fn run(
    request: &QualityCheckRequest,
    result: &mut QualityCheckResult,
    gate: &Arc<GateState>,
    start_watchdog: impl Fn() + Send,
) -> Result<()> {
    let started = Instant::now();
    // directory 只校验索引和安全路径；slot / active / list 会读 auth，禁止在这里使用。
    let home = qb_accounts::codex::directory(&qb_accounts::codex::root(), &request.target.id)
        .map_err(|_| error("所选 Codex 官方账户不存在或目录不安全"))?
        .join("home");
    crate::config_io::ensure_plain_path(&home)
        .map_err(|_| error("所选 Codex 官方账户目录不安全"))?;
    if !home.is_dir() {
        return Err(error("所选 Codex 官方账户目录不存在，请先完成官方登录"));
    }
    let exe = crate::plugins::sillytavern::find_official_codex()
        .map_err(|_| error("未找到官方 Codex CLI，请先安装"))?;
    #[cfg(windows)]
    if !exe
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err(error("质量检测需要官方 Codex 原生 exe，请先升级 CLI"));
    }
    // 仅记录可核实的调用上下文。账户身份仍由官方 CLI 处理，不读 auth 来证明身份。
    let metadata = std::fs::metadata(&exe).map_err(|_| error("无法读取官方 Codex CLI 文件信息"))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_nanos());
    let fingerprint = format!(
        "official-isolated-v1\n{}\n{}\n{}\n{}\n{modified:?}",
        request.target.id,
        home.display(),
        exe.display(),
        metadata.len()
    );
    registry::bind_source(&request.run_id, fingerprint.clone())?;
    result.source_stamp = Some(registry::source_stamp(&fingerprint));
    let runtime = Runtime::new()?;
    let mut lease = GateLease::open(gate, request, start_watchdog).await?;
    let outcome = async {
        tokio::select! {
            biased;
            _ = lease.revoked() => { registry::revoke(&request.run_id); return Err(error("门禁已收回检测租约，整个批次已取消")); },
            checked = supports_isolation(&exe, &runtime.0, &home) => checked?,
        }
        let job = KillOnCloseJob::new()?;
        let mut child = command(&exe, &runtime.0, &home)
            .args(args(&runtime.0, request))
            .spawn()
            .map_err(|_| error("无法启动官方 Codex 检测，请检查 CLI 安装和门禁"))?;
        job.assign(&child)?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| error("无法提供检测提示词"))?;
        // Job 在提示词写入之前建立；工具事件、错误、取消和超时均结束整棵专属进程树。
        stdin
            .write_all(request.prompt.as_bytes())
            .await
            .map_err(|_| error("Codex CLI 未接受检测提示词"))?;
        stdin
            .shutdown()
            .await
            .map_err(|_| error("Codex CLI 检测输入未完整提交"))?;
        drop(stdin);
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| error("Codex 检测没有输出通道"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| error("Codex 检测没有错误通道"))?;
        let complete = async {
            let (_, _, status) = tokio::try_join!(
                read_events(stdout, result, started),
                bounded(stderr, 128 * 1024),
                async {
                    child
                        .wait()
                        .await
                        .map_err(|_| error("等待 Codex 检测结束失败"))
                }
            )?;
            if !status.success() {
                return Err(error(
                    "Codex CLI 非正常退出；请选择该账户支持的模型和推理强度后重试",
                ));
            }
            Ok(())
        };
        tokio::select! {
            biased;
            _ = lease.revoked() => { registry::revoke(&request.run_id); Err(error("门禁已收回检测租约，已结束专属 Codex 进程树并取消整个批次")) },
            outcome = complete => outcome,
        }
    }
    .await;
    let closed = lease.close().await;
    outcome.and(closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_exec_arguments_are_isolated_readonly_and_keep_requested_effort() {
        let request = QualityCheckRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target: QualityTarget {
                kind: QualityTargetKind::CodexAccount,
                id: "slot".into(),
            },
            model: "gpt-test".into(),
            effort: "ultra".into(),
            prompt: "p".into(),
            max_output_tokens: 100,
            timeout_secs: 30,
        };
        let args = args(Path::new("isolated-runtime"), &request);
        for required in [
            "exec",
            "--json",
            "read-only",
            "--ignore-user-config",
            "--ephemeral",
            "features.shell_tool=false",
            "web_search=\"disabled\"",
            "model_reasoning_effort=\"ultra\"",
            "model_provider=\"openai\"",
        ] {
            assert!(args.iter().any(|a| a == required));
        }
        assert_eq!(args.last().unwrap(), "-");
        assert!(!args
            .iter()
            .any(|a| a.contains("dangerously") || a.contains("model_max_output")));
    }
    #[tokio::test]
    async fn mocked_pipes_are_bounded_and_require_full_cli_completion() {
        let overflow = vec![b'x'; 20];
        assert!(bounded(overflow.as_slice(), 10).await.is_err());
        let request = QualityCheckRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target: QualityTarget {
                kind: QualityTargetKind::CodexAccount,
                id: "slot".into(),
            },
            model: "gpt-test".into(),
            effort: "high".into(),
            prompt: "p".into(),
            max_output_tokens: 100,
            timeout_secs: 30,
        };
        let mut result = QualityCheckResult::blank(&request);
        let partial = b"{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"partial\"}}\n";
        assert!(read_events(partial.as_slice(), &mut result, Instant::now())
            .await
            .is_err());
        assert_eq!(result.text, "partial");
        assert_eq!(result.input_tokens, None);
    }
}
