//! VS Code 里的 Claude Code 插件走本机路由（中转站重构阶段 4，cc-switch 的 C5「VS Code 插件联动」）。
//!
//! cc-switch 的做法是往 `~/.claude/config.json` 写 `primaryApiKey = "any"`，让插件不弹登录、直接读
//! `~/.claude/settings.json` —— 它切换供应商时改的就是那一份。QB Gate 不碰 `~/.claude`（那是官方默认目录），
//! 中转环境在自己的目录里，所以换成插件自己的两个设置（官方文档 code.claude.com/docs/en/vs-code 的「Extension settings」）：
//!
//! - `claudeCode.environmentVariables` 里加两条：`CLAUDE_CONFIG_DIR` = 中转环境目录（插件只认绝对路径），
//!   `ANTHROPIC_AUTH_TOKEN` = 本机路由的**占位串**（中转环境的 `settings.json` 里只有地址，占位 Key 平时是启动时给的）；
//! - `claudeCode.disableLoginPrompt` = `true`（不弹登录）。
//!
//! ⛔ 真 Key 不进 VS Code 的设置：两条变量都不是 Key（仓库 `CLAUDE.md`「Key 不出后端」）。
//! 只加、只删这两条；使用者已经设了同名变量、值又不一样时**拒绝**，不覆盖。`settings.json` 是 JSONC，保留注释和格式。

use crate::error::{GateError, Result};
use crate::live::jsonc_document::JsoncDocument;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 认得的编辑器：`%APPDATA%` 下的目录名，和界面上的名字。都装的是同一个 Claude Code 插件。
pub const EDITORS: &[(&str, &str)] = &[
    ("Code", "VS Code"),
    ("Code - Insiders", "VS Code Insiders"),
    ("Cursor", "Cursor"),
    ("Windsurf", "Windsurf"),
    ("VSCodium", "VSCodium"),
];

const ENV_KEY: &str = "claudeCode.environmentVariables";
const PROMPT_KEY: &str = "claudeCode.disableLoginPrompt";

/// 这个编辑器的用户设置文件。
pub fn settings_path(appdata: &Path, editor_dir: &str) -> PathBuf {
    appdata.join(editor_dir).join("User").join("settings.json")
}

fn parse(existing: &str) -> Result<Value> {
    if existing.trim().is_empty() {
        return Ok(json!({}));
    }
    Ok(JsoncDocument::parse(existing)?.value().clone())
}

fn entries(doc: &Value) -> Result<Vec<Value>> {
    match doc.get(ENV_KEY) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => Ok(a.clone()),
        Some(_) => Err(GateError::Other(format!(
            "{ENV_KEY} 不是列表。为避免覆盖你的设置，本次没有写入任何文件"
        ))),
    }
}

fn name_of(e: &Value) -> Option<&str> {
    e.get("name").and_then(Value::as_str)
}

/// 这两条变量是不是都在、值也对得上。
pub fn linked(existing: &str, vars: &[(String, String)]) -> Result<bool> {
    let list = entries(&parse(existing)?)?;
    Ok(vars.iter().all(|(n, v)| {
        list.iter()
            .any(|e| name_of(e) == Some(n) && e.get("value").and_then(Value::as_str) == Some(v))
    }))
}

/// 加上这几条变量、关掉登录提示。返回新全文和 `disableLoginPrompt` 原来的值（撤销时还原用；`None` = 原来没有）。
pub fn link(existing: &str, vars: &[(String, String)]) -> Result<(String, Option<Value>)> {
    let doc = parse(existing)?;
    let mut list = entries(&doc)?;
    for (n, v) in vars {
        if let Some(other) = list
            .iter()
            .find(|e| name_of(e) == Some(n) && e.get("value").and_then(Value::as_str) != Some(v))
        {
            return Err(GateError::Other(format!(
                "这个编辑器的 {ENV_KEY} 里已经设了 {n} = {}。面板不覆盖你自己设的值，要联动请先把它删掉",
                other.get("value").unwrap_or(&Value::Null)
            )));
        }
    }
    for (n, v) in vars {
        if !list.iter().any(|e| name_of(e) == Some(n)) {
            list.push(json!({ "name": n, "value": v }));
        }
    }
    let previous = doc.get(PROMPT_KEY).cloned();
    let text = super::coexist::jsonc_set(existing, &[ENV_KEY], Some(Value::Array(list)))?;
    let text = super::coexist::jsonc_set(&text, &[PROMPT_KEY], Some(Value::Bool(true)))?;
    Ok((text, previous))
}

