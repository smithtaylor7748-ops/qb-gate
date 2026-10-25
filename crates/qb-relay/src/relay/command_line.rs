// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 阶段 4 加的两档中转命令行（Gemini CLI、Grok Build，cc-switch 的 C1 / C2）的配置写法。
//!
//! 跟 Claude Code / Codex 一样只写**指到本机路由的那几个键**，Key 是占位的 `ROUTER_KEY`（真 Key 由本机路由换上）。
//! 都在 [`crate::live::patch`] 的保序补丁上写：原文件解析不了就报错、一个字不写。
//!
//! | 软件 | 文件 | 写什么 |
//! |---|---|---|
//! | Gemini CLI | `<中转环境目录>\.gemini\.env` | `GOOGLE_GEMINI_BASE_URL`、`GEMINI_API_KEY`（删掉会抢先的 `GOOGLE_API_KEY` / `GOOGLE_GENAI_USE_VERTEXAI`） |
//! | Gemini CLI | `<中转环境目录>\.gemini\settings.json` | `security.auth.selectedType = "gemini-api-key"`（不然一启动就弹 Google 登录） |
//! | Grok Build | `~\.grok\config.toml` | 一整张 `[model."qb-gate"]` 表 + `models.default` 指向它 |
//!
//! ⛔ Gemini CLI 的中转环境目录是 QB Gate 自己的（`GEMINI_CLI_HOME` 指向它），**不是**官方 Gemini 槽位的目录；
//! Grok Build 没有能挪配置目录的环境变量，只能写它自己的 `~\.grok\config.toml` —— 只动那张表和那一个键，
//! 使用者自己的其他 `[model.*]` 表、别的设置一概不碰（cc-switch `live/project/grok.rs` 的 `proxy_contract`）。

use crate::error::{GateError, Result};
use crate::live::patch::dotenv::DotenvPatch;
use crate::live::patch::json::JsonPatch;
use crate::live::patch::toml::TomlPatch;
use crate::live::patch::{KeyPath, LivePatch};
use std::path::Path;

/// Grok Build 的模型表叫什么（`[model."qb-gate"]`）。
pub const GROK_MODEL_TABLE: &str = "qb-gate";

/// Grok Build 没写模型时用的（cc-switch 的 `GROK_BUILD_DEFAULT_MODEL`）。线路自己的模型映射会再换。
pub const GROK_DEFAULT_MODEL: &str = "grok-4.5";

fn utf8(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|e| GateError::Other(e.to_string()))
}

/// `<GEMINI_CLI_HOME>\.gemini\.env` 的新内容。注释、空行、别的变量和顺序照原样。
pub fn gemini_env(existing: &str, base_url: &str, key: &str) -> Result<String> {
    let patch = DotenvPatch {
        set: vec![
            ("GOOGLE_GEMINI_BASE_URL".into(), base_url.to_string()),
            ("GEMINI_API_KEY".into(), key.to_string()),
        ],
        // 这两个在 Gemini CLI 里比 `GEMINI_API_KEY` 优先：留着的话请求会绕开本机路由去走 Vertex / 别的 Key。
        remove: vec!["GOOGLE_API_KEY".into(), "GOOGLE_GENAI_USE_VERTEXAI".into()],
        ..DotenvPatch::default()
    };
    utf8(patch.apply(Path::new(".env"), Some(existing.as_bytes()))?)
}

/// `<GEMINI_CLI_HOME>\.gemini\settings.json` 的新内容：认证方式选 API Key，别的设置照原样。
pub fn gemini_settings(existing: &str) -> Result<String> {
    let patch = JsonPatch {
        set: vec![(
            KeyPath::new(&["security", "auth", "selectedType"]),
            serde_json::Value::String("gemini-api-key".into()),
        )],
        ..JsonPatch::default()
    };
    utf8(patch.apply(Path::new("settings.json"), Some(existing.as_bytes()))?)
}

