// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 聚合模型写进客户端配置（中转站重构阶段 4，补阶段 3 留下的那件）：让 `/model`、模型选择器里直接列出聚合的模型，
//! 不用再手填 id。三个软件各一种写法（照 cc-switch v4.0.14 的聚合模式）：
//!
//! | 软件 | 写在哪 | 认哪些是自己的 |
//! |---|---|---|
//! | Claude Code | `settings.json` 顶层 `modelPicker`（`replaceBuiltInOptions` + 每个聚合模型一行；要 Claude Code 2.1.243 以上） | `model` 以 `qbg-` 开头的行 |
//! | Claude 桌面端 | `claude_desktop_config.json` 的 `inferenceModels`（`name` + `labelOverride`） | `name` 里有 `-qbg-` 的条目 |
//! | Codex | 一份模型目录 JSON + `config.toml` 的 `model_catalog_json` 指过去 | 目录文件是 QB Gate 自己的；指针只认指向它的 |
//!
//! ⛔ **只增删自己的那几行**：使用者自己写的选择器行、模型条目、别处的模型目录一个不碰。聚合关掉（或名单空了）时把自己的删掉，
//! 删完空了才连那个键一起删。cc-switch 是「聚合模式下整份换掉、手写的不保留」；QB Gate 更保守。
//!
//! Codex 的目录**会整个替换 Codex 自己的模型列表**（设了 `model_catalog_json` Codex 就只认目录里的模型）。所以目录 =
//! Codex 自带的那一份（`codex debug models --bundled`，原样）+ 聚合的行接在后面：内置模型的提示词、工具、默认模型都不变。
//! 聚合的行：上游原生说 Responses、模型名在自带目录里有同名的，照抄那一行（cc-switch 的「GPT 行照官方」）；
//! 别的用一份中性模板（`codex_native_template.json`，cc-switch 的 `codex_native_responses_template.json`）——
//! 不带 freeform `apply_patch` 这类自定义工具（Anthropic / Chat 转换和很多原生网关都不收），改文件走 `shell_command`。

use crate::error::{GateError, Result};
use serde_json::{json, Map, Value};

/// 聚合模型 id 的前缀（Claude Code、Codex）。桌面端的是 `claude-fable-qbg-<数字>`。
pub const PREFIX: &str = "qbg-";
const DESKTOP_MARK: &str = "-qbg-";
/// QB Gate 写的 Codex 模型目录文件名（在中转环境目录里）。
pub const CODEX_CATALOG_FILE: &str = "qb-gate-model-catalog.json";
/// 聚合行的压缩兼容标记：不同家的行之间不共用 Codex 的加密压缩内容（cc-switch 的 `CODEX_STACK_COMP_HASH`）。
const COMP_HASH: &str = "qb-gate";

/// 要列出来的一个聚合模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedModel {
    /// 发布给客户端的 id（带前缀）。
    pub id: String,
    /// 选择器里的名字：`模型（线路名）`。
    pub label: String,
    /// 发往上游的模型名。
    pub upstream: String,
    /// （Codex）这条线的上游原生说 Responses —— 能照抄自带目录里的同名行。
    pub native_responses: bool,
}

fn ours_claude(row: &Value) -> bool {
    row.get("model")
        .and_then(Value::as_str)
        .is_some_and(|m| m.starts_with(PREFIX))
}

/// Claude Code 的 `settings.json`：顶层 `modelPicker` 里换上这几行（`models` 空 = 把自己的删掉）。
pub fn claude_model_picker(doc: &mut Value, models: &[ListedModel]) -> Result<()> {
    let root = doc
        .as_object_mut()
        .ok_or_else(|| GateError::Other("settings.json 顶层不是对象".into()))?;
    let picker = root.get("modelPicker").cloned();
    let mut kept: Vec<Value> = match &picker {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Object(p)) => match p.get("options") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(rows)) => rows.iter().filter(|r| !ours_claude(r)).cloned().collect(),
            Some(_) => return Err(GateError::Other(
                "settings.json 的 modelPicker.options 不是列表：为避免覆盖你的设置，没有写聚合模型"
                    .into(),
            )),
        },
        Some(_) => {
            return Err(GateError::Other(
                "settings.json 的 modelPicker 不是对象：为避免覆盖你的设置，没有写聚合模型".into(),
            ))
        }
    };
    kept.extend(
        models
            .iter()
            .map(|m| json!({ "model": m.id, "label": m.label, "description": m.upstream })),
    );
    match (picker, kept.is_empty()) {
        // 原来没有、这次也没东西：不建。
        (None | Some(Value::Null), true) => {}
        (None | Some(Value::Null), false) => {
            root.insert(
                "modelPicker".into(),
                json!({ "replaceBuiltInOptions": true, "options": kept }),
            );
        }
        (Some(Value::Object(mut p)), empty) => {
            let only_ours = p
                .keys()
                .all(|k| k == "options" || k == "replaceBuiltInOptions");
            if empty && only_ours {
                root.shift_remove("modelPicker");
            } else {
                p.insert("options".into(), Value::Array(kept));
                root.insert("modelPicker".into(), Value::Object(p));
            }
        }
        (Some(_), _) => unreachable!("上面已经拒绝了"),
    }
    Ok(())
}

