//! Official and relay workflows share infrastructure, never identity or configuration files.
use crate::{
    config_io::{self, Edit},
    domain::*,
    error::{GateError, Result},
    repository::Repository,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn snapshot() -> Result<Workspace> {
    let db = Repository::open()?;
    let mut environments = db.list::<Environment>("environments")?;
    for e in &mut environments {
        e.config_state = configuration_state(&db, e).unwrap_or_else(|_| "unreadable".into());
    }
    let mut credentials = db.list::<Credential>("credentials")?;
    for c in &mut credentials {
        c.available = db.key(&c.id).is_ok();
        if !c.available {
            c.masked = "需要重新填写".into();
        }
    }
    let mut installations = db.list::<ExtensionInstallation>("installations")?;
    for i in &mut installations {
        i.state =
            crate::extensions::installation_state(&db, i).unwrap_or_else(|_| "unverified".into());
    }
    Ok(Workspace {
        launch_plans: db.plans()?,
        providers: db.list("providers")?,
        credentials,
        environments,
        sessions: db.list("sessions")?,
        operations: db.list("operations")?,
        installations,
        migration_notes: db
            .meta("legacy-v1")?
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default(),
    })
}

fn next_revision(db: &Repository, table: &str, id: &str) -> Result<u32> {
    let list = db.list::<serde_json::Value>(table)?;
    Ok(list
        .iter()
        .find(|v| v["id"] == id)
        .and_then(|v| v["revision"].as_u64())
        .unwrap_or(0) as u32
        + 1)
}
fn check_revision(db: &Repository, table: &str, id: &str, expected: u32) -> Result<u32> {
    let next = next_revision(db, table, id)?;
    if next > 1 && next - 1 != expected {
        return Err(GateError::Other(
            "记录已在别处更新，请刷新后重试；草稿已保留".into(),
        ));
    }
    Ok(next)
}
pub fn provider_save(mut p: Provider) -> Result<Provider> {
    let db = Repository::open()?;
    db.transaction(|| {
        if p.name.trim().is_empty() {
            return Err(GateError::Other("请填写服务商名称".into()));
        }
        p.base_url = crate::endpoint::endpoint_base(&p.base_url)?;
        if p.id.is_empty() {
            p.id = config_io::id();
        }
        p.revision = check_revision(&db, "providers", &p.id, p.revision)?;
        db.put("providers", &p.id, &p)?;
        for mut e in db
            .list::<Environment>("environments")?
            .into_iter()
            .filter(|e| e.provider_id == p.id)
        {
            e.revision += 1;
            e.config_state = "saved".into();
            db.set_environment(&e)?;
        }
        Ok(p)
    })
}
pub fn credential_save(mut c: Credential, action: &str, key: Option<&str>) -> Result<Credential> {
    let db = Repository::open()?;
    db.transaction(|| {
        if c.id.is_empty() {
            c.id = config_io::id();
        }
        c.revision = check_revision(&db, "credentials", &c.id, c.revision)?;
        if c.label.trim().is_empty() {
            return Err(GateError::Other("请输入凭证名称".into()));
        }
        if let Ok(old) = db.get::<Credential>("credentials", &c.id) {
            if old.provider_id != c.provider_id
                && db
                    .list::<Environment>("environments")?
                    .iter()
                    .any(|e| e.credential_id.as_deref() == Some(&c.id))
            {
                return Err(GateError::Other(
                    "此凭证已被环境引用，不能更换所属服务商；请新建凭证".into(),
                ));
            }
        }

        let sealed = match action {
            "replace" => {
                let key = key
                    .filter(|k| !k.trim().is_empty())
                    .ok_or_else(|| GateError::Other("请填写新的 API Key".into()))?;
                crate::secret::seal(key.trim())?
            }
            "keep" => db.sealed(&c.id)?,
            "clear" => String::new(),
            _ => return Err(GateError::Other("凭证操作必须为保留、替换或清除".into())),
        };
        let plain = crate::secret::open(&sealed);
        c.masked = plain.as_deref().map(crate::relay::mask).unwrap_or_default();
        c.available = plain.is_some();
        db.set_credential(&c, &sealed)?;
        for mut e in db
            .list::<Environment>("environments")?
            .into_iter()
            .filter(|e| e.credential_id.as_deref() == Some(&c.id))
        {
            e.revision += 1;
            e.config_state = "saved".into();
            db.set_environment(&e)?;
        }
        Ok(c)
    })
}
pub fn environment_save(mut e: Environment) -> Result<Environment> {
    let db = Repository::open()?;
    if e.client == Client::ClaudeDesktop {
        return Err(GateError::Other(
            "桌面端当前只支持官方启动，请选择 Claude Code 或 Codex".into(),
        ));
    }
    if e.name.trim().is_empty() {
        return Err(GateError::Other("请填写使用环境名称".into()));
    }
    if !["responses", "chat"].contains(&e.wire_api.as_str())
        || !["env_key", "bearer_token", "none"].contains(&e.auth_style.as_str())
    {
        return Err(GateError::Other("协议或认证方式无效".into()));
    }
    if e.id.is_empty() {
        e.id = config_io::id();
    }
    e.revision = check_revision(&db, "environments", &e.id, e.revision)?;
    if e.revision > 1 && db.get::<Environment>("environments", &e.id)?.client != e.client {
        return Err(GateError::Other(
            "已建立环境的客户端不能更换，请复制为一个新环境".into(),
        ));
    }

    e.config_dir = crate::config_io::environment_dir(&db.root, &e.id)?
        .display()
        .to_string();
    e.applied_revision = if e.revision > 1 {
        db.get::<Environment>("environments", &e.id)?
            .applied_revision
    } else {
        None
    };
    e.config_state = "saved".into();
    db.set_environment(&e)?;
    Ok(e)
}

pub fn endpoint(base: &str, path: &str) -> Result<String> {
    let base = crate::endpoint::endpoint_base(base)?;
    let path = path
        .trim_start_matches('/')
        .strip_prefix("v1/")
        .unwrap_or(path.trim_start_matches('/'));
    if base.ends_with("/v1") {
        Ok(format!("{base}/{path}"))
    } else {
        Ok(format!("{base}/v1/{path}"))
    }
}

pub fn client_base(raw: &str, client: Client) -> Result<String> {
    let base = crate::endpoint::endpoint_base(raw)?;
    Ok(if client == Client::ClaudeCode {
        base.strip_suffix("/v1").unwrap_or(&base).to_string()
    } else if base.ends_with("/v1") {
        base
    } else {
        format!("{base}/v1")
    })
}
fn official_dir(client: Client, id: &str) -> Result<PathBuf> {
    if client == Client::ClaudeDesktop {
        return Err(GateError::Other("桌面端无需迁移 Code 配置".into()));
    }
    if client == Client::Codex {
        return Ok(dirs::home_dir().unwrap_or_default().join(".codex"));
    }
    if id.is_empty() {
        Ok(dirs::home_dir().unwrap_or_default().join(".claude"))
    } else {
        crate::accounts::validate_label(id)?;
        Ok(crate::accounts::AccountRoots::current().slot_dir(id))
    }
}
struct OfficialMigration {
    edits: Vec<Edit>,
    base: String,
    model: String,
    small: String,
    key: Option<String>,
    style: String,
}
fn official_migration(client: Client, dir: &Path) -> Result<OfficialMigration> {
    let mut has_residue = false;
    let mut result = OfficialMigration {
        edits: vec![],
        base: String::new(),
        model: String::new(),
        small: String::new(),
        key: None,
        style: "env_key".into(),
    };
    if client == Client::ClaudeCode {
        let path = dir.join("settings.json");
        let expected = config_io::read_optional(&path)?;
        let mut value = config_io::parse_object(expected.as_deref())?;
        if let Some(env) = value.get_mut("env").and_then(|v| v.as_object_mut()) {
            if [
                "CLAUDE_CODE_USE_BEDROCK",
                "CLAUDE_CODE_USE_VERTEX",
                "CLAUDE_CODE_USE_FOUNDRY",
            ]
            .iter()
            .any(|k| env.contains_key(*k))
            {
                return Err(GateError::Other(
                    "此官方目录使用云平台认证，请先在原客户端处理；未自动移除未知配置".into(),
                ));
            }
            has_residue = [
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
            ]
            .iter()
            .any(|k| {
                env.get(*k)
                    .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            });
            result.base = env
                .get("ANTHROPIC_BASE_URL")
                .and_then(|v| v.as_str())
                .unwrap_or("https://api.anthropic.com")
                .into();
            result.model = env
                .get("ANTHROPIC_MODEL")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into();
            result.small = env
                .get("ANTHROPIC_SMALL_FAST_MODEL")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into();
            result.key = env
                .get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env.get("ANTHROPIC_API_KEY"))
                .and_then(|v| v.as_str())
                .map(String::from);
            if env.contains_key("ANTHROPIC_AUTH_TOKEN") {
                result.style = "bearer_token".into();
            }
            for field in [
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_MODEL",
                "ANTHROPIC_SMALL_FAST_MODEL",
                "ANTHROPIC_DEFAULT_OPUS_MODEL",
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            ] {
                env.remove(field);
            }
        }
        result.edits.push(Edit {
            path,
            expected,
            body: Some(serde_json::to_vec_pretty(&value)?),
        });
    } else {
        let path = dir.join("config.toml");
        let expected = config_io::read_optional(&path)?;
        let text = String::from_utf8(expected.clone().unwrap_or_default())
            .map_err(|e| GateError::Other(e.to_string()))?;
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| GateError::Other(e.to_string()))?;
        let provider = doc
            .get("model_provider")
            .and_then(|v| v.as_str())
            .unwrap_or("openai")
            .to_string();
        has_residue = provider != "openai"
            || doc.get("forced_login_method").and_then(|v| v.as_str()) == Some("api")
            || doc
                .get("model_providers")
                .and_then(|v| v.get(&provider))
                .is_some();
        result.base = doc
            .get("model_providers")
            .and_then(|v| v.get(&provider))
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str())
            .unwrap_or(if provider == "openai" {
                "https://api.openai.com/v1"
            } else {
                ""
            })
            .to_string();
        result.model = doc
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into();
        if let Some(p) = doc.get("model_providers").and_then(|v| v.get(&provider)) {
            result.key = p
                .get("experimental_bearer_token")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        doc.remove("model_provider");
        doc.remove("model");
        doc.remove("forced_login_method");
        if let Some(table) = doc
            .get_mut("model_providers")
            .and_then(|t| t.as_table_mut())
        {
            table.remove(&provider);
        }
        result.edits.push(Edit {
            path,
            expected,
            body: Some(doc.to_string().into_bytes()),
        });
        let auth = dir.join("auth.json");
        if let Some(bytes) = config_io::read_optional(&auth)? {
            let v = config_io::object(
                std::str::from_utf8(&bytes).map_err(|e| GateError::Other(e.to_string()))?,
            )?;
            if let Some(key) = v["OPENAI_API_KEY"].as_str().filter(|s| !s.is_empty()) {
                if v.get("tokens").is_some() {
                    return Err(GateError::Other("认证文件混有 API 与 OAuth 数据，请使用客户端原生登录处理；未复制或删除 OAuth".into()));
                }
                has_residue = true;
                result.key = Some(key.into());
                result.edits.push(Edit {
                    path: auth,
                    expected: Some(bytes),
                    body: None,
                });
            }
        }
    }
    if !has_residue {
        return Err(GateError::Other(
            "没有发现可迁移的 API 配置，官方配置未改动".into(),
        ));
    }
    result.base = crate::endpoint::endpoint_base(&result.base)?;
    Ok(result)
}
fn previews(edits: Vec<Edit>, revision: u32) -> Vec<ConfigPreview> {
    let fingerprint = config_io::fingerprint(&edits, revision);
    edits
        .into_iter()
        .map(|e| ConfigPreview {
            path: e.path.display().to_string(),
            before: redacted_text(&e.expected),
            after: redacted_text(&e.body),
            fingerprint: fingerprint.clone(),
        })
        .collect()
}
pub fn official_preview(client: Client, id: &str) -> Result<Vec<ConfigPreview>> {
    Ok(previews(
        official_migration(client, &official_dir(client, id)?)?.edits,
        0,
    ))
}
pub fn official_migrate(client: Client, id: &str, fingerprint: &str) -> Result<String> {
    if crate::sessions::list().iter().any(|s| {
        s.context.identity_kind == IdentityKind::Official
            && s.context.client == client
            && s.state == "running"
    }) {
        return Err(GateError::Other(
            "请先停止此客户端的官方会话，再迁移配置".into(),
        ));
    }
    let db = Repository::open()?;
    let migration = official_migration(client, &official_dir(client, id)?)?;
    crate::sessions::ensure_verified(|s| {
        s.context.identity_kind == IdentityKind::Official && s.context.client == client
    })?;
    if config_io::fingerprint(&migration.edits, 0) != fingerprint {
        return Err(GateError::Other(
            "配置在预览后发生变化，请重新检查迁移预览".into(),
        ));
    }

    let p = Provider {
        id: config_io::id(),
        name: format!(
            "{} · 原官方目录 API",
            if id.is_empty() { "默认" } else { id }
        ),
        base_url: migration.base,
        website: String::new(),
        note: "从原官方配置目录显式迁移".into(),
        tags: vec!["迁移".into()],
        favorite: false,
        revision: 1,
    };
    let credential = migration.key.as_deref().map(|key| Credential {
        id: config_io::id(),
        provider_id: p.id.clone(),
        label: "迁移凭证".into(),
        masked: crate::relay::mask(key),
        available: true,
        revision: 1,
    });
    let eid = config_io::id();
    let env = Environment {
        id: eid.clone(),
        name: p.name.clone(),
        provider_id: p.id.clone(),
        credential_id: credential.as_ref().map(|c| c.id.clone()),
        client,
        model: migration.model,
        small_model: migration.small,
        wire_api: "responses".into(),
        auth_style: migration.style,
        config_dir: crate::config_io::environment_dir(&db.root, &eid)?
            .display()
            .to_string(),
        revision: 1,
        applied_revision: None,
        config_state: "saved".into(),
    };
    crate::repository::commit_database(&db, migration.edits, |_| {
        db.put("providers", &p.id, &p)?;
        if let Some(c) = &credential {
            db.set_credential(c, &crate::secret::seal(migration.key.as_deref().unwrap())?)?;
        }
        db.set_environment(&env)
    })?;
    Ok(format!(
        "已迁移到中转环境 {}；原配置已留恢复记录，官方 OAuth 凭证未复制",
        env.name
    ))
}
pub fn save_plan(mut plan: LaunchPlan) -> Result<LaunchPlan> {
    if plan.name.trim().is_empty() {
        return Err(GateError::Other("请输入启动方案名称".into()));
    }
    if plan.identity_kind == IdentityKind::Relay {
        let e: Environment = Repository::open()?.get("environments", &plan.identity_id)?;
        if e.client != plan.client {
            return Err(GateError::Other("启动方案客户端与环境不匹配".into()));
        }
    } else if !plan.identity_id.is_empty() {
        crate::accounts::validate_label(&plan.identity_id)?;
    }
    if !plan.working_dir.is_empty() && !Path::new(&plan.working_dir).is_dir() {
        return Err(GateError::Other("工作目录不存在".into()));
    }
    let db = Repository::open()?;
    let mut plans = db.plans()?;
    if plan.id.is_empty() {
        plan.id = config_io::id();
    }
    plans.retain(|p| p.id != plan.id);
    plans.insert(0, plan.clone());
    db.set_meta("launch-plans", &serde_json::to_string(&plans)?)?;
    Ok(plan)
}
pub fn remove_plan(id: &str) -> Result<()> {
    let db = Repository::open()?;
    let mut plans = db.plans()?;
    plans.retain(|p| p.id != id);
    db.set_meta("launch-plans", &serde_json::to_string(&plans)?)
}

