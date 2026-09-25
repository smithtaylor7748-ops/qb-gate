//! Claude 桌面端 · 中文界面（插件 `claude-desktop-zh-cn`，2026-09-25，使用者定的）。
//!
//! 接入 [javaht/claude-desktop-zh-cn](https://github.com/javaht/claude-desktop-zh-cn)（MIT）。
//! 面板**不内置**它的任何文件：运行时从它的 GitHub Release 取（一版一个标签、没有附件，
//! 取的是那个标签的源码归档），**只调用它 Windows 脚本的安全模式**（`-PatchMode safe`，
//! 上游叫「Cowork 兼容模式」）。上游平均一周一版 —— Claude 一改前端结构它就跟着改脚本，
//! 所以跟的是它的脚本本身，不是面板照着抄一份（那样上游的修复永远流不进来）。
//!
//! # ⛔ 合规边界（使用者 2026-09-25 选的：只接安全模式 + 事后核验）
//!
//! 上游 Windows 脚本有五个动作、两种补丁模式。面板只碰两个动作、一种模式：
//!
//! | 上游 | 做什么 | 面板 |
//! |---|---|---|
//! | `install -PatchMode safe` | 往 Claude 安装目录放三份翻译 JSON、改 `ion-dist\assets\v1\*.js` 里的语言白名单与硬编码英文、写 `config.json` 的 `locale` | ✅ 一键汉化 |
//! | `uninstall` | 从它自己的 `.zh-cn-backups` 还原、删翻译、locale 设回 `en-US` | ✅ 恢复英文 |
//! | `-PatchMode official` | 改 `app.asar`，再**重写 `Claude.exe` 内嵌的 asar 完整性哈希**（Authenticode 变 `HashMismatch`） | ⛔ 绕过防篡改 = Anthropic 消费者条款 §3「bypassing … protective measures」 |
//! | `frida-launch` | Frida 在内存里改掉「带调试开关就拒绝启动」的闸门与完整性值 | ⛔ 同上；`scripts\experimental\` 一个字节都不落盘 |
//! | `disable-updates` / `sync-skills` | 关 Claude 自动更新 / 同步 CC Switch 的 skills | ⛔ 跟汉化无关 |
//!
//! 参数只由 [`script_args`] 拼、动作只有 [`Action`] 两个值，单测钉着永远拼不出别的。
//! 安全模式也是**非官方修改**：条款没有条目禁止改本机界面文字，但 Anthropic 不支持它、
//! Claude 一更新就被覆盖 —— 界面与 DISCLAIMER 都这么写，不说成「官方支持」。
//!
//! ⛔ 反重力那套 CDP 注入引擎**不能**搬到 Claude 上：Claude 桌面端见到 `--remote-debugging-port`
//! 就拒绝启动（2.9939.2 字符串核过：`refusing to start — a debugging or network-override switch
//! is present`）。那是它有意设的保护，绕过去就是上面第三、四行。
//!
//! # 同步上游（使用者选的：打开汉化弹窗或插件页时查）
//!
//! 只问 `github.com/<repo>/releases/latest` 的跳转（不跟跳转，读 `Location`），**不走
//! `api.github.com`**（0.25.3 self_update 同一个理由：未登录一个出口 IP 一小时 60 次）。
//! 有新版只显示，点了「更新」才下载。每一版下载后都**重核许可证**（`LICENSE` 仍是 MIT 原文），
//! 不是就不用、留着旧版。归档注释里的提交号记下来（GitHub 源码归档自带）。
//!
//! # 只做位置表认得的那种装法
//!
//! Squirrel（`%LOCALAPPDATA%\AnthropicClaude\app-<版本>`，[`inventory::desktop_app_dir`]）。
//! MSIX 装在 `WindowsApps` 里，上游要 UAC 接管那个目录的 ACL —— 面板不做，如实说。
use crate::error::{GateError, Result};
use crate::install::inventory;
use crate::install::zipread;
use crate::plugins::{DependencyCheck, PluginState, PluginStatus};
use crate::sink::ProgressSink;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub const PLUGIN_ID: &str = "claude-desktop-zh-cn";
pub const PLUGIN_NAME: &str = "Claude 桌面端 · 中文界面";
/// 上游仓库。**常量** —— 不收使用者输入的地址（同插件清单那条规矩）。
pub const UPSTREAM_REPO: &str = "javaht/claude-desktop-zh-cn";
/// 汉化成哪种中文。上游还有 zh-TW / zh-HK，一键汉化只用这一种。
pub const LANGUAGE: &str = "zh-CN";
/// 上游 Windows 脚本在归档里的位置。
pub const SCRIPT: &str = "scripts/install_windows.ps1";
/// 关掉上游脚本自己问 `api.github.com` 的那一下 —— 查新版是面板的事，而且只在打开弹窗时。
pub const SKIP_UPDATE_CHECK: (&str, &str) = ("CLAUDE_ZH_SKIP_UPDATE_CHECK", "1");

/// 归档最多收多大。1.4.8 是 2.5 MB。
const ARCHIVE_MAX: usize = 64 << 20;
/// 单个文件最多多大。最大的翻译文件 1.4.8 是 561 KB。
const FILE_MAX: u64 = 16 << 20;

pub fn upstream_url() -> String {
    format!("https://github.com/{UPSTREAM_REPO}")
}

fn latest_url() -> String {
    format!("https://github.com/{UPSTREAM_REPO}/releases/latest")
}

fn archive_url(tag: &str) -> String {
    format!("https://github.com/{UPSTREAM_REPO}/archive/refs/tags/{tag}.zip")
}

// ================================================================ 纯函数

/// 标签只收这些字符：GitHub 标签能长成什么样都行，但要拼进地址、当目录名，就得收紧。
pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && !tag.starts_with(['.', '-'])
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// **纯函数**：`releases/latest` 跳到的地址里取标签。认不出就是 `None`，不猜。
pub fn tag_from_location(location: &str) -> Option<String> {
    let url = reqwest::Url::parse(location).ok()?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return None;
    }
    let parts: Vec<&str> = url.path_segments()?.collect();
    let repo: Vec<&str> = UPSTREAM_REPO.split('/').collect();
    match parts.as_slice() {
        [owner, name, "releases", "tag", tag]
            if owner.eq_ignore_ascii_case(repo[0]) && name.eq_ignore_ascii_case(repo[1]) =>
        {
            valid_tag(tag).then(|| tag.to_string())
        }
        _ => None,
    }
}

