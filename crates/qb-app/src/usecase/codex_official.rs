//! GPT 访问令牌到点了：**让官方 Codex CLI 自己换**（面板不碰刷新令牌）。
//!
//! 后台起一个 `codex app-server`（stdio，一行一条 JSON-RPC），`CODEX_HOME=<槽位>\home`：
//! 先 `initialize` / `initialized`，再发 `account/read {"refreshToken": true}` —— 官方注释说
//! managed 模式下它走自己正常的刷新流程并自己写回 `auth.json`；接着发 `account/rateLimits/read`，
//! 回来的额度当这一次的读数。30 秒超时；只关自己起的这个子进程（挂在 Job 里）。
//!
//! ⛔ 面板不读、不存、不打印任何令牌：判断「换没换」只比 `auth.json` 的修改时刻和长度。
//! 这个槽位的桌面端 / CLI 正在跑就不起（两个进程同时换新会把刷新令牌轮换掉一份）。
use super::login_health;
use crate::error::{GateError, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct OfficialRefreshOutcome {
    /// `auth.json` 变了 = 官方换新了。
    pub refreshed: bool,
    /// 给使用者看的一句话。
    pub detail: String,
    /// `account/rateLimits/read` 回来的额度（原样形状的子集，不含任何令牌）。没拿到就是 `None`。
    #[ts(type = "unknown")]
    pub rate_limits: Option<Value>,
}

/// 这个槽位有没有官方进程正用着（桌面端按 `--user-data-dir`，CLI 按命令行里的槽位目录）。
fn slot_in_use(slot_dir: &Path) -> Option<String> {
    let norm = |p: &str| {
        p.trim()
            .trim_matches('"')
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let want = norm(&slot_dir.join("desktop").display().to_string());
    if let Ok(d) = qb_install::install::codex_desktop::detect() {
        if d.processes
            .iter()
            .any(|p| p.profile.as_deref().is_some_and(|x| norm(x) == want))
        {
            return Some("这个槽位的 GPT 桌面端正开着".into());
        }
    }
    if crate::gpt_bridge::running() {
        if let Ok(Some((_, home, _))) = qb_accounts::codex::active(&qb_accounts::codex::root()) {
            if norm(&home.display().to_string())
                == norm(&slot_dir.join("home").display().to_string())
            {
                return Some("酒馆的 GPT 桥接正用着这个槽位".into());
            }
        }
    }
    None
}

pub async fn refresh(slot_id: &str) -> Result<OfficialRefreshOutcome> {
    let root = qb_accounts::codex::root();
    let dir = qb_accounts::codex::directory(&root, slot_id)?;
    let home = dir.join("home");
    let auth = home.join("auth.json");
    {
        let dir = dir.clone();
        if let Some(why) = tokio::task::spawn_blocking(move || slot_in_use(&dir))
            .await
            .map_err(|e| GateError::Other(e.to_string()))?
        {
            return Ok(OfficialRefreshOutcome {
                refreshed: false,
                detail: format!("{why}，它自己会换新，这次跳过。"),
                rate_limits: None,
            });
        }
    }
    let exe = crate::plugins::sillytavern::find_official_codex()?;
    if !exe
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Err(GateError::Other(
            "只找到 codex.cmd，没找到原生的 codex.exe —— 到「软件」页装托管的 Codex CLI 再试。"
                .into(),
        ));
    }
    let before = login_health::fingerprint(&auth);
    let env = crate::sessions::sanitized_environment(
        std::env::vars(),
        vec![("CODEX_HOME".into(), home.display().to_string())],
    );
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("app-server")
        .current_dir(&home)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let job = crate::process::KillOnCloseJob::new()?;
    let mut child = crate::process::hidden_tokio(cmd).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            GateError::Other(format!(
                "Windows 拒绝了启动 Codex CLI（{e}）—— 多半是执行锁还锁着，门禁没放行时起不了 codex。"
            ))
        } else {
            GateError::Other(format!("起不了 Codex CLI：{e}"))
        }
    })?;
    if let Err(e) = job.assign(&child) {
        let _ = child.kill().await;
        return Err(e);
    }
    let talk = talk(&mut child);
    let result = tokio::time::timeout(Duration::from_secs(30), talk).await;
    let _ = child.kill().await;
    drop(job);
    let after = login_health::fingerprint(&auth);
    let refreshed = after != before;
    let (detail, rate_limits) = match result {
        Err(_) => ("官方 Codex CLI 30 秒内没答完，已关掉它".to_string(), None),
        Ok(Err(e)) => (format!("官方 Codex CLI 没换成：{e}"), None),
        Ok(Ok(limits)) => (
            if refreshed {
                "官方 Codex CLI 已自己换新".to_string()
            } else {
                "官方 Codex CLI 答了，但 auth.json 没变（它认为不用换，或换新失败）".to_string()
            },
            limits,
        ),
    };
    if refreshed {
        login_health::forget(&login_health::codex_key(slot_id));
    }
    crate::audit::write(&format!(
        "让官方 Codex CLI 给 GPT 槽位 {slot_id} 换新：{}（不读不存令牌）",
        if refreshed { "换了" } else { "没换" }
    ));
    Ok(OfficialRefreshOutcome {
        refreshed,
        detail,
        rate_limits,
    })
}

