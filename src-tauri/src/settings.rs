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
//! 的开关，默认都得是关的 —— 否则用户升级一次面板，第二天发现 codex 打不开，
//! 而他根本不知道是这个程序干的。

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Codex 要不要也归 IP 门禁管。
    ///
    /// **默认 false，而且必须一直是 false。** 打开之后 `codex` 会跟
    /// `claude.exe` 一样被加上 Deny ExecuteFile —— 出口 IP 不在白名单时
    /// 命令直接被系统拒绝执行。这对「请求不能从没核实过的 IP 出去」是对的，
    /// 但对一个正在用 Codex 干活的人来说是个突然的变化，
    /// 所以只能由他自己在设置里打开。
    pub codex_under_gate: bool,

    /// 门禁被动关上之后，出口 IP 回到白名单时要不要自动重新放行。
    ///
    /// **默认 true。** 关掉它，看门狗收摊之后就此退出，从此没有任何东西看着 ——
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            codex_under_gate: false,
            gate_auto_rearm: true,
            managed_apps_dir: None,
        }
    }
}

pub fn path() -> std::path::PathBuf {
    crate::gate::state_dir().join("settings.json")
}

/// 读。文件不在或者内容坏了都退回默认值 ——
/// 一个读不出来的设置文件不该让面板起不来。
pub fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<()> {
    if let Some(d) = path().parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// 门禁那边每次枚举目标都要问一次，单独提出来省得到处 `load()`。
pub fn codex_under_gate() -> bool {
    load().codex_under_gate
}

/// 看门狗收摊之后要不要转入重整待命。默认开。
pub fn gate_auto_rearm() -> bool {
    load().gate_auto_rearm
}

/// 托管安装根目录的默认位置。跟面板的运行期数据放在一起 ——
/// 面板自己的安装 / 卸载器只动 `%LOCALAPPDATA%\QB Gate`，不会碰这里。
pub fn default_managed_dir() -> std::path::PathBuf {
    crate::gate::state_dir().join("apps")
}

/// 当前生效的托管安装根目录。
pub fn managed_apps_dir() -> std::path::PathBuf {
    load().managed_apps_dir.unwrap_or_else(default_managed_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_gate_defaults_to_off() {
        // 打开它会让 codex 在 IP 不合规时**直接跑不起来**。
        // 默认开着的话，用户升级一次面板第二天就发现 codex 打不开，
        // 而且不知道是这个程序干的。
        assert!(!Settings::default().codex_under_gate);
    }

    /// 重整待命默认必须是**开**的。
    ///
    /// 跟 `codex_under_gate` 正好相反，理由也相反：
    /// 那个开关打开会让命令**突然跑不起来**，所以默认关；
    /// 这个开关关掉会让门**关上之后永远不自己开**，所以默认开。
    /// 两边守的是同一条：默认值要是「最不意外」的那个。
    #[test]
    fn auto_rearm_defaults_to_on() {
        assert!(Settings::default().gate_auto_rearm);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(s.gate_auto_rearm, "旧配置文件里没有这个字段，不能默成 false");
    }

    #[test]
    fn broken_or_missing_file_falls_back_to_defaults() {
        for text in ["", "{ not json", "null", "[]"] {
            let s: Settings = serde_json::from_str(text).unwrap_or_default();
            assert!(!s.codex_under_gate);
        }
    }

    #[test]
    fn unknown_keys_do_not_break_older_builds() {
        // 新版加了字段、用户又装回旧版时，旧版得能照常读。
        let s: Settings =
            serde_json::from_str(r#"{"codex_under_gate":true,"something_new":42}"#).unwrap();
        assert!(s.codex_under_gate);
    }

    #[test]
    fn missing_field_uses_the_default_not_an_error() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.codex_under_gate);
    }

    #[test]
    fn managed_dir_defaults_to_none_so_no_machine_path_is_written() {
        // 默认不把本机的绝对路径写进配置 —— None 表示「用默认位置」。
        assert!(Settings::default().managed_apps_dir.is_none());
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(s.managed_apps_dir.is_none(), "旧配置文件没有这个字段，要落到默认位置");
        assert!(default_managed_dir().ends_with("apps"));
    }
}