#[derive(Serialize, Deserialize)]
pub struct RelayExport {
    pub schema: u32,
    pub providers: Vec<Provider>,
    pub credentials: Vec<Credential>,
    pub environments: Vec<Environment>,
}
pub fn export_relays() -> Result<RelayExport> {
    let db = Repository::open()?;
    let mut credentials = db.list::<Credential>("credentials")?;
    for c in &mut credentials {
        c.masked = String::new();
        c.available = false;
    }
    let mut environments = db.list::<Environment>("environments")?;
    for e in &mut environments {
        e.config_dir = String::new();
        e.applied_revision = None;
        e.config_state = "saved".into();
    }
    Ok(RelayExport {
        schema: 1,
        providers: db.list("providers")?,
        credentials,
        environments,
    })
}
pub fn import_relays(bundle: RelayExport) -> Result<usize> {
    if bundle.schema != 1
        || bundle.providers.len() > 500
        || bundle.credentials.len() > 2000
        || bundle.environments.len() > 2000
    {
        return Err(GateError::Other("导入格式或条目数量无效".into()));
    }
    let db = Repository::open()?;
    db.transaction(|| {
        let mut providers = BTreeMap::new();
        let mut credentials = BTreeMap::new();
        for mut p in bundle.providers {
            p.base_url = crate::endpoint::endpoint_base(&p.base_url)?;
            if p.name.trim().is_empty() {
                return Err(GateError::Other("服务商名称不能为空".into()));
            }
            let old = p.id.clone();
            p.id = config_io::id();
            p.revision = 1;
            if providers.insert(old, p.id.clone()).is_some() {
                return Err(GateError::Other("重复的服务商 ID".into()));
            }
            db.put("providers", &p.id, &p)?;
        }
        for mut c in bundle.credentials {
            let old = c.id.clone();
            c.id = config_io::id();
            c.provider_id = providers
                .get(&c.provider_id)
                .ok_or_else(|| GateError::Other("凭证引用的服务商不存在".into()))?
                .clone();
            c.available = false;
            c.masked = String::new();
            c.revision = 1;
            if credentials.insert(old, c.id.clone()).is_some() {
                return Err(GateError::Other("重复的凭证 ID".into()));
            }
            db.set_credential(&c, "")?;
        }
        let count = bundle.environments.len();
        for mut e in bundle.environments {
            if e.client == Client::ClaudeDesktop
                || e.name.trim().is_empty()
                || !["responses", "chat"].contains(&e.wire_api.as_str())
                || !["env_key", "bearer_token", "none"].contains(&e.auth_style.as_str())
            {
                return Err(GateError::Other("导入的环境字段无效".into()));
            }
            e.id = config_io::id();
            e.provider_id = providers
                .get(&e.provider_id)
                .ok_or_else(|| GateError::Other("环境引用的服务商不存在".into()))?
                .clone();
            e.credential_id = e
                .credential_id
                .map(|id| {
                    credentials
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| GateError::Other("环境引用的凭证不存在".into()))
                })
                .transpose()?;
            e.config_dir = crate::config_io::environment_dir(&db.root, &e.id)?
                .display()
                .to_string();
            e.revision = 1;
            e.applied_revision = None;
            e.config_state = "saved".into();
            db.set_environment(&e)?;
        }
        Ok(count)
    })
}