fn ours_desktop(row: &Value) -> bool {
    let name = match row {
        Value::String(s) => Some(s.as_str()),
        Value::Object(o) => o.get("name").and_then(Value::as_str),
        _ => None,
    };
    name.is_some_and(|n| n.contains(DESKTOP_MARK))
}

/// 桌面端的 `claude_desktop_config.json`：`inferenceModels` 里换上这几条（`models` 空 = 把自己的删掉）。
pub fn desktop_inference_models(doc: &mut Value, models: &[ListedModel]) -> Result<()> {
    let root = doc
        .as_object_mut()
        .ok_or_else(|| GateError::Other("桌面端配置顶层不是对象".into()))?;
    let mut kept: Vec<Value> = match root.get("inferenceModels") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(rows)) => rows.iter().filter(|r| !ours_desktop(r)).cloned().collect(),
        Some(_) => {
            return Err(GateError::Other(
                "桌面端配置的 inferenceModels 不是列表：为避免覆盖你的设置，没有写聚合模型".into(),
            ))
        }
    };
    let had = root.contains_key("inferenceModels");
    kept.extend(
        models
            .iter()
            .map(|m| json!({ "name": m.id, "labelOverride": m.label })),
    );
    if kept.is_empty() {
        if had {
            root.shift_remove("inferenceModels");
        }
    } else {
        root.insert("inferenceModels".into(), Value::Array(kept));
    }
    Ok(())
}

fn native_template() -> Value {
    serde_json::from_str(include_str!("codex_native_template.json"))
        .expect("仓库里那份 Codex 模板必须是合法 JSON")
}

/// Codex 的模型目录：自带那一份原样在前，聚合的行接在后面（`priority` 排在所有自带行之后，不会变成默认模型）。
pub fn codex_catalog(bundled: &[Value], models: &[ListedModel]) -> Value {
    let mut rows: Vec<Value> = bundled.to_vec();
    let last = bundled
        .iter()
        .filter_map(|r| r.get("priority").and_then(Value::as_i64))
        .max()
        .unwrap_or(0);
    for (i, m) in models.iter().enumerate() {
        let official = m
            .native_responses
            .then(|| {
                bundled
                    .iter()
                    .find(|r| r.get("slug").and_then(Value::as_str) == Some(m.upstream.as_str()))
            })
            .flatten();
        let mut row = official.cloned().unwrap_or_else(native_template);
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
        obj.insert("slug".into(), json!(m.id));
        obj.insert("display_name".into(), json!(m.label));
        obj.insert("description".into(), json!(m.upstream));
        obj.insert("visibility".into(), json!("list"));
        obj.insert("priority".into(), json!(last + 1 + i as i64));
        obj.insert("comp_hash".into(), json!(COMP_HASH));
        // 第三方不支持 Responses Lite（cc-switch 同样关掉）。
        if obj.get("use_responses_lite") == Some(&Value::Bool(true)) {
            obj.insert("use_responses_lite".into(), Value::Bool(false));
        }
        rows.push(row);
    }
    let mut out = Map::new();
    out.insert("models".into(), Value::Array(rows));
    Value::Object(out)
}

