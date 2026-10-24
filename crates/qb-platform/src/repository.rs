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
/// 数据库结构版本 = 迁移条数。
pub const SCHEMA: u32 = MIGRATIONS.len() as u32;

/// 迁移清单。第 `i` 条把库从版本 `i` 升到版本 `i+1`。
///
/// ⛔ **只许往后追加。已经发布过的那几条一个字都不许改。**
///
/// 改了的话：老库不会重跑它（`user_version` 早就越过去了），新库却按新写法建 ——
/// 两边的表结构从此不一样，而且**谁都不会报错**。真正出问题的是几个月后
/// 某条查询在一半人的机器上莫名其妙失败。
///
/// 要改结构就**再追加一条**。
const MIGRATIONS: &[&str] = &[
    // 1：最初那一版。
    "CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS providers(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS credentials(id TEXT PRIMARY KEY,provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,body TEXT NOT NULL,sealed TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS environments(id TEXT PRIMARY KEY,provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,credential_id TEXT REFERENCES credentials(id) ON DELETE RESTRICT,body TEXT NOT NULL,hashes TEXT NOT NULL DEFAULT '{}');
     CREATE TABLE IF NOT EXISTS installations(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS catalog(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS diagnostics(id TEXT PRIMARY KEY,body TEXT NOT NULL);",
    // 2：中转站线路池 / 检验轮次 / 请求日志（0.14.0）。
    //
    // 都用 `(id, body)`，跟上面几张一样 —— 通用的 list/get/put/prune 因此直接可用。
    // 线路**不加外键指向 providers**：站点被删时把线路留成孤儿是可恢复的，
    // 而 RESTRICT 会让「删站点」从此失败，那是在改一个已经发布的行为。
    "CREATE TABLE IF NOT EXISTS station_routes(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS station_audits(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS request_logs(id TEXT PRIMARY KEY,body TEXT NOT NULL);",
    // 3：线路按软件独立（0.16.0）。
    //
    // 线路 id 从两段 `站点␟分组` 变成三段 `软件␟站点␟分组`，body 里多一个
    // `client`。**老库里的线路全都是给 Claude Code 配的** —— 那时候面板只给
    // 它接了线，所以整批落到 `claude-code`，跟 `Client` 的 `#[default]`
    // 是同一个答案（两处对不上的话，反序列化补的值会和迁移写进去的打架）。
    //
    // `char(31)` 就是 `U+001F`，跟 `Route::make_id` 用的分隔符一致。
    // SQLite 的 `UPDATE ... SET` 右侧读的是行的**旧值**，所以 `id` 和 `body`
    // 在同一条语句里都还能拿到没改写的 `id`。
    "UPDATE station_routes
        SET body = json_set(body, '$.client', 'claude-code',
                                  '$.id', 'claude-code' || char(31) || id),
            id   = 'claude-code' || char(31) || id;",
    // 4：智能调度的落盘状态（2026-10-17 补上）。
    //
    // 0.16.0 把智能调度从前端的一个 useState 改成「落盘的承诺」，命令层从那时起就往
    // `station_schedule` 里读写 —— 可这张表**从来没有迁移建过**，`table()` 白名单里也没有它。
    // 于是每一次「开启调度」都在 `put` 那一步报「未知的数据表」，读的一侧又把错误吞成
    // 「关着」：真机上调度从没开成过，而演示模式喂的是假数据，`test:ui` 一直全绿。
    // 形状跟别的表一样是 `(id, body)`，一个软件一行，主键是软件名。
    "CREATE TABLE IF NOT EXISTS station_schedule(id TEXT PRIMARY KEY,body TEXT NOT NULL);",
    // 5：本机路由的请求日志第二版（2026-10-24，中转站重构阶段 1）。
    //
    // 原来的 `request_logs` 是 `(id, body)`：每次打开日志页都把整张表读出来再截断，
    // 裁旧的也是全读之后逐条删。日志是涨得最快的那张表，所以这一张用**正经的列**，
    // 按时间、按线路、按软件建索引，查询带时间窗和 LIMIT（[`Repository::router_requests`]）。
    // 多了试过的线路链（`attempts`，JSON 数组）、四类 token 与两种花费。
    //
    // 老表里的行原样搬过来（老日志只试过一条线，`attempts` 补那一条；没有 token、没有花费，
    // 留 NULL —— 是「没记」，不是 0）。**老表不删**：这条迁移万一漏搬了什么，原始数据还在。
    // 搬法跟 `qb_station::router::RouterRequest::from(RequestLog)` 一一对应，测试钉着两边一致。
    "CREATE TABLE IF NOT EXISTS router_requests(
        id TEXT PRIMARY KEY,
        at_ms INTEGER NOT NULL,
        client TEXT NOT NULL,
        route_id TEXT NOT NULL,
        model TEXT NOT NULL DEFAULT '',
        upstream_model TEXT NOT NULL DEFAULT '',
        status INTEGER,
        first_token_ms INTEGER,
        total_ms INTEGER,
        retry_after_ms INTEGER,
        streaming INTEGER NOT NULL DEFAULT 0,
        input_tokens INTEGER,
        output_tokens INTEGER,
        cache_read_tokens INTEGER,
        cache_write_tokens INTEGER,
        cache_write_1h_tokens INTEGER,
        station_cost REAL,
        official_cost REAL,
        session_id TEXT,
        fault TEXT,
        error TEXT,
        attempts TEXT NOT NULL DEFAULT '[]');
     CREATE INDEX IF NOT EXISTS router_requests_at ON router_requests(at_ms);
     CREATE INDEX IF NOT EXISTS router_requests_route ON router_requests(route_id, at_ms);
     CREATE INDEX IF NOT EXISTS router_requests_client ON router_requests(client, at_ms);
     INSERT OR IGNORE INTO router_requests(
        id, at_ms, client, route_id, model, status, first_token_ms, total_ms, retry_after_ms, attempts)
     SELECT json_extract(body, '$.id'),
            coalesce(json_extract(body, '$.at_ms'), 0),
            coalesce(json_extract(body, '$.client'), ''),
            coalesce(json_extract(body, '$.route_id'), ''),
            coalesce(json_extract(body, '$.model'), ''),
            json_extract(body, '$.status'),
            json_extract(body, '$.first_token_ms'),
            json_extract(body, '$.total_ms'),
            json_extract(body, '$.retry_after_ms'),
            json_array(json_object(
                'route_id', coalesce(json_extract(body, '$.route_id'), ''),
                'status', json_extract(body, '$.status'),
                'fault', NULL,
                'ms', coalesce(json_extract(body, '$.total_ms'), 0)))
       FROM request_logs
      WHERE json_extract(body, '$.id') IS NOT NULL;",
    // 6：本机路由每个软件一份的设置与派单队列（2026-10-24，中转站重构阶段 1）。
    //
    // 都是 `(id, body)`、一个软件一行、主键是软件名 —— 跟 `station_schedule` 同一个形状。
    // 派单队列里也存「上次选中的那条」：原来它只在内存里，面板一重启客户端就收到 503「还没选上游」。
    "CREATE TABLE IF NOT EXISTS router_settings(id TEXT PRIMARY KEY,body TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS dispatch_queue(id TEXT PRIMARY KEY,body TEXT NOT NULL);",
    // 7：测活记录与调度的动作日志（2026-10-24，中转站重构阶段 2）。
    //
    // 测活记录跟请求日志一样是高频表：正经的列，按线路 + 时间、按时间建索引，查询带 LIMIT。
    // 动作日志一天几条，`(id, body)` 就够。
    "CREATE TABLE IF NOT EXISTS probe_attempts(
        id TEXT PRIMARY KEY,
        route_id TEXT NOT NULL,
        at_ms INTEGER NOT NULL,
        model TEXT NOT NULL DEFAULT '',
        ok INTEGER NOT NULL DEFAULT 0,
        fault TEXT,
        total_ms INTEGER,
        first_ms INTEGER,
        paid INTEGER NOT NULL DEFAULT 0,
        cost_lo REAL,
        cost_hi REAL,
        error TEXT);
     CREATE INDEX IF NOT EXISTS probe_attempts_route ON probe_attempts(route_id, at_ms);
     CREATE INDEX IF NOT EXISTS probe_attempts_at ON probe_attempts(at_ms);
     CREATE TABLE IF NOT EXISTS scheduler_actions(id TEXT PRIMARY KEY,body TEXT NOT NULL);",
];

