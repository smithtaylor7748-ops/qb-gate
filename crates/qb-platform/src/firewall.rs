//! Windows 防火墙**出站**规则的增删查（0.19.0）。
//!
//! # 这是「两个口子」里的第一个
//!
//! `CLAUDE.md` 的「不许加的功能」里有一条「内置代理 / VPN」。0.19.0 由使用者
//! 拍板开了两个**有边界的**口子，这是其中之一。那一节的三条硬约束就是这个
//! 模块存在的全部前提，逐条对应到代码：
//!
//! | 约束 | 这里怎么落实 |
//! |---|---|
//! | 按 exe 路径限定，规则名统一前缀 | [`RULE_PREFIX`] + [`rule_name`]，`-Program` 收绝对路径 |
//! | 加了什么必须看得见、能一键撤销 | [`list`] 只列自己加的，[`remove_all`] 只删自己加的 |
//! | 绝不自动加 | 这个模块**没有**定时器、没有启动钩子，只有函数 |
//!
//! 第三条靠调用点保证：这里一个自动触发点都不提供，加规则只能由命令层
//! 在使用者当次点击后调用。**别给这个模块加任何「顺手帮你开」的入口。**
//!
//! # ⛔ 只加出站、只加 Block
//!
//! 两条都不是风格问题：
//!
//! - **入站规则一条都不加。** 目标是「别让浏览器从物理网卡直连出去」，
//!   那是出站方向的事。加入站规则既解决不了这个问题，又会改变这台机器
//!   对外的可达性 —— 那是使用者没要过的另一件事。
//! - **不加 Allow 规则。** Windows 防火墙默认就放行出站，所以一条
//!   「允许走 TUN」的规则**什么都没做**，却会让界面上看起来「我已经把
//!   流量绑到 TUN 上了」。那是一句谎话。真正起作用的只有 Block，
//!   而 Block 要**按接口限定**（见下）。
//!
//! # 为什么是「按物理网卡拦」而不是「只放行 TUN」
//!
//! 直觉写法是「允许走 TUN + 阻止其它」，但 Windows 防火墙里 Block 优先于
//! Allow，靠优先级凑不出这个语义。能精确表达的只有一种：
//! **把 Block 规则限定在使用者点名的那几个物理网卡上**（`-InterfaceAlias`）。
//! TUN 接口不在规则里，所以走 TUN 的流量根本不经过这条规则。
//!
//! 这也顺带满足了需求文档那条「不对未知接口、远程桌面、局域网共享或系统服务
//! 做广泛阻断」—— 规则只落在被点名的接口上，没点到的一概不受影响。
//!
//! # ⚠ 规则不随面板退出而消失
//!
//! 防火墙规则是系统级持久对象。面板关掉、卸载之后它还在，浏览器会继续上不了网，
//! 而使用者多半想不到是这里。DISCLAIMER §5.2 与 §6 都写了这一条，
//! 撤销入口必须一直留在界面上。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{GateError, Result};

/// 本面板加的规则统一用这个前缀。
///
/// **不只是为了自己找得到。** 更要紧的是让使用者在 Windows 自带的
/// 「高级安全 Windows Defender 防火墙」里一眼认出哪几条是这个面板加的、
/// 自己也删得掉 —— 看不见的规则等于埋雷。
pub const RULE_PREFIX: &str = "QB Gate - ";

/// 一条由本面板加的出站规则。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename = "FirewallRule")]
pub struct Rule {
    /// 规则显示名，一定带 [`RULE_PREFIX`]。
    pub name: String,
    /// 被限定的可执行文件绝对路径。
    pub program: Option<String>,
    /// 被限定的接口别名。`None` = 读不出来（**不等于「不限定」**）。
    pub interface: Option<String>,
    pub enabled: bool,
}

/// 一块网卡。**只如实报告，不替使用者判断哪块是物理网卡、哪块是 TUN。**
///
/// 猜错的后果是把 Block 规则加到 TUN 上（浏览器直接断网），
/// 或者漏掉真正的物理网卡（等于没拦，而界面显示已生效）。
/// 两种都比「让使用者自己选」糟得多 —— 跟 `sysenv::iana_to_windows`
/// 查不到就不猜是同一条规矩。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename = "NetAdapter")]
pub struct Adapter {
    pub name: String,
    /// 驱动报上来的描述，形如 `Realtek Gaming 2.5GbE Family Controller`。
    /// 使用者主要靠这一行认出哪块是虚拟网卡。
    pub description: String,
    pub up: bool,
}

