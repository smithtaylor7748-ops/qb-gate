//! 系统代理的读取、修改与回滚（0.19.0）。
//!
//! # 这是「两个口子」里的第二个
//!
//! `CLAUDE.md` 的「不许加的功能」里有一条「内置代理 / VPN」。0.19.0 由使用者
//! 拍板开了两个**有边界的**口子，这是其中之一（另一个是 `platform::firewall`）。
//! 那一节的约束逐条对应到代码：
//!
//! | 约束 | 这里怎么落实 |
//! |---|---|
//! | 只在当次点击后改 | 这个模块只有函数，**没有定时器、没有启动钩子** |
//! | 改前把原值记下来 | [`apply`] 返回改之前的 [`ProxyState`]，调用方存着 |
//! | 面板里能回滚 | [`restore`] 收那份原值，把四个键还原到位 |
//!
//! 第一条靠调用点保证。**别给这个模块加任何自动触发点** —— 看门狗、启动流程、
//! 任何定时器碰它，这个口子就变成了「面板会自己改你的网络设置」。
//!
//! # ⚠ 改错会当场断网，而且不只影响那一个浏览器
//!
//! 系统代理是**整机**设置，所有跟随它的程序都会跟着走。`checkup` 那边原来
//! 就是拿这一条论证「面板不做这种按钮」的 —— 那个理由一个字都没错，
//! 所以这里的谨慎一点都没减：拒绝明显自毁的组合（见 [`plan`]）、
//! 改完立刻读回核对、原值随时能还原。
//!
//! # ⛔「注册表写进去了」不等于「已经生效」
//!
//! [`apply`] 会读回来核对，但它核对的是**设置本身**，不是「所有程序都已经用上
//! 新设置」。已经在跑的程序多半要重启才会重新读 —— 跟 `locale` 那条
//! 「区域格式只对之后新开的程序生效」是同一类诚实。界面上不许把前者说成后者。
//!
//! # 为什么不拼字符串
//!
//! 这里走 `reg.exe` 并把参数**直接交给 `Command::args`**，不经过 shell，
//! 所以代理地址里有什么字符都不构成注入 —— 跟 `firewall` 那边拼 PowerShell
//! 字符串、必须自己转义是两种处境。**别"顺手"把这里改成拼命令行**，
//! 那会凭空造出一个本来不存在的注入面。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{GateError, Result};

/// 系统代理设置所在的注册表键。**HKCU —— 当前用户，不是整机策略。**
const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// 当前系统代理长什么样。
///
/// 每个可选项的 `None` 都表示**这个值不存在**，跟「存在但是空串」是两回事 ——
/// 还原时前者要把值删掉、后者要写一个空串。跟 `locale::LocaleState::original_ui`
/// 是同一条：合并这两种状态，回滚就会留下一个使用者原来没有的键。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "ProxyState")]
pub struct ProxyState {
    /// `ProxyEnable`。手动代理开没开。
    pub enabled: bool,
    /// `ProxyServer`，形如 `127.0.0.1:7890`。
    pub server: Option<String>,
    /// `ProxyOverride`，绕过列表。
    pub bypass: Option<String>,
    /// `AutoConfigURL`，PAC 脚本地址。
    ///
    /// **跟 `server` 是两条独立的路径。** 只看 `enabled` / `server` 会漏掉它：
    /// 手动代理关着、PAC 却还挂着的机器，流量照样被 PAC 接管，
    /// 而界面会显示「没开代理」。
    pub pac: Option<String>,
}

/// 一次注册表写操作。分出来是为了让「要动哪几个键」可以纯函数化、能单测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `reg add KEY /v name /t kind /d data /f`
    Set {
        name: &'static str,
        kind: &'static str,
        data: String,
    },
    /// `reg delete KEY /v name /f`
    Delete { name: &'static str },
}

impl Op {
    /// 这一步的 `reg.exe` 参数。**直接交给 `Command::args`，不拼字符串。**
    pub fn args(&self) -> Vec<String> {
        match self {
            Op::Set { name, kind, data } => vec![
                "add".into(),
                KEY.into(),
                "/v".into(),
                (*name).into(),
                "/t".into(),
                (*kind).into(),
                "/d".into(),
                data.clone(),
                "/f".into(),
            ],
            Op::Delete { name } => vec![
                "delete".into(),
                KEY.into(),
                "/v".into(),
                (*name).into(),
                "/f".into(),
            ],
        }
    }
}

