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
    crate::sessions::refresh()?;
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
    // 桌面端从 0.18.0 起可以有独立中转环境 —— 走的是它自己的
    // 「第三方网关」部署模式，见 [`desktop_gateway_json`]。
    // 0.17.0 之前这里直接回绝，因为 configuration_files 还不会写它那份配置。
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

/// 这个环境实际该用的 base_url。
///
/// 走本机路由的环境**不看 provider 上那个地址** —— 那个是站点的,
/// 而走路由的意义就是客户端不直接连站点。见 [`Environment::via_router`]。
///
/// ⛔ 端口写死成 [`local_router::DEFAULT_PORT`]:配置文件是落到磁盘上的,
/// 换个端口起路由会让已经写好的那份指向一个没人听的端口,而客户端报的是
/// 「连不上」,完全看不出是端口对不上。要支持自定义端口的话,
/// 端口得先变成环境自己的字段,再让这里和启动那一步都读它。
fn env_base(p: &Provider, e: &Environment) -> Result<String> {
    if e.via_router {
        let base =
            crate::local_router::client_base_url(e.client, crate::local_router::DEFAULT_PORT);
        return client_base(&base, e.client);
    }
    client_base(&p.base_url, e.client)
}

/// 走本机路由的那个环境的 id。**一个软件一个,固定。**
///
/// 固定 id 而不是每次新建:使用者点「启动」是想跑起来,不是想收藏一堆
/// 长得一样的环境。固定下来之后重复点只会复用同一份配置目录。
pub fn router_environment_id(client: Client) -> String {
    let c = match client {
        Client::ClaudeCode => "claude-code",
        Client::ClaudeDesktop => "claude-desktop",
        Client::Codex => "codex",
    };
    format!("qb-router-{c}")
}

/// 本机路由那个 provider 的 id。整机一个。
pub const ROUTER_PROVIDER_ID: &str = "qb-router";

/// 拿到(必要时建出)这个软件走本机路由的那个环境。
///
/// # 为什么是自动建而不是让使用者先去建一个
///
/// 这个环境里没有一个字段是使用者能自由选的:base_url 是路由的地址、
/// Key 由路由替换、协议由软件决定。让使用者先去「服务商与环境」那页
/// 手配一遍,配错了还起不来 —— 而正确答案只有一个。
///
/// 会保留的是**使用者后来改过的部分**:已经存在就只补齐那几个非填不可的
/// 字段(见下面那几行),名字、模型、工作目录一律不动。
pub fn router_environment(client: Client) -> Result<Environment> {
    let db = Repository::open()?;
    if db.get::<Provider>("providers", ROUTER_PROVIDER_ID).is_err() {
        db.put(
            "providers",
            ROUTER_PROVIDER_ID,
            &Provider {
                id: ROUTER_PROVIDER_ID.into(),
                name: "本机路由".into(),
                base_url: format!("http://127.0.0.1:{}", crate::local_router::DEFAULT_PORT),
                website: String::new(),
                note: "中转站页面自动建的。地址由本机路由决定，改这里不生效。".into(),
                tags: vec!["本机路由".into()],
                favorite: false,
                revision: 1,
            },
        )?;
    }
    let id = router_environment_id(client);
    let existing = db.get::<Environment>("environments", &id).ok();
    let mut e = existing.clone().unwrap_or(Environment {
        id: id.clone(),
        name: match client {
            Client::Codex => "Codex · 走本机路由",
            Client::ClaudeDesktop => "Claude 桌面端 · 走本机路由",
            Client::ClaudeCode => "Claude Code · 走本机路由",
        }
        .into(),
        client,
        provider_id: ROUTER_PROVIDER_ID.into(),
        credential_id: None,
        model: String::new(),
        small_model: String::new(),
        wire_api: if client == Client::Codex {
            "responses"
        } else {
            "chat"
        }
        .into(),
        auth_style: if client == Client::Codex {
            "env_key"
        } else {
            "bearer_token"
        }
        .into(),
        revision: 0,
        applied_revision: None,
        config_dir: String::new(),
        config_state: "saved".into(),
        via_router: true,
    });
    // 这几项是这个环境之所以成立的条件,每次都按回去 —— 使用者在别处
    // 把它们改歪了(比如把 provider 换成某个站点),下一次启动会打回原样,
    // 而不是带着一份指错地方的配置起客户端。
    e.id = id;
    e.client = client;
    e.provider_id = ROUTER_PROVIDER_ID.into();
    e.via_router = true;
    if existing
        .as_ref()
        .is_some_and(|old| serde_json::to_value(old).ok() == serde_json::to_value(&e).ok())
    {
        return Ok(e);
    }
    environment_save(e)
}