/// 用正经列存的高频表（不走通用的 `(id, body)`）。列名跟第 3 层那个类型的字段**同名**。
///
/// 这一层不认识那些类型，读写都经 serde 的对象形状换过去：写的时候按列名从对象里取值，
/// 读的时候按列名拼回对象。字段改名的话，这里不跟着改，那一列就永远是 NULL ——
/// `router_requests_round_trip_every_column` / `probe_attempts_round_trip_every_column` 钉着。
struct ColumnTable {
    name: &'static str,
    columns: &'static [&'static str],
    /// 存 0 / 1、读回来是 `true` / `false` 的列。
    bools: &'static [&'static str],
    /// 存 JSON 文本的列（数组 / 对象）。缺的时候存 `[]`。
    jsons: &'static [&'static str],
}

/// `router_requests`（迁移 5），对应 `qb_station::router::RouterRequest`。
const ROUTER_REQUESTS: ColumnTable = ColumnTable {
    name: "router_requests",
    columns: ROUTER_REQUEST_COLUMNS,
    bools: &["streaming"],
    jsons: &["attempts"],
};

/// `probe_attempts`（迁移 7），对应 `qb_station::schedule::liveness::ProbeAttempt`。
const PROBE_ATTEMPTS: ColumnTable = ColumnTable {
    name: "probe_attempts",
    columns: &[
        "id", "route_id", "at_ms", "model", "ok", "fault", "total_ms", "first_ms", "paid",
        "cost_lo", "cost_hi", "error",
    ],
    bools: &["ok", "paid"],
    jsons: &[],
};

