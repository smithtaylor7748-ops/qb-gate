//! 版本库与一键回滚。
//!
//! # 在这之前是什么样
//!
//! `managed::place` 把旧的那份改名成 `claude.exe.old.<时间戳>` 留在原地，
//! 下次安装时 `clean_old_backups` 一把删掉。于是：
//!
//! - 只留得住**一个**旧版本；
//! - 那个名字里没有版本号，你不知道留下来的是哪一版；
//! - 装第二次，上一版就永远没了 —— 新版本有问题时退不回去。
//!
//! 升级出事是个真实场景（`upgrade.rs` 里那条「写死 stable 会降级」的拦截
//! 就是撞出来的）。能往前走却退不回来，等于每次升级都是单程票。
//!
//! # 现在的样子
//!
//! ```text
//! <托管根>\claude-code\
//!   claude.exe                      ← 当前在用的
//!   versions\
//!     2.0.14\claude.exe + record.json
//!     2.0.11\claude.exe + record.json
//!     2.0.9\ claude.exe + record.json   ← 超过 KEEP 就从最老的开始删
//! ```
//!
//! 形态取自 cac（MIT）的版本管理；实现是自己写的（那边是 Shell）。
//!
//! # 两条不能破的
//!
//! 1. **版本库里的每一份都要上锁。** 它们是完整可执行的 `claude.exe`，
//!    漏一个就是现成的绕过入口（硬约束 1）。删之前也要先解锁，
//!    带着 Deny ACE 的文件删不掉。
//! 2. **`inventory.rs` 必须认识这个位置。** 「Claude 装在哪」全项目只有一张表，
//!    在别处再拼一份路径迟早对不上（硬约束 10）。

use std::path::{Path, PathBuf};
use ts_rs::TS;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::{GateError, Result};
use crate::install::managed::{App, Record};

/// 版本库里保留几份。再多了从最老的开始删。
///
/// 3 是拍脑袋但有道理的：够你退回「上一版」和「上上版」，
/// 而 Claude Code 一份 200 MB 上下，留十份就是 2 GB。
pub const KEEP: usize = 3;

pub fn versions_dir(app_dir: &Path) -> PathBuf {
    app_dir.join("versions")
}

pub fn version_dir(app_dir: &Path, version: &str) -> PathBuf {
    versions_dir(app_dir).join(version)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct VersionEntry {
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
    pub archived_at: String,
    /// 这一份现在就是在用的那个版本。
    pub is_current: bool,
    pub locked: bool,
}

/// 版本号能不能当目录名。
///
/// 拿版本号直接拼路径，不挡一下就是目录穿越 —— `..\..\` 或者带盘符的
/// 「版本号」会让回滚写到托管目录外面去。版本号来自官方清单，但**来源可信
/// 不等于可以不校验**：清单是网上下来的。
pub fn safe_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        && v != "."
        && v != ".."
}