/// 这个客户端要的 base_url 长什么样。
///
/// | 客户端 | 带不带 `/v1` | 为什么 |
/// |---|---|---|
/// | Claude Code | **不带** | 它自己往后面接 `/v1/messages` |
/// | Claude 桌面端 | **不带** | 同上 —— 它把 `inferenceGatewayBaseUrl` 当前缀，自己接 `/v1/...` |
/// | Codex | 带 | 它按 OpenAI 那套，base 就到 `/v1` 为止 |
///
/// ⛔ 桌面端这一档 0.17.0 之前落在 Codex 那个分支上(多接了一个 `/v1`)。
/// 当时看不出症状,因为桌面端根本起不来;现在起得来了,多一段就是 404,
/// 而错误信息里完全看不出多了什么。
pub fn client_base(raw: &str, client: Client) -> Result<String> {
    let base = crate::endpoint::endpoint_base(raw)?;
    Ok(
        if matches!(client, Client::ClaudeCode | Client::ClaudeDesktop) {
            base.strip_suffix("/v1").unwrap_or(&base).to_string()
        } else if base.ends_with("/v1") {
            base
        } else {
            format!("{base}/v1")
        },
    )
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
        // 从官方配置迁上来的是直连那家站点的环境，不是走本机路由的。
        via_router: false,
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
    } else if e.via_router {
        // 路由会把客户端的鉴权头整个换掉,这里填什么都到不了站点 ——
        // 但不能留空,留空客户端会转去走官方 OAuth。见 ROUTER_KEY。
        Some(crate::local_router::ROUTER_KEY.to_string())
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
    m.base_url = env_base(&p, e)?;
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
            let existing = text
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| GateError::Other(format!("原配置 TOML 无效：{e}")))?;
            if e.via_router && e.model.trim().is_empty() {
                m.model = existing
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            let mut doc = crate::relay::codex_config_toml(&text, &m, None)
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| GateError::Other(e.to_string()))?;
            // 中转环境一律 API-Key 模式：占位 Key 由路由换成线路自己的。官方 turn-state
            // 识别**不再走环境目录**（0.24.7 起直接作用于账户槽位，见 `usecase::turnstate_ops`），
            // 所以这里没有「官方分支」。
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
            let old_auth = config_io::read_optional(&auth)?;
            if let Some(old) = old_auth.clone() {
                let value = config_io::object(
                    &String::from_utf8(old).map_err(|e| GateError::Other(e.to_string()))?,
                )?;
                if value.get("tokens").is_some_and(|tokens| !tokens.is_null()) {
                    return Err(GateError::Other(
                        "中转环境目录出现 OAuth 凭证，已阻断启动；请检查目录来源".into(),
                    ));
                }
            }
            if e.via_router {
                // Desktop account/read expects API login state. This is only the public
                // loopback placeholder; the actual provider secret stays in DPAPI.
                edits.push(Edit {
                    path: auth,
                    expected: old_auth,
                    body: Some(serde_json::to_vec_pretty(&serde_json::json!({
                        "auth_mode": "apikey", "OPENAI_API_KEY": crate::local_router::ROUTER_KEY
                    }))?),
                });
            }
        }
        Client::ClaudeDesktop => {
            let path = dir.join(DESKTOP_CONFIG_FILE);
            let old = config_io::read_optional(&path)?;
            let text = String::from_utf8(old.clone().unwrap_or_default())
                .map_err(|e| GateError::Other(e.to_string()))?;
            let body = desktop_gateway_json(&text, &m.base_url, key.as_deref())?;
            edits.push(Edit {
                path,
                expected: old,
                body: Some(serde_json::to_vec_pretty(&body)?),
            });
        }
    }
    Ok(edits)
}

