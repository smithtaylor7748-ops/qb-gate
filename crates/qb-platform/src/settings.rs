//! 面板自己的开关。存 `%LOCALAPPDATA%\ClaudeIpGate\settings.json`。
//!
//! # 只放「会改变程序行为」的开关
//!
//! 界面偏好（折叠状态、当前分栏那些）走 `useSession`，关掉面板就没了，
//! 不进这个文件。这里放的是**下次启动仍然生效、且会改变门禁行为**的东西。
//!
//! # 每个开关的默认值都必须是「最不意外」的那个
//!
//! 默认值决定了用户装完不动任何设置时的行为。任何会让某个命令**突然跑不起来**
//! 的开关，默认都得是关的 —— 否则用户升级一次面板，第二天发现命令打不开，
//! 而他根本不知道是这个程序干的。
//!
//! 一个明确的例外：GPT（Codex）归门禁管这件事，0.25.0 起**默认开**，
//! 是使用者自己定的（见 `codex_outside_gate`）。他要的是「Claude 怎么管，GPT 就怎么管」，
//! 而「门禁默认只管一半」在他看来才是意外。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct Settings {
    /// GPT（Codex）要不要**退出** IP 门禁。
    ///
    /// **默认 false = 归门禁管。** 0.25.0 之前这个开关叫 `codex_under_gate`、默认关，
    /// 理由是文件头那条「默认值不能让命令突然跑不起来」。**使用者 2026-09-20 明确要求
    /// 「默认定死被 IP 锁接管」**，于是把开关反过来存：记的是「退出」而不是「加入」。
    /// 反过来存不是花样，是升级路径 —— 旧 `settings.json` 里躺着的 `codex_under_gate: false`
    /// 变成未知键被忽略，所有人升级后自动落到「归门禁」，不用写迁移；
    /// 以后在「IP 锁」弹窗里手动移出的才会写下 `codex_outside_gate: true`。
    ///
    /// 归门禁的含义跟 Claude 一样：`codex.exe` 加 Deny ExecuteFile、GPT 桌面端起之前
    /// 验 IP 解锁、起来之后持租约、看门狗判不过就收。突然跑不起来的那道风险由
    /// 「白名单为空时不上锁」兜着（`gate::lock_all` 的硬约束）。
    ///
    /// 读的一侧仍叫 [`codex_under_gate()`]，语义不变、调用点不用改。
    pub codex_outside_gate: bool,

    /// 反重力（Antigravity Hub 与 IDE）要不要**退出** IP 门禁（0.26.0）。
    ///
    /// **默认 false = 归门禁管**，跟 GPT 那条同款反义存法（使用者的原话：软件页新增这个
    /// 软件，「也被 IP 锁保护」）。归门禁的含义：`Antigravity.exe`、它的 `language_server.exe`、
    /// IDE 的两份 exe 与它的语言服务器一起加 Deny ExecuteFile，起之前验 IP 解锁、
    /// 起来之后持租约、看门狗判不过就收（桌面档，零宽限）。
    ///
    /// 两个产品共用一个开关：它们往外发请求用的是同一个 Google 账户，
    /// 只锁一个就是给另一个留门。读的一侧是 [`antigravity_under_gate()`]。
    pub antigravity_outside_gate: bool,

    /// 反重力的联网额度要不要问（0.32.0 起；2026-09-23 从「只管 Hub」扩成「Hub + 账户槽位」）。
    ///
    /// **默认 true。** 字段名没改，是为了已经写进 `settings.json` 的旧值照样生效。
    /// 开着的时候也**只在使用者点刷新图标时**才问（2026-09-23 使用者定的），没有定时器。
    ///
    /// Hub 的邮箱在凭据里那个 `id_token` 的载荷中，是纯本机解码，**不受这个开关影响**；
    /// 反重力 IDE 写在本机的 `userStatus` 也照读。
    ///
    /// 关掉它，点刷新图标会得到一句「设置里关掉了」，界面只剩本机那些数。
    /// 边界与免责措辞在 `qb-app::usecase::antigravity_quota` 的模块头与 DISCLAIMER §6。
    #[serde(default = "yes")]
    pub antigravity_hub_quota: bool,

    /// 门禁被动关上之后，出口 IP 回到白名单时要不要自动重新放行。
    ///
    /// **默认 true。** 关闭后监督服务继续运行，但不会自动重新放行 ——
    /// 出口 IP 后来恢复了也不会有人开门，使用者只会在下次开新会话时撞见
    /// `Claude Code couldn't start`，而且完全不知道这跟半小时前那次网络抖动有关。
    ///
    /// 打开它**不放松任何安全性质**：重新放行仍然要求出口 IP 已核实且在白名单里，
    /// 跟 `Tick::ReclaimLease` 守的是同一条不变量。而且它**只恢复租约、
    /// 不启动任何进程**，也不读限流状态 —— README 合规边界那四条一条都没碰。
    pub gate_auto_rearm: bool,

    /// 面板托管安装的根目录（v0.9.0）。Claude Code 装在 `<它>\claude-code\`，
    /// Codex 装在 `<它>\codex\`。
    ///
    /// **`None` = 默认位置** `%LOCALAPPDATA%\ClaudeIpGate\apps` —— 不把本机的绝对路径
    /// 写进配置，换一台机器、换一个用户名照样对。
    ///
    /// ⚠ 只能经 `managed::set_dir` 改（它会先实测新目录锁不锁得住、再把已装的搬过去）。
    /// `settings_save` 会忽略前端传来的这个字段 —— 不然前端改个开关顺手把它改了，
    /// 文件还在旧目录，面板就又「找不到软件」了。
    pub managed_apps_dir: Option<std::path::PathBuf>,

    /// 国家白名单（ISO 3166-1 alpha-2 大写）。
    ///
    /// **默认空，空 = 这一层不启用。** 不能让空名单等于全拒 ——
    /// 使用者装完面板、还没来得及配国家名单时，全拒会把他直接关在门外，
    /// 与硬约束 4「白名单为空时不许上锁」是同一类事故。界面上必须
    /// 显著标注「国家层未启用」，别让人以为配了。
    ///
    /// 启用之后这一层是**硬的**：出口 IP 落在名单外、查不出国家、
    /// 或者几个探测源报的国家互相打架，一律按不合格处理（见 `gate::judge`）。
    /// 代价写在 DISCLAIMER 里：GeoIP 不准会误杀正在进行的会话。
    pub country_allowlist: Vec<String>,

    /// 会话内门禁（装进 Claude Code 的 hook）要不要开。
    ///
    /// **默认 false** —— 它会在门禁判不过时拦下每一次请求，是个会让
    /// 「本来能用的东西突然不能用」的开关，必须由使用者自己打开。
    ///
    /// 这个字段存的是**意图**，不是现状：每个槽位的 `settings.json` 里
    /// 装没装才是现状。切换账户后 `usecase::hook_ops::follow_active_slot`
    /// 拿它把现状对齐回来。
    pub hook_enabled: bool,

    /// 启动 Claude Code 时关掉非必要遥测（崩溃报告、使用统计）。
    ///
    /// **默认 false。** 两个理由：一是它会改变现有使用者升级后的行为，
    /// 二是它放在 IP 锁和国家白名单旁边，极容易被读成「防封的一环」——
    /// **它跟封号风险无关**，让人对一件不保护他的事感到安全，比不做更糟。
    ///
    /// 具体设哪几个变量见 `launch::TELEMETRY_OFF`，界面上原样列出来供核对。
    pub disable_telemetry: bool,

    /// 启动面板时，把**系统时区**对齐到出口 IP 的归属地。
    ///
    /// **默认 true** —— 这是使用者明确要的（「时区对齐应该是本项目的默认启动功能」）。
    /// 已经一致时直接跳过，不做任何动作、也不弹 UAC；只有真的不一致才切，
    /// 而切时区需要管理员权限，那一次会弹一个 UAC。
    ///
    /// 查不到出口 IP、或者对照表里没有那个时区时**不切也不猜**，只在日志里记一句
    /// （`sysenv::iana_to_windows` 返回 `None` 就报错）。
    pub align_timezone_on_start: bool,

    /// 启动面板时，把**区域格式**（日期、数字、货币）对齐到出口 IP 的归属地。
    ///
    /// **默认 true**：`Set-Culture` 不需要管理员权限、不用重启，改完之后新开的
    /// 程序就读到新值 —— 代价小到可以默认开。
    pub align_locale_on_start: bool,

    /// 启动面板时，把**显示语言**对齐到出口 IP 的归属地。
    ///
    /// **默认 false，跟上面两个不一样。** 理由是代价完全不同：它要先装好对应的
    /// 语言包，而且**要注销一次才生效**。默认开的话，使用者装完面板第二天发现
    /// 系统提示要注销，或者语言包没装、设了等于没设 —— 两种都属于
    /// 「突然发生一件我没要求的事」，正是这个文件开头那条默认值规矩要挡的。
    ///
    /// 要用的人自己在设置里打开，界面上把「要语言包、要注销」写在开关旁边。
    pub align_display_language_on_start: bool,

    /// 启动面板时问一次 GitHub 有没有新版（0.25.3，使用者要的「打开软件时弹窗收到更新通知」）。
    ///
    /// **默认 true** —— 这个功能本来就是为「装了老版本、不会自己去 GitHub 看」的人做的，
    /// 默认关就等于没做。它只问本项目自己发布页上的一个小文件（`install::self_update`），
    /// 不带任何账户信息；**只提醒，不自动装** —— 装要使用者在弹窗里点「一键更新」。
    /// 关掉之后启动时一个请求都不发，设置页的「检查更新」照样能手动点。
    #[serde(default = "yes")]
    pub update_check_on_start: bool,

    /// 使用者在更新弹窗里点过「跳过这个版本」的那一版。启动检查遇到**正好这一版**就不弹，
    /// 更新的版本照弹。设置页仍然显示它、仍然能从那里更新。
    ///
    /// 只经 `update_skip` 改：`settings_save` 原样保留旧值 —— 设置页拿着一份打开页面时读的
    /// 旧设置去存别的开关，不该顺手把刚点的「跳过」冲掉。
    pub update_skipped_version: Option<String>,
}

