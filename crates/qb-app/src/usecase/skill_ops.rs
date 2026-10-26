// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! Skills 面板（中转站重构阶段 5，cc-switch 的 M2）：装一次，勾上哪些目标就复制到哪些 `skills\` 目录。
//!
//! 照 cc-switch：Skill 本体只放一份（面板自己的 `skills\<名字>`，cc-switch 的 SSOT），各客户端的 `skills\` 里是**复制**过去的
//! （cc-switch 默认先试符号链接；Windows 上建链接要开发者模式或管理员，复制最稳）。目标同 MCP 面板（`ext_place`）：
//! 中转环境 / 官方默认目录 / 官方槽位 / Grok Build / 共存型工具。
//!
//! 守住的几条：
//! - ⛔ **不覆盖不是面板装的同名 Skill**：目标里已经有同名目录、内容又对不上（而且不是面板之前放的）就拒绝；
//! - ⛔ **删除先进备份**（`skill-backups\`）：取消勾选、卸载、更新替换下来的那一份都挪进去，不直接删；卸载的能还原；
//! - 官方目录、各个工具都要使用者自己勾；目录不在（没装）就拒绝，不替它建；不顺着联结点 / 符号链接写；
//! - 来源只读元数据、只下文件，**不执行仓库里的任何脚本**（`qb_extensions::skills`）。

use super::ext_place::{is_link, Place};
use crate::domain::{ExtensionInstallation, ExtensionKind, IdentityKind};
use crate::error::{GateError, Result};
use crate::repository::Repository;
use crate::skills::{self, SkillCandidate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

const TABLE: &str = "skills";
const MIGRATED: &str = "skills-migrated-installations";

/// 装好的一个 Skill。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstalledSkill {
    /// 名字，也是各处的目录名。
    pub id: String,
    pub description: String,
    pub license: String,
    /// 从哪来（GitHub 目录地址 / 本地目录 / 从某个目标导入的那个目录）。
    pub source: String,
    pub revision: String,
    pub fingerprint: String,
    /// 面板那一份的文件哈希：目标里的内容跟它一样才算「是面板放的、没被改过」。
    pub hash: String,
    pub targets: Vec<String>,
    #[ts(type = "number")]
    pub installed_ms: i64,
    #[ts(type = "number")]
    pub updated_ms: i64,
}

/// 一个能放 Skill 的地方。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillTarget {
    pub id: String,
    pub label: String,
    /// 那个 `skills` 目录。
    pub path: String,
    pub official: bool,
    pub available: bool,
    /// 里面现在有哪些 Skill（有 `SKILL.md` 的子目录）。
    pub entries: Vec<String>,
}

/// 目标里有、面板没管着的 Skill（可以导进来）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnmanagedSkill {
    pub target: String,
    pub directory: String,
}

/// 卸载留下的备份（能还原）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillBackup {
    pub id: String,
    pub skill: String,
    #[ts(type = "number")]
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillsView {
    pub skills: Vec<InstalledSkill>,
    pub targets: Vec<SkillTarget>,
    pub unmanaged: Vec<UnmanagedSkill>,
    pub backups: Vec<SkillBackup>,
}

/// 有新一版的那几个。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillUpdate {
    pub id: String,
    pub revision: String,
}

// ------------------------------------------------------------------ 目标

#[derive(Debug, Clone)]
struct Target {
    id: String,
    label: String,
    dir: PathBuf,
    official: bool,
    available: bool,
}

