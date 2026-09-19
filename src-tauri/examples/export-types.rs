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
//! 加了新的 ts-rs 契约类型？在下面添一行。
//!
//! # ⚠ 「忘了添会当场红」只对改字段成立
//!
//! `npm run types:check` 的第一条断言比的是「重新生成的和仓库里那份一不一样」。
//! 改了某个已导出类型的字段，它会红；**新加一个从来没导出过的类型，两边都没有
//! 它，比什么都一样** —— 实测漏掉 `StationModel` / `StationModelsView` /
//! `ModelPrice` 三个，全绿。
//!
//! 所以 `check-types.mjs` 末尾另有一条断言：扫遍所有 `#[ts(export)]`，
//! 每一个都得在 `src/lib/generated` 里有对应的 `.ts`。漏了这里的一行，
//! 现在才是真的会红。
use ts_rs::TS;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/generated");
    let out = || -> std::result::Result<(), ts_rs::ExportError> {
        qb_gate_lib::domain::export_types(&root)?;
        qb_accounts::codex::CodexAccounts::export_all_to(&root)?;
        qb_accounts::codex::usage::CodexUsage::export_all_to(&root)?;
        qb_install::install::codex_desktop::CodexDesktop::export_all_to(&root)?;
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
        qb_gate_lib::probe::ip_lookup::IpLookupReport::export_all_to(&root)?;
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
        qb_gate_lib::accounts::DeleteOutcome::export_all_to(&root)?;
        qb_gate_lib::usecase::egress_checks::EgressChecks::export_all_to(&root)?;
        qb_gate_lib::accounts::tokens::TokenUsage::export_all_to(&root)?;
        qb_gate_lib::usecase::token_summary::TokenSummary::export_all_to(&root)?;
        qb_gate_lib::usecase::account_probe::ProbeResult::export_all_to(&root)?;
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
        qb_gate_lib::sysenv::ipv6::Ipv6Status::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::SecretHit::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::EnvHit::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::Checkup::export_all_to(&root)?;
        qb_gate_lib::sysenv::checkup::State::export_all_to(&root)?;
        // 启动时对齐（0.19.0）：区域格式与显示语言改之前长什么样，用来还原。
        qb_gate_lib::sysenv::locale::LocaleState::export_all_to(&root)?;
        // 系统代理（0.19.0，「两个口子」之二）。改之前的那一份也是这个形状，
        // 前端要拿它渲染「回滚到原值」。
        qb_gate_lib::sysenv::proxy::ProxyState::export_all_to(&root)?;
        qb_gate_lib::install::versions::VersionEntry::export_all_to(&root)?;
        qb_gate_lib::install::managed::App::export_all_to(&root)?;
        qb_gate_lib::install::managed::AppStatus::export_all_to(&root)?;
        qb_gate_lib::install::managed::Status::export_all_to(&root)?;
        qb_gate_lib::install::managed::Probe::export_all_to(&root)?;
        qb_gate_lib::install::managed::External::export_all_to(&root)?;
        qb_gate_lib::install::managed::Method::export_all_to(&root)?;
        qb_gate_lib::install::managed::CleanupReport::export_all_to(&root)?;
        // 完全卸载（0.19.0）。四个都带 `rename = "Purge…"`：`Target` 被门禁占着、
        // `Action` 被升级占着，不改名的话 ts-rs 会把先写的那份**悄悄盖掉**
        // （check-types.mjs 里那条唯一性断言就是为这件事加的）。
        qb_gate_lib::install::purge::Target::export_all_to(&root)?;
        qb_gate_lib::install::purge::Category::export_all_to(&root)?;
        qb_gate_lib::install::purge::Action::export_all_to(&root)?;
        qb_gate_lib::install::purge::Item::export_all_to(&root)?;
        qb_gate_lib::usecase::purge_ops::Report::export_all_to(&root)?;
        // 浏览器出站锁（0.19.0，「两个口子」之一）。两个都带 `rename`：
        // `Rule` / `Adapter` 太通用，留原名迟早跟别的类型撞上 —— 而 ts-rs
        // 撞名时是**悄悄覆盖**先写的那份，不报错（上面 Purge 那段同一个坑）。
        qb_gate_lib::firewall::Rule::export_all_to(&root)?;
        qb_gate_lib::firewall::Adapter::export_all_to(&root)?;
        qb_gate_lib::install::chrome::Trace::export_all_to(&root)?;
        qb_gate_lib::install::chrome::TraceReport::export_all_to(&root)?;
        // 浏览器隐私面审计（0.19.0）。四个都带 rename：`Extension` 会跟
        // `qb-extensions` 撞，`Audit` / `Policy` / `Scope` 太通用。
        qb_gate_lib::install::browser_audit::Extension::export_all_to(&root)?;
        qb_gate_lib::install::browser_audit::Scope::export_all_to(&root)?;
        qb_gate_lib::install::browser_audit::Policy::export_all_to(&root)?;
        qb_gate_lib::install::browser_audit::Audit::export_all_to(&root)?;
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
        qb_gate_lib::plugins::sillytavern::TavernConfig::export_all_to(&root)?;
        qb_gate_lib::plugins::codex_egress::EgressConfig::export_all_to(&root)?;
        qb_gate_lib::plugins::codex_egress::EgressInstall::export_all_to(&root)?;
        qb_gate_lib::usecase::turnstate_ops::Takeover::export_all_to(&root)?;
        qb_gate_lib::plugins::tavern_locate::TavernEvidence::export_all_to(&root)?;
        qb_gate_lib::plugins::tavern_locate::TavernCandidate::export_all_to(&root)?;
        qb_gate_lib::plugins::tavern_locate::TavernSurvey::export_all_to(&root)?;
        // 中转站 P1-P4。智能调度那三个是使用者直接勾的东西,
        // 前端必须拿生成的联合类型,手抄一份「便宜/快/稳」迟早跟 Rust 的枚举漂开。
        qb_gate_lib::schedule::Axis::export_all_to(&root)?;
        qb_gate_lib::schedule::Floors::export_all_to(&root)?;
        qb_gate_lib::schedule::Prefs::export_all_to(&root)?;
        qb_gate_lib::schedule::CheapBasis::export_all_to(&root)?;
        qb_gate_lib::station::model::Protocols::export_all_to(&root)?;
        qb_gate_lib::station::route::Route::export_all_to(&root)?;
        // 倍率三态。界面照它分「标称·未核实」/「已核实」/ 划掉标称写实测,
        // 少一个分支就会把没检验过的显示成属实。
        qb_gate_lib::station::route::RateTrust::export_all_to(&root)?;
        qb_gate_lib::station::audit::CheckKind::export_all_to(&root)?;
        qb_gate_lib::station::audit::Verdict::export_all_to(&root)?;
        qb_gate_lib::station::audit::Check::export_all_to(&root)?;
        qb_gate_lib::station::audit::TrustDelta::export_all_to(&root)?;
        qb_gate_lib::station::audit::EvidenceLevel::export_all_to(&root)?;
        qb_gate_lib::station::audit::AuditRound::export_all_to(&root)?;
        qb_gate_lib::router::RequestLog::export_all_to(&root)?;
        qb_gate_lib::schedule::AxisScore::export_all_to(&root)?;
        qb_gate_lib::schedule::FloorMiss::export_all_to(&root)?;
        qb_gate_lib::schedule::Row::export_all_to(&root)?;
        qb_gate_lib::schedule::Ranking::export_all_to(&root)?;
        qb_gate_lib::schedule::Schedule::export_all_to(&root)?;
        // 官方 Codex turn-state 的界面状态（导出名 TurnStateStatus / TurnStateModelStatus）。
        qb_gate_lib::turnstate::Status::export_all_to(&root)?;
        qb_gate_lib::turnstate::ModelStatus::export_all_to(&root)?;
        qb_gate_lib::commands::station::ProbeResult::export_all_to(&root)?;
        qb_gate_lib::commands::station::ClientConfig::export_all_to(&root)?;
        qb_gate_lib::commands::station::RouterStatus::export_all_to(&root)?;
        qb_gate_lib::commands::station::RouteHealthView::export_all_to(&root)?;
        qb_gate_lib::commands::station::DecisionView::export_all_to(&root)?;
        qb_gate_lib::commands::station::StoredAudit::export_all_to(&root)?;
        qb_gate_lib::commands::station::PriceStatusView::export_all_to(&root)?;
        qb_gate_lib::station::pricing::PriceBasis::export_all_to(&root)?;
        qb_gate_lib::station::pricing::UnitPrice::export_all_to(&root)?;
        qb_gate_lib::station::pricing::PriceStatus::export_all_to(&root)?;
        qb_gate_lib::station::pricing::PriceCategory::export_all_to(&root)?;
        qb_gate_lib::station::pricing::CategoryVerdict::export_all_to(&root)?;
        qb_gate_lib::station::pricing::StationRates::export_all_to(&root)?;
        qb_gate_lib::station::pricing::ResolvedPrice::export_all_to(&root)?;
        qb_gate_lib::station::pricing::ModelPrice::export_all_to(&root)?;
        // 「站点 → 分组 → 模型」那三级:第三级的候选来自站点自己的价目表。
        qb_gate_lib::station::pricing::StationModel::export_all_to(&root)?;
        qb_gate_lib::commands::station::StationModelsView::export_all_to(&root)?;
        qb_app::usecase::station_billing::StationBillingSettings::export_all_to(&root)?;
        qb_gate_lib::station::pricing::FetchedPrice::export_all_to(&root)
    };
    out().expect("export IPC contracts");
}