/// `#[serde(default)]` 对 `bool` 给的是 `false`。要默认开的字段用这个 ——
/// 升级上来的旧配置文件里没有那个键，落到 `false` 就等于悄悄替使用者关掉了功能。
fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            codex_outside_gate: false,
            antigravity_outside_gate: false,
            antigravity_hub_quota: true,
            gate_auto_rearm: true,
            managed_apps_dir: None,
            country_allowlist: Vec::new(),
            hook_enabled: false,
            disable_telemetry: false,
            align_timezone_on_start: true,
            align_locale_on_start: true,
            // 要语言包 + 要注销，代价跟上面两个不是一个量级 —— 见字段上的说明。
            align_display_language_on_start: false,
            update_check_on_start: true,
            update_skipped_version: None,
        }
    }
}

pub fn path() -> std::path::PathBuf {
    crate::paths::state_dir().join("settings.json")
}

/// 进程内缓存。**写时失效，不设过期时间。**
///
/// # 为什么要缓存
///
/// `load()` 原来每次都重读一遍磁盘、重解析一遍 JSON。而它在**看门狗的热路径上**：
/// `gate::targets::lockable()` 每次枚举目标都要问一次「Codex 归不归门禁管」，
/// 而看门狗每 5 秒枚举一轮，一轮里 `lockable()` 会被调好几次。
///
/// # 为什么不设过期时间
///
/// 带 TTL 的缓存会让「改完设置多久生效」变成一个说不清的数，而这里的设置
/// 直接决定门禁锁哪些文件 —— 那不是可以「过一会儿再生效」的东西。
/// 写时失效是确定的：改了就立刻作废，下一次读一定读到新值。
///
/// # 外部改文件怎么办
///
/// 使用者手改 `settings.json` 不会让缓存失效。**这是可以接受的** ——
/// 面板本来就要求通过界面改设置（`config_io::commit` 的两阶段提交会核对
/// 文件没被外部改过），手改本来就不在支持范围内。重启面板即可。
static CACHE: std::sync::OnceLock<std::sync::RwLock<Option<Settings>>> = std::sync::OnceLock::new();