/// 桌面端那份配置文件叫什么。
///
/// 跟它放 MCP 服务器的是**同一个文件** —— 所以只能并入，不能整份覆盖。
/// 覆盖掉的是使用者配了半天的 MCP，而且没有撤销。
pub const DESKTOP_CONFIG_FILE: &str = "claude_desktop_config.json";

/// 把桌面端切到「第三方网关」模式，指向 `base`。
///
/// # 这是桌面端自己的功能，不是我们发明的
///
/// `deploymentMode` / `inferenceProvider` / `inferenceGateway*` 这几个键
/// 都在桌面端自己的 `app.asar` 里（1.52386.6 上逐个核过）。它本来是给
/// 企业自建网关用的，这里借它把桌面端指到本机路由。
///
/// # ⛔ 只并入，不整份覆盖
///
/// 这个文件同时装着 `mcpServers` 和 `preferences` —— 整份写会把使用者
/// 配了半天的 MCP 抹掉，而且没有撤销。跟 `codex_auth_json` 那条是同一个教训。
///
/// # ⛔ 有两个键我们**故意不写**
///
/// | 键 | 别人写它干什么 | 为什么我们不写 |
/// |---|---|---|
/// | `disableDeploymentModeChooser` | 把「部署模式」那个选择器藏掉，防止使用者切回去 | 那是从使用者手里拿走一个开关。他想切回官方就该切得回去 |
/// | `coworkEgressAllowedHosts: ["*"]` | 放开 cowork 的出网白名单 | 这是**放宽一道安全限制**。中转站要的只是换个推理端点，不需要顺带把别的口子也打开 |
pub fn desktop_gateway_json(
    existing: &str,
    base: &str,
    key: Option<&str>,
) -> Result<serde_json::Value> {
    use serde_json::Value;
    // config_io::object 空文本回空对象、非对象直接报错 —— 两种都不用自己判。
    let mut root = config_io::object(existing)?;
    let body = root
        .as_object_mut()
        .ok_or_else(|| GateError::Other("桌面端配置结构非法".into()))?;
    body.insert("deploymentMode".into(), Value::String("3p".into()));
    body.insert("inferenceProvider".into(), Value::String("gateway".into()));
    body.insert(
        "inferenceGatewayBaseUrl".into(),
        Value::String(base.trim_end_matches('/').to_string()),
    );
    // ⛔ 认证方式写死 bearer。桌面端把这一格当成「怎么拼鉴权头」,
    // 而本机路由会把客户端带上来的头整个丢掉换成上游那把 —— 两边都不看它,
    // 但留空会让桌面端不知道该怎么拼，于是一个头都不发。
    body.insert(
        "inferenceGatewayAuthScheme".into(),
        Value::String("bearer".into()),
    );
    match key {
        Some(k) => body.insert("inferenceGatewayApiKey".into(), Value::String(k.into())),
        // 没有 Key 就把这一格删掉,**别留一个空串** —— 空串会让桌面端
        // 发一个 `Authorization: Bearer ` 的空头,上游回 401,
        // 而错误信息看起来像 Key 不对。
        None => body.remove("inferenceGatewayApiKey"),
    };
    Ok(root)
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
    apply_in(&db, id, fingerprint)
}