/// `ProxyEnable` 的值。**读不懂就回 `None`，不当成关着。**
///
/// 塌成 `false` 会让「读不出来」看起来像「代理没开」，
/// 而后者是一个会让人放心的结论 —— 档案里那条 `unwrap_or_default()` 的同一类。
pub fn parse_enable(raw: &str) -> Option<bool> {
    let t = raw.trim();
    match t {
        "0x1" | "1" => Some(true),
        "0x0" | "0" => Some(false),
        _ => None,
    }
}

/// 要把系统改成 `next` 需要动哪几个键。**纯函数，不碰注册表。**
///
/// ⛔ 这里拦一种会当场断网的组合：**开着代理却没给地址**。
/// 那等于告诉整台机器「所有流量走一个不存在的代理」，后果是全机断网，
/// 而使用者多半只会觉得「点了个按钮然后网没了」。
pub fn plan(next: &ProxyState) -> Result<Vec<Op>> {
    let server = next.server.as_deref().unwrap_or("").trim().to_string();
    if next.enabled && server.is_empty() {
        return Err(GateError::Other(
            "要开系统代理就得给出代理地址（形如 127.0.0.1:7890）—— \
             开着代理却没有地址会让整台机器断网"
                .into(),
        ));
    }

    let mut ops = vec![Op::Set {
        name: "ProxyEnable",
        kind: "REG_DWORD",
        data: if next.enabled { "1" } else { "0" }.into(),
    }];

    // 关代理时**不删 `ProxyServer`**：留着它，使用者下次开就不用重填，
    // 回滚也还原得回去。关掉的判据是 `ProxyEnable`，不是「地址还在不在」。
    push_str_or_delete(&mut ops, "ProxyServer", next.server.as_deref());
    push_str_or_delete(&mut ops, "ProxyOverride", next.bypass.as_deref());
    // PAC 要单独处理：留着一个 PAC 而只关手动代理，等于没关。
    push_str_or_delete(&mut ops, "AutoConfigURL", next.pac.as_deref());

    Ok(ops)
}

/// `Some(v)` → 写进去；`None` → 把这个值删掉。
///
/// 「没有这个值」和「值是空串」必须走不同的分支，否则回滚会给使用者
/// 留下一个他原来没有的键。
fn push_str_or_delete(ops: &mut Vec<Op>, name: &'static str, v: Option<&str>) {
    match v {
        Some(s) => ops.push(Op::Set {
            name,
            kind: "REG_SZ",
            data: s.to_string(),
        }),
        None => ops.push(Op::Delete { name }),
    }
}

