//! Claude Code 的两个小件（中转站重构阶段 4，cc-switch 的 C5）：跳过首次确认、VS Code 插件联动。
//!
//! **跳过首次确认**：在 `.claude.json` 里写 `hasCompletedOnboarding = true`（cc-switch 的 `set_has_completed_onboarding`），
//! Claude Code 起来就不再走欢迎页那几步。默认只给中转环境（`qb-router-claude-code` 目录里那份）；
//! 官方默认目录和官方槽位要使用者**点名**才改。撤销只撤面板自己设的 —— 使用者真走完过一次首次确认、
//! Claude Code 自己记下的那个 `true` 不动（库里 `metadata` 记着哪些是面板设的）。
//!
//! **VS Code 插件联动**：编辑器设置里加两条变量把插件指到中转环境，见 `relay::vscode`。只写占位 Key。
//! 联动前先把中转环境的配置写好（插件读的是那个目录里的 `settings.json`）。
//!
//! 写盘都走 `config_io::commit`（写前核对没被外部改过、留原内容、失败回滚）。

use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::relay::vscode;
use crate::repository::Repository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 一份可以跳过首次确认的 `.claude.json`。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OnboardingTarget {
    /// `relay` / `default` / `slot:<标签>`。
    pub id: String,
    pub label: String,
    pub path: String,
    /// 官方那一侧（默认目录或官方槽位）。界面上要点名才改。
    pub official: bool,
    /// 现在跳不跳（`hasCompletedOnboarding` 是不是 `true`）。
    pub skipped: bool,
    /// 是面板设的（只有这种能在面板里撤销）。
    pub ours: bool,
    pub problem: Option<String>,
}

/// 一个编辑器里插件联动的样子。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditorLink {
    /// `%APPDATA%` 下的目录名（`Code`、`Cursor`……），也是命令里用的名字。
    pub editor: String,
    pub label: String,
    pub path: String,
    /// 这个编辑器的用户设置目录在不在。不在多半是没装。
    pub installed: bool,
    pub linked: bool,
    pub problem: Option<String>,
}

/// 在哪找这些文件、在哪记写盘日志。平时是真目录；单测换成临时目录（单测不碰真实状态）。
struct Place {
    home: PathBuf,
    appdata: PathBuf,
    /// 中转环境（Claude Code · 走本机路由）的目录。
    relay_dir: PathBuf,
    /// 官方槽位：(标签, `CLAUDE_CONFIG_DIR`)。
    slots: Vec<(String, PathBuf)>,
    /// `None` = `config_io::commit` 自己的日志目录。
    journal: Option<PathBuf>,
}

impl Place {
    fn real(db: &Repository) -> Result<Self> {
        let roots = crate::accounts::AccountRoots::current();
        Ok(Place {
            home: dirs::home_dir().ok_or_else(|| GateError::Other("找不到用户目录".into()))?,
            appdata: dirs::config_dir()
                .ok_or_else(|| GateError::Other("找不到 %APPDATA%".into()))?,
            relay_dir: crate::config_io::environment_dir(
                &db.root,
                &crate::workspace::router_environment_id(Client::ClaudeCode),
            )?,
            slots: crate::accounts::slots_in(&roots)
                .into_iter()
                .map(|s| {
                    let dir = roots.slot_dir(&s.label);
                    (s.label, dir)
                })
                .collect(),
            journal: None,
        })
    }

    fn targets(&self) -> Vec<(String, String, PathBuf, bool)> {
        let mut out = vec![
            (
                "relay".to_string(),
                "中转环境（Claude Code · 走本机路由）".to_string(),
                self.relay_dir.join(".claude.json"),
                false,
            ),
            (
                "default".to_string(),
                "官方 · 默认目录".to_string(),
                // 没设 `CLAUDE_CONFIG_DIR` 时它在使用者目录下，不在 `~\.claude` 里。
                self.home.join(".claude.json"),
                true,
            ),
        ];
        for (label, dir) in &self.slots {
            out.push((
                format!("slot:{label}"),
                format!("官方 · 槽位 {label}"),
                dir.join(".claude.json"),
                true,
            ));
        }
        out
    }