fn meta(p: &Provider, e: &Environment) -> crate::relay::ProviderMeta {
    crate::relay::ProviderMeta {
        id: e.id.clone(),
        target: if e.client == Client::Codex {
            crate::relay::RelayTarget::Codex
        } else {
            crate::relay::RelayTarget::ClaudeCode
        },
        slug: "qb_relay".into(),
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        model: (!e.model.is_empty()).then(|| e.model.clone()),
        small_fast_model: (!e.small_model.is_empty()).then(|| e.small_model.clone()),
        wire_api: if e.wire_api == "chat" {
            crate::relay::WireApi::Chat
        } else {
            crate::relay::WireApi::Responses
        },
        auth_style: match e.auth_style.as_str() {
            "bearer_token" => crate::relay::AuthStyle::BearerToken,
            "none" => crate::relay::AuthStyle::None,
            _ => crate::relay::AuthStyle::EnvKey,
        },
        note: None,
        website: None,
        icon: None,
        sort: 0,
        created_at: String::new(),
    }
}

pub fn configuration_files(db: &Repository, e: &Environment) -> Result<Vec<Edit>> {
    let p: Provider = db.get("providers", &e.provider_id)?;
    let dir = crate::config_io::environment_dir(&db.root, &e.id)?;
    let key = if e.auth_style == "none" {
        None
    } else {
        e.credential_id
            .as_deref()
            .map(|id| db.key(id))
            .transpose()?
    };
    if e.auth_style != "none" && key.is_none() {
        return Err(GateError::Other("请为此中转环境选择可用凭证".into()));
    }
    let mut m = meta(&p, e);
    m.base_url = client_base(&p.base_url, e.client)?;
    let mut edits = Vec::new();
    match e.client {
        Client::ClaudeCode => {
            let path = dir.join("settings.json");
            config_io::read_object(&path)?;
            let old = config_io::read_optional(&path)?;
            let text = String::from_utf8(old.clone().unwrap_or_default())
                .map_err(|e| GateError::Other(e.to_string()))?;
            let mut body =
                config_io::object(&crate::relay::claude_settings_json(&text, &m, None)?)?;
            // Keys are injected into the child only; they are not duplicated into the profile JSON.
            // 中转不归 IP 门禁管（见 `LaunchTarget::gated`），所以这里既不写入
            // 会话内 hook，也要摘掉旧版本留下的那一份 —— 不摘的话它会继续
            // 逐次拦下中转请求，而界面上没有任何地方说得清为什么。
            crate::gate::hook::strip_from(&mut body);
            edits.push(Edit {
                path,
                expected: old,
                body: Some(serde_json::to_vec_pretty(&body)?),
            });
        }
        Client::Codex => {
            if e.wire_api != "responses" {
                return Err(GateError::Other(
                    "当前 Codex 客户端使用 Responses 协议；Chat 协议可诊断但不能启动为 Codex 环境"
                        .into(),
                ));
            }
            let path = dir.join("config.toml");
            let old = config_io::read_optional(&path)?;
            let text = String::from_utf8(old.clone().unwrap_or_default())
                .map_err(|e| GateError::Other(e.to_string()))?;
            text.parse::<toml_edit::DocumentMut>()
                .map_err(|e| GateError::Other(format!("原配置 TOML 无效：{e}")))?;
            let mut doc = crate::relay::codex_config_toml(&text, &m, None)
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| GateError::Other(e.to_string()))?;
            doc["forced_login_method"] = toml_edit::value("api");
            doc["cli_auth_credentials_store"] = toml_edit::value("file");
            if e.auth_style == "none" {
                if let Some(table) = doc["model_providers"]["qb_relay"].as_table_mut() {
                    table.remove("env_key");
                }
            } else {
                doc["model_providers"]["qb_relay"]["env_key"] = toml_edit::value("OPENAI_API_KEY");
            }
            doc["model_providers"]["qb_relay"]["requires_openai_auth"] = toml_edit::value(false);
            edits.push(Edit {
                path,
                expected: old,
                body: Some(doc.to_string().into_bytes()),
            });
            let auth = dir.join("auth.json");
            if let Some(old) = config_io::read_optional(&auth)? {
                let value = config_io::object(
                    &String::from_utf8(old).map_err(|e| GateError::Other(e.to_string()))?,
                )?;
                if value.get("tokens").is_some() {
                    return Err(GateError::Other(
                        "中转环境目录出现 OAuth 凭证，已阻断启动；请检查目录来源".into(),
                    ));
                }
            }
        }
        _ => return Err(GateError::Other("此客户端不支持独立中转环境".into())),
    }
    Ok(edits)
}