#[cfg(windows)]
fn reg(args: &[String]) -> Result<()> {
    let out = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(args)
        .output()?;
    // `reg delete` 删一个本来就不存在的值会返回非零 —— 那不是失败。
    if !out.status.success() && args.first().map(|s| s.as_str()) != Some("delete") {
        return Err(GateError::Other(format!(
            "写注册表失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn reg(_args: &[String]) -> Result<()> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

/// 读当前系统代理设置。
#[cfg(windows)]
pub fn read() -> Result<ProxyState> {
    let out = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(["query", KEY])
        .output()?;
    if !out.status.success() {
        return Err(GateError::Other("读不出系统代理设置".into()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let vals = super::checkup::parse_reg_values(&text);
    let get = |n: &str| {
        vals.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(n))
            .map(|(_, v)| v.clone())
    };
    Ok(ProxyState {
        // 读不懂时按「没开」呈现，但那是**显示**上的保守选择；
        // 真正的判据在 `parse_enable`，它把读不懂和关着分开。
        enabled: get("ProxyEnable")
            .as_deref()
            .and_then(parse_enable)
            .unwrap_or(false),
        server: get("ProxyServer"),
        bypass: get("ProxyOverride"),
        pac: get("AutoConfigURL"),
    })
}

#[cfg(not(windows))]
pub fn read() -> Result<ProxyState> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

/// 把系统代理改成 `next`，返回**改之前**的样子供回滚。
///
/// ⚠ **调用方必须已经拿到使用者当次的点击。** 这个模块不提供任何自动触发点。
///
/// 改完会读回来核对：对不上就报错，而不是报成功 —— 跟升级那条
/// 「不看安装器退出码，看磁盘」、卸载那条「执行完必须复扫」是同一条规矩。
pub fn apply(next: &ProxyState) -> Result<ProxyState> {
    let before = read()?;
    let ops = plan(next)?;
    for op in &ops {
        reg(&op.args())?;
    }
    let after = read()?;
    if &after != next {
        return Err(GateError::Other(format!(
            "系统代理写进去了，但读回来对不上（想要 {next:?}，读到 {after:?}）。\
             原值是 {before:?}，可以在面板里还原。"
        )));
    }
    crate::audit::write(&format!(
        "系统代理：{} -> {}",
        describe(&before),
        describe(&after)
    ));
    Ok(before)
}

/// 还原到 `original`。
pub fn restore(original: &ProxyState) -> Result<()> {
    for op in &plan(original)? {
        reg(&op.args())?;
    }
    crate::audit::write(&format!("系统代理已还原为 {}", describe(original)));
    Ok(())
}

/// 给日志看的人话。**不带出任何凭证** —— 代理地址里可能有 `user:pass@`。
pub fn describe(s: &ProxyState) -> String {
    let mut parts = Vec::new();
    parts.push(if s.enabled {
        "手动代理开".to_string()
    } else {
        "手动代理关".to_string()
    });
    if s.server.is_some() {
        parts.push("有地址".into());
    }
    if s.pac.is_some() {
        parts.push("挂着 PAC".into());
    }
    parts.join("、")
}

// ------------------------------------------------------------------ 原值备份

/// 改之前的那份存在哪。
///
/// **必须落盘。** 时区那份存在 `AppState` 的内存里是够的，因为它承诺的只是
/// 「退出时还原」；而这里承诺的是 DISCLAIMER §5.2 那句「面板里能回滚到原值」——
/// 只存内存的话，面板一关原值就没了，那句话当场变成空话。
fn backup_path() -> std::path::PathBuf {
    crate::paths::state_dir().join("proxy-backup.json")
}

/// 连续改两次时，备份要保留**最早**那一份。
///
/// 使用者点「修」→ 又改成别的 → 再点「还原」，期望回到的是面板动手**之前**，
/// 不是上一次改之前。所以第一份写下去之后就不许被覆盖，直到还原完成把它清掉。
/// 纯函数，单测钉着 —— 它不碰磁盘，也就不碰真实运行期状态。
pub fn keep_earliest<'a>(
    existing: Option<&'a ProxyState>,
    before: &'a ProxyState,
) -> &'a ProxyState {
    existing.unwrap_or(before)
}

/// 读回存着的原值。`None` = 没有备份，界面上不该出现「还原」按钮。
pub fn backup() -> Option<ProxyState> {
    let text = std::fs::read_to_string(backup_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// 记下原值。**已经有一份就不覆盖**，见 [`keep_earliest`]。
pub fn save_backup(before: &ProxyState) -> Result<()> {
    let existing = backup();
    let keep = keep_earliest(existing.as_ref(), before);
    let dir = crate::paths::state_dir();
    std::fs::create_dir_all(&dir)?;
    let text = serde_json::to_string_pretty(keep)
        .map_err(|e| GateError::Other(format!("存不下系统代理原值：{e}")))?;
    std::fs::write(backup_path(), text)?;
    Ok(())
}

/// 还原完了把备份清掉 —— 留着它，界面会一直显示一个没有意义的「还原」按钮。
pub fn clear_backup() {
    let _ = std::fs::remove_file(backup_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⛔ 连续改两次，要回滚到的是**面板动手之前**那一份。
    ///
    /// 覆盖成「上一次改之前」的话，使用者点还原会回到一个他从来没要过的中间态，
    /// 而且真正的原值再也找不回来了。
    #[test]
    fn a_second_change_does_not_overwrite_the_earliest_backup() {
        let original = ProxyState::default();
        let midway = ProxyState {
            enabled: true,
            server: Some("127.0.0.1:7890".into()),
            ..ProxyState::default()
        };
        assert_eq!(keep_earliest(Some(&original), &midway), &original);
        // 还没有备份时，这一份就是最早的。
        assert_eq!(keep_earliest(None, &midway), &midway);
    }

    /// ⛔ 开着代理却没给地址 = 整机断网。必须当场拒绝，不能写下去。
    #[test]
    fn enabling_without_a_server_is_refused() {
        let e = plan(&ProxyState {
            enabled: true,
            ..ProxyState::default()
        })
        .unwrap_err();
        assert!(e.to_string().contains("代理地址"), "{e}");

        // 空白串也算没给。
        let e = plan(&ProxyState {
            enabled: true,
            server: Some("   ".into()),
            ..ProxyState::default()
        })
        .unwrap_err();
        assert!(e.to_string().contains("代理地址"), "{e}");
    }

    /// 关代理时不删地址 —— 判据是 `ProxyEnable`，不是「地址还在不在」。
    #[test]
    fn turning_it_off_keeps_the_address_for_next_time() {
        let ops = plan(&ProxyState {
            enabled: false,
            server: Some("127.0.0.1:7890".into()),
            ..ProxyState::default()
        })
        .unwrap();
        assert!(ops.contains(&Op::Set {
            name: "ProxyEnable",
            kind: "REG_DWORD",
            data: "0".into()
        }));
        assert!(ops.contains(&Op::Set {
            name: "ProxyServer",
            kind: "REG_SZ",
            data: "127.0.0.1:7890".into()
        }));
    }

    /// ⛔「原来没有这个值」必须还原成**删掉**，不是写空串。
    ///
    /// 写空串会给使用者留下一个他原来没有的注册表项 —— 而 PAC 这一项
    /// 留下一个空的 `AutoConfigURL`，某些版本的 Windows 会当成「配置了 PAC」。
    #[test]
    fn a_value_that_was_absent_is_restored_by_deleting_it() {
        let ops = plan(&ProxyState::default()).unwrap();
        assert!(ops.contains(&Op::Delete {
            name: "ProxyServer"
        }));
        assert!(ops.contains(&Op::Delete {
            name: "ProxyOverride"
        }));
        assert!(ops.contains(&Op::Delete {
            name: "AutoConfigURL"
        }));
    }

    /// PAC 跟手动代理是两条独立的路径，计划里必须各有一步。
    ///
    /// 漏掉 PAC 的后果是：手动代理关了、PAC 还挂着，流量照样被接管，
    /// 而界面显示「没开代理」。
    #[test]
    fn a_pac_url_is_planned_separately_from_the_manual_proxy() {
        let ops = plan(&ProxyState {
            enabled: false,
            pac: Some("http://x/proxy.pac".into()),
            ..ProxyState::default()
        })
        .unwrap();
        assert!(ops.contains(&Op::Set {
            name: "AutoConfigURL",
            kind: "REG_SZ",
            data: "http://x/proxy.pac".into()
        }));
    }

    /// 读不懂的 `ProxyEnable` 要回 `None`，不能塌成「关着」。
    #[test]
    fn an_unreadable_enable_flag_is_unknown_not_off() {
        assert_eq!(parse_enable("0x1"), Some(true));
        assert_eq!(parse_enable("0x0"), Some(false));
        assert_eq!(parse_enable(""), None);
        assert_eq!(parse_enable("REG_DWORD"), None);
    }

    /// 参数是直接交给 `Command::args` 的，所以这里钉的是**参数向量**，
    /// 不是一条拼出来的命令行。地址里有什么字符都不构成注入。
    #[test]
    fn args_are_a_vector_not_a_command_line() {
        let a = Op::Set {
            name: "ProxyServer",
            kind: "REG_SZ",
            data: r#"127.0.0.1:7890" & del C:\ /s"#.into(),
        }
        .args();
        assert_eq!(a[0], "add");
        assert_eq!(a.last().unwrap(), "/f");
        // 危险的那一串原样待在**一个**参数里，没有被切开、也没有变成第二条命令。
        assert!(a.iter().any(|s| s.contains("del C:\\ /s")));
        assert_eq!(a.len(), 9);
    }

    /// 日志里不许出现代理地址 —— 它可能带 `user:pass@`。
    #[test]
    fn the_log_line_never_carries_the_address() {
        let s = describe(&ProxyState {
            enabled: true,
            server: Some("http://alice:hunter2@proxy.example.com:7890".into()),
            ..ProxyState::default()
        });
        assert!(!s.contains("hunter2"), "{s}");
        assert!(!s.contains("proxy.example.com"), "{s}");
        assert!(s.contains("有地址"), "{s}");
    }
}