/// 查 `probe_attempts` 的条件。新的在前。
#[derive(Debug, Clone, Default)]
pub struct ProbeAttemptQuery {
    pub route_id: Option<String>,
    pub since_ms: Option<i64>,
    /// 只要花钱的（`Some(true)`）/ 只要免费的（`Some(false)`）。
    pub paid: Option<bool>,
    pub limit: u32,
}

/// `router_requests` 的列，跟 `qb_station::router::RouterRequest` 的字段**同名**。
const ROUTER_REQUEST_COLUMNS: &[&str] = &[
    "id",
    "at_ms",
    "client",
    "route_id",
    "model",
    "upstream_model",
    "status",
    "first_token_ms",
    "total_ms",
    "retry_after_ms",
    "streaming",
    "input_tokens",
    "output_tokens",
    "cache_read_tokens",
    "cache_write_tokens",
    "cache_write_1h_tokens",
    "station_cost",
    "official_cost",
    "session_id",
    "fault",
    "error",
    "attempts",
];

/// 查 `router_requests` 的条件。都不填就是「最近 `limit` 条」。
#[derive(Debug, Clone, Default)]
pub struct RouterRequestQuery {
    /// 只要这个时刻（含）之后的。
    pub since_ms: Option<i64>,
    /// 只要这个软件的（`Client` 的 kebab-case 写法）。
    pub client: Option<String>,
    /// 只要这条线的。
    pub route_id: Option<String>,
    /// 最多几条，新的在前。0 当 1。
    pub limit: u32,
}

