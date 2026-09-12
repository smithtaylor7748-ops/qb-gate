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


// ------------------------------------------------------ 环境变量残留扫描

/// 会影响 Claude Code 行为、或者会让面板与实际请求走两条路的环境变量。
///
/// 想法来自 Agent-Guard（它的 README 开篇就是「`.zshrc` 里还留着 `HTTPS_PROXY`
/// 和 `ANTHROPIC_BASE_URL`」）。那个项目**没有源码可抄**（仓库只是落地页），
/// 所以这里从头写，Windows 侧看的是 `HKCU\Environment` 与当前进程环境。
const WATCHED_ENV: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "CLAUDE_CONFIG_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EnvHit {
    pub name: String,
    /// 在哪儿设的：`用户环境变量`（注册表，重启也还在）或 `当前进程`。
    pub scope: String,
    /// **已经掩码过的**值。见 `mask_env_value`。
    pub shown: String,
}

/// 值怎么显示 —— 与 `SecretHit` 同一条原则：结构里装不下的东西就别装。
///
/// | 变量 | 显示什么 |
/// |---|---|
/// | `*_API_KEY` / `*_AUTH_TOKEN` / `*SECRET*` | 只说「已设置」，值一个字都不出现 |
/// | `*_BASE_URL` / `*_PROXY` | 只留 `scheme://host[:port]` |
/// | 其余 | 原样 |
///
/// URL 只留 host 不是洁癖：有些中转站把 token 直接放在路径里
/// （`https://x.com/v1/sk-xxxx`），也有人把凭证写成 `https://user:token@host`。
/// 显示全量等于把 Key 印在界面上、截图里、issue 里。
pub fn mask_env_value(name: &str, value: &str) -> String {
    let n = name.to_uppercase();
    if n.contains("KEY") || n.contains("TOKEN") || n.contains("SECRET") || n.contains("PASSWORD") {
        return "（已设置，值不显示）".into();
    }
    if n.contains("URL") || n.contains("PROXY") {
        return origin_only(value);
    }
    value.to_string()
}

/// 从一个 URL 里只取 `scheme://host[:port]`。认不出来就说认不出来，**不回退成原值**。
fn origin_only(raw: &str) -> String {
    let v = raw.trim();
    let Some((scheme, rest)) = v.split_once("://") else {
        // 没有 scheme 的（比如 `127.0.0.1:7890` 这种代理写法）只取到第一个 `/` 为止。
        let host = v.split(['/', '?', '#']).next().unwrap_or("");
        // 仍然要去掉可能的 user:pass@
        let host = host.rsplit('@').next().unwrap_or(host);
        return if host.is_empty() { "（认不出）".into() } else { host.to_string() };
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // ⚠ `user:token@host` —— 凭证在 @ 前面，必须丢掉。
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty() {
        "（认不出）".into()
    } else {
        format!("{scheme}://{host}")
    }
}

/// 解析 `reg query <key>` 的输出。返回 (名字, 值)。
///
/// 输出形如：`    ANTHROPIC_BASE_URL    REG_SZ    https://x.com/v1`
/// 值里可以有空格，所以**按类型段切**，不能 `split_whitespace` 取最后一个。
pub fn parse_reg_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with("HKEY_") {
            continue;
        }
        // 找 `REG_xxx` 这一段，它前面是名字，后面是值。
        let Some(ti) = t.find("    REG_") else { continue };
        let name = t[..ti].trim().to_string();
        let after = &t[ti + 4..];
        let Some(vi) = after.find("    ") else { continue };
        let value = after[vi..].trim().to_string();
        if !name.is_empty() {
            out.push((name, value));
        }
    }
    out
}

#[cfg(windows)]
fn user_env_vars() -> Vec<(String, String)> {
    let Ok(out) = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(["query", r"HKCU\Environment"])
        .output()
    else {
        return Vec::new();
    };
    parse_reg_values(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(windows))]
fn user_env_vars() -> Vec<(String, String)> {
    Vec::new()
}

