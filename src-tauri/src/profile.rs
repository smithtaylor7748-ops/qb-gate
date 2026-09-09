//! 统一档案：把「一套用法」存成一个名字，一键整体切过去。
//!
//! # 它解决什么
//!
//! 现在换一套用法要点四个地方：账户页切槽位、中转站页给 Claude Code 选一条、
//! 再给 Codex 选一条、环境页调时区。四件事忘一件，症状是「怎么又不对了」。
//!
//! 档案把这四件事绑成一个名字。**这是本项目独有的东西** —— cc-switch 管
//! 供应商、Cockpit 管账号，没有哪个把「账号 + 供应商 + 时区」当成一个整体。
//!
//! # 政策边界
//!
//! 应用档案会切账户，所以**它必须是纯人工触发的**：没有定时器、没有
//! watchdog、没有任何自动调用点。这跟 `accounts::switch` 是同一条线
//! （README 合规边界第 2 条）。加自动调用点就变成了自动轮换账户。
//!
//! # 每一项都是可选的
//!
//! 档案里没填的部分**保持原样，不会被清空**。只想换中转站的人不该被迫
//! 也把账户绑进去 —— 那会让「切一下试试」变成一个有副作用的操作。

use serde::{Deserialize, Serialize};

use crate::error::{GateError, Result};
use crate::relay::RelayTarget;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub note: Option<String>,
    /// 账户槽位标签。`None` = 不动账户。
    #[serde(default)]
    pub account: Option<String>,
    /// 每个 target 要启用哪条中转站记录（存 provider id）。缺的不动。
    #[serde(default)]
    pub relays: std::collections::BTreeMap<String, String>,
    /// 系统时区（Windows 名）。`None` = 不动时区。
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub sort: u32,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileStore {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// 上一次应用的是哪一个。只是显示用 —— 档案不保证当前状态仍与它一致，
    /// 用户完全可能应用完档案又手动改了某一处。
    #[serde(default)]
    pub last_applied: Option<String>,
}

pub fn store_path() -> std::path::PathBuf {
    crate::gate::state_dir().join("profiles.json")
}

