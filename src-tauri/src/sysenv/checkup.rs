//! 运行环境体检。
//!
//! 「中文环境识别」那 10 项指纹是浏览器里算的（`src/lib/signals.ts`，改编自
//! FuckClaude）。这里补的是**只有本机才看得到**的那几项：系统代理、IPv6、
//! 浏览器 DoH 策略、以及 MCP 配置里有没有明文密钥。
//!
//! 项目划分参考 check-cc 与 claude-antiban-macos（都是 MIT）的体检清单，
//! Windows 侧的读法是自己写的。
//!
//! # 为什么大部分项「只报告不代劳」
//!
//! 使用者选的是「诊断 + 修可控项」。可是这几项里真正**安全可逆**的只有时区：
//!
//! - 关掉系统代理 → 可能当场把他的网断掉；
//! - 关掉 IPv6 → 要管理员权限 + 重启才生效，而且他可能正靠 v6 上网；
//! - 改 DoH 策略 → 那是 HKLM 下的企业策略，动它会影响整机所有用户。
//!
//! 所以这几项给的是**原样可复制的命令**加上代价说明，由他自己决定 ——
//! 与「中文字体那 14 分修不掉就不提供修复按钮，也不假装能修」是同一条。
//! 面板不做那种「点一下，然后你发现自己上不了网」的按钮。
//!
//! # 密钥扫描只报位置，绝不报内容
//!
//! 跟 `relay::ProviderView` 那条「结构上就装不下 Key」同一个原则：
//! 这里只回文件路径与字段名。把扫出来的密钥打进结构体，早晚会有人
//! 把它打进日志、截图、或者贴进 issue 里。

use serde::Serialize;
use std::path::PathBuf;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pass,
    Warn,
    Fail,
    /// 查不出来。**跟 Pass 是两回事**，界面上不许合并显示。
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckItem {
    pub id: String,
    pub label: String,
    pub state: State,
    pub detail: String,
    /// 面板能不能安全地替你修。
    pub fixable: bool,
    /// 不能代劳时，给一条原样可复制的命令或步骤。
    pub manual: Option<String>,
}

fn item(id: &str, label: &str, state: State, detail: impl Into<String>) -> CheckItem {
    CheckItem {
        id: id.into(),
        label: label.into(),
        state,
        detail: detail.into(),
        fixable: false,
        manual: None,
    }
}

// ------------------------------------------------------------ 注册表读取

#[cfg(windows)]
fn reg_query(path: &str, name: &str) -> Option<String> {
    let out = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(["query", path, "/v", name])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // 形如：`    ProxyEnable    REG_DWORD    0x1`
    let line = text.lines().find(|l| l.contains(name))?;
    let val = line.split_whitespace().last()?.to_string();
    Some(val)
}

#[cfg(not(windows))]
fn reg_query(_path: &str, _name: &str) -> Option<String> {
    None
}

// ------------------------------------------------------------------ 各项

fn check_proxy() -> CheckItem {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let enabled = reg_query(KEY, "ProxyEnable").map(|v| v.ends_with('1'));
    match enabled {
        None => item("proxy", "系统代理", State::Unknown, "读不出系统代理设置"),
        Some(false) => item(
            "proxy",
            "系统代理",
            State::Pass,
            "系统代理没开 —— 出口由路由/TUN 决定，跟面板量到的是同一条路",
        ),
        Some(true) => {
            let server = reg_query(KEY, "ProxyServer").unwrap_or_else(|| "（读不出地址）".into());
            let mut it = item(
                "proxy",
                "系统代理",
                State::Warn,
                format!(
                    "系统代理开着（{server}）。面板测出口时是绕过系统代理的，\
                     所以面板看到的出口和走系统代理的程序看到的可能不是同一个。"
                ),
            );
            it.manual = Some(
                "要关就去「设置 → 网络和 Internet → 代理」里关。\
                 面板不替你关 —— 你的网可能正是靠它出去的，关掉会当场断网。"
                    .into(),
            );
            it
        }
    }
}

