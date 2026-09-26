//! Durable account pointer changes. Credential directories are moved or linked, never copied.
use super::*;
use crate::config_io;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Step {
    link: PathBuf,
    target: PathBuf,
    backup: PathBuf,
    had_original: bool,
    original_target: Option<PathBuf>,
    preserve_backup: bool,
}
#[derive(Serialize, Deserialize)]
struct PointerFile {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    status: String,
    steps: Vec<Step>,
    pointer: Option<PointerFile>,
}
fn metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn matches(link: &Path, target: &Path) -> bool {
    junction_target(link).is_some_and(|p| crate::install::inventory::same_path(&p, target))
}
fn step(link: PathBuf, target: PathBuf, adopt: Option<PathBuf>) -> Result<Option<Step>> {
    if matches(&link, &target) {
        return Ok(None);
    }
    let meta = metadata(&link)?;
    if meta
        .as_ref()
        .is_some_and(|m| !m.is_dir() && !is_reparse_point(m))
    {
        return Err(GateError::Other(format!(
            "账户指向 {} 是普通文件，未删除或覆盖",
            link.display()
        )));
    }
    let preserve_backup = meta.as_ref().is_some_and(|m| !is_reparse_point(m));
    let backup = adopt.unwrap_or_else(|| {
        link.with_file_name(format!(
            "{}-backup-{}",
            link.file_name().unwrap_or_default().to_string_lossy(),
            config_io::id()
        ))
    });
    if metadata(&backup)?.is_some() {
        return Err(GateError::Other("账户备份位置已存在".into()));
    }
    let original_target = if meta.as_ref().is_some_and(is_reparse_point) {
        Some(std::fs::read_link(&link)?)
    } else {
        None
    };
    Ok(Some(Step {
        link,
        target,
        backup,
        had_original: meta.is_some(),
        original_target,
        preserve_backup,
    }))
}
fn save(path: &Path, journal: &Journal) -> Result<()> {
    config_io::replace(path, Some(&serde_json::to_vec_pretty(journal)?))
}
fn cleanup(journal: &Journal) -> Result<()> {
    for s in &journal.steps {
        if !s.preserve_backup && metadata(&s.backup)?.is_some() {
            if !metadata(&s.backup)?.is_some_and(|m| is_reparse_point(&m)) {
                return Err(GateError::Other("账户旧指向备份类型改变，保留现场".into()));
            }
            std::fs::remove_dir(&s.backup)?;
        }
    }
    Ok(())
}
fn rollback(path: &Path, journal: &mut Journal) -> Result<()> {
    if let Some(p) = &journal.pointer {
        let now = config_io::read_optional(&p.path)?;
        if now != p.before && now.as_deref() != Some(&p.after) {
            return Err(GateError::Other("账户标记被外部修改，保留恢复记录".into()));
        }
        config_io::replace(&p.path, p.before.as_deref())?;
    }
    for s in journal.steps.iter().rev() {
        if metadata(&s.backup)?.is_some() {
            if metadata(&s.link)?.is_some() {
                if !matches(&s.link, &s.target) {
                    return Err(GateError::Other(format!(
                        "{} 的指向被外部修改，未覆盖",
                        s.link.display()
                    )));
                }
                std::fs::remove_dir(&s.link)?;
            }
            std::fs::rename(&s.backup, &s.link)?;
        } else if !s.had_original {
            if metadata(&s.link)?.is_some() {
                if !matches(&s.link, &s.target) {
                    return Err(GateError::Other("新账户指向被外部修改".into()));
                }
                std::fs::remove_dir(&s.link)?;
            }
        } else if metadata(&s.link)?.is_none()
            || s.original_target
                .as_ref()
                .is_some_and(|p| !matches(&s.link, p))
        {
            return Err(GateError::Other(
                "原账户指向和备份均无法确认，保留恢复记录".into(),
            ));
        }
    }
    journal.status = "rolled-back".into();
    save(path, journal)
}
fn apply(path: &Path, journal: &mut Journal, after: impl Fn(usize) -> Result<()>) -> Result<()> {
    save(path, journal)?;
    let result = (|| -> Result<()> {
        for (index, s) in journal.steps.iter().enumerate() {
            if s.had_original {
                std::fs::rename(&s.link, &s.backup)?;
            }
            std::fs::create_dir_all(&s.target)?;
            if let Some(parent) = s.link.parent() {
                std::fs::create_dir_all(parent)?;
            }
            make_junction(&s.link, &s.target)?;
            if !matches(&s.link, &s.target) {
                return Err(GateError::Other("账户指向写入后核验失败".into()));
            }
            after(index)?;
        }
        if let Some(p) = &journal.pointer {
            if config_io::read_optional(&p.path)? != p.before {
                return Err(GateError::Other("账户标记在切换期间变化".into()));
            }
            config_io::replace(&p.path, Some(&p.after))?;
        }
        journal.status = "committed".into();
        save(path, journal)
    })();
    if let Err(error) = result {
        return match rollback(path, journal) {
            Ok(()) => Err(GateError::Other(format!(
                "账户切换失败，原指向已恢复：{error}"
            ))),
            Err(recovery) => Err(GateError::Other(format!(
                "账户切换失败：{error}；恢复未完成：{recovery}"
            ))),
        };
    }
    // A cleanup failure leaves only old link shells; the committed identity is authoritative.
    let _ = cleanup(journal);
    Ok(())
}
pub(super) fn switch(r: &AccountRoots, label: &str, mode: DesktopMode) -> Result<SwitchOutcome> {
    validate_existing_label(label)?;
    let target = r.slot_dir(label);
    if !target.is_dir() {
        return Err(GateError::NotFound(target.display().to_string()));
    }
    let mut journal = Journal {
        status: "prepared".into(),
        steps: vec![],
        pointer: None,
    };
    let mut out = SwitchOutcome::default();
    if let Some(s) = step(r.link(), target.clone(), None)? {
        journal.steps.push(s);
    }
    out.switched.push(format!("Claude Code → {label}"));
    if let Some(b) = &r.bridge {
        if let Some(s) = step(b.join(LINK), target.clone(), None)? {
            journal.steps.push(s);
        }
        let path = b.join("active-profile.txt");
        journal.pointer = Some(PointerFile {
            before: config_io::read_optional(&path)?,
            path,
            after: label.as_bytes().to_vec(),
        });
        out.switched.push(format!("酒馆桥接 → {label}"));
    }
    if let Some(d) = plan_desktop(r, label, mode, active_label(r).as_deref()) {
        if let Some(to) = &d.adopt_to {
            out.notes
                .push(format!("桌面端原来的资料目录已存为 {}", to.display()));
        }
        if d.create {
            out.notes.push(format!(
                "给桌面端新建了空白资料 {}，打开桌面端后需要重新登录",
                d.target.display()
            ));
        }
        if let Some(s) = step(d.link, d.target, d.adopt_to)? {
            journal.steps.push(s);
        }
        out.switched.push(format!("桌面端 → {label}"));
    }
    let path = r
        .panel
        .join("operations/accounts")
        .join(format!("{}.json", config_io::id()));
    apply(&path, &mut journal, |_| Ok(()))?;
    Ok(out)
}
/// 扫一遍账户切换的恢复记录。**单条失败只登记，不中断** —— 理由同
/// `config_io::recover`：一张结不清的条子不该让整个面板开不起来。
pub(super) fn recover(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let dir = root.join("operations/accounts");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut blocked = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if let Err(error) = recover_one(&path) {
            blocked.push((path, error.to_string()));
        }
    }
    Ok(blocked)
}

