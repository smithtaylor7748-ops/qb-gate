//! Internal metadata only. Native OAuth files are never imported into this database.
use crate::{
    domain::*,
    error::{GateError, Result},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};

/// SQLite 的错误进 `GateError`。
///
/// **分到 `Database` 而不是 `Other`**：这一层压平之后，调用方就分不出
/// 「这条记录已经有了」（UNIQUE 冲突）、「还有别的东西在引用它」（外键
/// RESTRICT）和「磁盘满了」—— 而这三件事该给使用者看的话完全不同。
/// 原始 SQLite 文本原样带在 `message` 里，排障时还查得到。
fn sql(e: rusqlite::Error) -> GateError {
    GateError::Database(e.to_string())
}
pub struct Repository {
    pub conn: Connection,
    pub root: PathBuf,
}

#[derive(Serialize, serde::Deserialize)]
pub struct DataBackup {
    pub schema: u32,
    pub providers: Vec<Provider>,
    pub credentials: Vec<(Credential, String)>,
    pub environments: Vec<(Environment, String)>,
    pub installations: Vec<ExtensionInstallation>,
    pub catalog: Vec<ExtensionManifest>,
    pub extension_config: Vec<(String, String)>,
    #[serde(default)]
    pub plans: Vec<crate::domain::LaunchPlan>,
}
impl Repository {
    pub fn open() -> Result<Self> {
        Self::open_in(&crate::paths::state_dir())
    }
    pub fn open_in(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let conn = Connection::open(root.join("workspace.sqlite3")).map_err(sql)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql)?;
        conn.execute_batch("PRAGMA foreign_keys=ON;").map_err(sql)?;
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(sql)?;
        if version > 1 {
            return Err(GateError::Other(
                "此数据库由更新版本创建，请使用相应版本；数据未修改".into(),
            ));
        }
        if version == 0 {
            conn.execute_batch("PRAGMA journal_mode=WAL; BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS providers(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS credentials(id TEXT PRIMARY KEY,provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,body TEXT NOT NULL,sealed TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS environments(id TEXT PRIMARY KEY,provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,credential_id TEXT REFERENCES credentials(id) ON DELETE RESTRICT,body TEXT NOT NULL,hashes TEXT NOT NULL DEFAULT '{}');
            CREATE TABLE IF NOT EXISTS installations(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS catalog(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS diagnostics(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            PRAGMA user_version=1; COMMIT;").map_err(sql)?;
        }
        Ok(Self {
            conn,
            root: root.into(),
        })
    }
    fn table(table: &str) -> Result<&str> {
        if [
            "providers",
            "credentials",
            "environments",
            "installations",
            "catalog",
            "operations",
            "sessions",
            "diagnostics",
        ]
        .contains(&table)
        {
            Ok(table)
        } else {
            Err(GateError::Other("未知的数据表".into()))
        }
    }
    pub fn list<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let query = format!(
            "SELECT body FROM {} ORDER BY rowid DESC",
            Self::table(table)?
        );
        let mut stmt = self.conn.prepare(&query).map_err(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(sql)?;
        rows.map(|r| Ok(serde_json::from_str(&r.map_err(sql)?)?))
            .collect()
    }
    pub fn get<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<T> {
        let query = format!("SELECT body FROM {} WHERE id=?1", Self::table(table)?);
        let body: Option<String> = self
            .conn
            .query_row(&query, [id], |r| r.get(0))
            .optional()
            .map_err(sql)?;
        serde_json::from_str(&body.ok_or_else(|| GateError::Other(format!("找不到记录 {id}")))?)
            .map_err(Into::into)
    }
    pub fn put<T: Serialize>(&self, table: &str, id: &str, body: &T) -> Result<()> {
        if ["credentials", "environments"].contains(&table) {
            return Err(GateError::Other("此数据需要引用关系校验".into()));
        }
        let query=format!("INSERT INTO {}(id,body) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",Self::table(table)?);
        self.conn
            .execute(&query, params![id, serde_json::to_string(body)?])
            .map_err(sql)?;
        Ok(())
    }
    pub fn remove(&self, table: &str, id: &str) -> Result<()> {
        self.conn
            .execute(
                &format!("DELETE FROM {} WHERE id=?1", Self::table(table)?),
                [id],
            )
            .map_err(sql)?;
        Ok(())
    }
    /// 按写入顺序只保留最新的 `keep` 条，其余删掉；`protect` 判真的一条都不动，
    /// 也不占 `keep` 的额度。返回删了几条。
    ///
    /// # 为什么必须有这个
    ///
    /// `operations` / `sessions` / `diagnostics` 三张表原来**只写不删**，而
    /// `workspace::snapshot` 每次都把 sessions + operations 整个读出来回前端，
    /// 前端又是每做一个动作就 `invalidateAll()`。于是「改个服务商名字点保存」
    /// 这个动作，要把全部历史序列化、过 IPC、再 JSON.parse 一遍。
    /// 刚装上没感觉，用半年之后每次保存都要卡一下 —— 而且只会越来越糟。
    ///
    /// 诊断记录尤其肥：一条里带着完整模型列表和请求导出，单条能到十几 KB。
    pub fn prune(
        &self,
        table: &str,
        keep: usize,
        protect: impl Fn(&serde_json::Value) -> bool,
    ) -> Result<usize> {
        let query = format!(
            "SELECT id,body FROM {} ORDER BY rowid DESC",
            Self::table(table)?
        );
        let rows: Vec<(String, String)> = self
            .conn
            .prepare(&query)
            .map_err(sql)?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql)?;
        let (mut kept, mut removed) = (0usize, 0usize);
        for (id, body) in rows {
            if protect(&serde_json::from_str(&body).unwrap_or_default()) {
                continue;
            }
            if kept < keep {
                kept += 1;
                continue;
            }
            self.remove(table, &id)?;
            removed += 1;
        }
        Ok(removed)
    }

    /// 三张只增不减的表统一收一次。启动时和每次写入之后都会调。
    ///
    /// 正在跑（或还没核验）的会话**永远不删** —— `sessions::recover`
    /// 与 `ensure_verified` 都靠这些行认人，删了等于把进程弄丢。
    pub fn prune_history(&self) -> Result<()> {
        self.prune("operations", 200, |_| false)?;
        self.prune("diagnostics", 50, |_| false)?;
        self.prune("sessions", 100, |s| {
            matches!(s["state"].as_str(), Some("running" | "unverified"))
        })?;
        Ok(())
    }

    pub fn set_credential(&self, c: &Credential, sealed: &str) -> Result<()> {
        self.conn.execute("INSERT INTO credentials(id,provider_id,body,sealed) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET provider_id=excluded.provider_id,body=excluded.body,sealed=excluded.sealed",params![c.id,c.provider_id,serde_json::to_string(c)?,sealed]).map_err(sql)?;
        Ok(())
    }
    pub fn sealed(&self, id: &str) -> Result<String> {
        self.conn
            .query_row("SELECT sealed FROM credentials WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(sql)
    }
    pub fn key(&self, id: &str) -> Result<String> {
        crate::secret::open(&self.sealed(id)?)
            .ok_or_else(|| GateError::Other("此凭证无法解密，请重新填写，原密文已保留".into()))
    }
    pub fn set_environment(&self, e: &Environment) -> Result<()> {
        if let Some(id) = &e.credential_id {
            let c: Credential = self.get("credentials", id)?;
            if c.provider_id != e.provider_id {
                return Err(GateError::Other("凭证不属于所选服务商".into()));
            }
        }
        self.conn.execute("INSERT INTO environments(id,provider_id,credential_id,body) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET provider_id=excluded.provider_id,credential_id=excluded.credential_id,body=excluded.body",params![e.id,e.provider_id,e.credential_id,serde_json::to_string(e)?]).map_err(sql)?;
        Ok(())
    }
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sql)
    }
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(sql)?;
        Ok(())
    }