/// `config.toml` 的 `model_catalog_json`：`Some(路径)` = 指向 QB Gate 的目录，`None` = 拿掉自己的指针。
/// 指着别的文件（使用者自己的目录）时不动，回 `Err` 说清楚。
pub fn codex_catalog_pointer(
    doc: &mut toml_edit::DocumentMut,
    ours: &str,
    point: bool,
) -> Result<()> {
    let current = doc
        .get("model_catalog_json")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let is_ours = |p: &str| {
        let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
        norm(p) == norm(ours)
            || norm(p).ends_with(&format!("/{}", norm(CODEX_CATALOG_FILE)))
            || norm(p) == norm(CODEX_CATALOG_FILE)
    };
    match (current.as_deref(), point) {
        (Some(p), _) if !is_ours(p) => Err(GateError::Other(format!(
            "Codex 的 config.toml 已经指着你自己的模型目录（{p}）：面板不改它，聚合模型没写进 Codex 的选择器"
        ))),
        (_, true) => {
            doc["model_catalog_json"] = toml_edit::value(ours);
            Ok(())
        }
        (Some(_), false) => {
            doc.remove("model_catalog_json");
            Ok(())
        }
        (None, false) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, upstream: &str, native: bool) -> ListedModel {
        ListedModel {
            id: id.into(),
            label: format!("{upstream}（站 · 组）"),
            upstream: upstream.into(),
            native_responses: native,
        }
    }

    #[test]
    fn the_claude_picker_keeps_the_user_s_rows_and_goes_away_when_empty() {
        let mut doc = json!({ "env": {} });
        claude_model_picker(&mut doc, &[m("qbg-claude-a--glm-5", "glm-5", false)]).unwrap();
        assert_eq!(doc["modelPicker"]["replaceBuiltInOptions"], true);
        assert_eq!(
            doc["modelPicker"]["options"][0]["model"],
            "qbg-claude-a--glm-5"
        );
        claude_model_picker(&mut doc, &[]).unwrap();
        assert!(
            doc.get("modelPicker").is_none(),
            "只有自己的行时整个删掉：{doc}"
        );

        let mut doc = json!({ "modelPicker": { "options": [{ "model": "mine" }], "x": 1 } });
        claude_model_picker(&mut doc, &[m("qbg-claude-a--glm-5", "glm-5", false)]).unwrap();
        let rows = doc["modelPicker"]["options"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["model"], "mine", "使用者自己的那行留着、排在前面");
        assert!(
            doc["modelPicker"].get("replaceBuiltInOptions").is_none(),
            "使用者的开关不替他改"
        );
        claude_model_picker(&mut doc, &[]).unwrap();
        assert_eq!(doc["modelPicker"]["options"], json!([{ "model": "mine" }]));
        assert!(claude_model_picker(&mut json!({ "modelPicker": [] }), &[]).is_err());
    }

    #[test]
    fn desktop_entries_are_added_and_removed_without_touching_the_user_s() {
        let mut doc = json!({ "inferenceModels": ["claude-sonnet-5", { "name": "claude-fable-qbg-123456" }] });
        desktop_inference_models(&mut doc, &[m("claude-fable-qbg-654321", "glm-5", false)])
            .unwrap();
        assert_eq!(
            doc["inferenceModels"],
            json!(["claude-sonnet-5", { "name": "claude-fable-qbg-654321", "labelOverride": "glm-5（站 · 组）" }]),
            "旧的聚合条目换掉、使用者的留着"
        );
        desktop_inference_models(&mut doc, &[]).unwrap();
        assert_eq!(doc["inferenceModels"], json!(["claude-sonnet-5"]));
        let mut empty = json!({});
        desktop_inference_models(&mut empty, &[]).unwrap();
        assert!(empty.get("inferenceModels").is_none(), "没有就不建");
    }

    #[test]
    fn the_codex_catalog_keeps_the_bundled_rows_and_appends_ours_after_them() {
        let bundled = vec![
            json!({ "slug": "gpt-6.1-sol", "priority": 3, "base_instructions": "official", "apply_patch_tool_type": "freeform" }),
            json!({ "slug": "gpt-6", "priority": 9, "base_instructions": "official" }),
        ];
        let c = codex_catalog(
            &bundled,
            &[
                m("qbg-a/gpt-6.1-sol", "gpt-6.1-sol", true),
                m("qbg-b/glm-5", "glm-5", true),
                m("qbg-c/gpt-6", "gpt-6", false),
            ],
        );
        let rows = c["models"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], bundled[0], "自带的行原样");
        assert_eq!(rows[2]["slug"], "qbg-a/gpt-6.1-sol");
        assert_eq!(
            rows[2]["apply_patch_tool_type"], "freeform",
            "原生 Responses 的同名 GPT 行照抄官方那行"
        );
        assert_eq!(rows[2]["priority"], 10, "排在所有自带行之后");
        assert_eq!(
            rows[3]["shell_type"], "shell_command",
            "没有同名的用中性模板"
        );
        assert!(
            rows[4].get("apply_patch_tool_type").is_none(),
            "转换线上的 GPT 名也用中性模板"
        );
        assert!(rows.iter().skip(2).all(|r| r["comp_hash"] == COMP_HASH));
    }

    #[test]
    fn the_codex_pointer_only_ever_moves_our_own() {
        let ours = r"C:\qb\env\qb-gate-model-catalog.json";
        let mut doc: toml_edit::DocumentMut = "model = \"x\"\n".parse().unwrap();
        codex_catalog_pointer(&mut doc, ours, true).unwrap();
        assert_eq!(doc["model_catalog_json"].as_str(), Some(ours));
        codex_catalog_pointer(&mut doc, ours, false).unwrap();
        assert!(doc.get("model_catalog_json").is_none());
        let mut theirs: toml_edit::DocumentMut =
            "model_catalog_json = \"D:/mine.json\"\n".parse().unwrap();
        assert!(codex_catalog_pointer(&mut theirs, ours, true).is_err());
        assert!(codex_catalog_pointer(&mut theirs, ours, false).is_err());
        assert_eq!(
            theirs["model_catalog_json"].as_str(),
            Some("D:/mine.json"),
            "使用者的指针不动"
        );
    }
}