fn targets(place: &Place) -> Vec<Target> {
    use crate::relay::coexist::CoexistTool;
    let mut out: Vec<Target> = place
        .client_slots()
        .into_iter()
        .map(|s| Target {
            dir: s.root.join("skills"),
            id: s.id,
            label: s.label,
            official: s.official,
            available: s.available,
        })
        .collect();
    let h = &place.home;
    let mut tool = |id: &str, label: &str, root: PathBuf| {
        out.push(Target {
            id: id.into(),
            label: label.into(),
            dir: root.join("skills"),
            official: false,
            available: root.is_dir(),
        })
    };
    tool("grok-build", "Grok Build", h.join(".grok"));
    tool(
        "opencode",
        "OpenCode",
        place.coexist_dir(CoexistTool::OpenCode),
    );
    tool(
        "minimax-code",
        "MiniMax Code",
        place.coexist_dir(CoexistTool::MiniMaxCode),
    );
    tool(
        "openclaw",
        "OpenClaw",
        place.coexist_dir(CoexistTool::OpenClaw),
    );
    tool("hermes", "Hermes", place.coexist_dir(CoexistTool::Hermes));
    tool("pi", "Pi", h.join(".pi").join("agent"));
    out
}

fn target(place: &Place, id: &str) -> Result<Target> {
    targets(place)
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| GateError::Other(format!("不认得这个目标：{id}")))
}

fn ssot(place: &Place) -> PathBuf {
    place.state.join("skills")
}

fn backups_dir(place: &Place) -> PathBuf {
    place.state.join("skill-backups")
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ------------------------------------------------------------------ 文件

/// 有 `SKILL.md` 的子目录名。
fn skill_dirs(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().join("SKILL.md").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    out.sort();
    out
}

/// 把一份文件写成 `dest` 目录：先写到旁边的临时目录，原来那份（有的话）挪进备份，再换上。
fn write_dir(
    place: &Place,
    dest: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    backup_note: &str,
) -> Result<()> {
    if is_link(dest) || dest.parent().is_some_and(is_link) {
        return Err(GateError::Other(format!(
            "{} 是联结点或符号链接，没往里写",
            dest.display()
        )));
    }
    let parent = dest
        .parent()
        .ok_or_else(|| GateError::Other("目标路径无效".into()))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".qb-gate-tmp-{}", crate::config_io::id()));
    for (rel, bytes) in files {
        let p = tmp.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, bytes)?;
    }
    if dest.exists() {
        move_to_backup(place, dest, backup_note)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// 挪进备份（不删）。回备份目录。
fn move_to_backup(place: &Place, path: &Path, note: &str) -> Result<PathBuf> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = backups_dir(place).join(format!("{}-{name}-{note}", now_ms()));
    std::fs::create_dir_all(dir.parent().expect("有上级"))?;
    std::fs::rename(path, &dir).or_else(|_| {
        // 跨盘挪不动就复制再删（中转环境和官方目录可能不在一个盘上）。
        let (files, _) = skills::read_dir(path)?;
        for (rel, bytes) in &files {
            let p = dir.join(rel);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(p, bytes)?;
        }
        std::fs::remove_dir_all(path).map_err(GateError::from)
    })?;
    Ok(dir)
}

// ------------------------------------------------------------------ 库

