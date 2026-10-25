//! Public IPC contracts. Exported by the export-types example; secrets only appear on inputs.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// `Hash` / `Ord`：本机路由要按软件各记一份当前上游，
// 没有这两个就没法拿它当 map 的键。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    /// 默认档。**这不是随手挑的**：0.16.0 之前线路池里没有 `client` 字段，
    /// 那些线路全都是给 Claude Code 配的，迁移也把它们落到这一档
    /// （`repository.rs` 的第 3 条迁移）。两处必须是同一个答案，
    /// 否则老库升上来之后，反序列化补的默认值和迁移写进去的对不上。
    #[default]
    ClaudeCode,
    ClaudeDesktop,
    Codex,
    /// Google Antigravity Hub（`Programs\antigravity\Antigravity.exe`，0.26.0）。
    ///
    /// **只有官方身份，没有中转路径**：它把 API 端点写死在自己的 `app.asar` 里
    /// （`--api_server_url` / `--cloud_code_endpoint`），只认 Google 登录。
    /// 中转侧的每一处（`environment_save` / `router_environment` / 线路池）都要
    /// **显式拒绝**这两个变体，不许静默兜底 —— 兜底出来的是一份指错地方、
    /// 而且没人说得清为什么起不来的配置。
    Antigravity,
    /// Google Antigravity IDE（`Programs\Antigravity IDE\`，VS Code 分支）。
    /// 跟 Hub 用同一个 Google 账户往外发请求，所以门禁要一起管；启动、上锁、
    /// 收进程都是独立的可执行文件，故独立成一档。中转侧同样显式拒绝。
    AntigravityIde,
    /// Gemini CLI 的**中转那一档**（2026-10-25，中转站重构阶段 4，cc-switch 的 C1）：说 Gemini 原生协议，
    /// 经本机路由 `/gemini` 打这一档自己的线路池。
    ///
    /// ⛔ 跟 `qb-accounts` 里的官方 Gemini 槽位**是两回事**：官方那一半走 Google 登录、`GEMINI_CLI_HOME`
    /// 指向槽位目录；这一档的 `GEMINI_CLI_HOME` 指向中转环境自己的目录，一个字不碰官方槽位。
    /// 不归门禁管：只走中转、不带官方身份（门禁也没给 Gemini CLI 上锁）。
    GeminiCli,
    /// Grok Build（xAI 的命令行工具）的中转那一档（cc-switch 的 C2）：说 Responses 协议，经本机路由 `/grok`。
    /// QB Gate 不管它的官方登录；不归门禁管。
    GrokBuild,
}

impl Client {
    /// 是不是反重力那两档。门禁开关（`settings::antigravity_under_gate`）、
    /// 「没有中转路径」的拒绝、看门狗档位都按这一组走。
    pub fn is_antigravity(self) -> bool {
        matches!(self, Client::Antigravity | Client::AntigravityIde)
    }