/// 归档里的一个文件要不要。**只要 Windows 安全模式用得到的**：
///
/// - 根目录的 `LICENSE`（每版重核）、`README.md`（给人看）；
/// - `resources\` 下的 `.json`（翻译、清单、版本号）；
/// - `scripts\` 下的 PowerShell（`.ps1` / `.psm1` / `.psd1`），**`scripts\experimental\` 除外** ——
///   那里是 Frida 内存补丁与绕过调试开关闸门的脚本，面板永远不调用，也就一个字节都不落盘。
///
/// 按规则挑而不是按文件名列死：上游以后把脚本拆成几个模块，照样跟得上。
fn wanted(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    if lower == "license" || lower == "readme.md" {
        return true;
    }
    if lower.starts_with("resources/") {
        return lower.ends_with(".json");
    }
    if lower.starts_with("scripts/") && !lower.starts_with("scripts/experimental/") {
        return lower.ends_with(".ps1") || lower.ends_with(".psm1") || lower.ends_with(".psd1");
    }
    false
}

/// 相对路径落盘前的检查：每一段非空、不是 `.` / `..`、不带盘符与反斜杠、不以点或空格结尾、
/// 不是 Windows 设备名。跟 `extensions::safe_relative` 同一套规矩 —— 那个在 `extensions` 里，
/// 而 `extensions` 已经引用 `plugins`（接入酒馆），这里再引用回去就成环了（`architecture.rs` 的
/// `module_cycles_only_ever_shrink` 当场抓住过）。
fn safe_relative(path: &str) -> bool {
    const DEVICES: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.contains(['\\', ':', '\0'])
                && !DEVICES.contains(&part.split('.').next().unwrap_or("").to_uppercase().as_str())
        })
}

/// **纯函数**：从归档目录里挑出要解的那些，返回 `(归档里的名字, 相对路径)`。
///
/// GitHub 源码归档所有条目都在一个顶层目录（`<仓库>-<标签>/`）下 —— 不是的话不认。
/// 相对路径过 [`safe_relative`]（不许 `..`、盘符、设备名）。
/// 必须挑到 `LICENSE` 和 [`SCRIPT`]，缺一个这一版就不用。
pub fn select_entries(names: &[&str]) -> Result<Vec<(String, String)>> {
    let first = names
        .iter()
        .find(|n| !n.is_empty())
        .ok_or_else(|| GateError::Other("上游归档是空的".into()))?;
    let prefix = match first.split_once('/') {
        Some((top, _)) if !top.is_empty() => format!("{top}/"),
        _ => {
            return Err(GateError::Other(
                "上游归档不是 GitHub 源码归档的样子".into(),
            ))
        }
    };
    let mut out = Vec::new();
    for name in names {
        let rel = name
            .strip_prefix(&prefix)
            .ok_or_else(|| GateError::Other(format!("上游归档里有不在顶层目录下的条目：{name}")))?;
        if rel.is_empty() || rel.ends_with('/') || !wanted(rel) {
            continue;
        }
        if !safe_relative(rel) {
            return Err(GateError::Other(format!("上游归档里有不安全的路径：{rel}")));
        }
        out.push((name.to_string(), rel.to_string()));
    }
    for must in ["LICENSE", SCRIPT] {
        if !out.iter().any(|(_, rel)| rel.eq_ignore_ascii_case(must)) {
            return Err(GateError::Other(format!(
                "上游这一版没有 {must}，面板不认这一版"
            )));
        }
    }
    Ok(out)
}

/// **纯函数**：`LICENSE` 还是不是 MIT。空白一律压成一个空格再比那三句原文。
pub fn license_is_mit(text: &str) -> bool {
    let norm = text.split_whitespace().collect::<Vec<_>>().join(" ");
    [
        "Permission is hereby granted, free of charge, to any person obtaining a copy of this software",
        "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.",
        "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND",
    ]
    .iter()
    .all(|s| norm.contains(s))
}

/// 上游脚本支持什么 —— 从它的 `param(...)` 里读出来，**不假设**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScriptCaps {
    /// 声明了 `-SkipAsarPatch`（那个开关会把 PatchMode 强制成 safe）。有就一并传，双保险。
    pub skip_asar_patch: bool,
}

/// `param(` 到与它配对的 `)`。括号计数时跳过引号里的内容（`ValidateSet("a", "b")` 里就有括号）。
fn param_block(text: &str) -> Option<&str> {
    let lower = text.to_ascii_lowercase();
    let start = lower.find("param(")? + "param(".len();
    let bytes = text.as_bytes();
    let mut depth = 1usize;
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[start..i]);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

/// 某个参数的 `ValidateSet(...)` 里列了哪些值。只看**这个参数自己的**那段属性
/// （上一个 `$变量` 之后、`$name` 之前）。没有 `ValidateSet` → `None`。
fn validate_set(block: &str, name: &str) -> Option<Vec<String>> {
    let lower = block.to_ascii_lowercase();
    let at = find_var(&lower, &name.to_ascii_lowercase())?;
    let region_start = lower[..at].rfind('$').map(|i| i + 1).unwrap_or(0);
    let region = &block[region_start..at];
    let vs = region.to_ascii_lowercase().find("validateset(")? + "validateset(".len();
    let end = region[vs..].find(')')? + vs;
    Some(
        region[vs..end]
            .split(',')
            .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

/// `$name` 作为一个完整变量名出现的位置（`$Action` 不能匹配到 `$ActionX`）。
fn find_var(lower: &str, name: &str) -> Option<usize> {
    let needle = format!("${name}");
    let mut from = 0;
    while let Some(i) = lower[from..].find(&needle) {
        let at = from + i;
        let after = lower[at + needle.len()..].chars().next();
        if !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(at);
        }
        from = at + needle.len();
    }
    None
}

/// **纯函数**：读上游脚本的参数，确认它还支持面板要用的那几样。
///
/// `PatchMode` 必须列着 `safe`、`Action` 必须列着 `install` 与 `uninstall`、`Language` 必须列着
/// `zh-CN` —— 缺一样就说明上游改了接口，这一版不用（等面板跟上），而不是闭着眼把参数塞过去。
/// 没有 `ValidateSet` 也算不认：那样面板就没法确认 `safe` 还是不是「不碰 app.asar」的那个意思。
pub fn script_caps(text: &str) -> Result<ScriptCaps> {
    let block = param_block(text)
        .ok_or_else(|| GateError::Other("上游脚本里找不到 param(...)，接口变了".into()))?;
    let need = |name: &str, values: &[&str]| -> Result<()> {
        let set = validate_set(block, name).ok_or_else(|| {
            GateError::Other(format!("上游脚本的 -{name} 没有列出可选值，接口变了"))
        })?;
        for v in values {
            if !set.iter().any(|s| s.eq_ignore_ascii_case(v)) {
                return Err(GateError::Other(format!(
                    "上游脚本的 -{name} 不再支持 {v}，接口变了"
                )));
            }
        }
        Ok(())
    };
    need("PatchMode", &["safe"])?;
    need("Action", &["install", "uninstall"])?;
    need("Language", &[LANGUAGE])?;
    Ok(ScriptCaps {
        skip_asar_patch: find_var(&block.to_ascii_lowercase(), "skipasarpatch").is_some(),
    })
}

/// 面板会让上游脚本做的**全部**动作。没有第三个值 —— 见模块头那张表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Uninstall,
}