pub(crate) fn records(db: &Repository) -> Result<Vec<InstalledSkill>> {
    let mut out: Vec<InstalledSkill> = db.list(TABLE)?;
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn record(db: &Repository, id: &str) -> Result<InstalledSkill> {
    db.find(TABLE, id)?
        .ok_or_else(|| GateError::Other(format!("没有装叫 {id} 的 Skill")))
}

fn ssot_files(place: &Place, id: &str) -> Result<BTreeMap<String, Vec<u8>>> {
    Ok(skills::read_dir(&ssot(place).join(id))?.0)
}

/// 把面板那一份放进一个目标。目标里已经有同名的：内容一样就算放好了；是面板之前放的（`was_ours`）就换掉；别的拒绝。
fn place_into(place: &Place, t: &Target, s: &InstalledSkill, was_ours: bool) -> Result<()> {
    if !t.available {
        return Err(GateError::Other(format!(
            "{} 没找到（{}）：多半还没装，不替它建",
            t.label,
            t.dir.display()
        )));
    }
    let dest = t.dir.join(&s.id);
    if dest.exists() {
        let same = skills::read_dir(&dest)
            .map(|(_, h)| h == s.hash)
            .unwrap_or(false);
        if same {
            return Ok(());
        }
        if !was_ours {
            return Err(GateError::Other(format!(
                "{} 里已经有一个叫 {} 的 Skill（不是面板装的），没覆盖。要让面板管它，用「导入」",
                t.label, s.id
            )));
        }
    }
    write_dir(place, &dest, &ssot_files(place, &s.id)?, "replaced")
}

fn take_out(place: &Place, t: &Target, id: &str) -> Result<()> {
    let dest = t.dir.join(id);
    if dest.exists() && !is_link(&dest) {
        move_to_backup(place, &dest, "removed")?;
    }
    Ok(())
}

// ------------------------------------------------------------------ 编排

fn view(place: &Place, db: &Repository) -> Result<SkillsView> {
    let all = records(db)?;
    let ts = targets(place);
    let mut unmanaged = Vec::new();
    for t in ts.iter().filter(|t| t.available) {
        for d in skill_dirs(&t.dir) {
            let managed = all.iter().any(|s| s.id == d && s.targets.contains(&t.id));
            if !managed {
                unmanaged.push(UnmanagedSkill {
                    target: t.id.clone(),
                    directory: d,
                });
            }
        }
    }
    let mut backups: Vec<SkillBackup> = std::fs::read_dir(backups_dir(place))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let rest = name.strip_suffix("-uninstalled")?;
            let (at, skill) = rest.split_once('-')?;
            Some(SkillBackup {
                id: name.clone(),
                skill: skill.into(),
                at_ms: at.parse().ok()?,
            })
        })
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.at_ms));
    Ok(SkillsView {
        skills: all,
        targets: ts
            .into_iter()
            .map(|t| SkillTarget {
                entries: skill_dirs(&t.dir),
                id: t.id,
                label: t.label,
                path: t.dir.display().to_string(),
                official: t.official,
                available: t.available,
            })
            .collect(),
        unmanaged,
        backups,
    })
}

pub fn list(db: &Repository) -> Result<SkillsView> {
    let place = Place::real(db)?;
    migrate_installations(&place, db)?;
    view(&place, db)
}

/// 装来源里挑好的几个，勾上 `targets`。已经装过同名的不重复装（用「更新」）。
///
/// 先联网把文件都取下来，再开库写 —— 库连接不能跨 `.await` 拿着（命令要的 future 得能跨线程）。
pub async fn install(picked: Vec<SkillCandidate>, targets: Vec<String>) -> Result<SkillsView> {
    let mut fetched = Vec::new();
    for c in picked {
        let files = skills::fetch(&c).await?;
        fetched.push((c, files));
    }
    let db = Repository::open()?;
    let place = Place::real(&db)?;
    for (c, files) in fetched {
        install_at(&place, &db, &c, files, &targets)?;
    }
    crate::audit::write("Skills：装了新的 Skill 并复制到勾选的目标");
    view(&place, &db)
}

/// 一个 ZIP 里有哪些 Skill（只读）。
pub fn discover_zip(path: &str) -> Result<Vec<SkillCandidate>> {
    Ok(skills::discover_zip(Path::new(path))?
        .into_iter()
        .map(|(c, _)| c)
        .collect())
}

/// 从 ZIP 装挑好的那几个（cc-switch 的 `install_from_zip`）。
pub fn install_zip(
    db: &Repository,
    path: &str,
    names: Vec<String>,
    targets: Vec<String>,
) -> Result<SkillsView> {
    let place = Place::real(db)?;
    for (c, files) in skills::discover_zip(Path::new(path))? {
        if names.contains(&c.name) {
            install_at(&place, db, &c, files, &targets)?;
        }
    }
    crate::audit::write("Skills：从 ZIP 装了 Skill 并复制到勾选的目标");
    view(&place, db)
}