/// 规则名：前缀 + 程序名 + 接口。
///
/// 要稳定且可反查 —— 加完之后要靠这个名字找回它、删掉它。
pub fn rule_name(exe: &str, interface: &str) -> String {
    let stem = std::path::Path::new(exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(exe);
    format!("{RULE_PREFIX}{stem} 禁走 {interface}")
}

/// 这条规则是不是本面板加的。
///
/// **删除路径上的唯一判据。** 不带前缀的一律不碰 —— 误删使用者自己的
/// 防火墙规则是不可逆的，而且他多半不知道是谁删的。
pub fn is_ours(name: &str) -> bool {
    name.starts_with(RULE_PREFIX)
}

/// 单引号在 PowerShell 单引号字符串里要写成两个。
///
/// 路径与接口别名都可能来自使用者输入或系统返回，不是我们自己造的常量。
fn esc(s: &str) -> String {
    s.replace('\'', "''")
}

/// 把 PowerShell 的通配符转义掉。**只用在「按名字找」的位置。**
///
/// 网卡别名里带 `*` 不是假想：Windows 自己就把 WiFi Direct 那几块虚拟网卡叫
/// 「本地连接* 1」/「Local Area Connection* 2」（本机 `Get-NetAdapter
/// -IncludeHidden` 一口气列出九块），而使用者还可以随手把网卡改成任何名字。
///
/// 这些名字会进规则名。到了 `Remove-NetFirewallRule -DisplayName` 那里，
/// `*` 是通配符 —— 一句「撤销这一条」会把所有前缀相同、后缀相同的规则一起
/// 删掉。被误删的都带着本面板的前缀，但**使用者没要求删的那几条也没了**，
/// 而防火墙规则删掉不可恢复，他还不会知道是谁删的。
///
/// ⛔ **别拿它去转 `New-NetFirewallRule -DisplayName`。** 那是创建，名字按
/// 字面量存；转了会把反引号本身写进规则名，界面上从此显示一串反引号，
/// 而且跟 [`rule_name`] 算出来的名字对不上，反而删不掉。
fn esc_wildcard(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '`') {
            out.push('`');
        }
        out.push(c);
    }
    out
}

/// 生成「加一条出站阻止规则」的脚本。**纯函数，单测钉着它的形状。**
///
/// 分出来是因为这条脚本写错的代价很高：方向反了会改变这台机器的可达性，
/// 动作反了（Allow）会变成一条什么都没做却让人以为生效了的规则，
/// 漏了 `-InterfaceAlias` 会把浏览器所有出口都拦掉、连 TUN 一起断。
pub fn add_script(exe: &str, interface: &str) -> String {
    format!(
        "New-NetFirewallRule -DisplayName '{}' -Direction Outbound -Action Block \
         -Program '{}' -InterfaceAlias '{}' -Profile Any -Enabled True | Out-Null",
        esc(&rule_name(exe, interface)),
        esc(exe),
        esc(interface),
    )
}

#[cfg(windows)]
fn powershell(script: &str) -> Result<String> {
    let out = crate::process::powershell_std(script).output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GateError::Other(if err.is_empty() {
            "防火墙操作失败（通常是没有管理员权限）".into()
        } else {
            err
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(windows))]
fn powershell(_script: &str) -> Result<String> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

/// 当前由本面板加的所有出站规则。
///
/// 只列带前缀的那些 —— 界面上那句「面板加了这几条」必须只包含面板加的。
pub fn list() -> Result<Vec<Rule>> {
    // ⚠ `ConvertTo-Json -InputObject @(...) -Compress` 是 5.1 上唯一两头都对的
    // 写法：空集出 `[]`、单条出 `[{...}]`。`-AsArray` 是 6.2 才有的参数，
    // 用了它整条管线会报错、stdout 全空（档案 §7.17 那个坑）。
    let script = format!(
        "$r = @(Get-NetFirewallRule -DisplayName '{p}*' -ErrorAction SilentlyContinue | \
           ForEach-Object {{ \
             $app = $_ | Get-NetFirewallApplicationFilter -ErrorAction SilentlyContinue; \
             $if  = $_ | Get-NetFirewallInterfaceFilter  -ErrorAction SilentlyContinue; \
             [pscustomobject]@{{ \
               name = $_.DisplayName; \
               program = $app.Program; \
               interface = ($if.InterfaceAlias -join ','); \
               enabled = [bool]($_.Enabled -eq 'True') }} }}); \
         ConvertTo-Json -InputObject @($r) -Compress",
        p = esc(RULE_PREFIX)
    );
    let text = powershell(&script)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let raw: Vec<Rule> = serde_json::from_str(&text)
        .map_err(|e| GateError::Other(format!("读不出防火墙规则列表：{e}")))?;
    // 再滤一次。`-DisplayName 'QB Gate - *'` 里的 `*` 是通配，理论上够用，
    // 但删除路径只认 `is_ours`，列出来的也得是同一套判据，否则界面上
    // 会出现一条「面板加的」却删不掉的规则。
    Ok(raw.into_iter().filter(|r| is_ours(&r.name)).collect())
}

/// 当前所有网卡。**不分类，只如实报告。**
pub fn adapters() -> Result<Vec<Adapter>> {
    // 跟 `list` 同一条：`ConvertTo-Json -InputObject @(...)` 是 5.1 上唯一两头
    // 都对的写法。这里不是 `format!`，所以大括号照原样写、不要转义。
    let script = "$r = @(Get-NetAdapter -ErrorAction SilentlyContinue | \
           ForEach-Object { [pscustomobject]@{ \
             name = $_.Name; \
             description = $_.InterfaceDescription; \
             up = [bool]($_.Status -eq 'Up') } }); \
         ConvertTo-Json -InputObject @($r) -Compress";
    let text = powershell(script)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).map_err(|e| GateError::Other(format!("读不出网卡列表：{e}")))
}