/// `~\.grok\config.toml` 的新内容：`[model."qb-gate"]` 整张表换成指到本机路由的那份（地址、占位 Key、
/// 接口类型 Responses —— 本机路由给 Grok 那一档只提供 Responses），`models.default` 指向它。
/// 整张表换而不是只改几个键：只换几个会留下上一次的 `env_key`、模型名（cc-switch 同样整表换）。
pub fn grok_config_toml(existing: &str, base_url: &str, key: &str, model: &str) -> Result<String> {
    use toml_edit::{value, Item, Table};
    let model = if model.trim().is_empty() {
        GROK_DEFAULT_MODEL
    } else {
        model.trim()
    };
    let mut table = Table::new();
    table.insert("name", value("QB Gate 本机路由"));
    table.insert("model", value(model));
    table.insert("base_url", value(base_url));
    table.insert("api_key", value(key));
    table.insert("api_backend", value("responses"));
    let patch = TomlPatch {
        set: vec![
            (
                KeyPath::new(&["model", GROK_MODEL_TABLE]),
                Item::Table(table),
            ),
            (
                KeyPath::new(&["models", "default"]),
                value(GROK_MODEL_TABLE),
            ),
        ],
        ..TomlPatch::default()
    };
    utf8(patch.apply(Path::new("config.toml"), Some(existing.as_bytes()))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gemini_env_points_at_the_router_and_drops_keys_that_would_win() {
        let existing =
            "# mine\nGOOGLE_API_KEY=real\nGEMINI_MODEL=gemini-3-pro\nGEMINI_API_KEY=old\n";
        let out = gemini_env(existing, "http://127.0.0.1:15721/gemini", "placeholder").unwrap();
        assert_eq!(
            out,
            "# mine\nGEMINI_MODEL=gemini-3-pro\nGEMINI_API_KEY=placeholder\nGOOGLE_GEMINI_BASE_URL=http://127.0.0.1:15721/gemini\n"
        );
        assert!(gemini_env("", "b", "k")
            .unwrap()
            .contains("GEMINI_API_KEY=k"));
    }

    #[test]
    fn gemini_settings_select_the_api_key_and_keep_the_rest() {
        let out = gemini_settings(
            r#"{"ui":{"theme":"dark"},"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["security"]["auth"]["selectedType"], "gemini-api-key");
        assert_eq!(v["ui"]["theme"], "dark");
        assert!(gemini_settings("{ broken").is_err(), "解析不了就不写");
    }

    #[test]
    fn grok_gets_one_whole_table_and_its_default_and_nothing_else_moves() {
        let existing = "# my grok\n[models]\ndefault = \"mine\"\n\n[model.mine]\nmodel = \"grok-4\"\napi_key = \"real\"\n\n[model.\"qb-gate\"]\nmodel = \"old\"\nenv_key = \"STALE\"\n";
        let out = grok_config_toml(
            existing,
            "http://127.0.0.1:15721/grok/v1",
            "placeholder",
            "",
        )
        .unwrap();
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert_eq!(doc["models"]["default"].as_str(), Some(GROK_MODEL_TABLE));
        let ours = &doc["model"][GROK_MODEL_TABLE];
        assert_eq!(
            ours["base_url"].as_str(),
            Some("http://127.0.0.1:15721/grok/v1")
        );
        assert_eq!(ours["api_key"].as_str(), Some("placeholder"));
        assert_eq!(ours["api_backend"].as_str(), Some("responses"));
        assert_eq!(ours["model"].as_str(), Some(GROK_DEFAULT_MODEL));
        assert!(
            ours.get("env_key").is_none(),
            "整张表换，上一次的 env_key 不许留：{out}"
        );
        assert_eq!(
            doc["model"]["mine"]["api_key"].as_str(),
            Some("real"),
            "使用者自己的表不碰"
        );
        assert!(out.starts_with("# my grok"), "注释照原样");
        assert!(grok_config_toml("[model\n", "b", "k", "m").is_err());
    }
}