fn install_at(
    place: &Place,
    db: &Repository,
    c: &SkillCandidate,
    files: BTreeMap<String, Vec<u8>>,
    targets: &[String],
) -> Result<()> {
    if !skills::valid_name(&c.name) {
        return Err(GateError::Other(format!(
            "Skill 名字只能是小写字母、数字和连字符：{}",
            c.name
        )));
    }
    if db.find::<InstalledSkill>(TABLE, &c.name)?.is_some() {
        return Err(GateError::Other(format!(
            "{} 已经装了：要换新版用「检查更新」",
            c.name
        )));
    }
    write_dir(place, &ssot(place).join(&c.name), &files, "stale")?;
    let mut s = InstalledSkill {
        id: c.name.clone(),
        description: c.description.clone(),
        license: c.license.clone(),
        source: c.source.clone(),
        revision: c.revision.clone(),
        fingerprint: c.fingerprint.clone(),
        hash: skills::hash(&files),
        targets: Vec::new(),
        installed_ms: now_ms(),
        updated_ms: now_ms(),
    };
    db.put(TABLE, &s.id, &s)?;
    for tid in targets {
        let t = target(place, tid)?;
        place_into(place, &t, &s, false)?;
        s.targets.push(tid.clone());
        db.put(TABLE, &s.id, &s)?;
    }
    Ok(())
}

/// 勾上 / 取消一个目标。取消时目标里那一份挪进备份。
pub fn set_target(db: &Repository, id: &str, target: &str, on: bool) -> Result<SkillsView> {
    let place = Place::real(db)?;
    set_target_at(&place, db, id, target, on)?;
    view(&place, db)
}

pub(crate) fn set_target_at(
    place: &Place,
    db: &Repository,
    id: &str,
    tid: &str,
    on: bool,
) -> Result<()> {
    let mut s = record(db, id)?;
    let t = target(place, tid)?;
    let had = s.targets.iter().any(|x| x == tid);
    if on {
        place_into(place, &t, &s, had)?;
        if !had {
            s.targets.push(tid.into());
            s.targets.sort();
        }
    } else {
        if had {
            take_out(place, &t, id)?;
        }
        s.targets.retain(|x| x != tid);
    }
    db.put(TABLE, id, &s)
}

/// 卸载：各个目标里那一份和面板那一份都挪进备份，删库里那一行。卸载的那一份能还原（[`restore`]）。
pub fn uninstall(db: &Repository, id: &str) -> Result<SkillsView> {
    let place = Place::real(db)?;
    uninstall_at(&place, db, id)?;
    crate::audit::write(&format!("Skills：卸载了 {id}（文件挪进了 skill-backups）"));
    view(&place, db)
}

fn uninstall_at(place: &Place, db: &Repository, id: &str) -> Result<()> {
    let s = record(db, id)?;
    for tid in &s.targets {
        if let Ok(t) = target(place, tid) {
            take_out(place, &t, id)?;
        }
    }
    let home = ssot(place).join(id);
    if home.exists() {
        let dir = move_to_backup(place, &home, "uninstalled")?;
        std::fs::write(
            dir.join(".qb-gate-skill.json"),
            serde_json::to_vec_pretty(&s)?,
        )?;
    }
    db.remove(TABLE, id)
}

/// 还原一次卸载：面板那一份放回去、库里那一行加回来（目标一个都不勾，使用者自己再勾）。
pub fn restore(db: &Repository, backup: &str) -> Result<SkillsView> {
    let place = Place::real(db)?;
    restore_at(&place, db, backup)?;
    view(&place, db)
}