/// 列版本库。最新的在前（按归档时间倒序，时间读不出来就按版本名倒序）。
pub fn history(app_dir: &Path, app: App, current: Option<&str>) -> Vec<VersionEntry> {
    let Ok(rd) = std::fs::read_dir(versions_dir(app_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<VersionEntry> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let version = e.file_name().to_string_lossy().to_string();
            if !safe_version(&version) {
                return None;
            }
            let exe = e.path().join(app.exe_name());
            if !exe.is_file() {
                return None;
            }
            let rec: Record = std::fs::read_to_string(e.path().join("record.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            Some(VersionEntry {
                is_current: current == Some(version.as_str()),
                locked: crate::acl::locked(&exe).unwrap_or(false),
                version,
                sha256: rec.sha256,
                archived_at: rec.installed_at,
                path: exe,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.archived_at
            .cmp(&a.archived_at)
            .then(b.version.cmp(&a.version))
    });
    out
}

/// 超出 `keep` 的那些（最老的在后，所以取尾巴）。纯函数，可单测。
///
/// **当前在用的那一版永远不删** —— 它被算进 keep 名额里没关系，
/// 但不能因为它排在末尾就被清掉。
pub fn pick_to_prune(entries: &[VersionEntry], keep: usize) -> Vec<&VersionEntry> {
    entries
        .iter()
        .filter(|e| !e.is_current)
        .skip(keep)
        .collect()
}

#[derive(Serialize, Deserialize, Clone)]
struct Unit {
    record: Record,
    files: BTreeMap<String, String>,
}

fn members(dir: &Path, app: App) -> Result<Vec<PathBuf>> {
    let mut out = vec![dir.join(app.exe_name())];
    if app == App::Codex {
        for name in [
            "codex-command-runner.exe",
            "codex-windows-sandbox-setup.exe",
        ] {
            let p = dir.join(name);
            if p.is_file() {
                out.push(p);
            }
        }
    }
    Ok(out)
}

fn checked_copy(src: &Path, dst: &Path, locked: bool) -> Result<String> {
    if locked {
        unlock(src)?;
    }
    let result = (|| -> Result<String> {
        std::fs::copy(src, dst)?;
        let want = crate::install::managed::sha256_file(src)
            .ok_or_else(|| GateError::Other("源文件哈希读取失败".into()))?;
        if crate::install::managed::sha256_file(dst).as_deref() != Some(want.as_str()) {
            return Err(GateError::Other("复制校验失败".into()));
        }
        Ok(want)
    })();
    if locked {
        lock(src)?;
    }
    result
}

/// Preserve the entire installation unit before replacing any member.
pub fn archive(app_dir: &Path, app: App, exe: &Path, rec: &Record) -> Result<PathBuf> {
    if !safe_version(&rec.version) {
        return Err(GateError::Other("当前版本记录不完整，无法归档".into()));
    }
    let dir = version_dir(app_dir, &rec.version);
    // Existing verified archives are immutable; repeated rollback must not relabel their files.
    if dir.join("unit.json").is_file() {
        let unit: Unit = serde_json::from_slice(&std::fs::read(dir.join("unit.json"))?)?;
        if unit.record.sha256 != rec.sha256 {
            return Err(GateError::Other(
                "同版本的归档校验值不同，请检查安装记录".into(),
            ));
        }
        for (name, expected) in &unit.files {
            if !allowed_member(app, name) {
                return Err(GateError::Other("归档文件名无效".into()));
            }
            if crate::install::managed::sha256_file(&dir.join(name)).as_deref()
                != Some(expected.as_str())
            {
                return Err(GateError::Other(format!("归档文件 {name} 已改变")));
            }
        }
        let current: BTreeMap<String, String> = members(app_dir, app)?
            .into_iter()
            .map(|p| {
                let hash = crate::install::managed::sha256_file(&p)
                    .ok_or_else(|| GateError::Other("当前安装文件不可读".into()))?;
                Ok((p.file_name().unwrap().to_string_lossy().to_string(), hash))
            })
            .collect::<Result<_>>()?;
        if current != unit.files {
            return Err(GateError::Other(
                "同版本安装的主程序或配套程序与归档不一致，请先检查文件".into(),
            ));
        }
        return Ok(dir.join(app.exe_name()));
    }
    std::fs::create_dir_all(&dir)?;
    let mut unit = Unit {
        record: rec.clone(),
        files: BTreeMap::new(),
    };
    let _protect = Protect(dir.join(app.exe_name()));
    let hash = checked_copy(exe, &dir.join(app.exe_name()), true)?;
    if !rec.sha256.is_empty() && hash != rec.sha256 {
        return Err(GateError::Other(
            "当前程序与安装记录不一致，已停止归档".into(),
        ));
    }
    unit.record.sha256 = hash.clone();
    unit.files.insert(app.exe_name().into(), hash);
    for p in members(app_dir, app)?.into_iter().skip(1) {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        unit.files
            .insert(name.clone(), checked_copy(&p, &dir.join(&name), false)?);
    }
    crate::config_io::replace(
        &dir.join("record.json"),
        Some(&serde_json::to_vec_pretty(&unit.record)?),
    )?;
    crate::config_io::replace(
        &dir.join("unit.json"),
        Some(&serde_json::to_vec_pretty(&unit)?),
    )?;
    lock(&dir.join(app.exe_name()))?;
    Ok(dir.join(app.exe_name()))
}

/// 删掉超额的旧版本。返回人话日志。
pub fn prune(app_dir: &Path, app: App, current: Option<&str>, keep: usize) -> Vec<String> {
    let all = history(app_dir, app, current);
    let mut notes = Vec::new();
    for e in pick_to_prune(&all, keep) {
        // 先解锁再删 —— 带着 Deny ACE 的文件删不掉，
        // 而删不掉又不报错的话，版本库会一直涨。
        let _ = unlock(&e.path);
        let dir = e.path.parent().unwrap_or(&e.path);
        match std::fs::remove_dir_all(dir) {
            Ok(()) => notes.push(format!("版本库：清掉旧版本 {}", e.version)),
            Err(err) => notes.push(format!("版本库：{} 删不掉（{err}），留到下次", e.version)),
        }
    }
    notes
}

/// 回滚到某一版。
///
/// 顺序是**先把当前这份收进版本库，再把目标版本换上来** —— 反过来的话
/// 中间那一刻当前版本已经没了，换不上去就两头空。
pub fn rollback(app_dir: &Path, app: App, to: &str, current: Option<&Record>) -> Result<String> {
    if !safe_version(to) {
        return Err(GateError::Other("版本号不合法".into()));
    }
    let source = version_dir(app_dir, to);
    let unit: Unit = if source.join("unit.json").is_file() {
        serde_json::from_slice(&std::fs::read(source.join("unit.json"))?)?
    } else {
        if app == App::Codex {
            return Err(GateError::Other(
                "此旧版 Codex 归档没有完整配套文件清单，不能可靠回滚；请重新安装目标版本".into(),
            ));
        }
        let record: Record = serde_json::from_slice(&std::fs::read(source.join("record.json"))?)?;
        Unit {
            files: BTreeMap::from([(app.exe_name().into(), record.sha256.clone())]),
            record,
        }
    };
    if unit.record.version != to || !unit.files.contains_key(app.exe_name()) {
        return Err(GateError::Other("版本库清单不匹配".into()));
    }
    for (name, hash) in &unit.files {
        if !matches!(
            name.as_str(),
            "claude.exe"
                | "codex.exe"
                | "codex-command-runner.exe"
                | "codex-windows-sandbox-setup.exe"
        ) {
            return Err(GateError::Other("版本库含不允许的文件名".into()));
        }
        let p = source.join(name);
        let main = name == app.exe_name();
        if main {
            unlock(&p)?;
        }
        let actual = crate::install::managed::sha256_file(&p);
        if main {
            lock(&p)?;
        }
        if hash.is_empty() || actual.as_deref() != Some(hash.as_str()) {
            return Err(GateError::Other(format!("归档文件 {name} 校验失败")));
        }
    }
    let current = current.ok_or_else(|| GateError::Other("没有当前安装记录，回滚已阻断".into()))?;
    if current.version == to {
        return Ok(format!("当前已经是 {to}"));
    }
    let files = unit
        .files
        .keys()
        .map(|name| (name.clone(), source.join(name)))
        .collect();
    deploy(app_dir, app, files, unit.record, Some(current))?;
    Ok(format!("已完整回滚到 {to}，安装记录已同步"))
}
fn allowed_member(app: App, name: &str) -> bool {
    name == app.exe_name()
        || (app == App::Codex
            && matches!(
                name,
                "codex-command-runner.exe" | "codex-windows-sandbox-setup.exe"
            ))
}
struct Protect(PathBuf);
impl Drop for Protect {
    fn drop(&mut self) {
        if self.0.is_file() {
            let _ = lock(&self.0);
        }
    }
}

/// Validated binaries are deployed as one recoverable unit, including installs.json.
pub fn deploy(
    app_dir: &Path,
    app: App,
    files: Vec<(String, PathBuf)>,
    record: Record,
    current: Option<&Record>,
) -> Result<()> {
    deploy_checked(app_dir, app, files, record, current, |_| Ok(()))
}
fn deploy_checked(
    app_dir: &Path,
    app: App,
    files: Vec<(String, PathBuf)>,
    record: Record,
    current: Option<&Record>,
    after_member: impl Fn(usize) -> Result<()>,
) -> Result<()> {
    if !files.iter().any(|(n, _)| n == app.exe_name())
        || files.iter().any(|(n, _)| !allowed_member(app, n))
    {
        return Err(GateError::Other("安装单元文件清单无效".into()));
    }
    let mut unique = std::collections::BTreeSet::new();
    if files.iter().any(|(n, _)| !unique.insert(n)) {
        return Err(GateError::Other("安装文件名重复".into()));
    }
    let target = app_dir.join(app.exe_name());
    if target.exists() {
        archive(
            app_dir,
            app,
            &target,
            current.ok_or_else(|| GateError::Other("现有程序缺少安装记录，已阻断覆盖".into()))?,
        )?;
    }
    std::fs::create_dir_all(app_dir)?;
    let stage = app_dir.join(format!(".rollback-{}", crate::config_io::id()));
    std::fs::create_dir(&stage)?;
    let _stage_protect = Protect(stage.join(app.exe_name()));
    let _backup_protect = Protect(stage.join(format!("{}.before", app.exe_name())));
    let _live_protect = Protect(target.clone());
    let mut hashes = BTreeMap::new();
    for (name, source) in &files {
        hashes.insert(
            name.clone(),
            checked_copy(source, &stage.join(name), name == app.exe_name())?,
        );
    }
    if hashes.get(app.exe_name()) != Some(&record.sha256) {
        return Err(GateError::Other("新程序与版本记录校验不一致".into()));
    }
    let mut names = files
        .iter()
        .map(|(n, _)| n.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for old in members(app_dir, app)? {
        if old.is_file() {
            names.insert(old.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    let originals: BTreeMap<String, Option<String>> = names
        .iter()
        .map(|n| {
            let path = app_dir.join(n);
            Ok((
                n.clone(),
                if path.is_file() {
                    Some(
                        crate::install::managed::sha256_file(&path)
                            .ok_or_else(|| GateError::Other("原安装文件无法校验".into()))?,
                    )
                } else {
                    None
                },
            ))
        })
        .collect::<Result<_>>()?;
    let journal = serde_json::json!({"status":"prepared","files":names,"originals":originals,"next":hashes,"record":current,"app":app});
    crate::config_io::replace(
        &stage.join("recovery.json"),
        Some(&serde_json::to_vec_pretty(&journal)?),
    )?;
    let result = (|| -> Result<()> {
        for (index, name) in names.iter().enumerate() {
            let dst = app_dir.join(name);
            if dst.exists() {
                if name == app.exe_name() {
                    unlock(&dst)?;
                }
                std::fs::rename(&dst, stage.join(format!("{name}.before")))?;
            }
            if hashes.contains_key(name) {
                if name == app.exe_name() {
                    unlock(&stage.join(name))?;
                }
                std::fs::rename(stage.join(name), &dst)?;
            }
            if let Some(expected) = hashes.get(name) {
                if crate::install::managed::sha256_file(&dst).as_deref() != Some(expected.as_str())
                {
                    return Err(GateError::Other("部署后校验失败".into()));
                }
            }
            after_member(index)?;
        }
        after_member(names.len())?;
        crate::install::managed::write_record(
            app_dir
                .parent()
                .ok_or_else(|| GateError::Other("安装根无效".into()))?,
            app,
            record,
        )?;
        lock(&target)?;
        let mut committed = journal.clone();
        committed["status"] = serde_json::json!("committed");
        crate::config_io::replace(
            &stage.join("recovery.json"),
            Some(&serde_json::to_vec_pretty(&committed)?),
        )
    })();
    if let Err(error) = result {
        return match recover_stage(app_dir, &stage) {
            Ok(()) => Err(GateError::Other(format!("部署失败，原安装已恢复：{error}"))),
            Err(recovery) => Err(GateError::Other(format!(
                "部署失败：{error}；自动恢复未完成：{recovery}"
            ))),
        };
    }
    if let Err(e) = cleanup_stage(&stage, app) {
        crate::audit::write(&format!("安装已提交，临时文件将在下次启动清理：{e}"));
    }
    Ok(())
}
fn cleanup_stage(stage: &Path, app: App) -> Result<()> {
    for path in [
        stage.join(app.exe_name()),
        stage.join(format!("{}.before", app.exe_name())),
    ] {
        unlock(&path)?;
    }
    std::fs::remove_dir_all(stage)?;
    Ok(())
}

fn recover_stage(app_dir: &Path, stage: &Path) -> Result<()> {
    let j: serde_json::Value =
        serde_json::from_slice(&std::fs::read(stage.join("recovery.json"))?)?;
    let app: App = serde_json::from_value(j["app"].clone())?;
    if j["status"] == "committed" {
        return cleanup_stage(stage, app);
    }
    for value in j["files"]
        .as_array()
        .ok_or_else(|| GateError::Other("恢复清单无效".into()))?
        .iter()
        .rev()
    {
        let name = value
            .as_str()
            .ok_or_else(|| GateError::Other("恢复文件名无效".into()))?;
        if !matches!(
            name,
            "claude.exe"
                | "codex.exe"
                | "codex-command-runner.exe"
                | "codex-windows-sandbox-setup.exe"
        ) {
            return Err(GateError::Other("恢复路径不合法".into()));
        }
        let before = stage.join(format!("{name}.before"));
        let dst = app_dir.join(name);
        if before.is_file() {
            if let Some(expected) = j["originals"][name].as_str() {
                if crate::install::managed::sha256_file(&before).as_deref() != Some(expected) {
                    return Err(GateError::Other("原安装备份校验失败".into()));
                }
            }
            if dst.is_file()
                && j["next"][name].as_str().is_some_and(|h| {
                    crate::install::managed::sha256_file(&dst).as_deref() != Some(h)
                })
            {
                return Err(GateError::Other(format!(
                    "{name} 在部署后被外部修改，保留恢复现场"
                )));
            }
            if dst.is_file() {
                if name == app.exe_name() {
                    unlock(&dst)?;
                }
                std::fs::remove_file(&dst)?;
            }
            if name == app.exe_name() {
                unlock(&before)?;
            }
            std::fs::rename(before, dst)?;
        } else if j["originals"].get(name).is_some_and(|v| v.is_null()) && dst.is_file() {
            if j["next"][name].as_str() != crate::install::managed::sha256_file(&dst).as_deref() {
                return Err(GateError::Other(
                    "新安装文件已有外部修改，保留恢复现场".into(),
                ));
            }
            if name == app.exe_name() {
                unlock(&dst)?;
            }
            std::fs::remove_file(dst)?;
        }
    }
    let root = app_dir
        .parent()
        .ok_or_else(|| GateError::Other("安装目录无效".into()))?;
    if j["record"].is_null() {
        crate::install::managed::remove_record(root, app)?;
    } else {
        crate::install::managed::write_record(
            root,
            app,
            serde_json::from_value(j["record"].clone())?,
        )?;
    }
    if app_dir.join(app.exe_name()).exists() {
        lock(&app_dir.join(app.exe_name()))?;
    }
    cleanup_stage(stage, app)?;
    Ok(())
}

pub fn recover_installations(root: &Path) -> Result<()> {
    for app in App::ALL {
        let dir = crate::install::managed::app_dir(root, app);
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let p = entry?.path();
            if p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".rollback-"))
                && p.join("recovery.json").is_file()
            {
                recover_stage(&dir, &p)?;
            }
        }
    }
    Ok(())
}
#[cfg(all(windows, not(test)))]
fn lock(p: &Path) -> Result<()> {
    let sid = crate::acl::current_user_sid()?;
    crate::acl::lock(p, &sid)
}
#[cfg(any(not(windows), test))]
fn lock(_p: &Path) -> Result<()> {
    Ok(())
}
#[cfg(all(windows, not(test)))]
fn unlock(p: &Path) -> Result<()> {
    if !p.exists() {
        return Ok(());
    }
    let sid = crate::acl::current_user_sid()?;
    crate::acl::unlock(p, &sid)
}
#[cfg(any(not(windows), test))]
fn unlock(_p: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_unit(root: &Path, version: &str, helper: bool) -> (Vec<(String, PathBuf)>, Record) {
        let dir = root.join(format!("source-{version}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("codex.exe"), version).unwrap();
        let mut files = vec![("codex.exe".into(), dir.join("codex.exe"))];
        if helper {
            for name in [
                "codex-command-runner.exe",
                "codex-windows-sandbox-setup.exe",
            ] {
                std::fs::write(dir.join(name), format!("{name}-{version}")).unwrap();
                files.push((name.into(), dir.join(name)));
            }
        }
        let record = Record {
            version: version.into(),
            sha256: crate::install::managed::sha256_file(&dir.join("codex.exe")).unwrap(),
            source: "fixture".into(),
            installed_at: version.into(),
        };
        (files, record)
    }
    #[test]
    fn consecutive_rollbacks_restore_helpers_and_version_record() {
        let root = std::env::temp_dir().join(format!("qb-version-{}", crate::config_io::id()));
        let app_dir = root.join("codex");
        let (v1, r1) = fixture_unit(&root, "1", true);
        deploy(&app_dir, App::Codex, v1, r1.clone(), None).unwrap();
        let (v2, r2) = fixture_unit(&root, "2", true);
        deploy(&app_dir, App::Codex, v2, r2.clone(), Some(&r1)).unwrap();
        rollback(&app_dir, App::Codex, "1", Some(&r2)).unwrap();
        assert_eq!(
            crate::install::managed::record_of(&root, App::Codex)
                .unwrap()
                .version,
            "1"
        );
        assert_eq!(
            std::fs::read_to_string(app_dir.join("codex-command-runner.exe")).unwrap(),
            "codex-command-runner.exe-1"
        );
        rollback(&app_dir, App::Codex, "2", Some(&r1)).unwrap();
        assert_eq!(
            crate::install::managed::record_of(&root, App::Codex)
                .unwrap()
                .version,
            "2"
        );
        assert_eq!(
            std::fs::read_to_string(app_dir.join("codex-windows-sandbox-setup.exe")).unwrap(),
            "codex-windows-sandbox-setup.exe-2"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_unit_deployment_removes_new_helpers_and_restores_original() {
        let root = std::env::temp_dir().join(format!("qb-version-{}", crate::config_io::id()));
        let app_dir = root.join("codex");
        let (v1, r1) = fixture_unit(&root, "1", false);
        deploy(&app_dir, App::Codex, v1, r1.clone(), None).unwrap();
        let (v2, r2) = fixture_unit(&root, "2", true);
        let result = deploy_checked(&app_dir, App::Codex, v2, r2, Some(&r1), |i| {
            if i == 2 {
                Err(GateError::Other("injected interruption".into()))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(app_dir.join("codex.exe")).unwrap(),
            "1"
        );
        assert!(!app_dir.join("codex-command-runner.exe").exists());
        assert!(!app_dir.join("codex-windows-sandbox-setup.exe").exists());
        assert_eq!(
            crate::install::managed::record_of(&root, App::Codex)
                .unwrap()
                .version,
            "1"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    fn entry(v: &str, at: &str, current: bool) -> VersionEntry {
        VersionEntry {
            version: v.into(),
            path: PathBuf::from(format!("C:\\x\\versions\\{v}\\claude.exe")),
            sha256: String::new(),
            archived_at: at.into(),
            is_current: current,
            locked: true,
        }
    }

    #[test]
    fn version_names_that_would_escape_the_directory_are_rejected() {
        // 版本号来自网上下来的清单。来源可信 ≠ 可以不校验。
        for bad in [
            "..",
            ".",
            "",
            "../../windows",
            "C:\\evil",
            "a/b",
            "a\\b",
            "x y",
        ] {
            assert!(!safe_version(bad), "{bad:?} 不该通过");
        }
        for ok in ["2.0.14", "0.28.0", "1.0.0-rc.1", "v2_1"] {
            assert!(safe_version(ok), "{ok:?} 该通过");
        }
    }

    #[test]
    fn prune_keeps_the_newest_n() {
        let all = vec![
            entry("2.0.14", "2026-09-12 09:00", false),
            entry("2.0.11", "2026-09-01 09:00", false),
            entry("2.0.9", "2026-08-20 09:00", false),
            entry("2.0.7", "2026-08-01 09:00", false),
        ];
        let doomed: Vec<&str> = pick_to_prune(&all, 3)
            .iter()
            .map(|e| e.version.as_str())
            .collect();
        assert_eq!(doomed, vec!["2.0.7"]);
    }

    /// 当前在用的那一版**永远不删**，哪怕它排在最后。
    ///
    /// 挨着删的话，回滚到一个老版本再装点别的，正在用的那份就没了 ——
    /// 而它就是 `claude.exe` 本体的备份来源。
    #[test]
    fn the_version_in_use_is_never_pruned() {
        let all = vec![
            entry("2.0.14", "2026-09-12 09:00", false),
            entry("2.0.11", "2026-09-01 09:00", false),
            entry("2.0.9", "2026-08-20 09:00", false),
            entry("2.0.7", "2026-08-01 09:00", true),
        ];
        let doomed = pick_to_prune(&all, 3);
        assert!(doomed.is_empty(), "在用的那版不该进删除名单");
    }

    #[test]
    fn nothing_to_prune_when_under_the_cap() {
        let all = vec![entry("2.0.14", "2026-09-12 09:00", false)];
        assert!(pick_to_prune(&all, KEEP).is_empty());
    }

    #[test]
    fn history_of_a_missing_dir_is_empty_not_an_error() {
        // 没装过、或者刚换了托管目录 —— 这是正常情况，不是错误。
        let out = history(
            Path::new("C:\\definitely\\not\\here"),
            App::ClaudeCode,
            None,
        );
        assert!(out.is_empty());
    }
}
