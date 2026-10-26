//! 本机路由此刻发布的聚合模型，记在库里给写配置那一步看（中转站重构阶段 4，补阶段 3 的那件）。
//!
//! 发布哪些模型是本机路由手里算的（名单 + 每条线的模型映射，`local_router::aggregate::published`），而写客户端配置的
//! `workspace::configuration_files` 只看得到库 —— 它在编排层下面，够不到路由状态。所以：路由设置存了、线路改了之后，
//! 编排层把算好的那份记进 `metadata`（[`store`]），变了才重写那个软件的中转环境配置；写配置时从这里读（[`listed`]）。
//! 启动客户端前重写配置时读到的也是这一份，不会跟路由答模型发现的那份对不上。
//!
//! Codex 的目录要带上 Codex 自带的那一份模型（[`codex_bundled`]，跑 `codex debug models --bundled`）。
//! 找不到 Codex 命令行就不写目录、记下原因（[`set_problem`]）—— 只有聚合的行没有自带的行，Codex 会只剩聚合模型可选。

use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::relay::client_models::ListedModel;
use crate::repository::Repository;
use qb_proxy::convert::plan::ApiFormat;
use qb_station::router::AggregateModel;
use qb_station::station::route::Route;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn key(client: Client) -> String {
    format!("aggregate-published:{client:?}")
}

fn problem_key(client: Client) -> String {
    format!("aggregate-config-problem:{client:?}")
}

/// 记下这一份。回 `true` = 跟原来的不一样（要重写那个软件的配置）。
pub fn store(db: &Repository, client: Client, models: &[AggregateModel]) -> Result<bool> {
    let text = serde_json::to_string(models)?;
    if db.meta(&key(client))?.as_deref().unwrap_or("[]") == text {
        return Ok(false);
    }
    db.set_meta(&key(client), &text)?;
    Ok(true)
}

/// 记着的那一份。没记过 / 读不出来 = 空（聚合关着）。
pub fn load(db: &Repository, client: Client) -> Vec<AggregateModel> {
    db.meta(&key(client))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 写配置时要列的那几个（Codex 那一档要知道每条线是不是原生 Responses）。
pub fn listed(db: &Repository, client: Client) -> Vec<ListedModel> {
    load(db, client)
        .into_iter()
        .map(|m| {
            let native_responses = db
                .get::<Route>("station_routes", &m.route_id)
                .is_ok_and(|r| crate::local_router::speaks(&r) == ApiFormat::OpenaiResponses);
            ListedModel {
                id: m.id,
                label: m.display_name,
                upstream: m.upstream,
                native_responses,
            }
        })
        .collect()
}

/// 写这个软件的配置时出的问题（写了 / 没问题就是 `None`）。界面在路由设置的聚合那一节显示。
pub fn set_problem(db: &Repository, client: Client, problem: Option<&str>) -> Result<()> {
    db.set_meta(&problem_key(client), problem.unwrap_or(""))
}

pub fn problem(db: &Repository, client: Client) -> Option<String> {
    db.meta(&problem_key(client))
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

/// PATH 上的 Codex 命令行（npm 装的是 `codex.cmd`）。
fn codex_cli() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| ["codex.exe", "codex.cmd", "codex"].map(|n| dir.join(n)))
        .find(|p| p.is_file())
}

/// Codex 自带的模型目录（`codex debug models --bundled`：跟账户无关、不联网，版本跟那个命令行一致）。
/// `home` 给它当 `CODEX_HOME`，免得它去读使用者 `~\.codex` 里的配置。20 秒没跑完就算失败。
pub fn codex_bundled(home: &Path) -> Result<Vec<Value>> {
    let cli = codex_cli().ok_or_else(|| {
        GateError::Other(
            "找不到 Codex 命令行（PATH 上没有 codex）：Codex 的模型目录没写，聚合 id 照样能手填"
                .into(),
        )
    })?;
    std::fs::create_dir_all(home)?;
    let mut cmd = std::process::Command::new(&cli);
    cmd.args(["debug", "models", "--bundled"])
        .env("CODEX_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = crate::process::hidden_std(cmd)
        .spawn()
        .map_err(|e| GateError::Other(format!("起不来 {}：{e}", cli.display())))?;
    let mut out = child.stdout.take().expect("上面接了管道");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut out, &mut buf).map(|_| buf)
    });
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            return Err(GateError::Other("codex debug models 20 秒没跑完".into()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let bytes = reader
        .join()
        .map_err(|_| GateError::Other("读 codex 的输出失败".into()))??;
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|e| GateError::Other(format!("codex debug models 的输出不是 JSON：{e}")))?;
    v.get("models")
        .and_then(Value::as_array)
        .filter(|m| !m.is_empty())
        .cloned()
        .ok_or_else(|| GateError::Other("codex debug models 的输出里没有模型".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_list_round_trips_and_reports_changes() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let m = AggregateModel {
            route_id: "r".into(),
            id: "qbg-claude-a--glm-5".into(),
            upstream: "glm-5".into(),
            display_name: "glm-5（站 · 组）".into(),
            one_m: false,
        };
        assert!(
            !store(&db, Client::ClaudeCode, &[]).unwrap(),
            "没记过等于空"
        );
        assert!(store(&db, Client::ClaudeCode, std::slice::from_ref(&m)).unwrap());
        assert!(
            !store(&db, Client::ClaudeCode, std::slice::from_ref(&m)).unwrap(),
            "一样就不算变"
        );
        assert_eq!(load(&db, Client::ClaudeCode), vec![m]);
        assert!(load(&db, Client::Codex).is_empty(), "按软件分开记");
        let l = listed(&db, Client::ClaudeCode);
        assert_eq!(l[0].label, "glm-5（站 · 组）");
        assert!(!l[0].native_responses, "线路不在库里就当不是原生 Responses");
        set_problem(&db, Client::Codex, Some("找不到")).unwrap();
        assert_eq!(problem(&db, Client::Codex).as_deref(), Some("找不到"));
        set_problem(&db, Client::Codex, None).unwrap();
        assert_eq!(problem(&db, Client::Codex), None);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