fn cache() -> &'static std::sync::RwLock<Option<Settings>> {
    CACHE.get_or_init(Default::default)
}

/// 让缓存作废。**所有写路径都要调它。**
///
/// 不调的后果不是「慢一点」，是**改完设置门禁行为不变**，而界面显示已经变了。
pub fn invalidate_cache() {
    *cache().write().unwrap() = None;
}

/// 读。文件不在或者内容坏了都退回默认值 ——
/// 一个读不出来的设置文件不该让面板起不来。
pub fn load() -> Settings {
    if let Some(s) = cache().read().unwrap().as_ref() {
        return s.clone();
    }
    let fresh = read_from_disk();
    *cache().write().unwrap() = Some(fresh.clone());
    fresh
}

fn read_from_disk() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn load_checked() -> Result<Settings> {
    match crate::config_io::read_optional(&path())? {
        None => Ok(Settings::default()),
        Some(bytes) => serde_json::from_slice(&bytes).map_err(Into::into),
    }
}
/// 只写盘。**不作废门禁判定缓存** —— 那是跨域的事，归
/// `usecase::settings_ops::save` 管，那才是唯一正确的保存入口。
///
/// # 名字为什么这么长
///
/// 原来它叫 `save`，可见性 `pub(crate)`。改设置**必须**同时作废
/// `gate-verdict.json`，否则门禁会拿一份陈旧的判定继续放行，
/// 而症状是「改完设置门禁没反应」—— 最难查的那一类。
///
/// 搬进 `qb-platform` 之后 `pub(crate)` 挡不住跨 crate 的调用了，只能放到
/// `pub`。护栏因此从「编译器」退成「名字」：一个叫
/// `write_without_invalidating_the_gate_verdict` 的函数，不会有人顺手就调。
pub fn write_without_invalidating_the_gate_verdict(s: &Settings) -> Result<()> {
    load_checked()?;
    invalidate_cache();
    crate::config_io::commit(vec![crate::config_io::Edit::text(
        path(),
        serde_json::to_string_pretty(s)?,
    )?])?;
    Ok(())
}

