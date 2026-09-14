//! Public IPC contracts. Exported by the export-types example; secrets only appear on inputs.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Operation {
    pub id: String,
    pub kind: String,
    pub target_id: String,
    pub phase: String,
    pub progress: u32,
    pub status: String,
    pub detail: String,
    pub can_cancel: bool,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProbeCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProbeReport {
    pub id: String,
    pub environment_id: Option<String>,
    pub revision: u32,
    pub checked_at: String,
    pub checks: Vec<ProbeCheck>,
    pub models: Vec<String>,
    pub evidence: Vec<String>,
    pub request_export: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionKind {
    Application,
    Mcp,
    Skill,
    Template,
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
    ProbeReport::export_all_to(path)?;
    ExtensionManifest::export_all_to(path)?;
    ExtensionInstallation::export_all_to(path)
}