    pub fn backup_data(&self) -> Result<DataBackup> {
        let mut stmt = self
            .conn
            .prepare("SELECT key,value FROM metadata WHERE key LIKE 'extension-config:%'")
            .map_err(sql)?;
        let extension_config = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        Ok(DataBackup {
            schema: 1,
            providers: self.list("providers")?,
            credentials: self
                .list::<Credential>("credentials")?
                .into_iter()
                .map(|c| self.sealed(&c.id).map(|sealed| (c, sealed)))
                .collect::<Result<_>>()?,
            environments: self
                .list::<Environment>("environments")?
                .into_iter()
                .map(|e| {
                    let hashes = self
                        .conn
                        .query_row(
                            "SELECT hashes FROM environments WHERE id=?1",
                            [&e.id],
                            |r| r.get(0),
                        )
                        .map_err(sql)?;
                    Ok((e, hashes))
                })
                .collect::<Result<_>>()?,
            installations: self.list("installations")?,
            catalog: self.list("catalog")?,
            extension_config,
            plans: self.plans()?,
        })
    }
    /// Caller commits this together with external configuration edits.
    pub fn restore_data(&self, data: &DataBackup) -> Result<()> {
        if data.schema != 1 {
            return Err(GateError::Other("数据库备份版本不支持".into()));
        }
        for table in [
            "environments",
            "credentials",
            "providers",
            "installations",
            "catalog",
        ] {
            self.conn
                .execute(&format!("DELETE FROM {table}"), [])
                .map_err(sql)?;
        }
        for p in &data.providers {
            crate::endpoint::endpoint_base(&p.base_url)?;
            self.put("providers", &p.id, p)?;
        }
        for (c, sealed) in &data.credentials {
            self.set_credential(c, sealed)?;
        }
        for (e, hashes) in &data.environments {
            let mut e = e.clone();
            e.config_dir = crate::config_io::environment_dir(&self.root, &e.id)?
                .display()
                .to_string();
            self.set_environment(&e)?;
            let _: std::collections::BTreeMap<String, String> = serde_json::from_str(hashes)?;
            self.conn
                .execute(
                    "UPDATE environments SET hashes=?1 WHERE id=?2",
                    params![hashes, e.id],
                )
                .map_err(sql)?;
        }
        for i in &data.installations {
            self.put("installations", &i.id, i)?;
        }
        for m in &data.catalog {
            self.put("catalog", &m.id, m)?;
        }
        self.conn.execute("DELETE FROM metadata WHERE key LIKE 'extension-config:%' OR key LIKE 'environment-undo:%' OR key LIKE 'applied-environment:%'",[]).map_err(sql)?;
        for (k, v) in &data.extension_config {
            if !k.starts_with("extension-config:") {
                return Err(GateError::Other("备份元数据键无效".into()));
            }
            self.set_meta(k, v)?;
        }
        self.set_meta("launch-plans", &serde_json::to_string(&data.plans)?)?;
        Ok(())
    }
    /// 仍然被某个环境的「回滚配置」指着的恢复记录 ID。
    ///
    /// 这些 journal 一条都不能按保留期删掉 —— 删了「回滚配置」按钮就会
    /// 报文件不存在，而使用者完全看不出是保留期干的。
    pub fn referenced_operations(&self) -> Result<std::collections::BTreeSet<String>> {
        let rows: Vec<String> = self
            .conn
            .prepare("SELECT value FROM metadata WHERE key LIKE 'environment-undo:%'")
            .map_err(sql)?
            .query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql)?;
        Ok(rows
            .iter()
            .filter_map(|v| serde_json::from_str::<serde_json::Value>(v).ok())
            .filter_map(|v| v["operation"].as_str().map(String::from))
            .collect())
    }

    pub fn plans(&self) -> Result<Vec<crate::domain::LaunchPlan>> {
        self.meta("launch-plans")?
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .unwrap_or(Ok(vec![]))
    }
    pub fn transaction<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        let value = action()?;
        tx.commit().map_err(sql)?;
        Ok(value)
    }

    pub fn migrate(&mut self) -> Result<Vec<String>> {
        if let Some(notes) = self.meta("legacy-v1")? {
            return Ok(serde_json::from_str(&notes)?);
        }
        let legacy = self.root.join("relay.json");
        let text = crate::config_io::read_text(&legacy)?;
        let old: LegacyRelayStore = if text.is_empty() {
            Default::default()
        } else {
            serde_json::from_str(&text)?
        };
        let backup = self
            .root
            .join("migration-backups")
            .join(crate::config_io::id());
        std::fs::create_dir_all(&backup)?;
        for name in ["relay.json", "profiles.json", "settings.json", "lease.json"] {
            let p = self.root.join(name);
            if let Some(bytes) = crate::config_io::read_optional(&p)? {
                std::fs::write(backup.join(name), bytes)?;
            }
        }
        self.conn.execute_batch("BEGIN IMMEDIATE").map_err(sql)?;
        let result = (|| -> Result<Vec<String>> {
            let mut notes = vec![format!(
                "旧配置备份：{}；官方登录目录未复制",
                backup.display()
            )];
            let mut providers = std::collections::BTreeMap::<String, String>::new();
            let mut environments = std::collections::BTreeMap::new();
            for p in old.providers {
                let provider_id = if let Some(id) = providers.get(&p.meta.base_url) {
                    id.clone()
                } else {
                    let id = crate::config_io::id();
                    self.put(
                        "providers",
                        &id,
                        &Provider {
                            id: id.clone(),
                            name: p.meta.name.clone(),
                            base_url: p.meta.base_url.clone(),
                            website: p.meta.website.clone().unwrap_or_default(),
                            note: p.meta.note.clone().unwrap_or_default(),
                            tags: vec!["旧版导入".into()],
                            favorite: false,
                            revision: 1,
                        },
                    )?;
                    providers.insert(p.meta.base_url.clone(), id.clone());
                    id
                };
                let credential_id = if p.key_sealed.is_empty() {
                    None
                } else {
                    let id = crate::config_io::id();
                    let plain = crate::secret::open(&p.key_sealed);
                    let sealed = match plain.as_deref() {
                        Some(k) => crate::secret::seal(k)?,
                        None => p.key_sealed.clone(),
                    };
                    self.set_credential(
                        &Credential {
                            id: id.clone(),
                            provider_id: provider_id.clone(),
                            label: format!("{} · 导入凭证", p.meta.name),
                            masked: plain
                                .as_deref()
                                .map(mask_legacy_key)
                                .unwrap_or_else(|| "需要重新填写".into()),
                            available: plain.is_some(),
                            revision: 1,
                        },
                        &sealed,
                    )?;
                    Some(id)
                };
                if p.meta.target == "claude-desktop" {
                    notes.push(format!(
                        "{}：桌面端中转保留为服务记录，不写入 Code 配置",
                        p.meta.name
                    ));
                    continue;
                }
                let id = crate::config_io::id();
                environments.insert(p.meta.id.clone(), id.clone());
                self.set_environment(&Environment {
                    id: id.clone(),
                    name: p.meta.name,
                    client: if p.meta.target == "codex" {
                        Client::Codex
                    } else {
                        Client::ClaudeCode
                    },
                    provider_id,
                    credential_id,
                    model: p.meta.model.unwrap_or_default(),
                    small_model: p.meta.small_fast_model.unwrap_or_default(),
                    wire_api: p.meta.wire_api,
                    auth_style: p.meta.auth_style,
                    revision: 1,
                    applied_revision: None,
                    config_dir: self
                        .root
                        .join("environments")
                        .join(&id)
                        .display()
                        .to_string(),
                    config_state: "saved".into(),
                })?;
            }
            self.set_meta(
                "legacy-environment-map",
                &serde_json::to_string(&environments)?,
            )?;
            let profiles_text = crate::config_io::read_text(&self.root.join("profiles.json"))?;
            let old_profiles: LegacyProfiles = if profiles_text.is_empty() {
                Default::default()
            } else {
                serde_json::from_str(&profiles_text)?
            };
            let mut plans = Vec::new();
            for p in old_profiles.profiles {
                if let Some(account) = p.account {
                    plans.push(crate::domain::LaunchPlan {
                        id: crate::config_io::id(),
                        name: format!("{} · 官方", p.name),
                        client: Client::ClaudeCode,
                        identity_kind: IdentityKind::Official,
                        identity_id: account,
                        working_dir: String::new(),
                    });
                }
                for (target, provider) in p.relays {
                    if let Some(id) = environments.get(&provider) {
                        plans.push(crate::domain::LaunchPlan {
                            id: crate::config_io::id(),
                            name: format!("{} · {}", p.name, target),
                            client: if target == "codex" {
                                Client::Codex
                            } else {
                                Client::ClaudeCode
                            },
                            identity_kind: IdentityKind::Relay,
                            identity_id: id.clone(),
                            working_dir: String::new(),
                        });
                    } else {
                        notes.push(format!(
                            "{} 的 {} 引用无法迁移，请重新选择环境",
                            p.name, target
                        ));
                    }
                }
                if p.timezone.is_some() {
                    notes.push(format!(
                        "{} 的系统时区记录已保留在旧备份，启动方案不会修改系统时区",
                        p.name
                    ));
                }
            }
            self.set_meta("launch-plans", &serde_json::to_string(&plans)?)?;
            notes.push(format!(
                "旧档案已拆分为 {} 个独立启动方案。官方目录中的未知残留不会自动删除。",
                plans.len()
            ));
            self.set_meta("legacy-v1", &serde_json::to_string(&notes)?)?;
            Ok(notes)
        })();
        match result {
            Ok(notes) => {
                self.conn.execute_batch("COMMIT").map_err(sql)?;
                Ok(notes)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}

/// `relay.json` 的**历史**形状。只给 [`Repository::migrate`] 用。
///
/// 跟 [`LegacyProfiles`] 同一个理由：迁移读的是一份已经躺在磁盘上的旧文件，
/// 形状属于 v1 那一刻。复用 `relay::RelayStore` 的话，中转站模块以后改一个
/// 字段（它是活的，中转站重做时必然会改），老用户的 `relay.json` 就会在迁移
/// 那一刻反序列化失败 —— 而他正在升级途中。
///
/// 顺带断掉 `repository → relay` 这条边：数据库这一层不该认识中转站。
///
/// ⚠ 枚举在这里退化成 `String`。**这是故意的** —— 历史文件里可能存着这个
/// 版本还不认识的值（手改过、或者来自更老的版本），解析成枚举会整份失败，
/// 而存成字符串最多是某一条迁移不过去，剩下的照样进得来。
#[derive(serde::Deserialize, Default)]
struct LegacyRelayStore {
    #[serde(default)]
    providers: Vec<LegacyProvider>,
}

#[derive(serde::Deserialize)]
struct LegacyProvider {
    #[serde(flatten)]
    meta: LegacyProviderMeta,
    /// DPAPI 密文。空串 = 这条没配 Key。
    #[serde(default)]
    key_sealed: String,
}

#[derive(serde::Deserialize)]
struct LegacyProviderMeta {
    /// 旧库里的稳定 id。新库另发一套 id，这个只用来把「旧 provider id →
    /// 新 environment id」这张映射表建起来，好让档案里的引用还找得到。
    id: String,
    name: String,
    /// `claude-desktop` / `claude-code` / `codex`。
    target: String,
    base_url: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    small_fast_model: Option<String>,
    /// `responses` / `chat`。
    #[serde(default = "default_wire_api")]
    wire_api: String,
    /// `env_key` / `bearer_token` / `none`。
    #[serde(default = "default_auth_style")]
    auth_style: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    website: Option<String>,
}

/// 默认值必须跟 `relay::WireApi` 的 `#[default]` 一致。
///
/// 写死成 `chat` 是中转站那边踩过的坑 1：绝大多数站走 Codex 原生的
/// `responses`，默认给错的话导进来的每一条都连不上。
fn default_wire_api() -> String {
    "responses".into()
}

/// 同上，跟 `relay::AuthStyle` 的 `#[default]` 一致。
fn default_auth_style() -> String {
    "env_key".into()
}

/// 掩码。跟 `relay::mask` 是同一套规则，但**不共用**：
/// 界面上的掩码将来可以改样式，而已经写进库里的这一份不该跟着变。
fn mask_legacy_key(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        return "\u{2022}".repeat(n);
    }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!(
        "{head}{}{tail}",
        "\u{2022}".repeat(n.saturating_sub(8).min(16))
    )
}

/// `profiles.json` 的**历史**形状。只给 [`Repository::migrate`] 用。
///
/// 为什么不复用 `profile::ProfileStore`：迁移读的是一份已经写死在磁盘上的
/// 旧文件，它的形状属于 v1 那一刻，而 `Profile` 是活的 —— 将来给它加一个
/// 必填字段，老用户的 `profiles.json` 就会在迁移时反序列化失败，
/// 而那一刻他正在升级途中，报错也无处可看。（`relay.json` 那条是同一个教训。）
///
/// 顺带断掉一条依赖：`repository` 是数据层，`profile` 是编排层
/// （它要调 `snapshot` 与 `usecase`）。数据层反过来依赖编排层，
/// 是全项目那个 10 模块大环上的一条边。
#[derive(serde::Deserialize, Default)]
struct LegacyProfiles {
    #[serde(default)]
    profiles: Vec<LegacyProfile>,
}

#[derive(serde::Deserialize)]
struct LegacyProfile {
    name: String,
    /// 账户槽位标签。`None` = 这条档案不动账户。
    #[serde(default)]
    account: Option<String>,
    /// target → provider id。
    #[serde(default)]
    relays: std::collections::BTreeMap<String, String>,
    /// 系统时区（Windows 名）。迁移只用它判断「这条档案带不带时区」，
    /// 带的话记一条迁移提示 —— 时区本身不进新库。
    #[serde(default)]
    timezone: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::config_io::{recover, replace, Journal};

    /// 两阶段提交的测试自带临时目录。**不碰真实运行期目录** ——
    /// 那是 CLAUDE.md 写死的规矩。
    fn fixture() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("qb-commitdb-{}", crate::config_io::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn database_failure_rolls_back_files_and_metadata() {
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        let file = p.join("settings.json");
        let result = commit_database(
            &db,
            vec![crate::config_io::Edit::text(file.clone(), "new".into()).unwrap()],
            |_| {
                db.set_meta("sample", "value")?;
                Err(GateError::Other("injected failure".into()))
            },
        );
        assert!(result.is_err());
        assert!(!file.exists());
        assert!(db.meta("sample").unwrap().is_none());
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn restart_uses_database_commit_decision() {
        for committed in [false, true] {
            let p = fixture();
            let db = Repository::open_in(&p).unwrap();
            let file = p.join("file");
            std::fs::write(&file, "before").unwrap();
            let op = commit_database(
                &db,
                vec![crate::config_io::Edit::text(file.clone(), "after".into()).unwrap()],
                |_| Ok(()),
            )
            .unwrap();
            let path = p.join("operations/files").join(format!("{op}.json"));
            let mut journal: Journal =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            journal.status = "prepared".into();
            replace(&path, Some(&serde_json::to_vec(&journal).unwrap())).unwrap();
            if !committed {
                db.conn
                    .execute(
                        "DELETE FROM metadata WHERE key=?1",
                        [format!("file-commit:{op}")],
                    )
                    .unwrap();
            }
            recover(&p.join("operations/files")).unwrap();
            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                if committed { "after" } else { "before" }
            );
            drop(db);
            std::fs::remove_dir_all(p).unwrap();
        }
    }
    use super::*;
    /// 历史记录要有上限，但正在跑 / 还没核验的会话一条都不能删。
    ///
    /// 删了它们等于把进程弄丢：`sessions::recover` 和 `ensure_verified`
    /// 都靠这些行认人，行没了就再也停不掉那个进程，界面还会显示「已退出」。
    #[test]
    fn history_is_capped_but_live_sessions_are_never_dropped() {
        let root = std::env::temp_dir().join(format!("qb-prune-{}", crate::config_io::id()));
        let db = Repository::open_in(&root).unwrap();
        for i in 0..5 {
            db.put(
                "sessions",
                &format!("live-{i}"),
                &serde_json::json!({"id": format!("live-{i}"), "state": "running"}),
            )
            .unwrap();
            db.put(
                "sessions",
                &format!("done-{i}"),
                &serde_json::json!({"id": format!("done-{i}"), "state": "exited"}),
            )
            .unwrap();
        }
        // 受保护的不占额度：keep=2 之后应当剩下 5 条 running + 最新 2 条 exited。
        assert_eq!(
            db.prune("sessions", 2, |s| s["state"] == "running")
                .unwrap(),
            3
        );
        let left: Vec<serde_json::Value> = db.list("sessions").unwrap();
        assert_eq!(left.len(), 7);
        assert_eq!(left.iter().filter(|s| s["state"] == "running").count(), 5);
        assert!(left.iter().any(|s| s["id"] == "done-4"));
        assert!(!left.iter().any(|s| s["id"] == "done-0"));
        // 幂等：再收一次没有可删的了。
        assert_eq!(
            db.prune("sessions", 2, |s| s["state"] == "running")
                .unwrap(),
            0
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn migration_preserves_files_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("qb-db-test-{}", crate::config_io::id()));
        let mut db = Repository::open_in(&root).unwrap();
        std::fs::write(root.join("relay.json"), "{\"providers\":[]}").unwrap();
        let notes = db.migrate().unwrap();
        assert_eq!(db.migrate().unwrap(), notes);
        assert_eq!(
            std::fs::read_to_string(root.join("relay.json")).unwrap(),
            "{\"providers\":[]}"
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_legacy_data_blocks_migration_without_replacement() {
        let root = std::env::temp_dir().join(format!("qb-db-test-{}", crate::config_io::id()));
        let mut db = Repository::open_in(&root).unwrap();
        std::fs::write(root.join("relay.json"), "broken").unwrap();
        assert!(db.migrate().is_err());
        assert!(db.meta("legacy-v1").unwrap().is_none());
        assert_eq!(
            std::fs::read_to_string(root.join("relay.json")).unwrap(),
            "broken"
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// 跨 SQLite 与外部配置文件的两阶段提交。
///
/// 原来在 `crate::repository::commit_database`，但它要拿 `&Repository` ——
/// 一个文件 I/O 模块因此反过来依赖了数据库，`config_io ↔ repository`
/// 这对循环依赖就是这么来的。
///
/// 事务边界属于拥有数据库的这一层。文件那半边仍然由
/// [`crate::config_io::commit_inner`] 做，这里只负责把「SQLite 提交了没」
/// 这个**唯一的裁决**写进去：重启时日志按这个标记决定是收尾还是回滚。
pub fn commit_database(
    db: &Repository,
    edits: Vec<crate::config_io::Edit>,
    change: impl FnOnce(&str) -> Result<()>,
) -> Result<String> {
    let tx = db
        .conn
        .unchecked_transaction()
        .map_err(|e| GateError::Database(e.to_string()))?;
    crate::config_io::commit_inner(
        &db.root.join("operations/files"),
        edits,
        Some(db.root.join("workspace.sqlite3")),
        |id| {
            change(id)?;
            db.set_meta(&format!("file-commit:{id}"), "committed")?;
            tx.commit().map_err(|e| GateError::Other(e.to_string()))
        },
    )
}