fn scan_env() -> (CheckItem, Vec<EnvHit>) {
    let mut hits = Vec::new();

    for (name, value) in user_env_vars() {
        if WATCHED_ENV.iter().any(|w| w.eq_ignore_ascii_case(&name)) && !value.trim().is_empty() {
            hits.push(EnvHit {
                shown: mask_env_value(&name, &value),
                name,
                scope: "用户环境变量".into(),
            });
        }
    }
    for name in WATCHED_ENV {
        if let Ok(v) = std::env::var(name) {
            if v.trim().is_empty() {
                continue;
            }
            // 注册表里已经报过同名的就不重复 —— 进程环境多半就是从那儿来的。
            if hits.iter().any(|h| h.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            hits.push(EnvHit {
                name: (*name).to_string(),
                scope: "当前进程".into(),
                shown: mask_env_value(name, &v),
            });
        }
    }

    let redirecting = hits
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case("ANTHROPIC_BASE_URL"));
    let proxied = hits.iter().any(|h| h.name.to_uppercase().contains("PROXY"));

    let it = if hits.is_empty() {
        item(
            "env_residue",
            "环境变量残留",
            State::Pass,
            "没有会影响 Claude Code 的环境变量残留。",
        )
    } else {
        let mut why: Vec<&str> = Vec::new();
        if redirecting {
            why.push(
                "设了 ANTHROPIC_BASE_URL —— Claude Code 会走它指的地方，\
                 而不是中转站页上显示的那条。两处说的不是一件事。",
            );
        }
        if proxied {
            why.push(
                "设了代理变量 —— 面板测出口时是绕过系统代理的，\
                 所以面板量到的出口和请求实际走的路可能不一样。",
            );
        }
        let mut it = item(
            "env_residue",
            "环境变量残留",
            if redirecting || proxied { State::Warn } else { State::Pass },
            format!("找到 {} 个相关的环境变量。{}", hits.len(), why.join("")),
        );
        if redirecting || proxied {
            it.manual = Some(
                "改用户环境变量：设置 → 系统 → 系统信息 → 高级系统设置 → 环境变量。\
                 改完要重开终端 / 重开 Claude Code 才生效。\
                 面板不替你删 —— 这些变量可能正是你别的工作要用的。"
                    .into(),
            );
        }
        it
    };
    (it, hits)
}

// ---------------------------------------------------------- 出口一致性