pub fn load() -> ProfileStore {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(s: &ProfileStore) -> Result<()> {
    if let Some(d) = store_path().parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(store_path(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

pub fn upsert(mut p: Profile) -> Result<String> {
    let mut s = load();
    if p.id.trim().is_empty() {
        p.id = format!("profile-{:x}", chrono::Utc::now().timestamp_millis());
        p.created_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    }
    let id = p.id.clone();
    match s.profiles.iter_mut().find(|x| x.id == id) {
        Some(old) => {
            p.created_at = old.created_at.clone();
            *old = p;
        }
        None => {
            if p.sort == 0 {
                p.sort = s.profiles.iter().map(|x| x.sort).max().unwrap_or(0) + 1;
            }
            s.profiles.push(p);
        }
    }
    s.profiles.sort_by_key(|x| x.sort);
    save(&s)?;
    Ok(id)
}

pub fn remove(id: &str) -> Result<()> {
    let mut s = load();
    s.profiles.retain(|p| p.id != id);
    if s.last_applied.as_deref() == Some(id) {
        s.last_applied = None;
    }
    save(&s)
}

/// 从当前状态生成一个档案，省得手填。
pub fn capture(name: &str) -> Profile {
    let mut relays = std::collections::BTreeMap::new();
    let store = crate::relay::store::load();
    for t in RelayTarget::ALL {
        if let Some(id) = store.active_id(t) {
            relays.insert(t.as_str().to_string(), id.to_string());
        }
    }
    Profile {
        id: String::new(),
        name: name.to_string(),
        note: None,
        account: crate::accounts::slots()
            .into_iter()
            .find(|s| s.active)
            .map(|s| s.label),
        relays,
        timezone: crate::sysenv::current_windows_tz().ok(),
        sort: 0,
        created_at: String::new(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub snapshot: Option<String>,
    pub detail: String,
}

/// 应用一个档案。
///
/// **只由界面上的点击触发。不要给它加任何自动调用点** —— 它会切账户，
/// 加了就变成自动轮换账户，直接踩政策线。
///
/// 每一步互相独立：某一步失败不影响别的步骤，全部结果一起报回去。
/// 一步失败就整体回滚听着更干净，但实际更糟 —— 用户会得到一个
/// 「什么都没变，也不知道哪一步不行」的结果。
pub fn apply(id: &str) -> Result<ApplyReport> {
    let s = load();
    let p = s
        .profiles
        .iter()
        .find(|x| x.id == id)
        .ok_or_else(|| GateError::Other(format!("找不到档案 {id}")))?
        .clone();

    // 动手之前先存一份。切账户 + 改中转站配置是好几处写入，
    // 出问题时用户得能整体退回去。
    let snapshot = crate::snapshot::create(&format!("应用档案「{}」前", p.name))
        .ok()
        .map(|e| e.id);

    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();

    // ---- 账户 ----
    match &p.account {
        Some(label) => match crate::accounts::switch(label) {
            Ok(()) => applied.push(format!("账户切到 {label}")),
            Err(e) => failed.push(format!("账户切到 {label} 失败：{e}")),
        },
        None => skipped.push("账户（档案没指定，保持原样）".into()),
    }

    // ---- 中转站 ----
    for t in RelayTarget::ALL {
        match p.relays.get(t.as_str()) {
            Some(pid) => match crate::relay::activate(t, pid) {
                Ok(()) => applied.push(format!("{} 中转站已切换", t.label())),
                Err(e) => failed.push(format!("{} 中转站切换失败：{e}", t.label())),
            },
            None => skipped.push(format!("{}（档案没指定，保持原样）", t.label())),
        }
    }

    // ---- 时区 ----
    // **不自动改时区。** 它要管理员权限会弹 UAC —— 在一个「切换档案」的
    // 动作里突然弹 UAC，用户会以为程序出了问题。只在不一致时提示。
    match (&p.timezone, crate::sysenv::current_windows_tz().ok()) {
        (Some(want), Some(now)) if want != &now => {
            skipped.push(format!(
                "时区（档案是 {want}，当前是 {now}；改时区要管理员权限，请到「环境与安装」页手动切换）"
            ));
        }
        (Some(_), _) => applied.push("时区已一致".into()),
        (None, _) => skipped.push("时区（档案没指定，保持原样）".into()),
    }

    let mut store = load();
    store.last_applied = Some(id.to_string());
    let _ = save(&store);

    let detail = if failed.is_empty() {
        format!("档案「{}」已应用：{} 项生效", p.name, applied.len())
    } else {
        format!(
            "档案「{}」应用完成，但有 {} 项失败：{}",
            p.name,
            failed.len(),
            failed.join("；")
        )
    };
    crate::gate::log::write(&detail);

    Ok(ApplyReport {
        applied,
        skipped,
        failed,
        snapshot,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> Profile {
        Profile {
            id: String::new(),
            name: name.into(),
            note: None,
            account: None,
            relays: Default::default(),
            timezone: None,
            sort: 0,
            created_at: String::new(),
        }
    }

    #[test]
    fn empty_fields_mean_leave_alone_not_clear() {
        // 只想换中转站的人不该被迫也把账户绑进去 ——
        // 那会让「切一下试试」变成一个有副作用的操作。
        let p = profile("只换中转站");
        assert!(p.account.is_none());
        assert!(p.timezone.is_none());
        assert!(p.relays.is_empty());
    }

    #[test]
    fn relay_keys_are_the_target_serde_names() {
        // relays 那张表按 RelayTarget::as_str() 做键，
        // 对不上就会静默跳过整个中转站切换。
        let mut p = profile("x");
        for t in RelayTarget::ALL {
            p.relays.insert(t.as_str().to_string(), "some-id".into());
        }
        for t in RelayTarget::ALL {
            assert!(p.relays.contains_key(t.as_str()), "{}", t.as_str());
        }
        assert_eq!(p.relays.len(), 3);
    }

    #[test]
    fn store_round_trips_and_tolerates_missing_fields() {
        let s: ProfileStore =
            serde_json::from_str(r#"{"profiles":[{"id":"a","name":"甲"}]}"#).unwrap();
        assert_eq!(s.profiles.len(), 1);
        assert!(s.profiles[0].account.is_none());
        assert!(s.last_applied.is_none());
    }

    #[test]
    fn broken_store_reads_as_empty_not_error() {
        let s: ProfileStore = serde_json::from_str("{ not json").unwrap_or_default();
        assert!(s.profiles.is_empty());
    }
}