    /// 只有中转那一档、QB Gate 不替它起官方身份的软件（阶段 4 加的两档）。它们不走 `LaunchTarget`、
    /// 不持门禁租约：起的是一个带窗口的命令行，配置指到本机路由。
    pub fn is_relay_only(self) -> bool {
        matches!(self, Client::GeminiCli | Client::GrokBuild)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum IdentityKind {
    Official,
    Relay,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LaunchContext {
    pub client: Client,
    pub identity_kind: IdentityKind,
    pub identity_id: String,
    pub config_dir: String,
    pub working_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Session {
    pub id: String,
    pub context: LaunchContext,
    pub pid: u32,
    /// 进程创建时间（Windows FILETIME）。**过 IPC 时是字符串。**
    ///
    /// 这个值现在量级在 1.3e17，远超 JS 的安全整数上限 2^53（约 9e15）：
    /// 当成 `number` 传过去，`JSON.parse` 会静默把末几位抹掉。今天前端只是
    /// 不用它，所以看不出症状；一旦有人把 `Session` 原样回传给后端做身份核验，
    /// 抹掉的那几位会让「PID 是不是被复用了」判错，而且完全不报错。
    #[serde(with = "crate::filetime")]
    #[ts(type = "string")]
    pub process_created: u64,
    pub started_at: String,
    pub state: String,
    pub gated: bool,
    pub detail: String,
    pub config_revision: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub website: String,
    pub note: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub revision: u32,
    /// 充值比例:在这家站买 **1 美元站内额度**要付几元(linux.do「中转站百科」帖说的
    /// 「1 美金 = 多少平台币」)。`None` = 没填,按最常见的 1 元 = 1 美元额度算
    /// ([`DEFAULT_TOPUP_PER_USD`])。
    ///
    /// 中转站的倍率和账单都按站内额度记。不同站放在一起比便宜,要先乘上各自的
    /// 充值比例,折成「每 $1 官方牌价付几元」—— 7 元买 1 美元额度、标 ×0.1 的站,
    /// 实际比 1 元买 1 美元额度、标 ×0.5 的还贵。只用于跨站比较(调度的「便宜」与
    /// 「倍率不高于」)和线路上那句「折合」;查套路的账单核对仍按站内数值,不换算。
    #[serde(default)]
    pub topup_per_usd: Option<f64>,
}

/// 充值比例没填时按多少算:1 元 = 1 美元站内额度(帖子原话「大部分设置的是 1 美金 = 1 人民币」)。
pub const DEFAULT_TOPUP_PER_USD: f64 = 1.0;

impl Provider {
    /// 这家站 1 美元站内额度付几元。没填、填了个不合法的数都按 [`DEFAULT_TOPUP_PER_USD`]。
    pub fn topup_per_usd(&self) -> f64 {
        self.topup_per_usd
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(DEFAULT_TOPUP_PER_USD)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Credential {
    pub id: String,
    pub provider_id: String,
    pub label: String,
    pub masked: String,
    pub available: bool,
    pub revision: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub client: Client,
    pub provider_id: String,
    pub credential_id: Option<String>,
    pub model: String,
    pub small_model: String,
    pub wire_api: String,
    pub auth_style: String,
    pub revision: u32,
    pub applied_revision: Option<u32>,
    pub config_dir: String,
    pub config_state: String,
    /// 这个环境走不走本机路由。
    ///
    /// `true` 时 base_url 不用 provider 上那个,改指本机路由 +
    /// 这个软件的路径前缀,Key 也由路由换成当前上游那把。换上游因此
    /// **不用重写配置、不用重启客户端** —— 那正是本机路由存在的理由。
    ///
    /// ⛔ 这一位决定写进使用者磁盘的配置文件长什么样,所以它必须是
    /// 环境自己的属性,不能靠「provider 的 base_url 看起来像回环」去推:
    /// 推出来的话,一个真的架在 127.0.0.1 上的自建中转会被当成本机路由,
    /// 它的 Key 会被悄悄换掉。
    #[serde(default)]
    pub via_router: bool,
}

/// 一次长操作的进度记录（`operations` 表）。
///
/// 原来还有 `can_cancel`：界面上的「取消」按钮早就没了，取消机制 2026-09-27 删掉，
/// 老数据里那一栏反序列化时直接忽略。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Operation {
    pub id: String,
    pub kind: String,
    pub target_id: String,
    pub phase: String,
    pub progress: u32,
    pub status: String,
    pub detail: String,
    pub started_at: String,
}

// 原来这里还有 `ProbeCheck` / `ProbeReport`：「服务商与环境」旧页草稿诊断的结果。
// 那一页和 `qb_app::diagnostics` 2026-09-27 一起删了；库里 `diagnostics` 表的老行留着不读。

/// 扩展的类别。
///
/// 原来还有 `Template`（「中转环境模板」：建直连站点的旧式环境、建完跳进「服务商与环境」旧页）。
/// 它只在内置目录里出现过、不产生安装记录，2026-09-27 连同那两条目录项一起删了。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionKind {
    Application,
    Mcp,
    Skill,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ExtensionKind,
    pub source: String,
    pub version: String,
    pub license: String,
    pub clients: Vec<Client>,
    pub install_method: String,
    pub configuration: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionInstallation {
    pub id: String,
    pub extension_id: String,
    pub environment_id: String,
    pub identity_kind: IdentityKind,
    pub client: Client,
    pub version: String,
    pub state: String,
    pub path: String,
    pub content_hash: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LaunchPlan {
    pub id: String,
    pub name: String,
    pub client: Client,
    pub identity_kind: IdentityKind,
    pub identity_id: String,
    pub working_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Workspace {
    pub launch_plans: Vec<LaunchPlan>,
    pub providers: Vec<Provider>,
    pub credentials: Vec<Credential>,
    pub environments: Vec<Environment>,
    pub sessions: Vec<Session>,
    pub operations: Vec<Operation>,
    pub installations: Vec<ExtensionInstallation>,
    pub migration_notes: Vec<String>,
}

/// 导出本模块里的契约类型。
///
/// **只导本模块自己的。** 原来这里还顺手导了 `diagnostics` / `workspace` /
/// `extensions` 三个模块里的五个类型 —— 那五行让「一堆纯数据类型」反过来
/// 依赖了三个业务模块，而 `workspace` 又依赖回 `repository`、`repository`
/// 又依赖 `domain`：全项目最大的那个 11 模块环就是从这里起头的。
///
/// 谁有哪些契约类型，由 `examples/export-types.rs` 那一处汇总 ——
/// 那是开发工具，本来就该知道全局，而领域类型不该。
pub fn export_types(path: &std::path::Path) -> std::result::Result<(), ts_rs::ExportError> {
    LaunchContext::export_all_to(path)?;
    Session::export_all_to(path)?;
    Workspace::export_all_to(path)?;
    ExtensionManifest::export_all_to(path)?;
    ExtensionInstallation::export_all_to(path)
}