fn check_ipv6() -> CheckItem {
    const KEY: &str = r"HKLM\SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters";
    let disabled = reg_query(KEY, "DisabledComponents");
    // 0xff = 全关。没有这个键 = 默认全开。
    let all_off = disabled
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("0xff"))
        .unwrap_or(false);
    if all_off {
        return item("ipv6", "IPv6", State::Pass, "IPv6 已全局关闭，不会从 v6 漏真实地址");
    }
    let mut it = item(
        "ipv6",
        "IPv6",
        State::Warn,
        "IPv6 开着。隧道只接管 IPv4 时，v6 流量会绕过它直接从本地出去 —— \
         这是最常见的一种「代理开着但还是暴露了」。",
    );
    it.manual = Some(
        r"管理员身份运行，然后重启：
reg add HKLM\SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters /v DisabledComponents /t REG_DWORD /d 0xff /f

面板不替你执行：它要管理员权限、要重启才生效，而且你可能正靠 IPv6 上网。"
            .into(),
    );
    it
}

fn check_doh() -> CheckItem {
    let chrome = reg_query(r"HKLM\SOFTWARE\Policies\Google\Chrome", "DnsOverHttpsMode");
    let edge = reg_query(r"HKLM\SOFTWARE\Policies\Microsoft\Edge", "DnsOverHttpsMode");
    match (chrome.as_deref(), edge.as_deref()) {
        (None, None) => {
            let mut it = item(
                "doh",
                "浏览器 DoH 策略",
                State::Unknown,
                "没有配置 DoH 策略 —— 浏览器用的是它自己的默认值，可能在用内置的 DoH 解析器。",
            );
            it.manual = Some(
                "要不要关掉浏览器自带的 DoH 取决于你的方案：\
                 走 TUN 全局接管时它无所谓；靠系统 DNS 分流时它会绕过你的分流。\
                 判断不了就交给「DNS 泄露 → 高级通过」那条让 Codex 看一眼。"
                    .into(),
            );
            it
        }
        (c, e) => item(
            "doh",
            "浏览器 DoH 策略",
            State::Pass,
            format!(
                "已有策略：Chrome={}，Edge={}",
                c.unwrap_or("未设"),
                e.unwrap_or("未设")
            ),
        ),
    }
}

/// 看起来像密钥的字段名。**只匹配名字，不看值。**
const SECRET_KEYS: &[&str] = &[
    "api_key", "apikey", "api-key", "token", "secret", "password", "auth_token",
    "access_key", "bearer",
];

/// 一处明文密钥。**只有位置，没有内容。**
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SecretHit {
    pub file: PathBuf,
    /// 字段名，比如 `api_key`。**值永远不带出来。**
    pub field: String,
}

/// 在一段文本里找像密钥的字段名。纯函数，可单测。
///
/// 故意做得很笨（只看字段名 + 后面跟着一个非空字符串），
/// 宁可多报也不漏报 —— 这一项的产出是「你自己去看一眼这个文件」，
/// 误报的代价只是多看一眼，漏报的代价是密钥躺在那儿没人知道。
pub fn find_secret_fields(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for k in SECRET_KEYS {
        let mut from = 0;
        while let Some(i) = lower[from..].find(k) {
            let at = from + i;
            let rest = &lower[at + k.len()..];
            // 后面得跟着 `"? *[:=] *"?` 再跟一个非空的值，才算「这里真写了个值」。
            let v = rest.trim_start_matches(['"', '\'', ' ']);
            if let Some(v) = v.strip_prefix([':', '=']) {
                let v = v.trim_start_matches(['"', '\'', ' ']);
                // `null` / `false` 是「没设」，跟空串一样不算泄漏。
                // 漏掉这一条的话，一份写着 "api_key": null 的干净配置会被报成红的，
                // 而使用者点开看什么都没有 —— 几次之后他就不看这一项了。
                let unset = v.is_empty()
                    || v.starts_with(['\n', ',', '}', ']'])
                    || v.starts_with("null")
                    || v.starts_with("false");
                if !unset {
                    if !out.contains(&k.to_string()) {
                        out.push(k.to_string());
                    }
                    break;
                }
            }
            from = at + k.len();
        }
    }
    out
}