/// 两条路出去的地方一样吗。
///
/// 面板测出口时**绕过系统代理**（量的是隧道），别的程序不一定 ——
/// 走系统代理的那些看到的可能是另一个出口。这一项就是把这个差异摆出来，
/// 专治那个最难查的问题：「面板说我在美国，为什么还是被当成国内」。
///
/// ⛔ 结果**不进门禁判定**。`gate::judge` 的输入永远只来自绕过代理的那一份 ——
/// 让代理软件决定门禁看到的出口，随便一个本地代理就能把出口伪装成白名单里那个。
pub fn compare_egress(
    direct: &crate::probe::ip::Reading,
    via_proxy: &crate::probe::ip::Reading,
) -> CheckItem {
    let (Some(a), Some(b)) = (direct.ip.as_deref(), via_proxy.ip.as_deref()) else {
        return item(
            "egress_consistency",
            "出口一致性",
            State::Unknown,
            "两条路里至少一条没探到出口，比不了。这跟「一致」不是一回事。",
        );
    };
    let ca = direct.distinct_countries();
    let cb = via_proxy.distinct_countries();

    if !ca.is_empty() && !cb.is_empty() && ca != cb {
        let mut it = item(
            "egress_consistency",
            "出口一致性",
            State::Fail,
            format!(
                "两条路出去的国家不一样：绕过代理是 {}，跟随系统代理是 {}。\
                 有程序会从另一个国家出去。",
                ca.join("/"),
                cb.join("/")
            ),
        );
        it.manual = Some(
            "常见原因是系统代理或某个环境变量里的代理只接管了一部分流量。\
             对照上面「环境变量残留」和「系统代理」两项一起看。"
                .into(),
        );
        return it;
    }
    if a != b {
        return item(
            "egress_consistency",
            "出口一致性",
            State::Warn,
            format!(
                "国家一致但 IP 不同（绕过代理 {a}，跟随系统代理 {b}）。\
                 双栈机器上很常见（一边 v4 一边 v6），通常不是问题。"
            ),
        );
    }
    item(
        "egress_consistency",
        "出口一致性",
        State::Pass,
        "绕过系统代理和跟随系统代理，两条路出去的是同一个地方。",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct Checkup {
    pub items: Vec<CheckItem>,
    /// 明文密钥的位置清单。**没有任何密钥内容。**
    pub secrets: Vec<SecretHit>,
    /// 相关环境变量的清单。值都掩码过，见 `mask_env_value`。
    pub env: Vec<EnvHit>,
}

/// 跑一轮体检。
///
/// 是 `async` 的唯一原因是出口一致性那一项要真发两轮请求（绕过代理 / 跟随代理）。
/// 其余几项全是本地读取，不联网。
pub async fn scan() -> Checkup {
    let (sec_item, secrets) = check_secrets();
    let (env_item, env) = scan_env();

    // 两条路并发问，省一半时间。
    let (direct, via_proxy) = tokio::join!(
        crate::probe::ip::reading(),
        crate::probe::ip::reading_via_system_proxy()
    );

    Checkup {
        items: vec![
            check_proxy(),
            compare_egress(&direct, &via_proxy),
            check_ipv6(),
            check_doh(),
            env_item,
            sec_item,
        ],
        secrets,
        env,
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

    // -------------------------------------------------- 环境变量掩码

    /// 核心不变量：**Key 的值一个字符都不许出现在输出里。**
    #[test]
    fn secret_env_values_never_appear() {
        for name in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "MY_SECRET", "X_PASSWORD"] {
            let out = mask_env_value(name, "sk-ant-super-secret-123");
            assert!(!out.contains("sk-ant"), "{name} 把值漏出来了：{out}");
            assert!(!out.contains("123"), "{name} 把值漏出来了：{out}");
        }
    }

    /// BASE_URL 的**路径段里可能带 token**，所以只留 host。
    /// 这条挂了，Key 会被印在界面上、截图里、issue 里。
    #[test]
    fn base_url_keeps_only_the_origin() {
        assert_eq!(
            mask_env_value("ANTHROPIC_BASE_URL", "https://relay.example.com/v1/sk-tok-abcd1234"),
            "https://relay.example.com"
        );
        assert_eq!(
            mask_env_value("ANTHROPIC_BASE_URL", "https://x.example.com:8443/v1?key=abc"),
            "https://x.example.com:8443"
        );
    }

    /// `https://user:token@host` —— 凭证在 @ 前面，必须丢掉。
    #[test]
    fn credentials_in_the_authority_are_dropped() {
        let out = mask_env_value("HTTPS_PROXY", "http://alice:hunter2@proxy.example.com:7890");
        assert_eq!(out, "http://proxy.example.com:7890");
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("alice"));
    }

    #[test]
    fn proxy_without_a_scheme_still_loses_the_path() {
        assert_eq!(mask_env_value("HTTP_PROXY", "127.0.0.1:7890"), "127.0.0.1:7890");
        assert_eq!(mask_env_value("HTTP_PROXY", "user:pw@127.0.0.1:7890"), "127.0.0.1:7890");
    }

    /// 模型名、配置目录不是秘密，原样显示才有用。
    #[test]
    fn harmless_values_are_shown_as_is() {
        assert_eq!(
            mask_env_value("ANTHROPIC_MODEL", "claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
    }

    // -------------------------------------------------- reg query 解析

    /// 值里有空格 —— 所以不能 `split_whitespace` 取最后一段。
    #[test]
    fn reg_values_with_spaces_survive() {
        let text = concat!(
            r"HKEY_CURRENT_USER\Environment", "\n",
            r"    ANTHROPIC_BASE_URL    REG_SZ    https://x.com/v1", "\n",
            r"    FOO    REG_EXPAND_SZ    C:\Program Files\a b", "\n",
        );
        let got = parse_reg_values(text);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "ANTHROPIC_BASE_URL");
        assert_eq!(got[0].1, "https://x.com/v1");
        assert_eq!(got[1].1, r"C:\Program Files\a b");
    }

    #[test]
    fn reg_header_line_is_not_a_value() {
        assert!(parse_reg_values(r"HKEY_CURRENT_USER\Environment").is_empty());
        assert!(parse_reg_values("").is_empty());
    }

    // -------------------------------------------------- 出口一致性

    fn reading(ip: Option<&str>, countries: &[(&str, &str)]) -> crate::probe::ip::Reading {
        crate::probe::ip::Reading {
            ip: ip.map(String::from),
            countries: countries.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }

    #[test]
    fn same_egress_both_ways_passes() {
        let a = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        assert_eq!(compare_egress(&a, &a).state, State::Pass);
    }

    /// 两条路出去的国家不一样 —— 这正是「面板说我在美国却被当成国内」的成因。
    #[test]
    fn different_country_is_a_failure_and_names_both() {
        let direct = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        let proxied = reading(Some("198.51.100.9"), &[("ippure", "HK")]);
        let it = compare_egress(&direct, &proxied);
        assert_eq!(it.state, State::Fail);
        assert!(it.detail.contains("US") && it.detail.contains("HK"), "两个都要列出来：{}", it.detail);
    }

    /// 双栈机器常见：国家一样、IP 不同。是提醒，不是错误。
    #[test]
    fn same_country_different_ip_is_only_a_warning() {
        let direct = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        let proxied = reading(Some("203.0.113.8"), &[("ippure", "US")]);
        assert_eq!(compare_egress(&direct, &proxied).state, State::Warn);
    }

    /// 有一条路探不到 → Unknown，**不是 Pass**。
    /// 「比不了」和「一致」是两回事，合并了就会在真有问题时报绿灯。
    #[test]
    fn missing_one_side_is_unknown_not_pass() {
        let direct = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        let nothing = reading(None, &[]);
        assert_eq!(compare_egress(&direct, &nothing).state, State::Unknown);
        assert_eq!(compare_egress(&nothing, &direct).state, State::Unknown);
    }

    #[test]
    fn unknown_is_not_pass() {
        // 「没扫到文件」和「扫了很干净」必须是两种结论。
        let it = item("x", "x", State::Unknown, "");
        assert_ne!(it.state, State::Pass);
    }
}
