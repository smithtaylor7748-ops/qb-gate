//! 把 IPC 契约导成 TypeScript。**开发工具，不随安装包发布。**
//!
//! 放在 `examples/` 而不是 `src/bin/` 是有原因的：`src/bin/*` 是 Cargo 的
//! bin 目标，`cargo build --release` 会产出它，而 Tauri 打包时会把
//! `target/release` 下的 bin 目标一并塞进安装包 —— v0.12.0 的第一版安装包里
//! 就真的多了一个 312 KB 的 `export-types.exe`。发给使用者的东西里不该有
//! 开发工具，而且多一个未签名 exe 就是多一份杀软误报面。
//!
//! 用法：`npm run types:generate`（= `cargo run --example export-types`）。
//! # 为什么汇总在这里
//!
//! 原来 `domain::export_types` 顺手把 `diagnostics` / `workspace` /
//! `extensions` 里的类型也导了，于是一个纯数据模块反向依赖了三个业务模块。
//! 汇总名单属于「知道全局」的那一层 —— 开发工具正是那一层，领域类型不是。
//!
//! 加了新的 ts-rs 契约类型？在下面添一行。忘了添的后果是前端那边的
//! `.ts` 不再更新，而 `npm run types:check` 会当场红 —— 它比对的是
//! 这里导出来的文件与仓库里那份。
use ts_rs::TS;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/generated");
    let out = || -> std::result::Result<(), ts_rs::ExportError> {
        qb_gate_lib::domain::export_types(&root)?;
        // 错误分类。前端用它做条件处理 —— 不导出的话那边只能手抄一份，
        // 而手抄的联合类型不会跟着 Rust 的枚举一起变。
        qb_gate_lib::error::ErrorKind::export_all_to(&root)?;
        qb_gate_lib::diagnostics::ProbeRequest::export_all_to(&root)?;
        qb_gate_lib::workspace::ConfigPreview::export_all_to(&root)?;
        qb_gate_lib::extensions::InstallPreview::export_all_to(&root)?;
        qb_gate_lib::extensions::McpCheck::export_all_to(&root)?;
        qb_gate_lib::extensions::InstallRequest::export_all_to(&root)?;
        // B0：原来是 `serde_json::json!{}` 临时拼的三个对象，前端手抄了一份
        // 没有源头的形状。现在它们是真结构体，`npm run types:check` 管得到了。
        qb_gate_lib::accounts::AccountsReport::export_all_to(&root)?;
        qb_gate_lib::install::detect::SoftwareReport::export_all_to(&root)?;
        qb_gate_lib::probe::verdict::PurityCriteria::export_all_to(&root)?;
        // B4：事件通道名。六处裸字符串收成一处 —— 打错一个字母不会报错，
        // 事件照发，只是永远没人收。
        qb_contract::channels::Channels::export_all_to(&root)?;
        // B1：`api.ts` 原来手抄了 75 个 interface，Rust 侧改了它们不会跟着变，
        // 而 `npm run types:check` 只看得见生成的那些 —— 手写的那 75 个是盲区。
        // 全部改成 ts-rs 生成之后，盲区归零。
        qb_gate_lib::probe::ip::IpInfo::export_all_to(&root)?;
        qb_gate_lib::probe::verdict::PanelVerdict::export_all_to(&root)?;
        qb_gate_lib::probe::verdict::Check::export_all_to(&root)?;
        qb_gate_lib::probe::dns::Resolver::export_all_to(&root)?;
        qb_gate_lib::probe::dns::DnsReport::export_all_to(&root)?;
        qb_gate_lib::gate::GateStatus::export_all_to(&root)?;
        qb_gate_lib::gate::targets::Target::export_all_to(&root)?;
        qb_gate_lib::gate::targets::TargetKind::export_all_to(&root)?;
        qb_gate_lib::gate::hook::HookStatus::export_all_to(&root)?;
        qb_gate_lib::gate::lease::Lease::export_all_to(&root)?;
        qb_gate_lib::gate::watchdog::WatchMode::export_all_to(&root)?;
        qb_gate_lib::accounts::CreateOutcome::export_all_to(&root)?;
        qb_gate_lib::relay::presets::Preset::export_all_to(&root)?;
        qb_gate_lib::relay::WireApi::export_all_to(&root)?;
        qb_gate_lib::relay::AuthStyle::export_all_to(&root)?;
        qb_gate_lib::relay::RelayTarget::export_all_to(&root)?;
        qb_gate_lib::settings::Settings::export_all_to(&root)?;
        qb_gate_lib::progress::StepRecord::export_all_to(&root)?;
        qb_gate_lib::progress::Progress::export_all_to(&root)?;
        qb_gate_lib::progress::StepState::export_all_to(&root)?;
        qb_gate_lib::progress::Risk::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::CheckItem::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::SecretHit::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::EnvHit::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::Checkup::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::State::export_all_to(&root)?;
        qb_gate_lib::install::versions::VersionEntry::export_all_to(&root)?;
        qb_gate_lib::install::managed::App::export_all_to(&root)?;
        qb_gate_lib::install::managed::AppStatus::export_all_to(&root)?;
        qb_gate_lib::install::managed::Status::export_all_to(&root)?;
        qb_gate_lib::install::managed::Probe::export_all_to(&root)?;
        qb_gate_lib::install::managed::External::export_all_to(&root)?;
        qb_gate_lib::install::managed::Method::export_all_to(&root)?;
        qb_gate_lib::install::managed::CleanupReport::export_all_to(&root)?;
        qb_gate_lib::install::chrome::Trace::export_all_to(&root)?;
        qb_gate_lib::install::chrome::TraceReport::export_all_to(&root)?;
        qb_gate_lib::install::winget::InstallProbe::export_all_to(&root)?;
        qb_gate_lib::install::winget::PackageProbe::export_all_to(&root)?;
        qb_gate_lib::install::winget::InstallResult::export_all_to(&root)?;
        qb_gate_lib::install::winget::InstallTarget::export_all_to(&root)?;
        qb_gate_lib::install::winget::Method::export_all_to(&root)?;
        qb_gate_lib::install::upgrade::UpgradePlan::export_all_to(&root)?;
        qb_gate_lib::install::upgrade::Channel::export_all_to(&root)?;
        qb_gate_lib::install::upgrade::Action::export_all_to(&root)?;
        qb_gate_lib::launch::LaunchResult::export_all_to(&root)?;
        qb_gate_lib::launch::LaunchTarget::export_all_to(&root)?;
        qb_gate_lib::killswitch::KillTarget::export_all_to(&root)?;
        qb_gate_lib::killswitch::KillReport::export_all_to(&root)?;
        qb_gate_lib::killswitch::Evidence::export_all_to(&root)?;
        qb_gate_lib::killswitch::Role::export_all_to(&root)?;
        qb_gate_lib::plugins::DependencyCheck::export_all_to(&root)?;
        qb_gate_lib::plugins::PluginStatus::export_all_to(&root)?;
        qb_gate_lib::plugins::OfficialCatalogStatus::export_all_to(&root)?;
        qb_gate_lib::plugins::PluginState::export_all_to(&root)?;
        qb_gate_lib::plugins::tavern_assets::AssetItem::export_all_to(&root)?;
        qb_gate_lib::plugins::tavern_assets::CategoryListing::export_all_to(&root)?;
        qb_gate_lib::plugins::tavern_assets::BackupEntry::export_all_to(&root)?;
        qb_gate_lib::snapshot::SnapshotEntry::export_all_to(&root)?;
        qb_gate_lib::snapshot::Manifest::export_all_to(&root)?;
        qb_gate_lib::profile::Profile::export_all_to(&root)?;
        qb_gate_lib::profile::ProfileStore::export_all_to(&root)?;
        qb_gate_lib::profile::ApplyReport::export_all_to(&root)?;
        qb_gate_lib::update::UpdateStatus::export_all_to(&root)?;
        qb_gate_lib::commands::accounts::SwitchReport::export_all_to(&root)?;
        qb_gate_lib::commands::install::MigrateReport::export_all_to(&root)?;
        qb_gate_lib::plugins::sillytavern::TavernConfig::export_all_to(&root)
    };
    out().expect("export IPC contracts");
}