#[derive(Serialize, ts_rs::TS)]
pub struct ConfigPreview {
    pub path: String,
    pub before: String,
    pub after: String,
    pub fingerprint: String,
}
pub fn preview(id: &str) -> Result<Vec<ConfigPreview>> {
    let db = Repository::open()?;
    let e: Environment = db.get("environments", id)?;
    Ok(previews(configuration_files(&db, &e)?, e.revision))
}
fn redacted_text(bytes: &Option<Vec<u8>>) -> String {
    let text = String::from_utf8_lossy(bytes.as_deref().unwrap_or_default());
    text.lines()
        .map(|l| {
            if [
                "API_KEY",
                "AUTH_TOKEN",
                "bearer_token",
                "access_token",
                "refresh_token",
            ]
            .iter()
            .any(|k| l.contains(k))
            {
                "    [凭证字段已脱敏]".into()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn apply(id: &str) -> Result<Environment> {
    apply_reviewed(id, None)
}
pub fn apply_reviewed(id: &str, fingerprint: Option<&str>) -> Result<Environment> {
    let db = Repository::open()?;
    let mut e: Environment = db.get("environments", id)?;
    let edits = configuration_files(&db, &e)?;
    if fingerprint.is_some_and(|f| f != config_io::fingerprint(&edits, e.revision)) {
        return Err(GateError::Other(
            "环境或配置在预览后发生变化，请重新预览".into(),
        ));
    }
    let hashes: BTreeMap<String, String> = edits
        .iter()
        .filter_map(|e| {
            e.body.as_ref().map(|b| {
                (
                    e.path.file_name().unwrap().to_string_lossy().to_string(),
                    hex::encode(Sha256::digest(b)),
                )
            })
        })
        .collect();
    let previous = db.meta(&format!("applied-environment:{id}"))?;
    e.applied_revision = Some(e.revision);
    e.config_state = "applied".into();
    crate::repository::commit_database(&db, edits, |operation| {
        db.conn
            .execute(
                "UPDATE environments SET hashes=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&hashes)?, id],
            )
            .map_err(|e| GateError::Database(e.to_string()))?;
        db.set_environment(&e)?;
        db.set_meta(
            &format!("environment-undo:{id}"),
            &serde_json::to_string(
                &serde_json::json!({"operation":operation,"previous":previous}),
            )?,
        )?;
        db.set_meta(
            &format!("applied-environment:{id}"),
            &serde_json::to_string(&e)?,
        )
    })?;
    Ok(e)
}
pub fn rollback_environment(id: &str) -> Result<Environment> {
    rollback_environment_in(&Repository::open()?, id)
}
fn rollback_environment_in(db: &Repository, id: &str) -> Result<Environment> {
    let data: serde_json::Value = serde_json::from_str(
        &db.meta(&format!("environment-undo:{id}"))?
            .ok_or_else(|| GateError::Other("此环境还没有可以回滚的配置".into()))?,
    )?;
    let edits = config_io::inverse(
        &db.root.join("operations/files"),
        data["operation"].as_str().unwrap_or_default(),
    )
    .map_err(|e| {
        // 装 MCP 会改写同一个 config.toml / .claude.json，于是这条回滚记录里
        // 记的「应用后的哈希」就对不上了，`inverse` 只会说「已被外部修改」。
        // 数据没坏，行为也是对的（宁可不动），但使用者看不出所以然 ——
        // 他并没有手改过任何文件。把真正的原因补上。
        let bound: Vec<String> = db
            .list::<ExtensionInstallation>("installations")
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.environment_id == id && i.identity_kind == IdentityKind::Relay)
            .map(|i| i.extension_id)
            .collect();
        if bound.is_empty() {
            e
        } else {
            GateError::Other(format!(
                "{e}。此环境在上次应用之后装过扩展（{}），它们改写了同一个配置文件，所以这条回滚记录已经不适用。先卸载这些扩展，或者重新预览并应用一次配置。",
                bound.join("、")
            ))
        }
    })?;
    let current: Environment = db.get("environments", id)?;
    let applied_before = db.meta(&format!("applied-environment:{id}"))?;
    let mut restored: Environment = if let Some(previous) = data["previous"].as_str() {
        serde_json::from_str(previous)?
    } else {
        current.clone()
    };
    restored.revision = current.revision + 1;
    restored.applied_revision = if data["previous"].is_string() {
        Some(restored.revision)
    } else {
        None
    };
    restored.config_state = if restored.applied_revision.is_some() {
        "applied"
    } else {
        "saved"
    }
    .into();
    let hashes: BTreeMap<String, String> = edits
        .iter()
        .filter_map(|e| {
            e.body.as_ref().map(|b| {
                (
                    e.path.file_name().unwrap().to_string_lossy().to_string(),
                    hex::encode(Sha256::digest(b)),
                )
            })
        })
        .collect();
    crate::repository::commit_database(db, edits, |operation| {
        db.set_environment(&restored)?;
        db.conn
            .execute(
                "UPDATE environments SET hashes=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&hashes)?, id],
            )
            .map_err(|e| GateError::Database(e.to_string()))?;
        if restored.applied_revision.is_some() {
            db.set_meta(
                &format!("applied-environment:{id}"),
                &serde_json::to_string(&restored)?,
            )?;
        } else {
            db.conn
                .execute(
                    "DELETE FROM metadata WHERE key=?1",
                    [format!("applied-environment:{id}")],
                )
                .map_err(|e| GateError::Database(e.to_string()))?;
        }
        db.set_meta(
            &format!("environment-undo:{id}"),
            &serde_json::to_string(
                &serde_json::json!({"operation":operation,"previous":applied_before}),
            )?,
        )
    })?;
    Ok(restored)
}

fn configuration_state(db: &Repository, e: &Environment) -> Result<String> {
    let hashes: String = db
        .conn
        .query_row(
            "SELECT hashes FROM environments WHERE id=?1",
            [&e.id],
            |r| r.get(0),
        )
        .map_err(|e| GateError::Database(e.to_string()))?;
    let hashes: BTreeMap<String, String> = serde_json::from_str(&hashes)?;
    for (name, hash) in hashes {
        if !["settings.json", "config.toml"].contains(&name.as_str()) {
            return Err(GateError::Other("配置清单文件名无效".into()));
        }
        let bytes = config_io::read_optional(
            &crate::config_io::environment_dir(&db.root, &e.id)?.join(name),
        )?;
        if bytes
            .as_ref()
            .map(|b| hex::encode(Sha256::digest(b)))
            .as_deref()
            != Some(&hash)
        {
            return Ok("external".into());
        }
    }
    Ok(if e.applied_revision == Some(e.revision) {
        "applied"
    } else {
        "saved"
    }
    .into())
}

pub fn references(kind: &str, id: &str) -> Result<Vec<String>> {
    let db = Repository::open()?;
    let mut refs = Vec::new();
    for e in db.list::<Environment>("environments")? {
        if (kind == "providers" && e.provider_id == id)
            || (kind == "credentials" && e.credential_id.as_deref() == Some(id))
        {
            refs.push(format!("使用环境：{}", e.name));
        }
    }
    if kind == "providers" {
        for c in db
            .list::<Credential>("credentials")?
            .into_iter()
            .filter(|c| c.provider_id == id)
        {
            refs.push(format!("API 凭证：{}", c.label));
        }
    }
    if kind == "environments" {
        for plan in db
            .plans()?
            .into_iter()
            .filter(|p| p.identity_kind == IdentityKind::Relay && p.identity_id == id)
        {
            refs.push(format!("启动方案：{}", plan.name));
        }
        for s in db.list::<Session>("sessions")?.into_iter().filter(|s| {
            s.context.identity_kind == IdentityKind::Relay
                && s.context.identity_id == id
                && ["running", "unverified"].contains(&s.state.as_str())
        }) {
            refs.push(format!("运行会话：{}", s.id));
        }
        for i in db
            .list::<ExtensionInstallation>("installations")?
            .into_iter()
            .filter(|i| i.environment_id == id && i.identity_kind == IdentityKind::Relay)
        {
            refs.push(format!("已绑定扩展：{}", i.extension_id));
        }
    }
    Ok(refs)
}
pub fn remove(kind: &str, id: &str) -> Result<()> {
    if !["providers", "credentials", "environments"].contains(&kind) {
        return Err(GateError::Other("不能删除此类记录".into()));
    }
    let refs = references(kind, id)?;
    if !refs.is_empty() {
        return Err(GateError::Other(format!(
            "请先解除引用：{}",
            refs.join("；")
        )));
    }
    let db = Repository::open()?;
    if kind != "environments" {
        return db.remove(kind, id);
    }
    // 隔离目录和这两条元数据都是面板自己的东西，记录删了就该一起收 ——
    // 留着的话：目录里那份 settings.json / config.toml 成了没人认领的残留，
    // `environment-undo:<id>` 还指着一条 journal，而那条 journal 里
    // 封着改动前的配置全文（含 Key 的密文），保留期也清不掉它。
    let dir = crate::config_io::environment_dir(&db.root, id)?;
    db.transaction(|| {
        db.remove(kind, id)?;
        for key in ["environment-undo", "applied-environment"] {
            db.conn
                .execute("DELETE FROM metadata WHERE key=?1", [format!("{key}:{id}")])
                .map_err(|e| GateError::Database(e.to_string()))?;
        }
        Ok(())
    })?;
    // 目录删不掉只记一笔，不让整个删除失败：记录已经提交了，此时报错会让
    // 使用者以为环境还在，而剩一个没人引用的空目录是无害残留。
    if dir.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            crate::audit::write(&format!("环境 {id} 的隔离目录未能删除：{e}"));
        }
    }
    Ok(())
}

pub async fn launch(
    client: Client,
    kind: IdentityKind,
    id: &str,
    working_dir: Option<String>,
    // 收 `&GateState` 而不是 `&AppState`：这里只用得到 `state.gate` 那一半，
    // 而 `AppState` 定义在 `lib.rs`（接口层）。收整个 AppState 的话，
    // 这个函数就永远搬不出 `src-tauri` —— 而它正是 `workspace.rs` 1367 行
    // 只有 4 个测试的原因。
    gate_state: &crate::gate::GateState,
) -> Result<Session> {
    let target = crate::launch::LaunchTarget::of(client);
    let exe = crate::launch::resolve(target)?;
    let working_dir = working_dir
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().display().to_string());
    if !Path::new(&working_dir).is_dir() {
        return Err(GateError::Other("工作目录不存在".into()));
    }
    let (config_dir, env, revision) = if kind == IdentityKind::Relay {
        let db = Repository::open()?;
        let stored: Environment = db.get("environments", id)?;
        if stored.client != client {
            return Err(GateError::Other("客户端与中转环境不匹配".into()));
        }
        if configuration_state(&db, &stored)? == "external" {
            return Err(GateError::Other(
                "配置已被外部修改，请预览差异并重新应用后启动".into(),
            ));
        }
        let e = if configuration_state(&db, &stored)? == "applied" {
            stored
        } else {
            apply(id)?
        };
        let p: Provider = db.get("providers", &e.provider_id)?;
        let key = if e.auth_style == "none" {
            None
        } else {
            e.credential_id
                .as_deref()
                .map(|id| db.key(id))
                .transpose()?
        };
        let mut env = vec![(
            if client == Client::Codex {
                "CODEX_HOME"
            } else {
                "CLAUDE_CONFIG_DIR"
            }
            .into(),
            e.config_dir.clone(),
        )];
        if client == Client::ClaudeCode {
            env.push((
                "ANTHROPIC_BASE_URL".into(),
                client_base(&p.base_url, client)?,
            ));
            if let Some(key) = key {
                env.push((
                    if e.auth_style == "bearer_token" {
                        "ANTHROPIC_AUTH_TOKEN"
                    } else {
                        "ANTHROPIC_API_KEY"
                    }
                    .into(),
                    key,
                ));
            }
        } else if let Some(key) = key {
            env.push(("OPENAI_API_KEY".into(), key));
        }
        (e.config_dir, env, e.revision)
    } else {
        let roots = crate::accounts::AccountRoots::current();
        if client != Client::Codex
            && !id.is_empty()
            && crate::accounts::active_label(&roots).as_deref() != Some(id)
        {
            return Err(GateError::Other("请先在官方账户页切换到此账户".into()));
        }
        let dir = if client == Client::Codex {
            dirs::home_dir().unwrap_or_default().join(".codex")
        } else {
            crate::accounts::active_slot_dir(&roots)
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"))
        };
        if client != Client::ClaudeDesktop {
            crate::residue::check_official(client, &dir)?;
        }
        if client == Client::ClaudeCode {
            crate::gate::hook::ensure_for_dir(&dir)?;
        }
        let env = if client == Client::ClaudeDesktop {
            vec![]
        } else {
            vec![(
                if client == Client::Codex {
                    "CODEX_HOME"
                } else {
                    "CLAUDE_CONFIG_DIR"
                }
                .into(),
                dir.display().to_string(),
            )]
        };
        (dir.display().to_string(), env, 0)
    };
    let mut env = env;
    if client == Client::ClaudeCode {
        env.push(("DISABLE_AUTOUPDATER".into(), "1".into()));
        if crate::settings::load().disable_telemetry {
            env.extend(
                crate::launch::TELEMETRY_OFF
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            );
        }
    }
    let holder = format!("launch-{}", config_io::id());
    // 解锁与租约看的是可执行文件（Deny ACE 按文件加，不认身份）；
    // 判不过时收不收这个会话，看的是身份。两件事分开，见 `stops_with_gate`。
    let gated = target.gated();
    let stops_with_gate = target.stops_with_gate(kind);
    if gated {
        crate::gate::open_authorized_with_mode(&holder, Some(target.watch_mode()), gate_state)
            .await?;
    }
    let result = crate::sessions::start(
        LaunchContext {
            client,
            identity_kind: kind,
            identity_id: id.into(),
            config_dir,
            working_dir,
        },
        &exe,
        env,
        stops_with_gate,
        revision,
    );
    match result {
        Ok(s) => {
            if gated {
                let mut lease = gate_state.lease.lock().unwrap();
                lease.grant_with_mode(&s.id, Vec::new(), Some(target.watch_mode()));
                lease.release_holder(&holder);
                crate::gate::lease::persist(&lease);
            }
            Ok(s)
        }
        Err(e) => {
            if gated {
                crate::gate::release_holder(gate_state, &holder)?;
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_migration_requires_real_api_residue_and_never_copies_oauth() {
        let root = std::env::temp_dir().join(config_io::id());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.toml"), "model='official-model'\n").unwrap();
        std::fs::write(
            root.join("auth.json"),
            br#"{"tokens":{"access_token":"synthetic-oauth"}}"#,
        )
        .unwrap();
        assert!(official_migration(Client::Codex, &root).is_err());
        std::fs::write(
            root.join("settings.json"),
            br#"{"env":{"OTHER_SETTING":"keep"}}"#,
        )
        .unwrap();
        assert!(official_migration(Client::ClaudeCode, &root).is_err());
        std::fs::write(root.join("settings.json"),br#"{"env":{"ANTHROPIC_BASE_URL":"https://example.invalid/v1","ANTHROPIC_API_KEY":"synthetic-key","OTHER_SETTING":"keep"}}"#).unwrap();
        let migration = official_migration(Client::ClaudeCode, &root).unwrap();
        assert_eq!(migration.key.as_deref(), Some("synthetic-key"));
        let after = config_io::parse_object(migration.edits[0].body.as_deref()).unwrap();
        assert_eq!(after["env"]["OTHER_SETTING"], "keep");
        assert!(after["env"].get("ANTHROPIC_API_KEY").is_none());
        assert!(std::fs::read_to_string(root.join("auth.json"))
            .unwrap()
            .contains("synthetic-oauth"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn repeated_environment_rollback_tracks_applied_configuration_not_unsaved_draft() {
        let root = std::env::temp_dir().join(config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let provider = Provider {
            id: "provider".into(),
            name: "Fixture".into(),
            base_url: "https://example.invalid".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
        };
        db.put("providers", &provider.id, &provider).unwrap();
        let mut environment = Environment {
            id: "environment".into(),
            name: "Fixture".into(),
            client: Client::Codex,
            provider_id: provider.id,
            credential_id: None,
            model: "model-a".into(),
            small_model: String::new(),
            wire_api: "responses".into(),
            auth_style: "none".into(),
            revision: 1,
            applied_revision: Some(1),
            config_dir: String::new(),
            config_state: "applied".into(),
        };
        db.set_environment(&environment).unwrap();
        let first = serde_json::to_string(&environment).unwrap();
        let path = crate::config_io::environment_dir(&root, &environment.id)
            .unwrap()
            .join("config.toml");
        config_io::replace(&path, Some(b"model='model-a'\n")).unwrap();
        environment.model = "model-b".into();
        environment.revision = 2;
        environment.applied_revision = Some(2);
        db.set_environment(&environment).unwrap();
        let second = serde_json::to_string(&environment).unwrap();
        crate::repository::commit_database(
            &db,
            vec![Edit {
                path: path.clone(),
                expected: Some(b"model='model-a'\n".to_vec()),
                body: Some(b"model='model-b'\n".to_vec()),
            }],
            |operation| {
                db.set_meta("applied-environment:environment", &second)?;
                db.set_meta(
                    "environment-undo:environment",
                    &serde_json::json!({"operation":operation,"previous":first}).to_string(),
                )
            },
        )
        .unwrap();
        environment.model = "unsaved-model-c".into();
        environment.revision = 3;
        db.set_environment(&environment).unwrap();
        assert_eq!(
            rollback_environment_in(&db, "environment").unwrap().model,
            "model-a"
        );
        assert_eq!(
            rollback_environment_in(&db, "environment").unwrap().model,
            "model-b"
        );
        assert!(std::fs::read_to_string(&path).unwrap().contains("model-b"));
        assert_eq!(
            rollback_environment_in(&db, "environment").unwrap().model,
            "model-a"
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn endpoints_do_not_repeat_protocol_prefix() {
        assert_eq!(
            endpoint("https://relay.test", "v1/messages").unwrap(),
            "https://relay.test/v1/messages"
        );
        assert_eq!(
            endpoint("https://relay.test/v1/", "v1/messages").unwrap(),
            "https://relay.test/v1/messages"
        );
        assert_eq!(
            endpoint("https://relay.test/prefix/v1", "models").unwrap(),
            "https://relay.test/prefix/v1/models"
        );
        assert!(endpoint("https://key@relay.test", "models").is_err());
    }
    #[test]
    fn environment_path_cannot_escape() {
        for id in ["..", "a/b", "C:\\x", ""] {
            assert!(crate::config_io::environment_dir(Path::new("root"), id).is_err());
        }
    }
}
