//! 官方目录里有没有中转/API 认证残留。
//!
//! # 为什么单独一个模块
//!
//! 这条规则原来叫 `workspace::check_official_configuration`，住在一个 1367 行的
//! 编排模块里。而 `plugins::sillytavern::start` 启动桥接之前也要问这一句 ——
//! 于是插件模块为了 40 行的一条判断，反向依赖了整个工作区编排，
//! 成了全项目那个 10 模块大环上的一条边。
//!
//! 规则本身零依赖：读两个文件、看几个键在不在。它本来就该是一片叶子。
//!
//! # 规则是什么
//!
//! 一个带着中转残留的原生目录，**不能诚实地以官方身份启动**。
//! 读出来、报出来 —— 但绝不在启动过程中替使用者删掉有歧义的配置。

use std::path::Path;

use crate::config_io;
use crate::domain::Client;
use crate::error::{GateError, Result};

pub fn check_official(client: Client, dir: &Path) -> Result<()> {
    let residue = if client == Client::Codex {
        let auth = config_io::read_object(&dir.join("auth.json"))?;
        let api_auth = auth["OPENAI_API_KEY"]
            .as_str()
            .is_some_and(|s| !s.is_empty());
        let text = config_io::read_text(&dir.join("config.toml"))?;
        let d = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| GateError::Other(format!("官方 Codex 配置无效：{e}")))?;
        api_auth
            || d.get("model_provider")
                .and_then(|v| v.as_str())
                .is_some_and(|v| v != "openai")
            || d.get("forced_login_method").and_then(|v| v.as_str()) == Some("api")
    } else {
        let v = config_io::read_object(&dir.join("settings.json"))?;
        v.get("env").is_some_and(|env| {
            [
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
                "CLAUDE_CODE_USE_BEDROCK",
                "CLAUDE_CODE_USE_VERTEX",
            ]
            .iter()
            .any(|key| {
                env.get(key)
                    .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            })
        })
    };
    if residue {
        return Err(GateError::Other(format!(
            "官方目录 {} 含中转或 API 认证配置。请在官方账户页预览残留配置并迁移；原文件未改动",
            dir.display()
        )));
    }
    Ok(())
}