fn apply_in(db: &Repository, id: &str, fingerprint: Option<&str>) -> Result<Environment> {
    let mut e: Environment = db.get("environments", id)?;
    let edits = configuration_files(db, &e)?;
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
    crate::repository::commit_database(db, edits, |operation| {
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
        if ![
            "settings.json",
            "config.toml",
            DESKTOP_CONFIG_FILE,
            "auth.json",
        ]
        .contains(&name.as_str())
        {
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
        for key in ["environment-undo", "applied-environment"]
            .into_iter()
            .chain((kind == "providers").then_some("station-billing"))
        {
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

/// 以中转打开桌面端之前，把正在跑的桌面端关干净。返回关掉时它有几个进程。
///
/// 桌面端是 Electron 单实例：已经在跑时再起一遍，只会把旧窗口拉到前面，
/// 新给的配置目录一个都不生效。原来 [`launch`] 只拦下来报错、让使用者自己去
/// 托盘退出；使用者的原话是「图片内的动作自己不能做吗，非要用户手动」——
/// 所以中转站那个「启动」现在先调这里替他关。
///
/// 三步，顺序有讲究：
/// 1. **面板自己起的桌面端会话按会话停**（Job Object，PID + 创建时间核验），
///    并交回它们持的租约 —— 只杀进程不交租约，租约会一直显示「有人持有」；
/// 2. 其余的（从开始菜单、托盘起的）走 [`crate::killswitch::execute_desktop`]：
///    跟一键关闭同一套证据，面板自己的祖先链一律放过，终端里的 Claude Code 不碰；
/// 3. 进程终止到真的从进程表里消失之间有延迟，**等它真的没了才返回**。
///
/// `before_closing` 在确实要关的时候调一次，调用方拿它把「正在关闭桌面端」
/// 写进操作进度 —— 没在跑就不调，界面上也就不会闪过这一句。
///
/// ⛔ 关不干净就返回 `Err`，调用方**不许接着启动**：起了也是走官方，
/// 而且没有任何报错，那正是单实例这件事最难查的地方。
pub async fn close_desktop_for_relay(
    gate_state: &crate::gate::GateState,
    before_closing: impl FnOnce(usize),
) -> Result<usize> {
    let running = crate::killswitch::desktop_processes()?;
    if running.is_empty() {
        return Ok(0);
    }
    before_closing(running.len());
    let stopped = crate::sessions::stop_matching(|s| s.context.client == Client::ClaudeDesktop)?;
    for id in &stopped {
        crate::gate::release_holder(gate_state, id)?;
    }
    let report = crate::killswitch::execute_desktop().await?;
    if !report.failed.is_empty() {
        return Err(GateError::Other(format!(
            "桌面端没关干净，没有启动：{}",
            report
                .failed
                .iter()
                .map(|(pid, why)| format!("PID {pid}：{why}"))
                .collect::<Vec<_>>()
                .join("；")
        )));
    }
    for _ in 0..8 {
        if crate::killswitch::desktop_processes()?.is_empty() {
            crate::audit::write(&format!(
                "以中转打开桌面端之前，先关掉了正在运行的桌面端（{} 个进程）",
                running.len()
            ));
            return Ok(running.len());
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    Err(GateError::Other(
        "桌面端没退干净，没有启动。可能是面板本身由桌面端拉起，或者读不出它的签名 —— \
         请从托盘完全退出桌面端后再点启动。"
            .into(),
    ))
}

/// Windows 的「拒绝访问」有三副面孔：本地化文案、英文文案、HRESULT。任一命中都算。
fn looks_like_access_denied(msg: &str) -> bool {
    msg.contains("0x80070005") || msg.contains("Access is denied") || msg.contains("拒绝访问")
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
    // ⛔ 桌面端是 Electron 单实例：已经在跑的时候再起一遍，只会把旧窗口
    // 拉到前面，**新给的环境变量一个都不生效**。不拦的话症状是
    // 「点了启动，窗口是弹出来了，可它走的还是官方」—— 没有任何报错，
    // 而这正是最难查的那一类。
    if client == Client::ClaudeDesktop && kind == IdentityKind::Relay {
        let running = crate::killswitch::desktop_processes().unwrap_or_default();
        if !running.is_empty() {
            // 中转站页面的「启动」会先走 `close_desktop_for_relay` 替使用者关掉；
            // 走到这里的是别的入口，或者那一步没关干净 —— 这道拦截仍然要留。
            //
            // （这句原来行尾写成了 `\",`：续行符后面多了一对引号和逗号，
            // 界面上的报错里会夹着 `",` 和一串缩进空格。）
            return Err(GateError::Other(
                "Claude 桌面端正在运行。它是单实例的，不退干净就换不了配置目录 —— \
                 请先从托盘完全退出桌面端再启动；中转站页面的启动按钮会自动替你关掉。"
                    .into(),
            ));
        }
    }
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
        let state = configuration_state(&db, &stored)?;
        if state == "external" && !stored.via_router {
            return Err(GateError::Other(
                "配置已被外部修改，请预览差异并重新应用后启动".into(),
            ));
        }
        let e = if state == "applied" && !stored.via_router {
            stored
        } else {
            apply(id)?
        };
        let p: Provider = db.get("providers", &e.provider_id)?;
        let key = if e.auth_style == "none" {
            None
        } else if e.via_router {
            // 真正的 Key 由路由按当前上游换上去,子进程里这一把只是占位。
            Some(crate::local_router::ROUTER_KEY.to_string())
        } else {
            e.credential_id
                .as_deref()
                .map(|id| db.key(id))
                .transpose()?
        };
        // 三个软件各认各的「配置目录」环境变量。
        //
        // ⛔ 桌面端那一档 0.17.0 之前是 `vec![]`（一个都不给），注释说它
        // 不吃环境变量 —— **那是个没验过的假设**。实际读它的 `app.asar`：
        // 主进程启动第一件事就是 `if (process.env.CLAUDE_USER_DATA_DIR)
        // { app.setPath("userData", …) }`，而且优先级最高（它自己那套
        // 3p 目录重定位包在 `if (!process.env.CLAUDE_USER_DATA_DIR)` 里）。
        let mut env = vec![(
            match client {
                Client::Codex => "CODEX_HOME",
                Client::ClaudeDesktop => "CLAUDE_USER_DATA_DIR",
                Client::ClaudeCode => "CLAUDE_CONFIG_DIR",
            }
            .into(),
            e.config_dir.clone(),
        )];
        // 桌面端的端点**不走环境变量**，走它自己那份 claude_desktop_config.json
        // 里的第三方网关配置（见 [`desktop_gateway_json`]）。这里只负责把它
        // 指到我们那个目录，配置文件由 `configuration_files` 写好了。
        if client == Client::ClaudeCode {
            env.push(("ANTHROPIC_BASE_URL".into(), env_base(&p, &e)?));
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
    if client == Client::Codex {
        // The desktop app reads CODEX_HOME reliably with an explicit Electron profile.
        // A separate userData directory also keeps the user's existing desktop task alive.
        env.push((
            "CODEX_ELECTRON_USER_DATA_PATH".into(),
            Path::new(&config_dir).join("desktop").display().to_string(),
        ));
    }
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
    // Store 版 Codex 更新后注册失效时，CreateProcessW 会回「拒绝访问 (0x80070005)」——
    // 跟账户页那条 os error 5 是同一件事，同样给可操作的说明，别让人在这里看到裸错误。
    let result = match result {
        Err(e) if client == Client::Codex && looks_like_access_denied(&e.to_string()) => Err(
            GateError::Other(qb_install::install::codex_desktop::registration_repair_message()),
        ),
        other => other,
    };
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

    fn env(client: Client, via_router: bool) -> Environment {
        Environment {
            id: "e".into(),
            name: "e".into(),
            client,
            provider_id: "p".into(),
            credential_id: None,
            model: String::new(),
            small_model: String::new(),
            wire_api: "responses".into(),
            auth_style: "bearer_token".into(),
            revision: 1,
            applied_revision: None,
            config_dir: String::new(),
            config_state: "saved".into(),
            via_router,
        }
    }

    fn provider(base: &str) -> Provider {
        Provider {
            id: "p".into(),
            name: "p".into(),
            base_url: base.into(),
            website: String::new(),
            note: String::new(),
            tags: Vec::new(),
            favorite: false,
            revision: 1,
        }
    }

    /// 桌面端换的是**整个数据目录**，不是 base_url 那个变量。
    ///
    /// ⛔ 0.17.0 之前这一档给的是 `vec![]`，注释说桌面端不吃环境变量 ——
    /// 没验过。它的 `app.asar` 里主进程第一件事就是读 `CLAUDE_USER_DATA_DIR`，
    /// 而且优先级最高。给不上的话，桌面端会读官方那份数据目录，
    /// 于是「点了启动、窗口也起来了，可它走的还是官方」。
    #[test]
    fn the_desktop_gets_its_own_user_data_dir_rather_than_a_base_url() {
        // 三个软件各认各的键，一个都不许串。
        let keys = |c: Client| -> &'static str {
            match c {
                Client::Codex => "CODEX_HOME",
                Client::ClaudeDesktop => "CLAUDE_USER_DATA_DIR",
                Client::ClaudeCode => "CLAUDE_CONFIG_DIR",
            }
        };
        assert_eq!(keys(Client::ClaudeDesktop), "CLAUDE_USER_DATA_DIR");
        assert_ne!(keys(Client::ClaudeDesktop), keys(Client::ClaudeCode));
        assert_ne!(keys(Client::ClaudeDesktop), keys(Client::Codex));
    }

    /// ⛔ 桌面端的 base 不许带 `/v1`。
    ///
    /// 它把 `inferenceGatewayBaseUrl` 当前缀，自己往后接 `/v1/...`。
    /// 多一段就是 404，而错误信息里完全看不出多了什么。
    #[test]
    fn the_desktop_base_url_carries_no_v1_suffix() {
        let got = client_base("https://relay.example.com/v1", Client::ClaudeDesktop).unwrap();
        assert!(!got.ends_with("/v1"), "{got}");
        // 跟 Claude Code 是同一档；Codex 才带。
        assert_eq!(
            got,
            client_base("https://relay.example.com/v1", Client::ClaudeCode).unwrap()
        );
        assert!(client_base("https://relay.example.com", Client::Codex)
            .unwrap()
            .ends_with("/v1"));
    }

    /// 桌面端那份配置**只并入，不整份覆盖**。
    ///
    /// 它跟 MCP 服务器共用同一个文件 —— 整份写会把使用者配了半天的 MCP
    /// 抹掉，而且没有撤销。
    #[test]
    fn writing_the_gateway_config_keeps_the_mcp_servers_that_were_already_there() {
        let existing = r#"{"mcpServers":{"fs":{"command":"npx"}},"preferences":{"theme":"dark"}}"#;
        let out = desktop_gateway_json(existing, "http://127.0.0.1:15721/cd", Some("k")).unwrap();
        assert!(out.get("mcpServers").is_some(), "MCP 被覆盖掉了：{out}");
        assert!(out.get("preferences").is_some(), "偏好被覆盖掉了：{out}");
        assert_eq!(out["deploymentMode"], "3p");
        assert_eq!(out["inferenceProvider"], "gateway");
        assert_eq!(out["inferenceGatewayBaseUrl"], "http://127.0.0.1:15721/cd");
        assert_eq!(out["inferenceGatewayApiKey"], "k");
    }

    /// ⛔ 没有 Key 时把那一格**删掉**，不留空串。
    ///
    /// 空串会让桌面端发一个 `Authorization: Bearer ` 的空头，上游回 401，
    /// 而错误信息看起来像 Key 填错了 —— 查错方向整个偏掉。
    #[test]
    fn an_absent_key_removes_the_field_instead_of_writing_an_empty_string() {
        let had = r#"{"inferenceGatewayApiKey":"old"}"#;
        let out = desktop_gateway_json(had, "http://127.0.0.1:15721/cd", None).unwrap();
        assert!(out.get("inferenceGatewayApiKey").is_none(), "{out}");
    }

    /// ⛔ 两个键我们故意不写，别让它们悄悄长回来。
    ///
    /// `disableDeploymentModeChooser` 是从使用者手里拿走一个开关；
    /// `coworkEgressAllowedHosts: ["*"]` 是放宽一道安全限制。
    /// 中转站要的只是换个推理端点，不需要顺带把别的口子也打开。
    #[test]
    fn the_gateway_config_never_widens_anything_beyond_the_endpoint() {
        let out = desktop_gateway_json("{}", "http://127.0.0.1:15721/cd", Some("k")).unwrap();
        assert!(out.get("disableDeploymentModeChooser").is_none(), "{out}");
        assert!(out.get("coworkEgressAllowedHosts").is_none(), "{out}");
    }

    /// 地址末尾的斜杠要去掉 —— 桌面端自己往后接 `/v1/...`，
    /// 留着会拼出 `//v1/messages`。
    #[test]
    fn a_trailing_slash_is_trimmed_before_it_becomes_a_double_slash() {
        let out = desktop_gateway_json("{}", "http://127.0.0.1:15721/cd/", Some("k")).unwrap();
        assert_eq!(out["inferenceGatewayBaseUrl"], "http://127.0.0.1:15721/cd");
    }

    /// 走本机路由的环境**不看 provider 上那个地址**。
    ///
    /// 看了的话，客户端会绕过路由直连站点：换上游不生效、日志里一条都不记、
    /// 熔断也永远不触发 —— 而每一发请求都成功，界面上完全正常。
    #[test]
    fn a_router_environment_points_at_the_loopback_not_at_the_station() {
        let p = provider("https://relay.example.com/v1");
        for client in [Client::ClaudeCode, Client::Codex] {
            let direct = env_base(&p, &env(client, false)).unwrap();
            let routed = env_base(&p, &env(client, true)).unwrap();
            assert!(
                direct.starts_with("https://relay.example.com"),
                "直连那条不该被改：{direct}"
            );
            assert!(
                routed.starts_with("http://127.0.0.1:"),
                "走路由那条还指着站点：{routed}"
            );
        }
    }

    /// Codex 要 `/v1`，Claude Code 不要 —— 走路由时这条规矩照旧。
    ///
    /// 两边都按自己那套拼，否则 Codex 打到 `127.0.0.1:15721/responses`
    /// （少一段 `/v1`），而 `route_of` 认的是路径尾巴，照样会认成 Codex、
    /// 照样转发出去，上游回 404 —— 错误信息里看不出少了一段。
    #[test]
    fn the_v1_suffix_follows_the_client_even_on_the_loopback() {
        let p = provider("https://relay.example.com/v1");
        let code = env_base(&p, &env(Client::ClaudeCode, true)).unwrap();
        let codex = env_base(&p, &env(Client::Codex, true)).unwrap();
        assert!(!code.ends_with("/v1"), "Claude Code 不该带 /v1：{code}");
        assert!(codex.ends_with("/v1"), "Codex 必须带 /v1：{codex}");
    }

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
            via_router: false,
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
    fn router_reapply_preserves_client_changes_and_accepts_all_three_config_files() {
        let root = std::env::temp_dir().join(config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let provider = Provider {
            id: "p".into(),
            name: "Fixture".into(),
            base_url: "https://unused.invalid".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
        };
        db.put("providers", "p", &provider).unwrap();
        for client in [Client::ClaudeCode, Client::ClaudeDesktop, Client::Codex] {
            let id = router_environment_id(client);
            let dir = config_io::environment_dir(&root, &id).unwrap();
            let env = Environment {
                id: id.clone(),
                name: "Fixture".into(),
                client,
                provider_id: "p".into(),
                credential_id: None,
                model: String::new(),
                small_model: String::new(),
                wire_api: "responses".into(),
                auth_style: "env_key".into(),
                revision: 1,
                applied_revision: None,
                config_dir: dir.display().to_string(),
                config_state: "saved".into(),
                via_router: true,
            };
            db.set_environment(&env).unwrap();
            let applied = apply_in(&db, &id, None).unwrap();
            assert_eq!(configuration_state(&db, &applied).unwrap(), "applied");
            let name = match client {
                Client::ClaudeCode => "settings.json",
                Client::ClaudeDesktop => DESKTOP_CONFIG_FILE,
                Client::Codex => "config.toml",
            };
            let path = dir.join(name);
            let original = std::fs::read_to_string(&path).unwrap();
            let edited = if client == Client::Codex {
                format!("{original}\n[projects.\"C:/work\"]\ntrust_level = \"trusted\"\n")
            } else {
                let mut value: serde_json::Value = serde_json::from_str(&original).unwrap();
                value["preferences"] = serde_json::json!({"keep_me": true});
                value.to_string()
            };
            std::fs::write(&path, edited).unwrap();
            assert_eq!(configuration_state(&db, &applied).unwrap(), "external");
            let reapplied = apply_in(&db, &id, None).unwrap();
            assert_eq!(configuration_state(&db, &reapplied).unwrap(), "applied");
            let after = std::fs::read_to_string(&path).unwrap();
            assert!(after.contains(if client == Client::Codex {
                "trust_level"
            } else {
                "keep_me"
            }));
            assert!(after.contains("127.0.0.1"));
            if client == Client::Codex {
                let auth = dir.join("auth.json");
                let saved = std::fs::read_to_string(&auth).unwrap();
                assert!(saved.contains(crate::local_router::ROUTER_KEY));
                std::fs::write(&auth, r#"{"tokens":{"access_token":"fixture-oauth"}}"#).unwrap();
                assert!(apply_in(&db, &id, None)
                    .unwrap_err()
                    .to_string()
                    .contains("OAuth"));
                assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
            }
        }
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