    fn commit(&self, path: PathBuf, expected: Option<Vec<u8>>, body: Vec<u8>) -> Result<()> {
        let edits = vec![crate::config_io::Edit {
            path,
            expected,
            body: Some(body),
        }];
        match &self.journal {
            Some(dir) => crate::config_io::commit_in(dir, edits),
            None => crate::config_io::commit(edits),
        }
        .map(drop)
    }
}

// ------------------------------------------------------------------ 跳过首次确认

const ONBOARDING_KEY: &str = "hasCompletedOnboarding";

fn onboarding_marker(path: &Path) -> String {
    format!("claude-onboarding-skip:{}", path.display())
}

fn ours(db: &Repository, path: &Path) -> bool {
    db.meta(&onboarding_marker(path))
        .ok()
        .flatten()
        .is_some_and(|v| v == "1")
}

fn read_json(
    path: &Path,
) -> Result<(
    Option<Vec<u8>>,
    Value,
    crate::relay_live::patch::json::JsonStyle,
)> {
    let bytes = crate::config_io::read_optional(path)?;
    let (doc, style) = crate::relay_live::patch::json::parse(path, bytes.as_deref())
        .map_err(|e| GateError::Other(e.to_string()))?;
    Ok((bytes, doc, style))
}

/// 中转环境和每个官方目录现在跳不跳首次确认。只读。
pub fn onboarding_status(db: &Repository) -> Result<Vec<OnboardingTarget>> {
    onboarding_status_at(&Place::real(db)?, db)
}

fn onboarding_status_at(place: &Place, db: &Repository) -> Result<Vec<OnboardingTarget>> {
    Ok(place
        .targets()
        .into_iter()
        .map(|(id, label, path, official)| {
            let (skipped, problem) = match read_json(&path) {
                Ok((_, doc, _)) => (doc.get(ONBOARDING_KEY) == Some(&Value::Bool(true)), None),
                Err(e) => (false, Some(e.to_string())),
            };
            OnboardingTarget {
                ours: skipped && ours(db, &path),
                id,
                label,
                path: path.display().to_string(),
                official,
                skipped,
                problem,
            }
        })
        .collect())
}

/// 设 / 撤「跳过首次确认」。`target` 是 [`OnboardingTarget::id`] —— 官方那几份只有点名到才会改。
pub fn onboarding_set(db: &Repository, target: &str, skip: bool) -> Result<Vec<OnboardingTarget>> {
    let place = Place::real(db)?;
    onboarding_set_at(&place, db, target, skip)?;
    onboarding_status_at(&place, db)
}