/// 给 `exe` 加一条「禁止从 `interface` 出站」的规则。
///
/// ⚠ **调用方必须已经拿到使用者当次的点击。** 这个模块不提供任何自动触发点，
/// 见文件头第三条硬约束。
pub fn add_block(exe: &str, interface: &str) -> Result<Rule> {
    let name = rule_name(exe, interface);
    powershell(&add_script(exe, interface))?;
    crate::audit::write(&format!("防火墙：已加出站阻止规则「{name}」"));
    Ok(Rule {
        name,
        program: Some(exe.to_string()),
        interface: Some(interface.to_string()),
        enabled: true,
    })
}

/// 删掉一条**本面板加的**规则。名字不带前缀一律拒绝。
pub fn remove(name: &str) -> Result<()> {
    if !is_ours(name) {
        return Err(GateError::Other(format!(
            "「{name}」不是 QB Gate 加的规则，面板不碰它"
        )));
    }
    // 先转通配符再转引号：两件事互不影响，但顺序反了读起来像在转义反引号。
    //
    // `-ErrorAction Stop` 而不是 `SilentlyContinue`：删不掉必须报出来。
    // 静默跳过会让「撤销失败」看起来像「撤销成功」，而没删掉的那条规则
    // 还在拦流量。
    powershell(&format!(
        "Remove-NetFirewallRule -DisplayName '{}' -ErrorAction Stop",
        esc(&esc_wildcard(name))
    ))?;
    crate::audit::write(&format!("防火墙：已撤销规则「{name}」"));
    Ok(())
}