fn scan_secrets() -> (Vec<SecretHit>, Vec<PathBuf>) {
    let mut hits = Vec::new();
    let mut looked = Vec::new();
    let home = dirs::home_dir();
    let mut files: Vec<PathBuf> = Vec::new();
    if let Some(h) = &home {
        files.push(h.join(".mcp.json"));
        files.push(h.join(".claude").join("settings.json"));
        files.push(h.join(".claude.json"));
        files.push(h.join(".codex").join("config.toml"));
    }
    // 每个账户槽位自己的 settings.json。
    let roots = crate::accounts::AccountRoots::current();
    if let Ok(rd) = std::fs::read_dir(&roots.panel) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                files.push(e.path().join("settings.json"));
                files.push(e.path().join(".mcp.json"));
            }
        }
    }
    for f in files {
        if !f.is_file() {
            continue;
        }
        looked.push(f.clone());
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for field in find_secret_fields(&text) {
            hits.push(SecretHit {
                file: f.clone(),
                field,
            });
        }
    }
    (hits, looked)
}

fn check_secrets() -> (CheckItem, Vec<SecretHit>) {
    let (hits, looked) = scan_secrets();
    let it = if looked.is_empty() {
        item(
            "secrets",
            "MCP / 配置里的明文密钥",
            State::Unknown,
            "没找到任何配置文件可扫 —— 跟「扫过了、很干净」不是一回事。",
        )
    } else if hits.is_empty() {
        item(
            "secrets",
            "MCP / 配置里的明文密钥",
            State::Pass,
            format!("扫了 {} 个配置文件，没看到明文密钥字段。", looked.len()),
        )
    } else {
        let mut it = item(
            "secrets",
            "MCP / 配置里的明文密钥",
            State::Fail,
            format!(
                "在 {} 处看到疑似明文密钥字段。面板只报位置不报内容，自己去看一眼。",
                hits.len()
            ),
        );
        it.manual = Some(
            "这些文件会被同步盘、备份、以及你随手贴出来的截图带走。\
             能换成环境变量或者面板的中转站配置（DPAPI 加密存）就换掉。"
                .into(),
        );
        it
    };
    (it, hits)
}

#[derive(Debug, Clone, Serialize)]
pub struct Checkup {
    pub items: Vec<CheckItem>,
    /// 明文密钥的位置清单。**没有任何密钥内容。**
    pub secrets: Vec<SecretHit>,
}

pub fn scan() -> Checkup {
    let (sec_item, secrets) = check_secrets();
    Checkup {
        items: vec![check_proxy(), check_ipv6(), check_doh(), sec_item],
        secrets,
    }
}

/// 目前没有任何一项是面板代劳的 —— 见文件头。保留这个入口是为了
/// 让「以后有安全可逆的项时加在哪」这件事只有一个答案。
pub fn fix(id: &str) -> Result<String> {
    Err(crate::error::GateError::Other(format!(
        "「{id}」这一项面板不代劳，旁边写了怎么自己动手以及代价。"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_plain_secrets_in_json_and_toml() {
        assert_eq!(
            find_secret_fields(r#"{"mcpServers":{"x":{"env":{"API_KEY":"sk-abc123"}}}}"#),
            vec!["api_key".to_string()]
        );
        assert_eq!(
            find_secret_fields("[provider]\ntoken = \"tok_live_1\"\n"),
            vec!["token".to_string()]
        );
    }

    /// 空值不报。`"api_key": ""` 是「还没填」，不是「泄漏」。
    #[test]
    fn empty_or_absent_values_are_not_hits() {
        assert!(find_secret_fields(r#"{"api_key": ""}"#).is_empty());
        assert!(find_secret_fields(r#"{"api_key": null}"#).is_empty());
        assert!(find_secret_fields("just the word token in prose").is_empty());
    }

    #[test]
    fn each_field_name_is_reported_once() {
        let hits = find_secret_fields(r#"{"api_key":"a","other":{"api_key":"b"}}"#);
        assert_eq!(hits, vec!["api_key".to_string()]);
    }

    /// 结构上就装不下密钥值 —— 与 `relay::ProviderView` 同一条原则。
    /// 这个断言是防未来的人「顺手」加个 value 字段。
    #[test]
    fn secret_hits_carry_no_value_field() {
        let h = SecretHit {
            file: PathBuf::from("C:\\x\\.mcp.json"),
            field: "api_key".into(),
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("api_key"));
        assert!(!json.contains("value"), "SecretHit 不许带密钥内容：{json}");
    }

    #[test]
    fn unknown_is_not_pass() {
        // 「没扫到文件」和「扫了很干净」必须是两种结论。
        let it = item("x", "x", State::Unknown, "");
        assert_ne!(it.state, State::Pass);
    }
}