fn onboarding_set_at(place: &Place, db: &Repository, target: &str, skip: bool) -> Result<()> {
    let (_, label, path, _) = place
        .targets()
        .into_iter()
        .find(|(id, ..)| id == target)
        .ok_or_else(|| GateError::Other(format!("不认得这个目标：{target}")))?;
    if path
        .parent()
        .is_none_or(|dir| target != "relay" && !dir.is_dir())
    {
        return Err(GateError::Other(format!("{label} 的目录不在")));
    }
    let _lock = crate::relay_live::engine::lock_app("Claude Code");
    let (old, mut doc, style) = read_json(&path)?;
    let now = doc.get(ONBOARDING_KEY) == Some(&Value::Bool(true));
    let obj = doc.as_object_mut().expect("parse 只放行对象");
    if skip {
        if now {
            return Ok(()); // 已经是 true：多半是 Claude Code 自己记的，不认领。
        }
        obj.insert(ONBOARDING_KEY.into(), Value::Bool(true));
    } else {
        if !now {
            return Ok(());
        }
        if !ours(db, &path) {
            return Err(GateError::Other(format!(
                "{label} 的首次确认是 Claude Code 自己记下的（走完过一次），不是面板设的，面板不动它"
            )));
        }
        obj.shift_remove(ONBOARDING_KEY);
    }
    let body = crate::relay_live::patch::json::serialize(&path, &doc, &style)
        .map_err(|e| GateError::Other(e.to_string()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    place.commit(path.clone(), old, body)?;
    db.set_meta(&onboarding_marker(&path), if skip { "1" } else { "" })?;
    crate::audit::write(&format!(
        "{}了 {label} 的 Claude Code 首次确认（{}）",
        if skip { "跳过" } else { "恢复" },
        path.display()
    ));
    Ok(())
}

// ------------------------------------------------------------------ VS Code 插件联动

fn link_vars(place: &Place) -> Vec<(String, String)> {
    vec![
        (
            "CLAUDE_CONFIG_DIR".into(),
            place.relay_dir.display().to_string(),
        ),
        // ⛔ 占位串，不是 Key（模块头）。
        (
            "ANTHROPIC_AUTH_TOKEN".into(),
            crate::local_router::ROUTER_KEY.into(),
        ),
    ]
}

fn link_marker(editor: &str) -> String {
    format!("vscode-claude-link:{editor}")
}

fn editor_label(editor: &str) -> Result<&'static str> {
    vscode::EDITORS
        .iter()
        .find(|(dir, _)| *dir == editor)
        .map(|(_, label)| *label)
        .ok_or_else(|| GateError::Other(format!("不认得这个编辑器：{editor}")))
}

fn read_text(path: &Path) -> Result<(Option<Vec<u8>>, String)> {
    let bytes = crate::config_io::read_optional(path)?;
    let text = String::from_utf8(bytes.clone().unwrap_or_default())
        .map_err(|_| GateError::Other(format!("{} 不是 UTF-8 文本", path.display())))?;
    Ok((bytes, text))
}

/// 每个认得的编辑器里插件联动了没有。只读。
pub fn vscode_status(db: &Repository) -> Result<Vec<EditorLink>> {
    vscode_status_at(&Place::real(db)?)
}

fn vscode_status_at(place: &Place) -> Result<Vec<EditorLink>> {
    let vars = link_vars(place);
    Ok(vscode::EDITORS
        .iter()
        .map(|(dir, label)| {
            let path = vscode::settings_path(&place.appdata, dir);
            let installed = path.parent().is_some_and(Path::is_dir);
            let (linked, problem) =
                match read_text(&path).and_then(|(_, t)| vscode::linked(&t, &vars)) {
                    Ok(l) => (l, None),
                    Err(e) => (false, Some(e.to_string())),
                };
            EditorLink {
                editor: dir.to_string(),
                label: label.to_string(),
                path: path.display().to_string(),
                installed,
                linked,
                problem,
            }
        })
        .collect())
}

/// 让这个编辑器里的 Claude Code 插件走中转环境。先把中转环境的配置写好。
pub fn vscode_link(db: &Repository, editor: &str) -> Result<Vec<EditorLink>> {
    let env = crate::workspace::router_environment(Client::ClaudeCode)?;
    crate::workspace::apply(&env.id)?;
    let place = Place::real(db)?;
    vscode_link_at(&place, db, editor)?;
    vscode_status_at(&place)
}

fn vscode_link_at(place: &Place, db: &Repository, editor: &str) -> Result<()> {
    let label = editor_label(editor)?;
    let path = vscode::settings_path(&place.appdata, editor);
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(GateError::Other(format!(
            "没找到 {label} 的用户设置目录（{}）：多半还没装",
            path.parent().unwrap_or(&path).display()
        )));
    }
    let (old, text) = read_text(&path)?;
    let vars = link_vars(place);
    if vscode::linked(&text, &vars)? {
        return Ok(());
    }
    let (next, previous) = vscode::link(&text, &vars)?;
    place.commit(path.clone(), old, next.into_bytes())?;
    db.set_meta(
        &link_marker(editor),
        &serde_json::to_string(&previous.unwrap_or(Value::Null))?,
    )?;
    crate::audit::write(&format!(
        "已让 {label} 里的 Claude Code 插件走本机路由（{}）",
        path.display()
    ));
    Ok(())
}

/// 撤销联动：只删面板加的那两条，登录提示还原。
pub fn vscode_unlink(db: &Repository, editor: &str) -> Result<Vec<EditorLink>> {
    let place = Place::real(db)?;
    vscode_unlink_at(&place, db, editor)?;
    vscode_status_at(&place)
}