/// 一键撤销：把本面板加过的规则全删掉。返回删掉了几条。
///
/// 逐条走 [`remove`]，所以「只删自己加的」这条判据只有一处实现。
///
/// # ⛔ 删不掉的要报出来
///
/// 老代码写的是 `if remove(&r.name).is_ok() { n += 1 }` —— 删失败的那几条被
/// 整个吞掉，界面上只看到一句「已撤销 N 条规则」的成功提示。而这个功能的
/// 全部意义就是**加了什么必须撤得干净**：规则不随面板退出消失，撤不掉的
/// 那几条会一直拦着浏览器，而使用者刚被告知「撤销完了」。
///
/// 同一个文件里 `removing_a_foreign_rule_is_refused` 钉的是同一条规矩：
/// 「静默跳过会让『撤销失败』看起来像『撤销成功』」。那里守住了，这里漏了。
pub fn remove_all() -> Result<usize> {
    let mine = list()?;
    let mut n = 0;
    let mut failed: Vec<String> = Vec::new();
    for r in &mine {
        match remove(&r.name) {
            Ok(()) => n += 1,
            Err(e) => failed.push(format!("「{}」：{e}", r.name)),
        }
    }
    if !failed.is_empty() {
        return Err(GateError::Other(format!(
            "撤销了 {n} 条，还有 {} 条没删掉（它们仍然在拦流量）：{}",
            failed.len(),
            failed.join("；")
        )));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 规则名必须带前缀 —— 它是删除路径的唯一判据。
    #[test]
    fn every_rule_we_make_carries_the_prefix() {
        let n = rule_name(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            "以太网",
        );
        assert!(is_ours(&n), "{n}");
        assert!(n.contains("chrome.exe"), "{n}");
        assert!(n.contains("以太网"), "{n}");
    }

    /// ⛔ 别人的规则一条都不许认。误删防火墙规则不可逆。
    #[test]
    fn other_peoples_rules_are_never_ours() {
        assert!(!is_ours("Chrome"));
        assert!(!is_ours("Core Networking - DNS (UDP-Out)"));
        assert!(!is_ours("qb gate - 小写前缀不算"));
        assert!(!is_ours(""));
    }

    /// 删除路径对不带前缀的名字必须**报错**，不能静默跳过。
    ///
    /// 静默跳过会让「撤销失败」看起来像「撤销成功」。
    #[test]
    fn removing_a_foreign_rule_is_refused() {
        let e = remove("Core Networking - DNS (UDP-Out)").unwrap_err();
        assert!(e.to_string().contains("不是 QB Gate"), "{e}");
    }

    /// ⛔ 只出站、只 Block、必须按 exe 与接口限定。
    ///
    /// 这四条任何一条写错，这个功能的语义就变了：
    /// 方向反了改的是可达性；Allow 是一条什么都没做的规则；
    /// 漏了 Program 会拦掉整台机器；漏了 InterfaceAlias 会连 TUN 一起断。
    #[test]
    fn the_rule_is_outbound_block_scoped_to_one_exe_and_one_interface() {
        let s = add_script(r"C:\x\chrome.exe", "以太网");
        assert!(s.contains("-Direction Outbound"), "{s}");
        assert!(s.contains("-Action Block"), "{s}");
        assert!(s.contains(r"-Program 'C:\x\chrome.exe'"), "{s}");
        assert!(s.contains("-InterfaceAlias '以太网'"), "{s}");
        // 入站一条都不加。
        assert!(!s.contains("Inbound"), "{s}");
        // Allow 规则是谎话 —— 出站本来就默认放行。
        assert!(!s.contains("-Action Allow"), "{s}");
    }

    /// 网卡名里的 `*` 不许当通配符用。
    ///
    /// 「本地连接* 1」是 Windows 自己给 WiFi Direct 虚拟网卡起的名字，
    /// 不转义的话「撤销这一条」会顺手把别的规则一起删掉。
    #[test]
    fn a_star_in_an_adapter_name_cannot_widen_a_delete() {
        assert_eq!(esc_wildcard("本地连接* 1"), "本地连接`* 1");
        assert_eq!(
            esc_wildcard("Local Area Connection* 2"),
            "Local Area Connection`* 2"
        );
        assert_eq!(esc_wildcard("以太网"), "以太网");
        // `?` `[` `]` 和反引号自己也都是通配语法的一部分。
        assert_eq!(esc_wildcard("a?b[c]d`e"), "a`?b`[c`]d``e");
    }

    /// ⛔ 创建那条**不能**转义：规则名按字面量存，转了就跟 `rule_name` 对不上。
    #[test]
    fn the_name_we_create_is_the_name_we_look_up() {
        let exe = r"C:\x\chrome.exe";
        let alias = "本地连接* 1";
        let name = rule_name(exe, alias);
        let s = add_script(exe, alias);
        assert!(s.contains(&format!("-DisplayName '{name}'")), "{s}");
        assert!(!s.contains('`'), "创建脚本里不该有反引号：{s}");
    }

    /// 路径里的单引号要转义，否则就是一条命令注入。
    ///
    /// **别断言「注入的文字消失了」。** 第一版就是那么写的，当场挂 ——
    /// 转义的本意恰恰是把那串文字原样留在引号里当**数据**，所以
    /// `Remove-NetFirewallRule -All` 这几个字当然还在。要钉的不是它在不在，
    /// 是它**跳不跳得出那对引号**。
    ///
    /// PowerShell 单引号串里唯一的转义就是 `''`，于是判据有两条：
    /// 把成对的引号全抠掉之后不该剩落单的；按 `'' -> '` 还原要等于原文。
    #[test]
    fn a_quote_in_a_path_cannot_break_out_of_the_string() {
        let payload = r"C:\x'; Remove-NetFirewallRule -All; '\chrome.exe";
        let escaped = esc(payload);

        assert!(
            !escaped.replace("''", "").contains('\''),
            "有落单的引号，能提前闭合字符串：{escaped}"
        );
        assert_eq!(escaped.replace("''", "'"), payload, "转义改动了内容");

        // 整条脚本里，这个路径只以转义后的形态出现。
        let s = add_script(payload, "以太网");
        assert!(s.contains(&escaped), "{s}");
    }
}