impl Action {
    fn arg(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Uninstall => "uninstall",
        }
    }
}

/// **纯函数**：拼 `powershell` 的参数。每个参数一个 argv（`winget::run_streaming_env`），
/// 不经过任何 shell 拼接。
///
/// - `-ExecutionPolicy RemoteSigned` 而不是 `Bypass`：脚本是面板自己下的、没有网络来源标记，
///   两者效果一样，少一个杀软信号（CLAUDE.md「杀软会把这个面板当木马」）；
/// - `-PatchMode safe` **写死**，声明了 `-SkipAsarPatch` 就再加一道。
pub fn script_args(action: Action, caps: ScriptCaps, script: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "RemoteSigned",
        "-File",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(script.display().to_string());
    args.push(action.arg().into());
    args.push(LANGUAGE.into());
    args.push("-PatchMode".into());
    args.push("safe".into());
    if caps.skip_asar_patch {
        args.push("-SkipAsarPatch".into());
    }
    args
}

/// **纯函数**：上游脚本输出里 `app: <目录>` 那一行 —— 它实际改的是哪个 Claude。
/// 没有这一行（上游改了措辞）就是 `None`，由调用方按文件核验。
pub fn reported_app_dir(lines: &[String]) -> Option<String> {
    lines.iter().find_map(|l| {
        l.trim()
            .strip_prefix("app:")
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
    })
}

/// 两个目录是不是同一处（大小写、斜杠、结尾分隔符都不算）。
pub fn same_dir(a: &str, b: &Path) -> bool {
    inventory::same_path(Path::new(a), b)
}

// ---------------------------------------------------------------- config.json 里的 locale