/// 清掉结清已久的账户切换记录。返回删掉的路径。
///
/// 只删两类：`rolled-back` 的，以及 `committed` 且 `cleanup` 已经无事可做的
/// （每个步骤的备份要么本来就该留着，要么已经清掉了）。还挂着待清理备份的
/// 一条都不碰 —— 那个目录还等着这条记录才知道该不该删。
pub(super) fn retain(root: &Path, cutoff: std::time::SystemTime) -> Result<Vec<String>> {
    let dir = root.join("operations/accounts");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if !std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .is_ok_and(|m| m < cutoff)
        {
            continue;
        }
        // 读不出来的留着：宁可占地方，也不删可能还需要的记录。
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(journal) = serde_json::from_slice::<Journal>(&bytes) else {
            continue;
        };
        let settled = match journal.status.as_str() {
            "rolled-back" => true,
            "committed" => journal
                .steps
                .iter()
                .all(|s| s.preserve_backup || metadata(&s.backup).is_ok_and(|m| m.is_none())),
            _ => false,
        };
        if settled {
            std::fs::remove_file(&path)?;
            removed.push(path.display().to_string());
        }
    }
    Ok(removed)
}

fn recover_one(path: &Path) -> Result<()> {
    let mut journal: Journal = serde_json::from_slice(&std::fs::read(path)?)?;
    match journal.status.as_str() {
        "prepared" => rollback(path, &mut journal),
        "committed" => cleanup(&journal),
        "rolled-back" => Ok(()),
        other => Err(GateError::Other(format!("未知账户恢复状态 {other}"))),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_multi_pointer_change_restores_both_originals() {
        let root = std::env::temp_dir().join(config_io::id());
        std::fs::create_dir_all(&root).unwrap();
        let old = root.join("old");
        let new = root.join("new");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&new).unwrap();
        let a = root.join("a");
        let b = root.join("b");
        make_junction(&a, &old).unwrap();
        make_junction(&b, &old).unwrap();
        let mut j = Journal {
            status: "prepared".into(),
            steps: vec![
                step(a.clone(), new.clone(), None).unwrap().unwrap(),
                step(b.clone(), new, None).unwrap().unwrap(),
            ],
            pointer: None,
        };
        assert!(apply(&root.join("journal.json"), &mut j, |n| if n == 0 {
            Err(GateError::Other("injected".into()))
        } else {
            Ok(())
        })
        .is_err());
        assert!(matches(&a, &old));
        assert!(matches(&b, &old));
        std::fs::remove_dir(&a).unwrap();
        std::fs::remove_dir(&b).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restart_restores_a_moved_real_directory_without_copying_credentials() {
        let root = std::env::temp_dir().join(config_io::id());
        std::fs::create_dir_all(&root).unwrap();
        let link = root.join("profile");
        let target = root.join("other");
        std::fs::create_dir(&link).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(link.join("fixture.txt"), "keep").unwrap();
        let j = Journal {
            status: "prepared".into(),
            steps: vec![step(link.clone(), target.clone(), None).unwrap().unwrap()],
            pointer: None,
        };
        let path = root.join("operations/accounts/test.json");
        save(&path, &j).unwrap();
        std::fs::rename(&link, &j.steps[0].backup).unwrap();
        make_junction(&link, &target).unwrap();
        recover(&root).unwrap();
        assert!(!metadata(&link)
            .unwrap()
            .is_some_and(|m| is_reparse_point(&m)));
        assert_eq!(
            std::fs::read_to_string(link.join("fixture.txt")).unwrap(),
            "keep"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