fn vscode_unlink_at(place: &Place, db: &Repository, editor: &str) -> Result<()> {
    let label = editor_label(editor)?;
    let path = vscode::settings_path(&place.appdata, editor);
    let (old, text) = read_text(&path)?;
    // 记着的是联动前 `disableLoginPrompt` 的值；`null` = 原来没有。没记（老数据）就当原来没有。
    let previous = db
        .meta(&link_marker(editor))?
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|v| !v.is_null());
    let next = vscode::unlink(&text, &link_vars(place), previous)?;
    if next != text {
        place.commit(path.clone(), old, next.into_bytes())?;
        crate::audit::write(&format!(
            "已撤销 {label} 里 Claude Code 插件的联动（{}）",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(root: &Path) -> Place {
        Place {
            home: root.join("home"),
            appdata: root.join("appdata"),
            relay_dir: root.join("env"),
            slots: vec![("a".into(), root.join("slot-a"))],
            journal: Some(root.join("journal")),
        }
    }

    #[test]
    fn onboarding_skip_only_undoes_what_the_panel_set() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = place(&root);
        std::fs::create_dir_all(&p.home).unwrap();
        std::fs::create_dir_all(&p.slots[0].1).unwrap();
        // 官方默认那份：使用者自己走完过首次确认。
        std::fs::write(
            p.home.join(".claude.json"),
            "{\n  \"hasCompletedOnboarding\": true,\n  \"projects\": {}\n}\n",
        )
        .unwrap();

        // 中转环境那份还不存在：照样能设（就是给第一次启动用的）。
        onboarding_set_at(&p, &db, "relay", true).unwrap();
        let all = onboarding_status_at(&p, &db).unwrap();
        let relay = all.iter().find(|t| t.id == "relay").unwrap();
        assert!(relay.skipped && relay.ours && !relay.official, "{relay:?}");
        let default = all.iter().find(|t| t.id == "default").unwrap();
        assert!(default.skipped && !default.ours && default.official);
        let slot = all.iter().find(|t| t.id == "slot:a").unwrap();
        assert!(!slot.skipped, "官方槽位没点名就不改");

        // 使用者自己记下的那个 true，面板不撤。
        let err = onboarding_set_at(&p, &db, "default", false).unwrap_err();
        assert!(err.to_string().contains("不是面板设的"), "{err}");
        let text = std::fs::read_to_string(p.home.join(".claude.json")).unwrap();
        assert!(text.contains("\"projects\""), "别的字段要原样留着");

        // 面板设的能撤，撤完键也不留。
        onboarding_set_at(&p, &db, "relay", false).unwrap();
        let text = std::fs::read_to_string(p.relay_dir.join(".claude.json")).unwrap();
        assert!(!text.contains(ONBOARDING_KEY), "{text}");

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn vscode_link_needs_the_editor_and_unlink_restores_the_user_s_settings() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = place(&root);

        assert!(vscode_link_at(&p, &db, "Code").is_err(), "没装就不建目录");
        assert!(!p.appdata.join("Code").exists());

        let file = vscode::settings_path(&p.appdata, "Code");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let original = "{\n  // 我自己的\n  \"claudeCode.disableLoginPrompt\": false\n}\n";
        std::fs::write(&file, original).unwrap();
        vscode_link_at(&p, &db, "Code").unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(crate::local_router::ROUTER_KEY), "{text}");
        let status = vscode_status_at(&p).unwrap();
        let code = status.iter().find(|e| e.editor == "Code").unwrap();
        assert!(code.installed && code.linked, "{code:?}");
        assert!(
            !status
                .iter()
                .find(|e| e.editor == "Cursor")
                .unwrap()
                .installed
        );

        vscode_unlink_at(&p, &db, "Code").unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        let v: Value = json5::from_str(&text).unwrap();
        assert_eq!(
            v["claudeCode.disableLoginPrompt"], false,
            "还原成原来的值：{text}"
        );
        assert!(v.get("claudeCode.environmentVariables").is_none(), "{text}");
        assert!(text.contains("// 我自己的"));

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