/// 通用读写（`list` / `get` / `find` / `put` / `remove` / `prune`）认得的表。
///
/// ⛔ **加一张表要同时做两件事**：在 [`MIGRATIONS`] 末尾追加建表语句，并把名字写进这里。
/// 只写进这里，第一次读写就是「no such table」；只建表不写进这里，就是「未知的数据表」——
/// 迁移 4 之前的 `station_schedule` 就是后一种，而调用方把错误吞掉了。
/// `every_whitelisted_table_is_created_by_a_migration` 钉着前一种。
const TABLES: &[&str] = &[
    "providers",
    "credentials",
    "environments",
    "installations",
    "catalog",
    "operations",
    "sessions",
    "diagnostics",
    "station_routes",
    "station_audits",
    "request_logs",
    "station_schedule",
    "router_settings",
    "dispatch_queue",
    "scheduler_actions",
];

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
        if version > SCHEMA {
            return Err(GateError::Other(
                "此数据库由更新版本创建，请使用相应版本；数据未修改".into(),
            ));
        }
        if version < SCHEMA {
            conn.execute_batch("PRAGMA journal_mode=WAL;")
                .map_err(sql)?;
            for (i, step) in MIGRATIONS.iter().enumerate() {
                let target = i as u32 + 1;
                if version >= target {
                    continue;
                }
                // 一条迁移一个事务，并且**在同一个事务里**把 user_version 推上去。
                // 分开写的话，中途断电会留下「表建好了但版本号没动」的库，
                // 下次启动重跑同一条迁移 —— `CREATE TABLE IF NOT EXISTS` 撑得住，
                // 但将来任何一条 `ALTER`/`UPDATE` 型迁移都会被跑第二遍。
                conn.execute_batch(&format!(
                    "BEGIN IMMEDIATE;
{step}
PRAGMA user_version={target};
COMMIT;"
                ))
                .map_err(sql)?;
            }
        }
        Ok(Self {
            conn,
            root: root.into(),
        })
    }
    /// 只给测试读结构版本用。
    #[cfg(test)]
    fn pragma_query_value_user_version(&self) -> Result<u32> {
        self.conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(sql)
    }
    fn table(table: &str) -> Result<&str> {
        if TABLES.contains(&table) {
            Ok(table)
        } else {
            Err(GateError::Other(format!("未知的数据表 {table}")))
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
        self.find(table, id)?
            .ok_or_else(|| GateError::Other(format!("找不到记录 {id}")))
    }
    /// 按主键取一行，**没有这一行时给 `None`**。
    ///
    /// 跟 [`get`](Self::get) 只差「没有」这一档：`get` 把它当错误，这里把它当正常结果；
    /// 别的失败（表不在白名单里、库读不了、body 解析不了）照样是错误。
    /// 拿 `get(..).unwrap_or_else(默认值)` 的话，这两类会被一起吞成「没有」——
    /// `station_schedule` 那张表从没建过，就是这样从 0.16.0 一直藏到迁移 4。
    pub fn find<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<Option<T>> {
        let query = format!("SELECT body FROM {} WHERE id=?1", Self::table(table)?);
        let body: Option<String> = self
            .conn
            .query_row(&query, [id], |r| r.get(0))
            .optional()
            .map_err(sql)?;
        body.map(|b| serde_json::from_str(&b).map_err(Into::into))
            .transpose()
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

    // ------------------------------------------------------------ 本机路由的请求日志

    /// 一批请求日志落库（同一个 id 再写一次就覆盖）。一个事务，返回写了几条。
    ///
    /// `T` 序列化出来必须是对象，键名见 [`ROUTER_REQUEST_COLUMNS`]；缺的键当 NULL。
    pub fn insert_router_requests<T: Serialize>(&self, rows: &[T]) -> Result<usize> {
        self.insert_columns(&ROUTER_REQUESTS, rows)
    }

    /// 按条件读请求日志，新的在前。**走索引、带 LIMIT** —— 不像通用的 `list` 那样整表读。
    pub fn router_requests<T: DeserializeOwned>(&self, q: &RouterRequestQuery) -> Result<Vec<T>> {
        let mut filter: Vec<(&str, rusqlite::types::Value)> = Vec::new();
        if let Some(since) = q.since_ms {
            filter.push(("at_ms >= ?", since.into()));
        }
        if let Some(client) = &q.client {
            filter.push(("client = ?", client.clone().into()));
        }
        if let Some(route) = &q.route_id {
            filter.push(("route_id = ?", route.clone().into()));
        }
        self.select_columns(&ROUTER_REQUESTS, filter, q.limit)
    }

    /// 删掉 `before_ms` 之前的请求日志（走 `at_ms` 的索引）。返回删了几条。
    pub fn prune_router_requests(&self, before_ms: i64) -> Result<usize> {
        self.prune_columns(&ROUTER_REQUESTS, before_ms)
    }

    // ------------------------------------------------------------ 测活记录（迁移 7）

    /// 一批测活记录落库（同一个 id 再写一次就覆盖）。
    pub fn insert_probe_attempts<T: Serialize>(&self, rows: &[T]) -> Result<usize> {
        self.insert_columns(&PROBE_ATTEMPTS, rows)
    }

    /// 按条件读测活记录，新的在前（走 `route_id, at_ms` / `at_ms` 的索引，带 LIMIT）。
    pub fn probe_attempts<T: DeserializeOwned>(&self, q: &ProbeAttemptQuery) -> Result<Vec<T>> {
        let mut filter: Vec<(&str, rusqlite::types::Value)> = Vec::new();
        if let Some(route) = &q.route_id {
            filter.push(("route_id = ?", route.clone().into()));
        }
        if let Some(since) = q.since_ms {
            filter.push(("at_ms >= ?", since.into()));
        }
        if let Some(paid) = q.paid {
            filter.push(("paid = ?", i64::from(paid).into()));
        }
        self.select_columns(&PROBE_ATTEMPTS, filter, q.limit)
    }

    /// 删掉 `before_ms` 之前的测活记录。
    pub fn prune_probe_attempts(&self, before_ms: i64) -> Result<usize> {
        self.prune_columns(&PROBE_ATTEMPTS, before_ms)
    }

    fn insert_columns<T: Serialize>(&self, table: &ColumnTable, rows: &[T]) -> Result<usize> {
        let placeholders = (1..=table.columns.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "INSERT OR REPLACE INTO {}({}) VALUES({placeholders})",
            table.name,
            table.columns.join(",")
        );
        self.transaction(|| {
            let mut stmt = self.conn.prepare_cached(&query).map_err(sql)?;
            for row in rows {
                let value = serde_json::to_value(row)?;
                let object = value.as_object().ok_or_else(|| {
                    GateError::Other(format!(
                        "写进 {} 的那一行序列化出来不是一个对象",
                        table.name
                    ))
                })?;
                let cells: Vec<rusqlite::types::Value> = table
                    .columns
                    .iter()
                    .map(|column| to_cell(table, column, object.get(*column)))
                    .collect::<Result<_>>()?;
                stmt.execute(rusqlite::params_from_iter(cells))
                    .map_err(sql)?;
            }
            Ok(rows.len())
        })
    }

    fn select_columns<T: DeserializeOwned>(
        &self,
        table: &ColumnTable,
        filter: Vec<(&str, rusqlite::types::Value)>,
        limit: u32,
    ) -> Result<Vec<T>> {
        let (clauses, mut args): (Vec<&str>, Vec<rusqlite::types::Value>) =
            filter.into_iter().unzip();
        let filter = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        args.push(i64::from(limit.max(1)).into());
        let query = format!(
            "SELECT {} FROM {} {filter} ORDER BY at_ms DESC, rowid DESC LIMIT ?",
            table.columns.join(","),
            table.name
        );
        let mut stmt = self.conn.prepare(&query).map_err(sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args), |r| {
                let mut object = serde_json::Map::new();
                for (i, column) in table.columns.iter().enumerate() {
                    object.insert((*column).to_string(), from_cell(table, column, r.get(i)?));
                }
                Ok(serde_json::Value::Object(object))
            })
            .map_err(sql)?;
        rows.map(|r| Ok(serde_json::from_value(r.map_err(sql)?)?))
            .collect()
    }

    fn prune_columns(&self, table: &ColumnTable, before_ms: i64) -> Result<usize> {
        self.conn
            .execute(
                &format!("DELETE FROM {} WHERE at_ms < ?1", table.name),
                [before_ms],
            )
            .map_err(sql)
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
                            topup_per_usd: None,
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
                    // 从老 profiles 迁上来的都是直连站点的环境 —— 本机路由那一条
                    // 是后来才有的,迁移不该替使用者打开它。
                    via_router: false,
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

/// 对象里的一个字段换成列表的一格。JSON 列存 JSON 文本，布尔存 0 / 1。
fn to_cell(
    table: &ColumnTable,
    column: &str,
    value: Option<&serde_json::Value>,
) -> Result<rusqlite::types::Value> {
    use rusqlite::types::Value as Cell;
    use serde_json::Value as Json;
    if table.jsons.contains(&column) {
        return Ok(match value {
            Some(v) => Cell::Text(serde_json::to_string(v)?),
            None => Cell::Text("[]".into()),
        });
    }
    Ok(match value {
        None | Some(Json::Null) => Cell::Null,
        Some(Json::Bool(b)) => Cell::Integer(i64::from(*b)),
        Some(Json::Number(n)) => match n.as_i64() {
            Some(i) => Cell::Integer(i),
            None => Cell::Real(n.as_f64().unwrap_or(f64::NAN)),
        },
        Some(Json::String(s)) => Cell::Text(s.clone()),
        Some(other) => Cell::Text(other.to_string()),
    })
}

/// [`to_cell`] 反过来。
fn from_cell(table: &ColumnTable, column: &str, cell: rusqlite::types::Value) -> serde_json::Value {
    use rusqlite::types::Value as Cell;
    use serde_json::Value as Json;
    if table.jsons.contains(&column) {
        return match cell {
            Cell::Text(s) => serde_json::from_str(&s).unwrap_or(Json::Array(vec![])),
            _ => Json::Array(vec![]),
        };
    }
    if table.bools.contains(&column) {
        if let Cell::Integer(i) = cell {
            return Json::Bool(i != 0);
        }
    }
    match cell {
        Cell::Null => Json::Null,
        Cell::Integer(i) => Json::from(i),
        Cell::Real(f) => serde_json::Number::from_f64(f).map_or(Json::Null, Json::Number),
        Cell::Text(s) => Json::String(s),
        Cell::Blob(_) => Json::Null,
    }
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
    fn a_fresh_database_lands_on_the_latest_schema_and_reopening_is_a_no_op() {
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        let v: u32 = db
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA);
        drop(db);
        // 再开一次不该重跑任何迁移，也不该报错。
        let again = Repository::open_in(&p).unwrap();
        let v2: u32 = again.pragma_query_value_user_version().unwrap();
        assert_eq!(v2, SCHEMA);
    }

    #[test]
    fn an_old_database_is_upgraded_in_place_without_losing_its_rows() {
        // 这是迁移框架存在的全部理由：老用户的库要能升上来，
        // 而且升的过程中一行数据都不能掉。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.put("providers", "p1", &serde_json::json!({"id":"p1"}))
                .unwrap();
            // 手工退回版本 1，装成一个 0.13.x 建的库。
            db.conn
                .execute_batch(
                    "DROP TABLE IF EXISTS station_routes;
                                DROP TABLE IF EXISTS station_audits;
                                DROP TABLE IF EXISTS request_logs;
                                PRAGMA user_version=1;",
                )
                .unwrap();
        }
        let db = Repository::open_in(&p).unwrap();
        assert_eq!(db.pragma_query_value_user_version().unwrap(), SCHEMA);
        // 老数据还在。
        let got: serde_json::Value = db.get("providers", "p1").unwrap();
        assert_eq!(got["id"], "p1");
        // 新表能用了。
        db.put("station_routes", "r1", &serde_json::json!({"id":"r1"}))
            .unwrap();
        let routes: Vec<serde_json::Value> = db.list("station_routes").unwrap();
        assert_eq!(routes.len(), 1);
    }

    #[test]
    fn old_routes_are_rehomed_under_claude_code_rather_than_left_without_a_client() {
        // 0.16.0 把线路 id 从两段改成三段（`软件␟站点␟分组`）。老库里那些
        // 线路全是给 Claude Code 配的 —— 迁移不把它们认领走的话，三个软件
        // 分页一个都不会显示它们，看起来就是「升级之后线路全没了」。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.put(
                "station_routes",
                "moxi\u{1f}低价组",
                &serde_json::json!({"id":"moxi\u{1f}低价组","station_id":"moxi","group":"低价组"}),
            )
            .unwrap();
            // 退回版本 2，装成一个 0.15.x 建的库（那时候表已经有了，只是没有 client）。
            db.conn.execute_batch("PRAGMA user_version=2;").unwrap();
        }
        let db = Repository::open_in(&p).unwrap();
        assert_eq!(db.pragma_query_value_user_version().unwrap(), SCHEMA);

        let routes: Vec<serde_json::Value> = db.list("station_routes").unwrap();
        assert_eq!(routes.len(), 1, "一条都不许掉");
        let r = &routes[0];
        assert_eq!(r["client"], "claude-code");
        assert_eq!(r["id"], "claude-code\u{1f}moxi\u{1f}低价组");
        // 主键也得跟着改写，否则 body 里的 id 和行的 id 对不上，
        // 下一次 put 会照着新 id 再插一行，池子里出现两条同样的线。
        let by_key: serde_json::Value = db
            .get("station_routes", "claude-code\u{1f}moxi\u{1f}低价组")
            .unwrap();
        assert_eq!(by_key["station_id"], "moxi");
        // 站点和分组一个字都没动 —— 站点是共享的，迁移只认领软件这一维。
        assert_eq!(r["station_id"], "moxi");
        assert_eq!(r["group"], "低价组");
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_instead_of_being_downgraded() {
        // 降级打开会让新版本写进去的字段被旧代码悄悄抹掉。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.conn
                .execute_batch(&format!("PRAGMA user_version={};", SCHEMA + 1))
                .unwrap();
        }
        // 用 match：Repository 没有 Debug，unwrap_err 要求 Ok 侧有。
        let err = match Repository::open_in(&p) {
            Ok(_) => panic!("更新版本的库被降级打开了"),
            Err(e) => e,
        };
        assert!(format!("{err}").contains("更新版本"), "{err}");
    }

    #[test]
    fn the_migration_list_is_append_only_by_construction() {
        // SCHEMA 就是迁移条数 —— 两者不可能对不上。
        // 有人删掉一条迁移的话，SCHEMA 会**变小**，
        // 于是所有已经升上去的库都会被判成「更新版本创建的」而拒绝打开。
        assert_eq!(SCHEMA as usize, MIGRATIONS.len());
        assert!(SCHEMA >= 2, "已经发布过版本 2，不许再变小");
    }

    #[test]
    fn every_whitelisted_table_is_created_by_a_migration() {
        // 白名单与迁移是两份清单。只进了白名单的表，第一次读写就是「no such table」——
        // 而调用方要是把错误吞成默认值，那就是 `station_schedule` 那种「功能从没生效过」。
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        for table in TABLES {
            let found: Option<String> = db
                .conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .optional()
                .unwrap();
            assert_eq!(found.as_deref(), Some(*table), "{table} 没有迁移建过");
            // 通用读写确实认得它。
            let _: Vec<serde_json::Value> = db.list(table).unwrap();
        }
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn a_schedule_switched_on_is_still_on_after_reopening_the_database() {
        // 迁移 4 之前：这一句 put 报「未知的数据表」，读的一侧把错误吞成「关着」——
        // 智能调度在真机上从没开成过，面板重启后「接着跑」也就无从谈起。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.put(
                "station_schedule",
                "claude-code",
                &serde_json::json!({"client": "claude-code", "enabled": true}),
            )
            .unwrap();
        }
        let db = Repository::open_in(&p).unwrap();
        let got: serde_json::Value = db.get("station_schedule", "claude-code").unwrap();
        assert_eq!(got["enabled"], true);
        // 一个软件一行：别的软件没存过就是没有，不是报错。
        assert!(db
            .find::<serde_json::Value>("station_schedule", "codex")
            .unwrap()
            .is_none());
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn a_version_three_database_gains_the_schedule_table_and_keeps_its_routes() {
        // 装着 0.16.0–0.25.6 的人，库停在版本 3：升上来要多出调度表，线路一条都不能掉。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.put(
                "station_routes",
                "claude-code\u{1f}moxi\u{1f}",
                &serde_json::json!({"id": "claude-code\u{1f}moxi\u{1f}", "client": "claude-code"}),
            )
            .unwrap();
            db.conn
                .execute_batch("DROP TABLE station_schedule; PRAGMA user_version=3;")
                .unwrap();
        }
        let db = Repository::open_in(&p).unwrap();
        assert_eq!(db.pragma_query_value_user_version().unwrap(), SCHEMA);
        let routes: Vec<serde_json::Value> = db.list("station_routes").unwrap();
        assert_eq!(routes.len(), 1, "一条都不许掉");
        // 迁移 3 是 UPDATE 型的：重跑一遍会把 id 再前缀一次。版本号在同一个事务里推上去，
        // 所以从 3 升到 4 只跑第 4 条。
        assert_eq!(routes[0]["id"], "claude-code\u{1f}moxi\u{1f}");
        db.put(
            "station_schedule",
            "codex",
            &serde_json::json!({"client": "codex", "enabled": true}),
        )
        .unwrap();
        let got: serde_json::Value = db.get("station_schedule", "codex").unwrap();
        assert_eq!(got["enabled"], true);
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    fn sample_request(id: &str, at_ms: i64, client: &str, route: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "at_ms": at_ms, "client": client, "route_id": route,
            "model": "claude-opus-5", "upstream_model": "relay-opus",
            "status": 200, "first_token_ms": 812, "total_ms": 4031, "retry_after_ms": null,
            "streaming": true,
            "input_tokens": 12, "output_tokens": 7, "cache_read_tokens": 300,
            "cache_write_tokens": 40, "cache_write_1h_tokens": 40,
            "station_cost": 0.0123, "official_cost": 0.5,
            "session_id": "s-1", "fault": null, "error": null,
            "attempts": [
                {"route_id": "a", "status": 502, "fault": "upstream-down", "ms": 31},
                {"route_id": route, "status": 200, "fault": null, "ms": 4000}
            ]
        })
    }

    #[test]
    fn router_requests_round_trip_every_column() {
        // 列名跟字段名对不上的那一列会永远是 NULL —— 而读回来的那一格看起来只是「没记」。
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        let row = sample_request("1", 1_000, "claude-code", "b");
        assert_eq!(db.insert_router_requests(&[row.clone()]).unwrap(), 1);
        let back: Vec<serde_json::Value> = db
            .router_requests(&RouterRequestQuery {
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(back, vec![row], "有一列没存进去或者读回来变了样");
        // 每个字段都有对应的列（少一个就是上面那种静默的 NULL）。
        let keys: Vec<&str> = back[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), ROUTER_REQUEST_COLUMNS.len());
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn router_requests_filter_by_time_client_and_route_newest_first() {
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        let rows = [
            sample_request("1", 1_000, "claude-code", "x"),
            sample_request("2", 2_000, "codex", "y"),
            sample_request("3", 3_000, "claude-code", "y"),
            sample_request("4", 4_000, "claude-code", "x"),
        ];
        db.insert_router_requests(&rows).unwrap();
        let ids = |q: RouterRequestQuery| -> Vec<String> {
            db.router_requests::<serde_json::Value>(&q)
                .unwrap()
                .into_iter()
                .map(|v| v["id"].as_str().unwrap().to_string())
                .collect()
        };
        let all = RouterRequestQuery {
            limit: 100,
            ..Default::default()
        };
        assert_eq!(ids(all.clone()), ["4", "3", "2", "1"], "要新的在前");
        assert_eq!(
            ids(RouterRequestQuery {
                limit: 2,
                ..all.clone()
            }),
            ["4", "3"]
        );
        assert_eq!(
            ids(RouterRequestQuery {
                client: Some("claude-code".into()),
                since_ms: Some(2_000),
                ..all.clone()
            }),
            ["4", "3"]
        );
        assert_eq!(
            ids(RouterRequestQuery {
                route_id: Some("y".into()),
                ..all.clone()
            }),
            ["3", "2"]
        );
        // 同一个 id 再写一次是覆盖，不是第二行。
        let mut again = rows[0].clone();
        again["status"] = serde_json::json!(503);
        db.insert_router_requests(&[again]).unwrap();
        assert_eq!(ids(all.clone()).len(), 4);
        // 裁旧的按时间走。
        assert_eq!(db.prune_router_requests(2_500).unwrap(), 2);
        assert_eq!(ids(all), ["4", "3"]);
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    fn sample_probe(id: &str, at_ms: i64, route: &str, paid: bool) -> serde_json::Value {
        serde_json::json!({
            "id": id, "route_id": route, "at_ms": at_ms, "model": "claude-sonnet-5",
            "ok": !paid, "fault": if paid { serde_json::json!("timeout") } else { serde_json::Value::Null },
            "total_ms": 1234, "first_ms": if paid { serde_json::Value::Null } else { serde_json::json!(410) },
            "paid": paid,
            "cost_lo": if paid { serde_json::json!(0.0012) } else { serde_json::Value::Null },
            "cost_hi": if paid { serde_json::json!(0.0031) } else { serde_json::Value::Null },
            "error": if paid { serde_json::json!("首字 30 秒没来") } else { serde_json::Value::Null },
        })
    }

    #[test]
    fn probe_attempts_round_trip_every_column() {
        // 布尔列存 0 / 1，读回来必须还是 true / false（不然 ProbeAttempt 反序列化不回去）。
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        let rows = [
            sample_probe("1", 1_000, "a", false),
            sample_probe("2", 2_000, "a", true),
        ];
        assert_eq!(db.insert_probe_attempts(&rows).unwrap(), 2);
        let back: Vec<serde_json::Value> = db
            .probe_attempts(&ProbeAttemptQuery {
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(back, vec![rows[1].clone(), rows[0].clone()]);
        assert_eq!(
            back[0].as_object().unwrap().len(),
            PROBE_ATTEMPTS.columns.len(),
            "每个字段都要有一列"
        );
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn probe_attempts_filter_by_route_time_and_paid_newest_first() {
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        db.insert_probe_attempts(&[
            sample_probe("1", 1_000, "x", false),
            sample_probe("2", 2_000, "y", true),
            sample_probe("3", 3_000, "x", true),
            sample_probe("4", 4_000, "x", false),
        ])
        .unwrap();
        let ids = |q: ProbeAttemptQuery| -> Vec<String> {
            db.probe_attempts::<serde_json::Value>(&q)
                .unwrap()
                .into_iter()
                .map(|v| v["id"].as_str().unwrap().to_string())
                .collect()
        };
        let all = ProbeAttemptQuery {
            limit: 100,
            ..Default::default()
        };
        assert_eq!(ids(all.clone()), ["4", "3", "2", "1"]);
        assert_eq!(
            ids(ProbeAttemptQuery {
                route_id: Some("x".into()),
                since_ms: Some(2_000),
                ..all.clone()
            }),
            ["4", "3"]
        );
        // 「今天花了多少」只数花钱的那几次。
        assert_eq!(
            ids(ProbeAttemptQuery {
                paid: Some(true),
                ..all.clone()
            }),
            ["3", "2"]
        );
        assert_eq!(
            ids(ProbeAttemptQuery {
                paid: Some(false),
                limit: 1,
                ..all.clone()
            }),
            ["4"]
        );
        assert_eq!(db.prune_probe_attempts(2_500).unwrap(), 2);
        assert_eq!(ids(all), ["4", "3"]);
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn a_version_six_database_gains_the_probe_tables_and_keeps_its_logs() {
        // 阶段 1 的库停在版本 6：升上来要多出测活表与动作日志，请求日志一条不掉。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.insert_router_requests(&[sample_request("r1", 1_000, "codex", "a")])
                .unwrap();
            db.conn
                .execute_batch(
                    "DROP TABLE probe_attempts; DROP TABLE scheduler_actions; PRAGMA user_version=6;",
                )
                .unwrap();
        }
        let db = Repository::open_in(&p).unwrap();
        assert_eq!(db.pragma_query_value_user_version().unwrap(), SCHEMA);
        let logs: Vec<serde_json::Value> = db
            .router_requests(&RouterRequestQuery {
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(logs.len(), 1);
        db.insert_probe_attempts(&[sample_probe("p1", 5_000, "a", false)])
            .unwrap();
        db.put(
            "scheduler_actions",
            "s1",
            &serde_json::json!({"id": "s1", "kind": "switch"}),
        )
        .unwrap();
        let actions: Vec<serde_json::Value> = db.list("scheduler_actions").unwrap();
        assert_eq!(actions.len(), 1);
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn migration_5_carries_the_old_request_logs_over() {
        // 装着 0.14.0–0.25.6 的人：老日志一条都不许丢，新表里的形状跟
        // `RouterRequest::from(RequestLog)` 一致（只试过一条线、没有 token）。
        let p = fixture();
        {
            let db = Repository::open_in(&p).unwrap();
            db.put(
                "request_logs",
                "old-1",
                &serde_json::json!({
                    "id": "old-1", "at_ms": 5, "route_id": "r", "client": "codex",
                    "model": "gpt", "status": 502, "first_token_ms": 9, "total_ms": 40,
                    "retry_after_ms": null
                }),
            )
            .unwrap();
            db.conn
                .execute_batch("DROP TABLE router_requests; PRAGMA user_version=4;")
                .unwrap();
        }
        let db = Repository::open_in(&p).unwrap();
        assert_eq!(db.pragma_query_value_user_version().unwrap(), SCHEMA);
        let rows: Vec<serde_json::Value> = db
            .router_requests(&RouterRequestQuery {
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(rows.len(), 1, "老日志没搬过来");
        let r = &rows[0];
        assert_eq!(
            (
                r["id"].as_str(),
                r["route_id"].as_str(),
                r["status"].as_i64()
            ),
            (Some("old-1"), Some("r"), Some(502))
        );
        assert_eq!(r["total_ms"], 40);
        assert_eq!(
            r["input_tokens"],
            serde_json::Value::Null,
            "老日志没有 token —— 是「没记」，不是 0"
        );
        assert_eq!(r["streaming"], false);
        assert_eq!(
            r["attempts"],
            serde_json::json!([{"route_id": "r", "status": 502, "fault": null, "ms": 40}])
        );
        // 老表还在（只读的后路）。
        let old: Vec<serde_json::Value> = db.list("request_logs").unwrap();
        assert_eq!(old.len(), 1);
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn find_tells_a_missing_row_apart_from_a_broken_lookup() {
        let p = fixture();
        let db = Repository::open_in(&p).unwrap();
        // 没有这一行：正常结果。
        assert!(db
            .find::<serde_json::Value>("station_schedule", "codex")
            .unwrap()
            .is_none());
        // 表名不在白名单里：错误，而且说得出是哪张表。
        let err = db
            .find::<serde_json::Value>("station_schedules", "codex")
            .unwrap_err()
            .to_string();
        assert!(err.contains("未知的数据表 station_schedules"), "{err}");
        // body 读不出来：错误，不是「没有」。
        db.conn
            .execute(
                "INSERT INTO station_schedule(id,body) VALUES('codex','not json')",
                [],
            )
            .unwrap();
        assert!(db
            .find::<serde_json::Value>("station_schedule", "codex")
            .is_err());
        // get 在「没有」那一档仍然是错误（老调用方靠这个）。
        assert!(db
            .get::<serde_json::Value>("station_schedule", "claude-code")
            .unwrap_err()
            .to_string()
            .contains("找不到记录"));
        drop(db);
        std::fs::remove_dir_all(p).unwrap();
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