fn restore_at(place: &Place, db: &Repository, backup: &str) -> Result<()> {
    if backup.contains(['/', '\\']) || backup.contains("..") {
        return Err(GateError::Other("备份名无效".into()));
    }
    let dir = backups_dir(place).join(backup);
    let mut s: InstalledSkill =
        serde_json::from_slice(&std::fs::read(dir.join(".qb-gate-skill.json"))?)?;
    if db.find::<InstalledSkill>(TABLE, &s.id)?.is_some() {
        return Err(GateError::Other(format!(
            "{} 现在又装着了：先卸载再还原",
            s.id
        )));
    }
    std::fs::remove_file(dir.join(".qb-gate-skill.json"))?;
    let dest = ssot(place).join(&s.id);
    if dest.exists() {
        move_to_backup(place, &dest, "stale")?;
    }
    std::fs::create_dir_all(ssot(place))?;
    std::fs::rename(&dir, &dest)?;
    s.targets.clear();
    s.updated_ms = now_ms();
    db.put(TABLE, &s.id, &s)
}

/// 把目标里一个面板没管着的 Skill 导进来：复制一份当面板那一份，并记成「这个目标勾着」。原来那份不动。
pub fn import(db: &Repository, target: &str, directory: &str) -> Result<SkillsView> {
    let place = Place::real(db)?;
    import_at(&place, db, target, directory)?;
    view(&place, db)
}

fn import_at(place: &Place, db: &Repository, tid: &str, directory: &str) -> Result<()> {
    let t = target(place, tid)?;
    if !skills::valid_name(directory) {
        return Err(GateError::Other(format!(
            "目录名不是合法的 Skill 名字（小写字母、数字、连字符）：{directory}"
        )));
    }
    let src = t.dir.join(directory);
    let (files, hash) = skills::read_dir(&src)?;
    let (_, description, license) = skills::metadata(&files)?;
    if let Some(mut s) = db.find::<InstalledSkill>(TABLE, directory)? {
        if s.hash != hash {
            return Err(GateError::Other(format!(
                "面板里已经有一个 {directory}，跟 {} 里那份内容不一样：没合并",
                t.label
            )));
        }
        if !s.targets.iter().any(|x| x == tid) {
            s.targets.push(tid.into());
            s.targets.sort();
            db.put(TABLE, directory, &s)?;
        }
        return Ok(());
    }
    write_dir(place, &ssot(place).join(directory), &files, "stale")?;
    let s = InstalledSkill {
        id: directory.into(),
        description,
        license: license.unwrap_or_else(|| "参见原目录".into()),
        source: src.display().to_string(),
        revision: hash.clone(),
        fingerprint: hash.clone(),
        hash,
        targets: vec![tid.into()],
        installed_ms: now_ms(),
        updated_ms: now_ms(),
    };
    db.put(TABLE, directory, &s)
}

fn candidate_of(s: &InstalledSkill) -> SkillCandidate {
    SkillCandidate {
        name: s.id.clone(),
        description: s.description.clone(),
        license: s.license.clone(),
        source: s.source.clone(),
        revision: s.revision.clone(),
        fingerprint: s.fingerprint.clone(),
    }
}

/// 看哪些有新的一版（只在点的时候联网）。
pub async fn check_updates() -> Result<Vec<SkillUpdate>> {
    let all = records(&Repository::open()?)?;
    let mut out = Vec::new();
    for s in all {
        if let Ok(Some(c)) = skills::check(&candidate_of(&s)).await {
            out.push(SkillUpdate {
                id: s.id,
                revision: c.revision,
            });
        }
    }
    Ok(out)
}

/// 换成新的一版：面板那一份换掉（旧的进备份），勾着的目标跟着换。
pub async fn update(id: String) -> Result<SkillsView> {
    let s = record(&Repository::open()?, &id)?;
    let c = skills::check(&candidate_of(&s))
        .await?
        .ok_or_else(|| GateError::Other(format!("{id} 已经是最新的")))?;
    let files = skills::fetch(&c).await?;
    let db = Repository::open()?;
    let place = Place::real(&db)?;
    update_at(&place, &db, record(&db, &id)?, &c, files)?;
    crate::audit::write(&format!("Skills：{id} 换成了新的一版"));
    view(&place, &db)
}