/// 跟 app-server 说那几句。返回 `account/rateLimits/read` 的结果。
async fn talk(child: &mut tokio::process::Child) -> Result<Option<Value>> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| GateError::Other("拿不到 Codex CLI 的标准输入".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GateError::Other("拿不到 Codex CLI 的标准输出".into()))?;
    let mut lines = BufReader::new(stdout).lines();
    let send = |v: Value| {
        let mut s = v.to_string();
        s.push('\n');
        s
    };
    let version = env!("CARGO_PKG_VERSION");
    let init = json!({"method":"initialize","id":1,"params":{"clientInfo":{"name":"qb_gate","title":"QB Gate","version":version}}});
    stdin.write_all(send(init).as_bytes()).await?;
    wait_for(&mut lines, 1).await?;
    stdin
        .write_all(send(json!({"method":"initialized"})).as_bytes())
        .await?;
    stdin
        .write_all(
            send(json!({"method":"account/read","id":2,"params":{"refreshToken":true}})).as_bytes(),
        )
        .await?;
    let account = wait_for(&mut lines, 2).await?;
    if account.get("account").is_some_and(Value::is_null) {
        return Err(GateError::Other(
            "官方 Codex CLI 说这个槽位没有登录（要重新登录）".into(),
        ));
    }
    stdin
        .write_all(send(json!({"method":"account/rateLimits/read","id":3})).as_bytes())
        .await?;
    let limits = wait_for(&mut lines, 3).await.ok();
    Ok(limits.map(strip))
}

/// 读到 id 对上的那一条回复。错误回复转成人话（只取 message，截短）。
async fn wait_for(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: i64,
) -> Result<Value> {
    while let Some(line) = lines.next_line().await? {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if v.get("id").and_then(Value::as_i64) != Some(id) || v.get("method").is_some() {
            continue;
        }
        if let Some(e) = v.get("error") {
            let msg: String = e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("未知错误")
                .chars()
                .take(160)
                .collect();
            return Err(GateError::Other(msg));
        }
        return Ok(v.get("result").cloned().unwrap_or(Value::Null));
    }
    Err(GateError::Other("Codex CLI 提前退出了".into()))
}

/// 只留额度相关的那几样（防御：万一以后回复里多了别的，不往界面带）。
fn strip(v: Value) -> Value {
    let mut out = serde_json::Map::new();
    for k in [
        "rateLimits",
        "rateLimitsByLimitId",
        "rateLimitResetCredits",
        "accountId",
        "ordinaryUsageAllowed",
    ] {
        if let Some(x) = v.get(k) {
            out.insert(k.into(), x.clone());
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_quota_fields_survive() {
        let v = serde_json::json!({"rateLimits":{"primary":{"usedPercent":3}},"accessToken":"secret","accountId":"a"});
        let s = super::strip(v).to_string();
        assert!(s.contains("usedPercent") && s.contains("\"a\""));
        assert!(!s.contains("secret"));
    }
}