/// GPT（Codex）归不归门禁管。门禁那边每次枚举目标都要问一次，单独提出来省得到处 `load()`。
///
/// 存的是反义的 `codex_outside_gate`（见字段说明），这里翻回来 —— 调用点关心的是
/// 「管不管」，不是「文件里怎么记」。
pub fn codex_under_gate() -> bool {
    !load().codex_outside_gate
}

/// 反重力（Hub + IDE）归不归门禁管。默认归；反义存法同 [`codex_under_gate()`]。
pub fn antigravity_under_gate() -> bool {
    !load().antigravity_outside_gate
}

/// 反重力的联网额度（Hub + 账户槽位）要不要问。默认开，见字段说明。
pub fn antigravity_hub_quota() -> bool {
    load().antigravity_hub_quota
}

/// 看门狗收摊之后要不要转入重整待命。默认开。
pub fn gate_auto_rearm() -> bool {
    load().gate_auto_rearm
}

/// 国家白名单。空 = 国家层不启用（**不是全拒**，见字段上的说明）。
///
/// 读出来就规整成两位大写，免得使用者手打小写把整层悄悄废掉。
pub fn country_allowlist() -> Vec<String> {
    normalize_countries(load().country_allowlist)
}

/// 规整国家白名单：去空白、转大写、丢掉不是两位字母的、去重。
///
/// 认不出的条目**直接丢掉而不是报错** —— 但这意味着一份全是错别字的名单
/// 会变成空名单，也就是「整层关掉」。所以 `settings_save` 那边要把
/// 丢掉了哪几条回给界面，不能默默吞掉。
pub fn normalize_countries(raw: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = raw
        .into_iter()
        .map(|c| c.trim().to_uppercase())
        .filter(|c| c.len() == 2 && c.chars().all(|ch| ch.is_ascii_alphabetic()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// 托管安装根目录的默认位置。跟面板的运行期数据放在一起 ——
/// 面板自己的安装 / 卸载器只动 `%LOCALAPPDATA%\QB Gate`，不会碰这里。
pub fn default_managed_dir() -> std::path::PathBuf {
    crate::paths::state_dir().join("apps")
}

/// 当前生效的托管安装根目录。
pub fn managed_apps_dir() -> std::path::PathBuf {
    load().managed_apps_dir.unwrap_or_else(default_managed_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GPT 默认归门禁管（使用者 2026-09-20 定的，见字段说明）。
    #[test]
    fn codex_gate_defaults_to_on() {
        assert!(!Settings::default().codex_outside_gate);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(
            !s.codex_outside_gate,
            "旧配置文件里没有这个字段，不能默成退出门禁"
        );
    }

    /// 反重力默认归门禁管（使用者 2026-09-20 定的：「也被 IP 锁保护」）。
    /// 0.26.0 之前的文件没有这个键，升上来必须落到「归门禁」。
    #[test]
    fn antigravity_gate_defaults_to_on() {
        assert!(!Settings::default().antigravity_outside_gate);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.antigravity_outside_gate);
        let s: Settings = serde_json::from_str(r#"{"antigravity_outside_gate":true}"#).unwrap();
        assert!(s.antigravity_outside_gate);
    }

    /// 升级路径：0.25.0 之前的文件写的是 `codex_under_gate: false`。
    /// 那个键现在是未知键，必须被忽略 —— 旧文件升上来要落到「归门禁」，
    /// 而不是把「以前没纳入」延续下去。
    #[test]
    fn an_old_codex_under_gate_false_no_longer_opts_out() {
        let s: Settings = serde_json::from_str(r#"{"codex_under_gate":false}"#).unwrap();
        assert!(!s.codex_outside_gate);
        // 反过来也一样：旧键的 true 不会被误读成「退出」。
        let s: Settings = serde_json::from_str(r#"{"codex_under_gate":true}"#).unwrap();
        assert!(!s.codex_outside_gate);
        // 新键才算数。
        let s: Settings = serde_json::from_str(r#"{"codex_outside_gate":true}"#).unwrap();
        assert!(s.codex_outside_gate);
    }

    /// 重整待命默认必须是**开**的。
    ///
    /// 关掉会让门**关上之后永远不自己开**，所以默认开 ——
    /// 默认值要是「最不意外」的那个。
    #[test]
    fn auto_rearm_defaults_to_on() {
        assert!(Settings::default().gate_auto_rearm);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(
            s.gate_auto_rearm,
            "旧配置文件里没有这个字段，不能默成 false"
        );
    }

    #[test]
    fn broken_or_missing_file_falls_back_to_defaults() {
        for text in ["", "{ not json", "null", "[]"] {
            let s: Settings = serde_json::from_str(text).unwrap_or_default();
            assert!(!s.codex_outside_gate, "读不出来就是默认：归门禁");
        }
    }

    #[test]
    fn unknown_keys_do_not_break_older_builds() {
        // 新版加了字段、用户又装回旧版时，旧版得能照常读。
        let s: Settings =
            serde_json::from_str(r#"{"codex_outside_gate":true,"something_new":42}"#).unwrap();
        assert!(s.codex_outside_gate);
    }

    #[test]
    fn missing_field_uses_the_default_not_an_error() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.codex_outside_gate);
        assert!(s.gate_auto_rearm);
    }

    /// 国家白名单默认必须是**空**的。
    ///
    /// 空 = 这一层不启用。默认给一份名单等于替使用者做了判定，
    /// 而且升级面板的人第二天会发现门禁按一份他没见过的名单在收进程。
    #[test]
    fn country_allowlist_defaults_to_empty_meaning_layer_off() {
        assert!(Settings::default().country_allowlist.is_empty());
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(
            s.country_allowlist.is_empty(),
            "旧配置文件里没有这个字段，不能凭空长出一份名单"
        );
    }

    #[test]
    fn country_codes_are_normalized_deduped_and_sorted() {
        let got = normalize_countries(vec![
            " us ".into(),
            "US".into(),
            "tw".into(),
            "".into(),
            "USA".into(),
            "1".into(),
        ]);
        assert_eq!(got, vec!["TW".to_string(), "US".to_string()]);
    }

    /// 遥测开关默认必须是**关**的。
    ///
    /// 它会改变现有使用者升级后的行为；而且它跟封号风险无关，
    /// 默认打开等于替他做了一个不保护他的决定。
    #[test]
    fn telemetry_switch_defaults_to_off() {
        assert!(!Settings::default().disable_telemetry);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.disable_telemetry, "旧配置文件没有这个字段，不能默成开");
    }

    /// 启动时检查更新默认**开**：0.25.3 之前的文件没有这个键，升上来必须落到「开」——
    /// 否则装了带更新功能的那一版，也永远收不到下一版的提醒。
    #[test]
    fn update_check_defaults_to_on_and_nothing_is_skipped() {
        assert!(Settings::default().update_check_on_start);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(
            s.update_check_on_start,
            "旧配置文件里没有这个字段，不能默成关"
        );
        assert!(s.update_skipped_version.is_none());
        let s: Settings = serde_json::from_str(r#"{"update_check_on_start":false}"#).unwrap();
        assert!(!s.update_check_on_start);
    }

    #[test]
    fn managed_dir_defaults_to_none_so_no_machine_path_is_written() {
        // 默认不把本机的绝对路径写进配置 —— None 表示「用默认位置」。
        assert!(Settings::default().managed_apps_dir.is_none());
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(
            s.managed_apps_dir.is_none(),
            "旧配置文件没有这个字段，要落到默认位置"
        );
        assert!(default_managed_dir().ends_with("apps"));
    }
}