fn update_at(
    place: &Place,
    db: &Repository,
    mut s: InstalledSkill,
    c: &SkillCandidate,
    files: BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    if c.name != s.id {
        return Err(GateError::Other("新版的名字变了：卸载后重新装".into()));
    }
    write_dir(place, &ssot(place).join(&s.id), &files, "updated")?;
    s.hash = skills::hash(&files);
    s.revision = c.revision.clone();
    s.fingerprint = c.fingerprint.clone();
    s.description = c.description.clone();
    s.updated_ms = now_ms();
    db.put(TABLE, &s.id, &s)?;
    for tid in s.targets.clone() {
        if let Ok(t) = target(place, &tid) {
            place_into(place, &t, &s, true)?;
        }
    }
    Ok(())
}

/// 原来扩展中心装的 Skill（`installations` 里 Skill 那几行）迁成新的记录：从装着的那个目录复制一份当面板那一份，
/// 目标按装在哪推。只迁一次；老表留着。
fn migrate_installations(place: &Place, db: &Repository) -> Result<()> {
    if db.meta(MIGRATED)?.as_deref() == Some("1") {
        return Ok(());
    }
    let catalog = crate::extensions::catalog_in(db).unwrap_or_default();
    for i in db
        .list::<ExtensionInstallation>("installations")
        .unwrap_or_default()
    {
        let is_skill = catalog
            .iter()
            .any(|m| m.id == i.extension_id && m.kind == ExtensionKind::Skill)
            || i.extension_id.starts_with("skill-");
        if !is_skill || i.client != crate::domain::Client::ClaudeCode {
            continue;
        }
        let tid = match i.identity_kind {
            IdentityKind::Relay
                if i.environment_id == crate::workspace::router_environment_id(i.client) =>
            {
                "claude-code:relay".to_string()
            }
            IdentityKind::Relay => continue,
            _ if i.environment_id.is_empty() => "claude-code:default".to_string(),
            _ => format!("claude-code:slot:{}", i.environment_id),
        };
        let dir = PathBuf::from(&i.path);
        let Some(name) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        if !dir.join("SKILL.md").is_file()
            || !skills::valid_name(&name)
            || target(place, &tid).is_err()
        {
            continue;
        }
        if import_at(place, db, &tid, &name).is_err() {
            continue;
        }
        if let Some(m) = catalog.iter().find(|m| m.id == i.extension_id) {
            if let Some(mut s) = db.find::<InstalledSkill>(TABLE, &name)? {
                s.source = m.source.clone();
                db.put(TABLE, &name, &s)?;
            }
        }
    }
    db.set_meta(MIGRATED, "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str) -> (SkillCandidate, BTreeMap<String, Vec<u8>>) {
        let mut files = BTreeMap::new();
        files.insert(
            "SKILL.md".to_string(),
            format!("---\nname: {name}\ndescription: test\n---\nbody\n").into_bytes(),
        );
        files.insert("ref/notes.md".to_string(), b"notes".to_vec());
        let hash = skills::hash(&files);
        (
            SkillCandidate {
                name: name.into(),
                description: "test".into(),
                license: "MIT".into(),
                source: "https://github.com/o/r/tree/HEAD/skills/x".into(),
                revision: "rev1".into(),
                fingerprint: hash,
            },
            files,
        )
    }

    #[test]
    fn a_skill_is_copied_to_the_ticked_targets_and_removals_go_to_backups() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = Place::under(&root);
        let (c, files) = candidate("hello-skill");
        install_at(&p, &db, &c, files, &["claude-code:relay".into()]).unwrap();
        let placed = root.join("env-claude").join("skills").join("hello-skill");
        assert!(placed.join("ref").join("notes.md").is_file());
        assert!(
            p.state
                .join("skills")
                .join("hello-skill")
                .join("SKILL.md")
                .is_file(),
            "面板那一份"
        );

        // 官方默认目录没装（`~\.claude` 不在）：勾不上、不建。
        assert!(set_target_at(&p, &db, "hello-skill", "claude-code:default", true).is_err());
        assert!(!root.join("home").join(".claude").exists());

        // 取消：目标里那份进备份，不删。
        set_target_at(&p, &db, "hello-skill", "claude-code:relay", false).unwrap();
        assert!(!placed.exists());
        let backups: Vec<_> = std::fs::read_dir(p.state.join("skill-backups"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(backups.len(), 1);

        // 别人的同名 Skill：不覆盖。
        std::fs::create_dir_all(&placed).unwrap();
        std::fs::write(
            placed.join("SKILL.md"),
            "---\nname: hello-skill\ndescription: mine\n---\n",
        )
        .unwrap();
        let err = set_target_at(&p, &db, "hello-skill", "claude-code:relay", true).unwrap_err();
        assert!(err.to_string().contains("不是面板装的"), "{err}");
        let shown = view(&p, &db).unwrap();
        assert!(
            shown.unmanaged.iter().any(|u| u.directory == "hello-skill"),
            "可以导入"
        );

        // 卸载再还原。
        std::fs::remove_dir_all(&placed).unwrap();
        uninstall_at(&p, &db, "hello-skill").unwrap();
        assert!(records(&db).unwrap().is_empty());
        let b = view(&p, &db).unwrap().backups;
        assert_eq!(b.len(), 1);
        restore_at(&p, &db, &b[0].id).unwrap();
        let back = records(&db).unwrap();
        assert_eq!(back[0].id, "hello-skill");
        assert!(back[0].targets.is_empty(), "还原后目标要自己再勾");
        assert!(p
            .state
            .join("skills")
            .join("hello-skill")
            .join("SKILL.md")
            .is_file());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn importing_and_updating_follow_the_panel_copy() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = Place::under(&root);
        let mine = root.join("env-claude").join("skills").join("my-skill");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(
            mine.join("SKILL.md"),
            "---\nname: my-skill\ndescription: mine\n---\n",
        )
        .unwrap();
        import_at(&p, &db, "claude-code:relay", "my-skill").unwrap();
        let s = record(&db, "my-skill").unwrap();
        assert_eq!(s.targets, ["claude-code:relay"]);
        assert!(view(&p, &db).unwrap().unmanaged.is_empty());

        // 更新：面板那一份和勾着的目标一起换。
        let (mut c, mut files) = candidate("my-skill");
        files.insert("new.md".into(), b"v2".to_vec());
        c.revision = "rev2".into();
        update_at(&p, &db, s, &c, files).unwrap();
        assert!(mine.join("new.md").is_file(), "目标跟着换");
        assert_eq!(record(&db, "my-skill").unwrap().revision, "rev2");

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn old_extension_center_skill_installs_are_read_into_the_new_records() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = Place::under(&root);
        let old = root.join("slot-a").join("skills").join("skill-creator");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("SKILL.md"),
            "---\nname: skill-creator\ndescription: make skills\n---\n",
        )
        .unwrap();
        db.put(
            "installations",
            "i1",
            &ExtensionInstallation {
                id: "i1".into(),
                extension_id: "skill-creator".into(),
                environment_id: "a".into(),
                identity_kind: IdentityKind::Official,
                client: crate::domain::Client::ClaudeCode,
                version: "1".into(),
                state: "installed".into(),
                path: old.display().to_string(),
                content_hash: String::new(),
                installed_at: String::new(),
            },
        )
        .unwrap();
        migrate_installations(&p, &db).unwrap();
        let all = records(&db).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].targets, ["claude-code:slot:a"]);
        assert!(p
            .state
            .join("skills")
            .join("skill-creator")
            .join("SKILL.md")
            .is_file());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