/// 撤销：只删值对得上的那几条；登录提示还是 `true`（没被使用者改过）才还原成 `previous`。
pub fn unlink(
    existing: &str,
    vars: &[(String, String)],
    previous: Option<Value>,
) -> Result<String> {
    let doc = parse(existing)?;
    let list = entries(&doc)?;
    let kept: Vec<Value> = list
        .iter()
        .filter(|e| {
            !vars.iter().any(|(n, v)| {
                name_of(e) == Some(n) && e.get("value").and_then(Value::as_str) == Some(v)
            })
        })
        .cloned()
        .collect();
    let mut text = existing.to_string();
    if kept.len() != list.len() {
        text = super::coexist::jsonc_set(
            &text,
            &[ENV_KEY],
            (!kept.is_empty()).then_some(Value::Array(kept)),
        )?;
    }
    if doc.get(PROMPT_KEY) == Some(&Value::Bool(true)) && previous != Some(Value::Bool(true)) {
        text = super::coexist::jsonc_set(&text, &[PROMPT_KEY], previous)?;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vec<(String, String)> {
        vec![
            ("CLAUDE_CONFIG_DIR".into(), r"C:\qb\env\claude".into()),
            ("ANTHROPIC_AUTH_TOKEN".into(), "qb-router".into()),
        ]
    }

    #[test]
    fn link_adds_two_variables_and_keeps_the_user_s_own_settings() {
        let existing = "{\n  // 我自己的\n  \"editor.fontSize\": 14,\n  \"claudeCode.environmentVariables\": [\n    { \"name\": \"HTTPS_PROXY\", \"value\": \"http://127.0.0.1:7890\" }\n  ],\n}\n";
        let (out, previous) = link(existing, &vars()).unwrap();
        assert_eq!(previous, None);
        assert!(out.contains("// 我自己的"), "注释丢了：{out}");
        let v: Value = json5::from_str(&out).unwrap();
        assert_eq!(v["editor.fontSize"], 14);
        let list = v[ENV_KEY].as_array().unwrap();
        assert_eq!(list.len(), 3, "{out}");
        assert_eq!(list[0]["name"], "HTTPS_PROXY", "使用者自己的那条要留着");
        assert_eq!(v[PROMPT_KEY], true);
        assert!(linked(&out, &vars()).unwrap());
        // 再点一次不重复加。
        let (again, _) = link(&out, &vars()).unwrap();
        let v: Value = json5::from_str(&again).unwrap();
        assert_eq!(v[ENV_KEY].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_different_value_the_user_set_is_refused_not_overwritten() {
        let existing = r#"{ "claudeCode.environmentVariables": [ { "name": "CLAUDE_CONFIG_DIR", "value": "D:\\mine" } ] }"#;
        let err = link(existing, &vars()).unwrap_err().to_string();
        assert!(err.contains("不覆盖"), "{err}");
        let existing = r#"{ "claudeCode.environmentVariables": { "x": 1 } }"#;
        assert!(link(existing, &vars()).is_err(), "不是列表就停");
    }

    #[test]
    fn unlink_removes_only_ours_and_restores_the_login_prompt() {
        let existing = "{\n  \"claudeCode.disableLoginPrompt\": false,\n  \"claudeCode.environmentVariables\": [\n    { \"name\": \"HTTPS_PROXY\", \"value\": \"x\" }\n  ]\n}\n";
        let (linked_text, previous) = link(existing, &vars()).unwrap();
        assert_eq!(previous, Some(Value::Bool(false)));
        let out = unlink(&linked_text, &vars(), previous).unwrap();
        let v: Value = json5::from_str(&out).unwrap();
        assert_eq!(v[ENV_KEY].as_array().unwrap().len(), 1);
        assert_eq!(v[PROMPT_KEY], false, "还原成原来的值");
        assert!(!linked(&out, &vars()).unwrap());

        // 原来没有这两个键：撤销后两个键都不留。
        let (linked_text, previous) = link("{}", &vars()).unwrap();
        let out = unlink(&linked_text, &vars(), previous).unwrap();
        let v: Value = json5::from_str(&out).unwrap();
        assert!(
            v.get(ENV_KEY).is_none() && v.get(PROMPT_KEY).is_none(),
            "{out}"
        );
    }
}