/// **纯函数**：JSON 对象最外层 `key` 的字符串值。不是对象 / 不是 JSON → 报错。
pub fn top_level_string(text: &str, key: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| GateError::Other(format!("config.json 不是有效 JSON：{e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| GateError::Other("config.json 不是一个 JSON 对象".into()))?;
    Ok(obj.get(key).and_then(|v| v.as_str()).map(str::to_string))
}

/// **纯函数**：把 JSON 对象最外层 `key` 的值换成字符串 `value`，**别的字节一个不动**。
///
/// Claude 的 `config.json` 里装着它自己的登录令牌缓存（`oauth:tokenCache`）。整份解析再序列化
/// 会重排键、改缩进，还等于把令牌读出来又写回去 —— 这里只改那一个值的字节，令牌那几行原样不碰。
/// 没有这个键就插在最前面，照第一项的缩进写。先整份解析一遍：拿不准的文件不写。
pub fn set_top_level_string(text: &str, key: &str, value: &str) -> Result<String> {
    top_level_string(text, key)?; // 校验：是 JSON 对象
    let b = text.as_bytes();
    let json = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let mut i = skip_ws(b, 0);
    if b.get(i) != Some(&b'{') {
        return Err(GateError::Other("config.json 不是一个 JSON 对象".into()));
    }
    let open = i;
    i += 1;
    loop {
        i = skip_ws(b, i);
        match b.get(i) {
            Some(b'}') => break,
            Some(b'"') => {}
            _ => return Err(GateError::Other("config.json 的结构读不懂".into())),
        }
        let key_end = string_end(b, i)?;
        let found: String = serde_json::from_str(&text[i..key_end])
            .map_err(|_| GateError::Other("config.json 的键读不懂".into()))?;
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return Err(GateError::Other("config.json 的结构读不懂".into()));
        }
        let value_start = skip_ws(b, i + 1);
        let value_end = value_end(b, value_start)?;
        if found == key {
            return Ok(format!(
                "{}{}{}",
                &text[..value_start],
                json(value),
                &text[value_end..]
            ));
        }
        i = skip_ws(b, value_end);
        match b.get(i) {
            Some(b',') => i += 1,
            Some(b'}') => break,
            _ => return Err(GateError::Other("config.json 的结构读不懂".into())),
        }
    }
    // 没有这个键：插在 `{` 后面，照第一项前面那段空白（`\n\t` 之类）写。
    let first = skip_ws(b, open + 1);
    let indent = &text[open + 1..first];
    if b.get(first) == Some(&b'}') {
        return Ok(format!(
            "{}{}{}: {}{}{}",
            &text[..=open],
            indent,
            json(key),
            json(value),
            indent,
            &text[first..]
        ));
    }
    Ok(format!(
        "{}{}{}: {},{}",
        &text[..=open],
        indent,
        json(key),
        json(value),
        &text[open + 1..]
    ))
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

/// `b[i]` 是一个字符串的开头引号，返回结尾引号**之后**的位置。
fn string_end(b: &[u8], i: usize) -> Result<usize> {
    let mut j = i + 1;
    while let Some(&c) = b.get(j) {
        match c {
            b'\\' => j += 2,
            b'"' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err(GateError::Other("config.json 里有没闭合的字符串".into()))
}

/// 从 `i` 开始的一个 JSON 值的结尾（之后的位置）。输入已经整份解析过，这里只管走到头。
fn value_end(b: &[u8], i: usize) -> Result<usize> {
    match b.get(i) {
        Some(b'"') => string_end(b, i),
        Some(b'{') | Some(b'[') => {
            let mut depth = 0usize;
            let mut j = i;
            while let Some(&c) = b.get(j) {
                match c {
                    b'"' => {
                        j = string_end(b, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            Err(GateError::Other("config.json 里有没闭合的对象".into()))
        }
        Some(_) => {
            let mut j = i;
            while b
                .get(j)
                .is_some_and(|c| !c.is_ascii_whitespace() && !b",}]".contains(c))
            {
                j += 1;
            }
            Ok(j)
        }
        None => Err(GateError::Other("config.json 读到一半就没了".into())),
    }
}

// ================================================================ 落盘的状态

/// 插件自己记的东西。**证据优先**：「汉化了没」看的是 Claude 安装目录里的文件，不是这里。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// 已下载、核过许可证与参数的上游版本。
    pub tag: Option<String>,
    /// 那一版对应的提交（GitHub 写在归档注释里）。
    pub commit: Option<String>,
    /// 下载下来的归档的 SHA-256（审计与排查用；上游不发校验和，**不作判据**）。
    pub archive_sha256: Option<String>,
    pub downloaded_at: Option<String>,
    /// 最近一次查到的上游最新版与时间。
    pub latest_tag: Option<String>,
    pub latest_checked_at: Option<String>,
    /// 核验发现越线、被停用的上游版本。**不再用它们**，除非上游发了新版。
    pub blocked: Vec<String>,
    /// 使用者点过「一键汉化」、没点「恢复英文」。用来说「Claude 更新了，需要重新应用」。
    pub desired: bool,
    /// 上一次成功应用：上游版本、Claude 版本、时间。
    pub applied_tag: Option<String>,
    pub applied_claude: Option<String>,
    pub applied_at: Option<String>,
    /// 应用之前各份 Claude 资料的 `locale`（恢复英文时写回）。键是资料目录。
    pub previous_locales: BTreeMap<String, Option<String>>,
}

pub fn state_path() -> PathBuf {
    crate::paths::state_dir()
        .join("claude-zh")
        .join("state.json")
}

pub fn load_state() -> State {
    std::fs::read(state_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_state(s: &State) -> Result<()> {
    crate::config_io::replace(&state_path(), Some(&serde_json::to_vec_pretty(s)?))
}

// ================================================================ 本机的上游副本

/// 上游副本放在哪。一版一个目录，只留当前那一版。
pub fn packages_root() -> PathBuf {
    crate::paths::state_dir().join("plugins").join(PLUGIN_ID)
}

/// 一份下载好、核过的上游副本。
#[derive(Debug, Clone)]
pub struct Package {
    pub tag: String,
    pub dir: PathBuf,
    pub script: PathBuf,
    pub caps: ScriptCaps,
}

/// 当前那一版副本。目录、脚本、许可证都在、参数也还认得，才算有。
pub fn current_package(state: &State) -> Option<Package> {
    let tag = state.tag.clone().filter(|t| valid_tag(t))?;
    let dir = packages_root().join(&tag);
    let script = dir.join(SCRIPT.replace('/', "\\"));
    let license = std::fs::read_to_string(dir.join("LICENSE")).ok()?;
    if !license_is_mit(&license) {
        return None;
    }
    let caps = script_caps(&std::fs::read_to_string(&script).ok()?).ok()?;
    Some(Package {
        tag,
        dir,
        script,
        caps,
    })
}

/// 问最新版（`follow_redirects = false`）的那个带 30 秒总超时；下载那个不设总超时 ——
/// 慢网速下 2.5 MB 可能要一两分钟，卡没卡住由 [`download_capped`] 的「60 秒一个字节都没收到」判。
fn client(follow_redirects: bool) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("QB Gate/", env!("CARGO_PKG_VERSION")));
    if !follow_redirects {
        b = b
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30));
    }
    b.build()
        .map_err(|e| GateError::Other(format!("建不了网络客户端：{e}")))
}

/// 问一次上游最新版，**只问、不记**（诊断例子用它，不动插件自己的记录）。会联网。
pub async fn latest_tag_online() -> Result<String> {
    let resp = client(false)?
        .get(latest_url())
        .send()
        .await
        .map_err(|e| GateError::Other(format!("问 GitHub 上游最新版失败：{e}")))?;
    let status = resp.status();
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    location.as_deref().and_then(tag_from_location).ok_or_else(|| {
        GateError::Other(format!(
            "GitHub 没说上游最新版是哪个（HTTP {status}{}）。可能是上游还没发过 Release，或者 GitHub 改了页面。",
            location.map(|l| format!("，跳到 {l}")).unwrap_or_default()
        ))
    })
}

/// 问一次上游最新版并记下来（几点问的、问到的是哪一版）。**会联网**，
/// 只在打开汉化弹窗 / 插件页、或点「检查更新」时调。
pub async fn fetch_latest_tag() -> Result<String> {
    let found = latest_tag_online().await;
    let mut state = load_state();
    state.latest_checked_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
    if let Ok(tag) = &found {
        state.latest_tag = Some(tag.clone());
    }
    save_state(&state)?;
    found
}

/// 下载好、逐项核过、**还没落盘**的一版上游。
#[derive(Debug, Clone)]
pub struct Verified {
    pub tag: String,
    /// GitHub 写在归档注释里的提交号。
    pub commit: Option<String>,
    /// 归档的 SHA-256（审计与排查用，不作判据 —— 上游不发校验和）。
    pub archive_sha256: String,
    /// 挑出来的文件：（相对路径，内容）。
    pub files: Vec<(String, Vec<u8>)>,
    pub caps: ScriptCaps,
}

/// 下载上游这一版并**逐项核过**：归档 → 挑文件 → 核许可证 → 核脚本参数。只在内存里，不落盘。
pub async fn fetch_verified(tag: &str, rep: &dyn ProgressSink, step: u32) -> Result<Verified> {
    if !valid_tag(tag) {
        return Err(GateError::Other(format!("上游标签不认：{tag}")));
    }
    rep.log(step, &format!("下载 {}", archive_url(tag)));
    let bytes = download_capped(&archive_url(tag)).await?;
    let sha = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(&bytes))
    };
    rep.log(
        step,
        &format!("下载完成：{} KB，SHA-256 {sha}", bytes.len() / 1024),
    );
    let archive = zipread::read_index(&bytes)?;
    let names: Vec<&str> = archive.entries.iter().map(|e| e.name.as_str()).collect();
    let selected = select_entries(&names)?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, rel) in &selected {
        let entry = archive
            .entries
            .iter()
            .find(|e| &e.name == name)
            .expect("selected from this archive");
        files.push((rel.clone(), zipread::extract(&bytes, entry, FILE_MAX)?));
    }
    let text = |rel: &str| -> Result<String> {
        let (_, body) = files
            .iter()
            .find(|(r, _)| r.eq_ignore_ascii_case(rel))
            .ok_or_else(|| GateError::Other(format!("上游这一版没有 {rel}")))?;
        String::from_utf8(body.clone())
            .map_err(|_| GateError::Other(format!("上游的 {rel} 不是 UTF-8 文本")))
    };
    if !license_is_mit(&text("LICENSE")?) {
        return Err(GateError::Other(format!(
            "上游 {tag} 的许可证不再是 MIT 原文，面板不用这一版（已有的那一版原样留着）。"
        )));
    }
    // 上游脚本存成带 BOM 的 UTF-8（PowerShell 5.1 要它才认中文），读的时候去掉 BOM 再看参数。
    let script_text = text(SCRIPT)?;
    let caps = script_caps(script_text.trim_start_matches('\u{feff}'))?;
    rep.log(
        step,
        &format!(
            "核过：许可证仍是 MIT；脚本支持 install / uninstall 与安全模式{}；挑出 {} 个文件（scripts\\experimental 一个都没解）",
            if caps.skip_asar_patch { "（带 -SkipAsarPatch）" } else { "" },
            files.len()
        ),
    );
    Ok(Verified {
        tag: tag.to_string(),
        commit: archive.commit().map(str::to_string),
        archive_sha256: sha,
        files,
        caps,
    })
}

/// 下载、核过、换进来。任何一步不对，已有的那一版原样留着。
pub async fn download(tag: &str, rep: &dyn ProgressSink, step: u32) -> Result<Package> {
    let v = fetch_verified(tag, rep, step).await?;
    install_package(v)
}

/// 把核过的一版落盘：先写到暂存目录，整份写完再换名就位 —— 不会留下一个写了一半的版本。
fn install_package(v: Verified) -> Result<Package> {
    let Verified {
        tag,
        commit,
        archive_sha256: sha,
        files,
        ..
    } = v;
    let tag = tag.as_str();
    let root = packages_root();
    std::fs::create_dir_all(&root)?;
    let staging = root.join(format!(".incoming-{}", crate::config_io::id()));
    let written = (|| -> Result<()> {
        for (rel, body) in &files {
            let path = staging.join(rel.replace('/', "\\"));
            std::fs::create_dir_all(path.parent().unwrap_or(&staging))?;
            std::fs::write(&path, body)?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let target = root.join(tag);
    if target.exists() {
        let _ = std::fs::remove_dir_all(&target);
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(GateError::Other(format!("上游副本换不进去：{e}")));
    }
    // 只留这一版：旧版本与上次没收完的暂存目录都清掉。
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            if e.path() != target && e.path().is_dir() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let mut state = load_state();
    state.tag = Some(tag.to_string());
    state.commit = commit;
    state.archive_sha256 = Some(sha);
    state.downloaded_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
    save_state(&state)?;
    crate::audit::write(&format!(
        "Claude 汉化插件：已取上游 {UPSTREAM_REPO} {tag}（提交 {}），许可证 MIT，只解了安全模式用得到的 {} 个文件",
        state.commit.as_deref().unwrap_or("未知"),
        files.len()
    ));
    current_package(&state).ok_or_else(|| GateError::Other("上游副本写好了却读不回来".into()))
}

/// 下载到内存，最多 [`ARCHIVE_MAX`]。60 秒一个字节都没收到就放弃。
async fn download_capped(url: &str) -> Result<Vec<u8>> {
    let mut resp = client(true)?
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| GateError::Other(format!("下载上游归档失败：{e}")))?;
    let mut out = Vec::new();
    loop {
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(60), resp.chunk())
            .await
            .map_err(|_| GateError::Other("下载卡住，60 秒没有收到任何数据，已停止".into()))?
            .map_err(|e| GateError::Other(format!("下载上游归档失败：{e}")))?;
        let Some(chunk) = chunk else { break };
        out.extend_from_slice(&chunk);
        if out.len() > ARCHIVE_MAX {
            return Err(GateError::Other("上游归档大得不正常，已停止下载".into()));
        }
    }
    Ok(out)
}

// ================================================================ Claude 那一侧（证据）

/// 位置表认得的 Squirrel 装法：（版本, `app-<版本>` 目录）。
pub fn target() -> Option<(String, PathBuf)> {
    inventory::desktop_app_dir(&dirs::data_local_dir()?)
}

/// 这一版 Claude 上中文文件在不在（上游安全模式放的那两份）。
pub fn zh_files_present(app_dir: &Path) -> bool {
    let res = app_dir.join("resources");
    res.join(format!("{LANGUAGE}.json")).is_file()
        && res
            .join("ion-dist")
            .join("i18n")
            .join(format!("{LANGUAGE}.json"))
            .is_file()
}

/// Claude 桌面端的各份资料（每个账户槽位一份 `%APPDATA%\Claude-<名字>`，当前那份经联结点
/// `%APPDATA%\Claude` 指过去；`accounts::AccountRoots::desktop_profile_dirs` 按真实位置去重）。
/// 改名留底的 `Claude-backup-*` 不算 —— 那是备份，不是在用的资料。只要里面有 `config.json` 的。
pub fn profile_dirs() -> Vec<PathBuf> {
    crate::accounts::AccountRoots::current()
        .desktop_profile_dirs()
        .into_iter()
        .filter(|d| {
            !d.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("Claude-backup-"))
        })
        .filter(|d| d.join("config.json").is_file())
        .collect()
}

/// 一份资料的 `locale`。读不出来就是 `None`（**只读这一个键**，令牌缓存一概不看）。
pub fn profile_locale(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    top_level_string(&text, "locale").ok().flatten()
}

/// 把一份资料的 `locale` 设成 `value`。只改那一个值的字节。返回改没改。
pub fn set_profile_locale(dir: &Path, value: &str) -> Result<bool> {
    let path = dir.join("config.json");
    let before = crate::config_io::read_optional(&path)?
        .ok_or_else(|| GateError::Other(format!("{} 不在", path.display())))?;
    let text = String::from_utf8(before.clone())
        .map_err(|_| GateError::Other(format!("{} 不是 UTF-8 文本", path.display())))?;
    let next = set_top_level_string(&text, "locale", value)?;
    if next == text {
        return Ok(false);
    }
    crate::config_io::commit(vec![crate::config_io::Edit {
        path,
        expected: Some(before),
        body: Some(next.into_bytes()),
    }])?;
    Ok(true)
}

// ================================================================ 给界面的状态

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum ClaudeZhInstall {
    /// Squirrel 装法（位置表认得）：能做。
    Squirrel,
    /// MSIX（WindowsApps 里）：上游要 UAC 接管那个目录的权限，面板不做。
    Msix,
    /// 没装 Claude 桌面端。
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum ClaudeZhState {
    /// 这一版 Claude 上没有中文文件。
    Off,
    /// 中文文件在，当前资料的 locale 也是 zh-CN。核实过的。
    On,
    /// 中文文件在，但当前资料的 locale 不是 zh-CN —— 半套（使用者自己在 Claude 里改回了别的语言也是这样）。
    Partial,
    /// 点过「一键汉化」，而 Claude 自己更新成了新的 app-版本，汉化随之没了。
    NeedsReapply,
    /// 做不了（没装 / MSIX）。
    Unsupported,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhProfile {
    pub dir: String,
    /// 这份资料 `config.json` 里的 `locale`。读不出来是 `None`。
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhStatus {
    pub install: ClaudeZhInstall,
    pub claude_version: Option<String>,
    pub app_dir: Option<String>,
    pub state: ClaudeZhState,
    /// 一句能照着做的话。
    pub detail: String,
    pub files_present: bool,
    pub profiles: Vec<ClaudeZhProfile>,
    /// 本机那份上游副本。
    pub package_tag: Option<String>,
    pub package_commit: Option<String>,
    /// 最近一次查到的上游最新版。`None` = 还没查过。
    pub latest_tag: Option<String>,
    pub latest_checked_at: Option<String>,
    /// 本机已有一版、而上游最新版跟它不一样（而且没被停用）。一版都没下过时是 `false`。
    pub update_available: bool,
    pub blocked: Vec<String>,
    pub applied_tag: Option<String>,
    pub applied_claude: Option<String>,
    pub applied_at: Option<String>,
    /// Claude 桌面端开着几个进程。`None` = 查不到（**不是「没有」**）。编排层填。
    pub desktop_running: Option<u32>,
    pub upstream: String,
}

/// 只读本机：安装目录里的文件、各份资料的 locale、插件自己的记录。**不联网、不起进程**
/// （MSIX 那一档由调用方另查后传进来）。
pub fn status(msix: Option<String>) -> ClaudeZhStatus {
    let state = load_state();
    let package = current_package(&state);
    let target = target();
    let profiles: Vec<ClaudeZhProfile> = profile_dirs()
        .iter()
        .map(|d| ClaudeZhProfile {
            dir: d.display().to_string(),
            locale: profile_locale(d),
        })
        .collect();
    let active_locale = dirs::config_dir()
        .map(|a| a.join("Claude"))
        .and_then(|d| profile_locale(&d));
    let package_tag = package.as_ref().map(|p| p.tag.clone());
    // 「有新版」只在本机已经有一版时才说 —— 一版都没下过时点「一键汉化」本来就取最新版，
    // 那时说「有新版」会让人以为要先点「更新」。
    let update_available = match (&state.latest_tag, &package_tag) {
        (Some(latest), Some(have)) => latest != have && !state.blocked.contains(latest),
        _ => false,
    };
    let (install, claude_version, app_dir, files_present) = match (&target, msix) {
        (Some((v, dir)), _) => (
            ClaudeZhInstall::Squirrel,
            Some(v.clone()),
            Some(dir.display().to_string()),
            zh_files_present(dir),
        ),
        (None, Some(v)) => (ClaudeZhInstall::Msix, Some(v), None, false),
        (None, None) => (ClaudeZhInstall::Missing, None, None, false),
    };
    let (state_kind, detail) = match install {
        ClaudeZhInstall::Missing => (
            ClaudeZhState::Unsupported,
            "没检测到 Claude 桌面端。先到「软件」页装好，再回来汉化。".to_string(),
        ),
        ClaudeZhInstall::Msix => (
            ClaudeZhState::Unsupported,
            "这是 MSIX 方式装的 Claude 桌面端：上游要以管理员接管 WindowsApps 目录的权限才改得了，面板不做。要汉化请换成官网安装包（Squirrel 装法），或者自己按上游 README 操作。".to_string(),
        ),
        ClaudeZhInstall::Squirrel if files_present && active_locale.as_deref() == Some(LANGUAGE) => (
            ClaudeZhState::On,
            format!("已汉化：这一版 Claude（{}）上的中文文件在，当前资料的界面语言是 {LANGUAGE}。", claude_version.as_deref().unwrap_or("?")),
        ),
        ClaudeZhInstall::Squirrel if files_present => (
            ClaudeZhState::Partial,
            format!(
                "中文文件在，但当前资料的界面语言是 {}。点「一键汉化」补上，或者在 Claude 的设置里自己选。",
                active_locale.as_deref().unwrap_or("未知")
            ),
        ),
        ClaudeZhInstall::Squirrel if state.desired => (
            ClaudeZhState::NeedsReapply,
            format!(
                "Claude 已经更新到 {}（上次汉化的是 {}），更新会换一个新目录，汉化随之没了。点一下重新应用。",
                claude_version.as_deref().unwrap_or("?"),
                state.applied_claude.as_deref().unwrap_or("旧版")
            ),
        ),
        ClaudeZhInstall::Squirrel => (
            ClaudeZhState::Off,
            "还没汉化。点「一键汉化」：下载上游、关掉 Claude 桌面端、跑上游的安全模式、核验。".to_string(),
        ),
    };
    ClaudeZhStatus {
        install,
        claude_version,
        app_dir,
        state: state_kind,
        detail,
        files_present,
        profiles,
        package_tag,
        package_commit: package.as_ref().and(state.commit.clone()),
        latest_tag: state.latest_tag.clone(),
        latest_checked_at: state.latest_checked_at.clone(),
        update_available,
        blocked: state.blocked.clone(),
        applied_tag: state.applied_tag.clone(),
        applied_claude: state.applied_claude.clone(),
        applied_at: state.applied_at.clone(),
        desktop_running: None,
        upstream: upstream_url(),
    }
}

/// 扩展中心列表里那一行。
pub fn plugin_status() -> PluginStatus {
    let s = status(None);
    let checks = vec![
        DependencyCheck::new(
            "Claude 桌面端",
            s.install == ClaudeZhInstall::Squirrel,
            match (s.install, &s.claude_version) {
                (ClaudeZhInstall::Squirrel, Some(v)) => {
                    format!("{v}（Squirrel 装法，位置表 install::inventory）")
                }
                _ => "没检测到 Squirrel 装法的 Claude 桌面端（MSIX 装的面板不做）".to_string(),
            },
        ),
        DependencyCheck::new(
            "上游副本",
            s.package_tag.is_some(),
            match &s.package_tag {
                Some(t) => format!(
                    "{UPSTREAM_REPO} {t}{}（许可证 MIT，已核）",
                    s.package_commit
                        .as_deref()
                        .map(|c| format!(" · 提交 {}", &c[..c.len().min(8)]))
                        .unwrap_or_default()
                ),
                None => "还没下载。点「一键汉化」会先从上游 Release 取".to_string(),
            },
        ),
    ];
    PluginStatus {
        id: PLUGIN_ID,
        name: PLUGIN_NAME,
        state: match s.state {
            ClaudeZhState::Unsupported => PluginState::Missing,
            ClaudeZhState::On => PluginState::Running,
            ClaudeZhState::NeedsReapply | ClaudeZhState::Partial => PluginState::Broken,
            ClaudeZhState::Off => PluginState::Ready,
        },
        detail: s.detail,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 上游 1.4.8 `install_windows.ps1` 的 `param(...)` 原样。
    const PARAMS_148: &str = r#"param(
    [switch]$Interactive,
    [switch]$SkipAsarPatch,
    [ValidateSet("safe", "official")]
    [string]$PatchMode = "safe",

    [Parameter(Position = 0)]
    [ValidateSet("install", "uninstall", "disable-updates", "enable-updates", "sync-skills", "unsync-skills", "frida-launch")]
    [string]$Action = "install",

    [Parameter(Position = 1)]
    [ValidateSet("zh-CN", "zh-TW", "zh-HK")]
    [string]$Language = "zh-CN",

    [string]$OriginalUserSid = "",
    [string]$OriginalUserProfile = "",
    [string]$OriginalAppData = "",
    [string]$OriginalLocalAppData = ""
)

$ErrorActionPreference = "Stop"
"#;

    #[test]
    fn the_latest_tag_comes_from_the_redirect_and_nothing_else() {
        assert_eq!(
            tag_from_location("https://github.com/javaht/claude-desktop-zh-cn/releases/tag/1.4.8")
                .as_deref(),
            Some("1.4.8")
        );
        // 别的仓库、别的域、http、奇怪的标签：一律不认。
        for bad in [
            "https://github.com/someone/else/releases/tag/1.4.8",
            "https://evil.example/javaht/claude-desktop-zh-cn/releases/tag/1.4.8",
            "http://github.com/javaht/claude-desktop-zh-cn/releases/tag/1.4.8",
            "https://github.com/javaht/claude-desktop-zh-cn/releases",
            "https://github.com/javaht/claude-desktop-zh-cn/releases/tag/..%2F..",
            "https://github.com/javaht/claude-desktop-zh-cn/releases/tag/-rm",
            "not a url",
        ] {
            assert_eq!(tag_from_location(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_what_safe_mode_needs_is_selected_and_experimental_never_is() {
        let names = [
            "claude-desktop-zh-cn-1.4.8/",
            "claude-desktop-zh-cn-1.4.8/LICENSE",
            "claude-desktop-zh-cn-1.4.8/README.md",
            "claude-desktop-zh-cn-1.4.8/install-windows.bat",
            "claude-desktop-zh-cn-1.4.8/install-mac.command",
            "claude-desktop-zh-cn-1.4.8/docs/images/wechat-group.png",
            "claude-desktop-zh-cn-1.4.8/resources/frontend-zh-CN.json",
            "claude-desktop-zh-cn-1.4.8/resources/Localizable.strings",
            "claude-desktop-zh-cn-1.4.8/scripts/install_windows.ps1",
            "claude-desktop-zh-cn-1.4.8/scripts/patch_claude_zh_cn.py",
            "claude-desktop-zh-cn-1.4.8/scripts/experimental/frida_cdp_gate_win.js",
            "claude-desktop-zh-cn-1.4.8/scripts/experimental/run_frida_zh_win.ps1",
            "claude-desktop-zh-cn-1.4.8/scripts/experimental/frida-zh-resident-ctl.ps1",
            "claude-desktop-zh-cn-1.4.8/tests/test_shortcut_repair.py",
        ];
        let picked: Vec<String> = select_entries(&names)
            .unwrap()
            .into_iter()
            .map(|(_, rel)| rel)
            .collect();
        assert_eq!(
            picked,
            [
                "LICENSE",
                "README.md",
                "resources/frontend-zh-CN.json",
                "scripts/install_windows.ps1"
            ]
        );
        assert!(!picked.iter().any(|p| p.contains("experimental")));
    }

    #[test]
    fn an_archive_without_the_script_or_license_is_refused() {
        assert!(select_entries(&["r-1/LICENSE", "r-1/resources/a.json"]).is_err());
        assert!(select_entries(&["r-1/scripts/install_windows.ps1"]).is_err());
        // 不在同一个顶层目录下 = 不是 GitHub 源码归档的样子。
        assert!(select_entries(&[
            "r-1/LICENSE",
            "r-1/scripts/install_windows.ps1",
            "other/x.json"
        ])
        .is_err());
    }

    #[test]
    fn unsafe_paths_inside_the_archive_are_refused() {
        for bad in [
            "r-1/resources/../../x.json",
            "r-1/resources/CON.json",
            "r-1/scripts/a:b.ps1",
            "r-1/resources/a\\b.json",
        ] {
            let names = ["r-1/LICENSE", "r-1/scripts/install_windows.ps1", bad];
            assert!(select_entries(&names).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_mit_text_is_recognised_and_other_licenses_are_not() {
        let mit = "MIT License\n\nCopyright (c) 2025 javaht\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the \"Software\"), to deal\nin the Software without restriction...\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED...";
        assert!(license_is_mit(mit));
        assert!(!license_is_mit(
            "GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007\n..."
        ));
        assert!(!license_is_mit("All rights reserved."));
    }

    #[test]
    fn the_real_148_parameters_are_understood() {
        let caps = script_caps(PARAMS_148).unwrap();
        assert!(caps.skip_asar_patch);
    }

    /// 上游改了接口（没有 safe、没有 uninstall、没有 ValidateSet）就不用这一版。
    #[test]
    fn a_script_that_changed_its_interface_is_refused() {
        let no_safe = PARAMS_148.replace(r#""safe", "official""#, r#""official""#);
        assert!(script_caps(&no_safe).is_err());
        let no_uninstall = PARAMS_148.replace(r#""uninstall", "#, "");
        assert!(script_caps(&no_uninstall).is_err());
        let no_set = PARAMS_148.replace(r#"[ValidateSet("safe", "official")]"#, "");
        assert!(script_caps(&no_set).is_err());
        assert!(script_caps("Write-Host hi").is_err());
        // 没有 -SkipAsarPatch 也行，只是少一道保险。
        let no_skip = PARAMS_148.replace("[switch]$SkipAsarPatch,", "");
        assert_eq!(
            script_caps(&no_skip).unwrap(),
            ScriptCaps {
                skip_asar_patch: false
            }
        );
    }

    /// ⛔ 这条测试就是合规边界本身：不管哪个动作、哪种能力，拼出来的参数里
    /// 都不许出现 official 模式、Frida、关自动更新、同步 skills、交互模式。
    #[test]
    fn the_arguments_can_never_ask_for_anything_but_safe_mode() {
        let script =
            Path::new(r"C:\QB\plugins\claude-desktop-zh-cn\1.4.8\scripts\install_windows.ps1");
        for action in [Action::Install, Action::Uninstall] {
            for caps in [
                ScriptCaps {
                    skip_asar_patch: true,
                },
                ScriptCaps {
                    skip_asar_patch: false,
                },
            ] {
                let args = script_args(action, caps, script);
                let joined = args.join(" ").to_ascii_lowercase();
                for banned in [
                    "official",
                    "frida",
                    "disable-updates",
                    "enable-updates",
                    "sync-skills",
                    "-interactive",
                    "bypass",
                ] {
                    assert!(!joined.contains(banned), "{banned} 出现在 {joined}");
                }
                let at = args.iter().position(|a| a == "-PatchMode").unwrap();
                assert_eq!(args[at + 1], "safe");
                assert_eq!(
                    args[args.iter().position(|a| a == "-File").unwrap() + 1],
                    script.display().to_string()
                );
                assert!(args.contains(&action.arg().to_string()));
                assert_eq!(
                    caps.skip_asar_patch,
                    args.contains(&"-SkipAsarPatch".to_string())
                );
            }
        }
    }

    #[test]
    fn the_app_dir_the_script_touched_is_read_from_its_output() {
        let lines = vec![
            "=== Claude Desktop Windows 简体中文 补丁 ===".to_string(),
            "  app: C:\\Users\\demo\\AppData\\Local\\AnthropicClaude\\app-2.9939.2".to_string(),
            "  resources: C:\\Users\\demo\\AppData\\Local\\AnthropicClaude\\app-2.9939.2\\resources"
                .to_string(),
        ];
        let got = reported_app_dir(&lines).unwrap();
        assert!(same_dir(
            &got,
            Path::new(r"c:\users\demo\appdata\local\anthropicclaude\app-2.9939.2\")
        ));
        assert_eq!(reported_app_dir(&["nothing here".to_string()]), None);
    }

    /// Claude 的 config.json 是 tab 缩进、里面有令牌缓存。只改 locale 那一个值的字节。
    const CONFIG: &str = "{\n\t\"updaterLastSeenVersion\": \"2.9939.2\",\n\t\"locale\": \"en-US\",\n\t\"oauth:tokenCache\": \"v10abc\\\"def{}[]\",\n\t\"nested\": {\n\t\t\"locale\": \"fr-FR\",\n\t\t\"list\": [1, \"}\", {\"a\": null}]\n\t},\n\t\"flag\": true\n}";

    #[test]
    fn only_the_top_level_locale_bytes_change() {
        assert_eq!(
            top_level_string(CONFIG, "locale").unwrap().as_deref(),
            Some("en-US")
        );
        let out = set_top_level_string(CONFIG, "locale", "zh-CN").unwrap();
        assert_eq!(
            out,
            CONFIG.replacen("\"locale\": \"en-US\"", "\"locale\": \"zh-CN\"", 1)
        );
        assert_eq!(
            top_level_string(&out, "locale").unwrap().as_deref(),
            Some("zh-CN")
        );
        // 嵌套对象里同名的键不动。
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["nested"]["locale"], "fr-FR");
        assert_eq!(v["oauth:tokenCache"], "v10abc\"def{}[]");
    }

    #[test]
    fn a_missing_locale_is_inserted_with_the_file_s_own_indentation() {
        let text = "{\n\t\"flag\": true,\n\t\"nested\": {\"locale\": \"x\"}\n}";
        let out = set_top_level_string(text, "locale", "zh-CN").unwrap();
        assert_eq!(
            out,
            "{\n\t\"locale\": \"zh-CN\",\n\t\"flag\": true,\n\t\"nested\": {\"locale\": \"x\"}\n}"
        );
        let empty = set_top_level_string("{}", "locale", "zh-CN").unwrap();
        assert_eq!(
            top_level_string(&empty, "locale").unwrap().as_deref(),
            Some("zh-CN")
        );
    }

    #[test]
    fn a_file_we_cannot_read_is_left_alone() {
        for bad in ["", "[1,2]", "{\"a\":", "\u{feff}{\"locale\":\"en-US\"}"] {
            assert!(
                set_top_level_string(bad, "locale", "zh-CN").is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn tags_are_restricted_to_safe_characters() {
        for ok in ["1.4.8", "v1.0.0", "2026.09.24-rc1"] {
            assert!(valid_tag(ok), "{ok}");
        }
        for bad in [
            "",
            "..",
            ".hidden",
            "-x",
            "a/b",
            "a b",
            "1.4.8%2F",
            &"9".repeat(65),
        ] {
            assert!(!valid_tag(bad), "{bad}");
        }
    }
}
