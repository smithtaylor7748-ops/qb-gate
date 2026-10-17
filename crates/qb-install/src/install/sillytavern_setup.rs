//! 软件页「SillyTavern（酒馆）」：下载安装 · 更新 · 卸载（2026-09-29，使用者定的）。
//!
//! 使用者：「酒馆变成本项目的功能之一，软件页增加下载酒馆的功能，然后被本项目接管」。
//! 问他时定了：安装 + 检查新版 / 更新 + 卸载；装托管版时把他现有那份的数据**复制**过来（原来那份不动）；
//! 没有 Node.js 时确认框里给「先用 winget 装 Node.js LTS」的勾选。位置表在 [`super::sillytavern`]。
//!
//! # ⛔ 边界（动任何一条同步改 DISCLAIMER 与 ATTRIBUTION 的 SillyTavern 那一节）
//!
//! 1. **只从 SillyTavern 官方 GitHub 取**：最新版读 `releases/latest` 的跳转（**不走 `api.github.com`**，
//!    同 `self_update` 的理由：共用出口的人会撞上未登录 60 次 / 小时的限额），源码包是那个标签的
//!    `archive/refs/tags/<tag>.zip`。只收 https，跳转只许落在 [`allowed_url`] 那三个域名。
//! 2. **不打包、不镜像、不改包。** 解出来的就是官方那个标签的源码，原样放着，用使用者本机的 Node.js 跑。
//!    SillyTavern 是 AGPL-3.0，面板只把它当独立进程启动（ATTRIBUTION 里那一节）。
//! 3. **官方不公布源码包的哈希**：边下边算的 SHA-256 **只记日志、写进标记**，不作判据 ——
//!    没有可信的对照值时拿它当判据就是自己跟自己比（同 `antigravity_setup` 那条）。
//! 4. **更新与卸载只碰带标记的那份**（[`super::sillytavern::is_managed`]）。使用者自己的酒馆一个字不改。
//! 5. **导入、迁移只复制，不搬**：源目录原样留着；更新时旧版整份留在 `sillytavern.previous`（只留一份），
//!    卸载前先把数据复制到 `%LOCALAPPDATA%\ClaudeIpGate\tavern-uninstall\<时间>`。
//! 6. **Node.js 只在使用者勾了之后才用 winget 装**（`OpenJS.NodeJS.LTS`，官方包，会弹 UAC）。
//! 7. **npm 不经 shell**：`node <npm-cli.js> install --prefix <目录> …`，PATH 前置 node 所在目录 ——
//!    刚用 winget 装上的 Node 不在面板进程那份旧 PATH 里。只有 node 旁边找不到 npm-cli.js
//!    （Scoop 之类的垫片）才退回 `cmd /c npm`。
//!
//! # 单测不联网
//!
//! 纯函数全在这里有单测（只用临时目录）。GitHub 的跳转形状、源码包的顶层布局换了时，先跑
//! `cargo run -p qb-install --example sillytavern-release -- latest`（只问、只打印标签与版本）。

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use super::sillytavern as st;
use crate::error::{GateError, Result};
use crate::sink::ProgressSink;

/// 七段。前端进度条按这个总数画。
pub const TOTAL: u32 = 7;
pub const REPO: &str = "SillyTavern/SillyTavern";
/// **唯一的「最新版」来源。** 它 302 到 `…/releases/tag/<标签>`。
pub const LATEST_URL: &str = "https://github.com/SillyTavern/SillyTavern/releases/latest";
pub const NODE_WINGET_ID: &str = "OpenJS.NodeJS.LTS";
/// 源码包的上限。正常的几十 MB；超过这个数就不是我们要的那个东西。
const ARCHIVE_MAX: u64 = 512 << 20;
/// `package.json` 的上限。
const PACKAGE_MAX: usize = 256 * 1024;

/// 照 SillyTavern 自带 `Start.bat` 的那一行（它每次启动都会再跑一遍；1.18.0 与 1.19.0 逐字相同，
/// 2026-09-29 用 `examples/sillytavern-release.rs download` 核过）。
/// 装的时候先跑一次，是为了让失败在「安装」这一步就说出来，而不是等第一次启动才撞上。
const NPM_ARGS: &[&str] = &[
    "--no-save",
    "--no-audit",
    "--no-fund",
    "--loglevel=error",
    "--no-progress",
    "--omit=dev",
    "--ignore-scripts",
];

pub fn archive_url(tag: &str) -> String {
    format!("https://github.com/{REPO}/archive/refs/tags/{tag}.zip")
}

pub fn package_url(tag: &str) -> String {
    format!("https://raw.githubusercontent.com/{REPO}/{tag}/package.json")
}

pub fn release_page(tag: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/{tag}")
}

fn allowed(u: &reqwest::Url) -> bool {
    u.scheme() == "https"
        && u.username().is_empty()
        && u.password().is_none()
        && u.port().is_none()
        && matches!(
            u.host_str(),
            Some("github.com") | Some("codeload.github.com") | Some("raw.githubusercontent.com")
        )
}

/// 这个地址能不能去。只认 https + 三个域名：`github.com`（发布页与源码包）、
/// `codeload.github.com`（源码包真正的下载 CDN）、`raw.githubusercontent.com`（标签下的 `package.json`）。
pub fn allowed_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| allowed(&u))
}

/// 跳转只许落在 [`allowed_url`] 认的域名上的客户端。
fn restricted_client() -> Result<reqwest::Client> {
    super::managed::http_with(reqwest::redirect::Policy::custom(|a| {
        if a.previous().len() >= 5 {
            a.error("跳转次数太多")
        } else if allowed(a.url()) {
            a.follow()
        } else {
            let host = a.url().host_str().unwrap_or("?").to_string();
            a.error(format!("跳到了不认的地址（{host}），没有跟过去"))
        }
    }))
}

/// 标签长什么样：可带 `v`，后面是严格的三段数字。SillyTavern 的发布标签是 `1.18.0` 这种。
pub fn valid_tag(tag: &str) -> bool {
    tag.len() <= 32
        && super::self_update::parse_version(tag.strip_prefix('v').unwrap_or(tag)).is_some()
}

/// 纯函数：`releases/latest` 回的 `Location` → 标签。只认本仓库的 `releases/tag/<标签>`，
/// 绝对地址（`https://github.com/…`）与站内相对地址都认；别的仓库、别的主机、认不出的标签一律 `None`。
pub fn tag_from_location(location: &str) -> Option<String> {
    let path = location
        .trim()
        .strip_prefix("https://github.com")
        .unwrap_or(location.trim());
    let rest = path.strip_prefix(&format!("/{REPO}/releases/tag/"))?;
    let tag = rest.split(['?', '#']).next()?;
    valid_tag(tag).then(|| tag.to_string())
}

/// 最新的正式版。**只用来显示与比较**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TavernRelease {
    pub tag: String,
    pub version: String,
    /// 那个标签的 `package.json` 里的 `engines.node`。读不到（`raw.githubusercontent.com`
    /// 被墙之类）是 `None` —— 下载解压之后还会按解出来的那份再核一次。
    pub node_engine: Option<String>,
    /// 本机的 Node.js 满不满足它：没装是 `false`；读不出版本、`engines` 读不到或认不出是 `None`。
    /// 确认框靠它决定要不要露出「先用 winget 装 Node.js LTS」。
    pub node_ok: Option<bool>,
    pub page_url: String,
    pub archive_url: String,
}

/// 问 GitHub 最新的正式版（不含预发布）。**联网**：一次 `github.com`，一次 `raw.githubusercontent.com`。
pub async fn latest() -> Result<TavernRelease> {
    release(None).await
}

/// 某一个正式版（`tag` 为 `None` 就是最新的那个）。**联网**。
///
/// 指定标签时先核格式（[`valid_tag`]）；那个标签的 `package.json` 读不到不算失败（有的网络里
/// `raw.githubusercontent.com` 不通）—— 标签不存在的话下载源码包那一步会拿到 404，照实报出来。
pub async fn release(tag: Option<&str>) -> Result<TavernRelease> {
    let tag = match tag.map(str::trim) {
        Some(t) if valid_tag(t) => t.to_string(),
        Some(t) => {
            return Err(GateError::Other(format!(
                "「{}」不是 SillyTavern 的正式版标签（长这样：1.19.0）",
                t.chars().take(40).collect::<String>()
            )))
        }
        None => latest_tag().await?,
    };
    let package = match fetch_package(&tag).await {
        Ok(text) => st::parse_package(&text),
        Err(_) => st::Package::default(),
    };
    // `where node.exe` + `node --version` 是两次起进程，放到阻塞线程池里。
    let node = tokio::task::spawn_blocking(node_info)
        .await
        .unwrap_or_default();
    let node_ok = match (&node.path, &node.version, &package.node_engine) {
        (None, _, _) => Some(false),
        (Some(_), Some(v), Some(e)) => node_satisfies(e, v),
        _ => None,
    };
    Ok(TavernRelease {
        version: package
            .version
            .unwrap_or_else(|| tag.trim_start_matches('v').to_string()),
        node_engine: package.node_engine,
        node_ok,
        page_url: release_page(&tag),
        archive_url: archive_url(&tag),
        tag,
    })
}

async fn latest_tag() -> Result<String> {
    let c = super::managed::http_with(reqwest::redirect::Policy::none())?;
    let resp = tokio::time::timeout(std::time::Duration::from_secs(30), c.get(LATEST_URL).send())
        .await
        .map_err(|_| GateError::Other("问 GitHub 最新版超过 30 秒没有回应".into()))?
        .map_err(|e| GateError::Other(format!("问 GitHub 最新版失败：{e}")))?;
    let status = resp.status();
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if !status.is_redirection() {
        return Err(GateError::Other(format!(
            "GitHub 没有给出 SillyTavern 的最新版（HTTP {status}）"
        )));
    }
    tag_from_location(&location).ok_or_else(|| {
        GateError::Other(format!(
            "GitHub 回的最新版地址认不出（{}）。先跑 cargo run -p qb-install --example sillytavern-release -- latest 看它现在长什么样。",
            location.chars().take(120).collect::<String>()
        ))
    })
}

// ------------------------------------------------------------------ 历史版本（2026-09-29）

/// 最近的正式版清单。**不走 `api.github.com`**（同 [`LATEST_URL`] 的理由：未登录一个出口 IP 一小时 60 次），
/// 读的是发布页自带的 Atom（`github.com` 本站，最近 10 个）。
pub const RELEASES_FEED: &str = "https://github.com/SillyTavern/SillyTavern/releases.atom";
/// Atom 的上限（10 个版本带更新说明，正常一两百 KB）。
const FEED_MAX: usize = 4 << 20;
/// 每一版的更新说明最多给多少字。
const NOTES_MAX: usize = 6000;

/// 发布清单里的一个版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TavernReleaseNote {
    pub tag: String,
    pub version: String,
    /// 发布日期（`YYYY-MM-DD`）。
    pub date: String,
    /// 更新说明（从发布页的 HTML 转成的纯文本，列表项以「• 」开头）。
    pub notes: String,
    pub page_url: String,
}

/// 纯函数：XML / HTML 里常见的几种实体换回字符。认不出的原样留着。
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..end];
        let ch = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 纯函数：发布说明的 HTML → 纯文本。`<li>` 变「• 」，块级标签换行，其余标签去掉；**不执行、不保留任何 HTML**。
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(end) = rest[i..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[i + 1..i + end].trim().to_ascii_lowercase();
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if name == "li" && !tag.starts_with('/') {
            out.push_str("\n• ");
        } else if matches!(
            name.as_str(),
            "br" | "p"
                | "div"
                | "ul"
                | "ol"
                | "li"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "tr"
        ) {
            out.push('\n');
        }
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    let text = unescape(&out);
    let mut lines: Vec<&str> = Vec::new();
    for l in text.lines().map(str::trim_end) {
        if l.trim().is_empty() && lines.last().is_none_or(|p: &&str| p.trim().is_empty()) {
            continue;
        }
        lines.push(l);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(close)? + a;
    Some(&s[a..b])
}

/// 纯函数：发布页 Atom → 版本清单（新的在前，照 Atom 的顺序）。标签只认本仓库 `releases/tag/<标签>` 的链接。
pub fn parse_releases_atom(xml: &str) -> Vec<TavernReleaseNote> {
    xml.split("<entry>")
        .skip(1)
        .filter_map(|raw| {
            let entry = raw.split("</entry>").next()?;
            let href = entry
                .split("<link")
                .skip(1)
                .find(|l| l.contains("rel=\"alternate\""))
                .and_then(|l| between(l, "href=\"", "\""))?;
            let tag = tag_from_location(&unescape(href))?;
            let date: String = between(entry, "<updated>", "</updated>")
                .unwrap_or_default()
                .chars()
                .take(10)
                .collect();
            let content = entry
                .find("<content")
                .and_then(|i| {
                    let body = &entry[i..];
                    let start = body.find('>')? + 1;
                    let end = body.find("</content>")?;
                    (start <= end).then(|| &body[start..end])
                })
                .unwrap_or_default();
            // content 是转义过一次的 HTML：先把 XML 实体换回来，再去标签。
            let mut notes = html_to_text(&unescape(content));
            if notes.chars().count() > NOTES_MAX {
                notes = format!("{}…", notes.chars().take(NOTES_MAX).collect::<String>());
            }
            Some(TavernReleaseNote {
                version: tag.trim_start_matches('v').to_string(),
                page_url: release_page(&tag),
                tag,
                date,
                notes,
            })
        })
        .collect()
}

/// 最近的正式版（最多 10 个，带更新说明）。**联网**，只在使用者点了之后问，没有定时器。
pub async fn releases() -> Result<Vec<TavernReleaseNote>> {
    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        restricted_client()?.get(RELEASES_FEED).send(),
    )
    .await
    .map_err(|_| GateError::Other("问 GitHub 版本清单超过 30 秒没有回应".into()))?
    .and_then(|r| r.error_for_status())
    .map_err(|e| GateError::Other(format!("问 GitHub 版本清单失败：{e}")))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| GateError::Other(format!("读 GitHub 版本清单失败：{e}")))?;
    if bytes.len() > FEED_MAX {
        return Err(GateError::Other(
            "GitHub 回的版本清单大得不正常，没有读".into(),
        ));
    }
    let list = parse_releases_atom(&String::from_utf8_lossy(&bytes));
    if list.is_empty() {
        return Err(GateError::Other(
            "GitHub 回的版本清单里一个版本都认不出来 —— 它的格式可能改了。「指定版本」里手填标签仍然能用。"
                .into(),
        ));
    }
    Ok(list)
}

async fn fetch_package(tag: &str) -> Result<String> {
    let url = package_url(tag);
    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        restricted_client()?.get(&url).send(),
    )
    .await
    .map_err(|_| GateError::Other(format!("读 {url} 超时")))?
    .and_then(|r| r.error_for_status())
    .map_err(|e| GateError::Other(format!("读 {url} 失败：{e}")))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| GateError::Other(format!("读 {url} 失败：{e}")))?;
    if bytes.len() > PACKAGE_MAX {
        return Err(GateError::Other(format!("{url} 大得不像 package.json")));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ------------------------------------------------------------------ Node.js

/// 本机的 Node.js。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct NodeInfo {
    pub path: Option<String>,
    /// `node --version` 的原样输出（`v24.14.1`）。
    pub version: Option<String>,
}

pub fn node_info() -> NodeInfo {
    let exe = super::detect::node_exe();
    let version = exe.as_deref().and_then(super::detect::node_version);
    NodeInfo {
        path: exe.map(|p| p.display().to_string()),
        version,
    }
}

/// `20` / `20.11` / `20.11.1`（可带 `v`）→ 三段数字；缺的段按 0。
fn parse_loose(v: &str) -> Option<[u64; 3]> {
    let v = v.trim().trim_start_matches('v');
    let mut out = [0u64; 3];
    let mut n = 0;
    for (i, part) in v.split('.').enumerate() {
        if i >= 3 || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out[i] = part.parse().ok()?;
        n += 1;
    }
    (n > 0).then_some(out)
}

/// 纯函数：本机这个 Node 满足 `engines.node` 吗。
///
/// 只认三种写法：`>= 20`（可带空格、1–3 段）、`^20.1`、`20` / `20.x`。
/// 带 `||`、区间（`>=18 <21`）或者别的写法一律 `None` —— **认不出就不下判断**，
/// 界面说「判断不了」，而不是猜一个「够」或「不够」。
pub fn node_satisfies(engine: &str, node_version: &str) -> Option<bool> {
    let have = parse_loose(node_version)?;
    let e = engine.trim();
    if e.contains("||") {
        return None;
    }
    if let Some(rest) = e.strip_prefix(">=") {
        let rest = rest.trim();
        if rest.contains(char::is_whitespace) {
            return None;
        }
        return Some(have >= parse_loose(rest)?);
    }
    if let Some(rest) = e.strip_prefix('^') {
        let need = parse_loose(rest)?;
        return Some(have[0] == need[0] && have >= need);
    }
    let major = e.strip_suffix(".x").unwrap_or(e);
    if major.bytes().all(|b| b.is_ascii_digit()) && !major.is_empty() {
        return Some(have[0] == major.parse::<u64>().ok()?);
    }
    None
}

/// Node 够用就返回 node.exe；不够又没勾「用 winget 装」就说清楚缺什么、怎么办。
async fn ensure_node(
    engine: Option<&str>,
    install_node: bool,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<PathBuf> {
    let verdict = |info: &NodeInfo| match (&info.path, &info.version, engine) {
        (None, _, _) => Some(false),
        (Some(_), Some(v), Some(e)) => node_satisfies(e, v),
        // 版本读不出、或者 SillyTavern 没写要求：判断不了，不拦。
        _ => None,
    };
    let info = node_info();
    let need = engine.unwrap_or("未写明");
    match verdict(&info) {
        Some(true) | None => {
            log.push(format!(
                "Node.js {}（{}），SillyTavern 要求 {need}",
                info.version.as_deref().unwrap_or("版本读不出"),
                info.path.as_deref().unwrap_or("?")
            ));
            return Ok(PathBuf::from(info.path.unwrap_or_default()));
        }
        Some(false) => {}
    }
    let what = match (&info.path, &info.version) {
        (None, _) => "本机没有 Node.js".to_string(),
        (Some(_), v) => format!(
            "本机的 Node.js {} 不够新（SillyTavern 要求 {need}）",
            v.as_deref().unwrap_or("?")
        ),
    };
    if !install_node {
        return Err(GateError::Other(format!(
            "{what}。勾上「先用 winget 装 Node.js LTS」再点一次，或者到 https://nodejs.org 装 LTS 版之后再来。"
        )));
    }
    rep.phase(2, &format!("{what}：用 winget 装 Node.js LTS（会弹 UAC）"));
    if super::winget::winget_version().await.is_none() {
        return Err(GateError::Other(format!(
            "{what}，而本机没有 winget，装不了。到 https://nodejs.org 下载 LTS 版装上再来。"
        )));
    }
    let args: Vec<String> = [
        "install",
        "--id",
        NODE_WINGET_ID,
        "-e",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    log.push(format!("winget install --id {NODE_WINGET_ID}"));
    if !super::winget::run_streaming("winget", &args, rep, 2, log).await? {
        return Err(GateError::Other(
            "winget 没装成 Node.js（退出码非零，它自己的话在进度日志里）。".into(),
        ));
    }
    let after = node_info();
    match verdict(&after) {
        Some(false) => Err(GateError::Other(format!(
            "winget 说装好了，可还是{}。重新登录一次 Windows 让 PATH 生效，或者到 https://nodejs.org 手动装。",
            if after.path.is_none() {
                "找不到 node.exe".to_string()
            } else {
                format!("只找到 {}", after.version.as_deref().unwrap_or("?"))
            }
        ))),
        _ => Ok(PathBuf::from(after.path.unwrap_or_default())),
    }
}

// ------------------------------------------------------------------ 数据

/// 要从一份酒馆带到另一份去的东西（相对路径）。照 SillyTavern 自己的 `Update-Instructions.txt`：
/// 1.12 起用户数据全在 `data\` 下；`config.yaml` 在根目录；服务端插件 `plugins\` 与全局扩展
/// `public\scripts\extensions\third-party\` 里**使用者自己加的**也带上（上游自带的占位文件不算）。
///
/// ⛔ **绝不整份拷 `public\`**（上游原话：会把新版弄坏）、不拷 `node_modules`。
pub fn import_plan(src: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if src.join("data").is_dir() {
        out.push(PathBuf::from("data"));
    }
    if src.join("config.yaml").is_file() {
        out.push(PathBuf::from("config.yaml"));
    }
    let third_party: PathBuf = ["public", "scripts", "extensions", "third-party"]
        .iter()
        .collect();
    let groups: [(PathBuf, &[&str]); 2] = [
        (PathBuf::from("plugins"), &[".gitkeep", "package.json"]),
        (third_party, &[".gitkeep"]),
    ];
    for (dir, placeholders) in groups {
        let Ok(rd) = std::fs::read_dir(src.join(&dir)) else {
            continue;
        };
        let mut names: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| {
                !placeholders
                    .iter()
                    .any(|p| e.file_name().eq_ignore_ascii_case(p))
            })
            .map(|e| dir.join(e.file_name()))
            .collect();
        names.sort();
        out.extend(names);
    }
    out
}

/// 复制一棵树。**跳过符号链接与目录联结**（记下来、不跟过去）—— 跟过去就可能把盘上别处的东西
/// 整份拷进来，或者在删的时候删到别处。返回复制了几个文件。
fn copy_tree(src: &Path, dst: &Path, skipped: &mut Vec<String>) -> std::io::Result<u64> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        skipped.push(src.display().to_string());
        return Ok(0);
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        let mut n = 0;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            n += copy_tree(&e.path(), &dst.join(e.file_name()), skipped)?;
        }
        Ok(n)
    } else {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::copy(src, dst)?;
        Ok(1)
    }
}

/// 按 [`import_plan`] 把 `src` 里的东西复制到 `dst`。**`src` 一个字节都不动。**
pub fn copy_plan(src: &Path, dst: &Path, plan: &[PathBuf]) -> Result<(u64, Vec<String>)> {
    let mut skipped = Vec::new();
    let mut files = 0;
    for rel in plan {
        files += copy_tree(&src.join(rel), &dst.join(rel), &mut skipped).map_err(|e| {
            GateError::Other(format!(
                "复制 {} 没成：{e}。已经复制过去的留在新的那份里，源目录没动。",
                src.join(rel).display()
            ))
        })?;
    }
    Ok((files, skipped))
}

/// 导入的源比要装的新版还新：不许导。新版 SillyTavern 读得懂旧版的数据，反过来不一定。
/// 任何一边的版本读不出就放行（写进日志），不猜。
pub fn check_import_version(source: Option<&str>, target: &str) -> Result<()> {
    if let Some(s) = source {
        if super::self_update::is_newer(s, target) {
            return Err(GateError::Other(format!(
                "要导入的那份酒馆是 {s}，比要装的正式版 {target} 还新 —— 旧版读新版的数据可能出错，没有导入。"
            )));
        }
    }
    Ok(())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

// ------------------------------------------------------------------ 下载与就位

/// 暂存目录的看守：没被 [`Staged::keep`] 解除时，析构时把它删掉。
/// 任何一步失败都不会在托管根下留一份半截的新版。
struct Staged {
    dir: PathBuf,
    armed: bool,
}

impl Staged {
    fn keep(&mut self) {
        self.armed = false;
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn clear(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(GateError::Other(format!(
            "清不掉上一次留下的 {}：{e}",
            dir.display()
        ))),
    }
}

/// 解压目录里唯一的那个顶层目录（`SillyTavern-<标签>`）。不是正好一个就不认。
fn single_subdir(dir: &Path) -> Result<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    match entries.as_slice() {
        [only] if only.is_dir() => Ok(only.clone()),
        _ => Err(GateError::Other(format!(
            "源码包的布局跟预期不一样（顶层有 {} 项，应该只有一个 SillyTavern-<版本> 目录）。先跑 cargo run -p qb-install --example sillytavern-release -- download 看看。",
            entries.len()
        ))),
    }
}

/// 下载 → 解压 → 放进 `target` 同级的暂存目录。返回（压缩包 SHA-256，暂存目录的看守）。
async fn fetch(
    rel: &TavernRelease,
    target: &Path,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<(String, Staged)> {
    let work = st::work_of(target);
    let staging = st::staging_of(target);
    clear(&work)?;
    clear(&staging)?;
    std::fs::create_dir_all(&work)?;
    let url = archive_url(&rel.tag);
    if !allowed_url(&url) {
        return Err(GateError::Other(format!("不认的下载地址：{url}")));
    }
    rep.phase(3, &format!("从 GitHub 下载 SillyTavern {}", rel.version));
    let zip = work.join("sillytavern.zip");
    let sha = super::managed::download_with(
        &restricted_client()?,
        &url,
        &zip,
        None,
        Some(ARCHIVE_MAX),
        rep,
        3,
    )
    .await;
    let sha = match sha {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            return Err(e);
        }
    };
    log.push(format!(
        "下载 {url}，SHA-256 {sha}（只留痕：官方不公布源码包的哈希）"
    ));
    rep.log(3, &format!("SHA-256 {sha}（只留痕）"));
    rep.phase(4, "解压");
    let out = work.join("x");
    let unpacked = async {
        super::managed::unzip(&zip, &out).await?;
        let top = single_subdir(&out)?;
        if !st::looks_like_sillytavern(&top) {
            return Err(GateError::Other(
                "解出来的不像 SillyTavern（没有 server.js，或者 package.json 的名字不对），没有装。"
                    .into(),
            ));
        }
        std::fs::rename(&top, &staging)
            .map_err(|e| GateError::Other(format!("解好的文件挪不进 {}：{e}", staging.display())))
    }
    .await;
    let _ = std::fs::remove_dir_all(&work);
    unpacked?;
    Ok((
        sha,
        Staged {
            dir: staging,
            armed: true,
        },
    ))
}

/// 只下载、解压，把解好的那份留在 `target` 同级的 `sillytavern.staging` 里 —— **给诊断例子用**
/// （`examples/sillytavern-release.rs download`）：核 GitHub 源码包的布局，不跑 npm、不写配置、不就位。
pub async fn download_and_unpack(
    rel: &TavernRelease,
    target: &Path,
    rep: &dyn ProgressSink,
) -> Result<(String, PathBuf)> {
    let mut log = Vec::new();
    let (sha, mut staged) = fetch(rel, target, rep, &mut log).await?;
    staged.keep();
    Ok((sha, staged.dir.clone()))
}

/// node.exe 旁边的 npm 本体（官方安装包、nvm-windows 都是这个布局）。
fn npm_cli_beside(node: &Path) -> Option<PathBuf> {
    let p = node
        .parent()?
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");
    p.is_file().then_some(p)
}

async fn npm_install(
    node: &Path,
    dir: &Path,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<()> {
    rep.phase(6, "装酒馆的依赖（npm install，首次要几分钟）");
    let node_dir = node
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let path = format!("{node_dir};{}", std::env::var("PATH").unwrap_or_default());
    let env = [("PATH", path.as_str()), ("NODE_ENV", "production")];
    let mut common: Vec<String> = vec![
        "install".into(),
        "--prefix".into(),
        dir.display().to_string(),
    ];
    common.extend(NPM_ARGS.iter().map(|s| s.to_string()));
    let ok = match npm_cli_beside(node) {
        Some(cli) => {
            let mut args = vec![cli.display().to_string()];
            args.extend(common);
            super::winget::run_streaming_env(&node.display().to_string(), &args, &env, rep, 6, log)
                .await?
        }
        None => {
            let mut args = vec!["/c".to_string(), "npm".to_string()];
            args.extend(common);
            super::winget::run_streaming_env("cmd", &args, &env, rep, 6, log).await?
        }
    };
    if !ok {
        return Err(GateError::Other(
            "npm install 没成（退出码非零，最后几行在进度日志里）—— 多半是连不上 npm 源。新下载的那份没有启用。"
                .into(),
        ));
    }
    Ok(())
}

/// 装好（或更新好）的结果。命令层拿它写酒馆配置。
#[derive(Debug, Clone)]
pub struct Installed {
    pub dir: PathBuf,
    pub launcher: PathBuf,
    pub version: String,
    /// 新那份 `config.yaml` 里的 `port`（导入过来的才有；全新安装由酒馆第一次启动时生成）。
    pub config_port: Option<u16>,
    /// 给界面的几句话（导入了多少、跳过了什么、旧版在哪）。
    pub notes: Vec<String>,
    /// 安装日志（给审计）。
    pub log: Vec<String>,
}

fn marker_for(rel: &TavernRelease, version: &str, sha: &str) -> st::Marker {
    st::Marker {
        version: version.to_string(),
        tag: rel.tag.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        archive_sha256: sha.to_string(),
        source: archive_url(&rel.tag),
    }
}

fn ask_phase(rep: &dyn ProgressSink, tag: Option<&str>) {
    match tag {
        Some(t) => rep.phase(1, &format!("核对 SillyTavern {t}")),
        None => rep.phase(1, "问 GitHub 最新的 SillyTavern 正式版"),
    }
}

/// 下载安装到 `<托管根>\sillytavern`，可选从一份已有的酒馆复制数据过来。
/// `tag` 给了就装那一版（2026-09-29「指定版本」），否则装最新的正式版。
///
/// 目标里已经是面板装的那份时不装（换版本走 [`update`]）；目标里有别的东西时不装。
pub async fn install(
    import_from: Option<&Path>,
    install_node: bool,
    tag: Option<&str>,
    rep: &dyn ProgressSink,
) -> Result<Installed> {
    let target = st::managed_dir();
    if st::is_managed(&target) {
        return Err(GateError::Other(format!(
            "{} 已经是面板装的那份了，要换新版请用「更新」。",
            target.display()
        )));
    }
    st::check_target(&target)?;
    if let Some(src) = import_from {
        if !st::looks_like_sillytavern(src) {
            return Err(GateError::Other(format!(
                "要导入的 {} 不像一份 SillyTavern（没有 server.js），没有装。",
                src.display()
            )));
        }
        if same_dir(src, &target) {
            return Err(GateError::Other("导入的来源就是要装的地方".into()));
        }
    }
    let mut log = Vec::new();
    ask_phase(rep, tag);
    let rel = release(tag).await?;
    if let Some(src) = import_from {
        let source_version = st::read_package(src).and_then(|p| p.version);
        check_import_version(source_version.as_deref(), &rel.version)?;
    }
    rep.phase(2, "检查 Node.js");
    let mut node = ensure_node(rel.node_engine.as_deref(), install_node, rep, &mut log).await?;
    let (sha, mut staged) = fetch(&rel, &target, rep, &mut log).await?;
    let pkg = st::read_package(&staged.dir).unwrap_or_default();
    if pkg.node_engine.is_some() && pkg.node_engine != rel.node_engine {
        node = ensure_node(pkg.node_engine.as_deref(), install_node, rep, &mut log).await?;
    }
    let mut notes = Vec::new();
    if let Some(src) = import_from {
        rep.phase(
            5,
            &format!("从 {} 复制你的数据（源目录不动）", src.display()),
        );
        let plan = import_plan(src);
        let (files, skipped) = copy_plan(src, &staged.dir, &plan)?;
        notes.push(format!(
            "从 {} 复制了 {files} 个文件（{}），原来那份没动",
            src.display(),
            plan.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        ));
        if !skipped.is_empty() {
            notes.push(format!("跳过了 {} 个链接（没跟过去）", skipped.len()));
        }
    }
    npm_install(&node, &staged.dir, rep, &mut log).await?;
    rep.phase(7, "就位");
    // 空目录（上次卸载留下的）先删掉：Windows 上改名改不到一个已存在的目录上。
    let _ = std::fs::remove_dir(&target);
    std::fs::rename(&staged.dir, &target)
        .map_err(|e| GateError::Other(format!("新装的那份挪不进 {}：{e}", target.display())))?;
    staged.keep();
    let version = pkg.version.unwrap_or_else(|| rel.version.clone());
    st::write_marker(&target, &marker_for(&rel, &version, &sha))?;
    Ok(Installed {
        launcher: st::launcher_in(&target),
        config_port: st::config_port(&target),
        dir: target,
        version,
        notes,
        log,
    })
}

/// 换上新版的三次改名，任何一步失败都尽力回到原样。
///
/// `current` → `previous`（先删掉更早的那份，只留一份），`staging` → `current`。
/// 第二步失败就把 `previous` 改回 `current`；连这一步也失败，两份都原样留着、如实说在哪。
pub fn swap_in(current: &Path, staging: &Path, previous: &Path) -> Result<()> {
    clear(previous)?;
    std::fs::rename(current, previous).map_err(|e| {
        GateError::Other(format!(
            "没能把现在这份挪开（{e}）—— 多半是酒馆或者别的程序还开着里面的文件。新版没有换上，现在这份原样没动。"
        ))
    })?;
    if let Err(e) = std::fs::rename(staging, current) {
        return Err(match std::fs::rename(previous, current) {
            Ok(()) => GateError::Other(format!(
                "新版没能就位（{e}），已经把原来那份放回去了。"
            )),
            Err(back) => GateError::Other(format!(
                "新版没能就位（{e}），原来那份也没能放回去（{back}）：原来那份在 {}，新版在 {}，都没删。",
                previous.display(),
                staging.display()
            )),
        });
    }
    Ok(())
}

/// 纯函数：从 `have` 换到 `want` 行不行。没指定版本（`explicit = false`）只许往新了换；
/// 指定了版本可以往旧了换（降级），但同一版不换。
pub fn check_switch(have: &str, want: &str, explicit: bool) -> Result<()> {
    let same = have.trim_start_matches('v') == want.trim_start_matches('v');
    if explicit {
        if same {
            return Err(GateError::Other(format!("现在就是 {have}，没有换。")));
        }
        return Ok(());
    }
    if !super::self_update::is_newer(want, have) {
        return Err(GateError::Other(format!(
            "已经是最新的正式版（现在 {have}，GitHub 上最新 {want}），没有更新。"
        )));
    }
    Ok(())
}

/// 把面板装的那份换成最新的正式版（`tag` 给了就换成那一版，可以是更旧的）：
/// 数据复制到新那份里，原来那份整份留在 `sillytavern.previous`。
pub async fn update(
    current: &Path,
    install_node: bool,
    tag: Option<&str>,
    rep: &dyn ProgressSink,
) -> Result<Installed> {
    if !st::is_managed(current) {
        return Err(GateError::Other(format!(
            "{} 不是面板装的那份，面板不替它更新（用它自己的方式更新，例如 git pull）。",
            current.display()
        )));
    }
    let mut log = Vec::new();
    ask_phase(rep, tag);
    let rel = release(tag).await?;
    let have = st::read_package(current)
        .and_then(|p| p.version)
        .or_else(|| st::read_marker(current).map(|m| m.version))
        .unwrap_or_default();
    check_switch(&have, &rel.version, tag.is_some())?;
    let downgrade = super::self_update::is_newer(&have, &rel.version);
    rep.phase(2, "检查 Node.js");
    let mut node = ensure_node(rel.node_engine.as_deref(), install_node, rep, &mut log).await?;
    let (sha, mut staged) = fetch(&rel, current, rep, &mut log).await?;
    let pkg = st::read_package(&staged.dir).unwrap_or_default();
    if pkg.node_engine.is_some() && pkg.node_engine != rel.node_engine {
        node = ensure_node(pkg.node_engine.as_deref(), install_node, rep, &mut log).await?;
    }
    rep.phase(5, "把你的数据复制到要换上的那一版（原来那份原样留着）");
    let plan = import_plan(current);
    let (files, skipped) = copy_plan(current, &staged.dir, &plan)?;
    npm_install(&node, &staged.dir, rep, &mut log).await?;
    rep.phase(7, "换上去");
    let previous = st::previous_of(current);
    // 交给 `swap_in`：它自己知道哪种失败要删、哪种要留（两份都放不回去时新版不能删）。
    staged.keep();
    if let Err(e) = swap_in(current, &staged.dir, &previous) {
        if current.is_dir() && staged.dir.is_dir() {
            let _ = std::fs::remove_dir_all(&staged.dir);
        }
        return Err(e);
    }
    let version = pkg.version.unwrap_or_else(|| rel.version.clone());
    st::write_marker(current, &marker_for(&rel, &version, &sha))?;
    let mut notes = vec![
        format!("从 {have} 换到了 {version}"),
        format!("你的数据复制了 {files} 个文件过来"),
        format!(
            "原来那份（{have}）整份留在 {}（只留这一份，下次换版本会换掉它）",
            previous.display()
        ),
    ];
    if downgrade {
        notes.push(
            "这是降级：新版可能已经把数据改成旧版不认识的样子；出了问题点「换回留着的那一版」就回到原来那份".into(),
        );
    }
    if !skipped.is_empty() {
        notes.push(format!("跳过了 {} 个链接（没跟过去）", skipped.len()));
    }
    Ok(Installed {
        dir: current.to_path_buf(),
        launcher: st::launcher_in(current),
        config_port: st::config_port(current),
        version,
        notes,
        log,
    })
}

fn version_of(dir: &Path) -> String {
    st::read_package(dir)
        .and_then(|p| p.version)
        .or_else(|| st::read_marker(dir).map(|m| m.version))
        .unwrap_or_else(|| "?".into())
}

fn remove_item(p: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// 「换回留着的那一版」（2026-09-29，使用者选的「版本回退」）：现役与 `sillytavern.previous` 对调。
///
/// * `carry_data = true`：先把留着那份**自己的**数据复制到 `backup_root\<时间>` 再清掉，然后把现在这份的数据复制过去 ——
///   换过去之后聊天、角色卡还是现在这些（新版改过的数据旧版可能读不懂，确认框里写着）。
/// * `carry_data = false`：用留着那份自己的数据（它被换下来那一刻的样子）。
///
/// 两种都不删掉任何一份数据：对调之后现在这份整份留在 `sillytavern.previous`，再点一次就换回来。
/// 只对面板装的那份做（[`st::is_managed`]）。
pub fn swap_with_previous(
    current: &Path,
    carry_data: bool,
    backup_root: &Path,
) -> Result<Installed> {
    if !st::is_managed(current) {
        return Err(GateError::Other(format!(
            "{} 不是面板装的那份，面板不替它换版本。",
            current.display()
        )));
    }
    let previous = st::previous_of(current);
    if !st::looks_like_sillytavern(&previous) {
        return Err(GateError::Other(
            "没有留着的那一版（sillytavern.previous 不在，或者不像一份 SillyTavern）。".into(),
        ));
    }
    let (have, other) = (version_of(current), version_of(&previous));
    let mut notes = vec![format!("从 {have} 换回了 {other}")];
    if carry_data {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let backup = (1..1000)
            .map(|n| {
                if n == 1 {
                    backup_root.join(&stamp)
                } else {
                    backup_root.join(format!("{stamp}-{n}"))
                }
            })
            .find(|p| !p.exists())
            .ok_or_else(|| GateError::Other("备份目录同一秒建得太多了".into()))?;
        let own = import_plan(&previous);
        copy_plan(&previous, &backup, &own)?;
        if previous.join("data").is_dir() && !backup.join("data").is_dir() {
            return Err(GateError::Other(format!(
                "留着那份自己的数据没有备份成（{} 里没有 data），没有换。",
                backup.display()
            )));
        }
        for rel in &own {
            remove_item(&previous.join(rel)).map_err(|e| {
                GateError::Other(format!(
                    "清留着那份的 {} 没成（{e}）；它自己的数据已经备份在 {}，两份酒馆都还没换。",
                    rel.display(),
                    backup.display()
                ))
            })?;
        }
        let (files, _) = copy_plan(current, &previous, &import_plan(current))?;
        notes.push(format!("带过去了现在的数据（{files} 个文件）"));
        notes.push(format!("留着那份原来自己的数据备份在 {}", backup.display()));
    } else {
        notes.push("用的是留着那份自己的数据（它被换下来那一刻的样子）".into());
    }
    let temp = st::staging_of(current);
    clear(&temp)?;
    std::fs::rename(current, &temp).map_err(|e| {
        GateError::Other(format!(
            "没能把现在这份挪开（{e}）—— 多半是酒馆或者别的程序还开着里面的文件。没有换。"
        ))
    })?;
    if let Err(e) = std::fs::rename(&previous, current) {
        return Err(match std::fs::rename(&temp, current) {
            Ok(()) => GateError::Other(format!(
                "留着那份没能就位（{e}），现在这份已经放回去了。"
            )),
            Err(back) => GateError::Other(format!(
                "留着那份没能就位（{e}），现在这份也没能放回去（{back}）：它在 {}，留着那份在 {}，都没删。",
                temp.display(),
                previous.display()
            )),
        });
    }
    if let Err(e) = std::fs::rename(&temp, &previous) {
        notes.push(format!(
            "换下来的那份没能改名成 sillytavern.previous（{e}），它在 {}",
            temp.display()
        ));
    } else {
        notes.push(format!("换下来的 {have} 整份留在 {}", previous.display()));
    }
    Ok(Installed {
        dir: current.to_path_buf(),
        launcher: st::launcher_in(current),
        config_port: st::config_port(current),
        version: other,
        notes,
        log: Vec::new(),
    })
}

/// 卸载的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TavernUninstalled {
    /// 数据备份到了哪。
    pub backup: String,
    /// 备份了几个文件。
    #[ts(type = "number")]
    pub backed_up: u64,
    /// 删掉了哪些目录。
    pub removed: Vec<String>,
    /// 没删掉的（多半有文件正被占用），原样说出来。
    pub left: Vec<String>,
}

/// 卸载面板装的那份：先把数据**复制**到 `backup_root\<时间>`，核过备份再删。
pub fn uninstall(current: &Path, backup_root: &Path) -> Result<TavernUninstalled> {
    if !st::is_managed(current) {
        return Err(GateError::Other(format!(
            "{} 不是面板装的那份，面板不删它。",
            current.display()
        )));
    }
    let dest = backup_root.join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
    let plan = import_plan(current);
    let (files, _) = copy_plan(current, &dest, &plan)?;
    // 源里有 data 而备份里没有：不删。
    if current.join("data").is_dir() && !dest.join("data").is_dir() {
        return Err(GateError::Other(format!(
            "数据没有备份成（{} 里没有 data），没有卸载。",
            dest.display()
        )));
    }
    std::fs::remove_dir_all(current).map_err(|e| {
        GateError::Other(format!(
            "删 {} 删到一半停下了：{e}（多半有文件正被占用）。你的数据已经备份在 {}。",
            current.display(),
            dest.display()
        ))
    })?;
    let mut removed = vec![current.display().to_string()];
    let mut left = Vec::new();
    for extra in [
        st::previous_of(current),
        st::staging_of(current),
        st::work_of(current),
    ] {
        if !extra.exists() {
            continue;
        }
        match std::fs::remove_dir_all(&extra) {
            Ok(()) => removed.push(extra.display().to_string()),
            Err(e) => left.push(format!("{}（{e}）", extra.display())),
        }
    }
    Ok(TavernUninstalled {
        backup: dest.display().to_string(),
        backed_up: files,
        removed,
        left,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qb-st-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_https_on_the_three_github_hosts_is_allowed() {
        assert!(allowed_url(LATEST_URL));
        assert!(allowed_url(&archive_url("1.18.0")));
        assert!(allowed_url(&package_url("1.18.0")));
        assert!(allowed_url(
            "https://codeload.github.com/SillyTavern/SillyTavern/zip/refs/tags/1.18.0"
        ));
        assert!(!allowed_url("http://github.com/SillyTavern/SillyTavern"));
        assert!(!allowed_url("https://github.com.evil.example/x"));
        assert!(!allowed_url("https://github.com@evil.example/x"));
        assert!(!allowed_url("https://github.com:8443/x"));
        assert!(!allowed_url("https://objects.githubusercontent.com/x"));
        assert!(!allowed_url("not a url"));
    }

    #[test]
    fn the_latest_tag_is_read_from_this_repositorys_release_redirect_only() {
        let abs = "https://github.com/SillyTavern/SillyTavern/releases/tag/1.18.0";
        assert_eq!(tag_from_location(abs).as_deref(), Some("1.18.0"));
        assert_eq!(
            tag_from_location("/SillyTavern/SillyTavern/releases/tag/v1.13.4?x=1").as_deref(),
            Some("v1.13.4")
        );
        // 别的仓库、别的主机、认不出的标签：一律不认。
        assert_eq!(
            tag_from_location("https://github.com/someone/SillyTavern/releases/tag/1.18.0"),
            None
        );
        assert_eq!(
            tag_from_location("https://evil.example/SillyTavern/SillyTavern/releases/tag/1.18.0"),
            None
        );
        assert_eq!(
            tag_from_location("https://github.com/SillyTavern/SillyTavern/releases/tag/../../x"),
            None
        );
        assert_eq!(
            tag_from_location("https://github.com/SillyTavern/SillyTavern/releases"),
            None
        );
        assert_eq!(tag_from_location(""), None);
        assert!(valid_tag("1.12.14") && valid_tag("v1.12.14"));
        assert!(!valid_tag("1.12") && !valid_tag("1.12.14-staging") && !valid_tag("latest"));
    }

    #[test]
    fn the_node_requirement_is_judged_only_when_it_can_be_read() {
        // SillyTavern 1.18 写的是 `>= 20`（带空格）。
        assert_eq!(node_satisfies(">= 20", "v24.14.1"), Some(true));
        assert_eq!(node_satisfies(">=18", "v16.20.2"), Some(false));
        assert_eq!(node_satisfies(">=18.17.0", "v18.17.0"), Some(true));
        assert_eq!(node_satisfies("^20.1", "v20.0.9"), Some(false));
        assert_eq!(node_satisfies("^20.1", "v21.0.0"), Some(false));
        assert_eq!(node_satisfies("20.x", "v20.11.1"), Some(true));
        // 认不出的写法：不下判断，不猜。
        assert_eq!(node_satisfies(">=18 <21", "v20.0.0"), None);
        assert_eq!(node_satisfies("^18 || ^20", "v20.0.0"), None);
        assert_eq!(node_satisfies("lts", "v20.0.0"), None);
        assert_eq!(node_satisfies(">= 20", "not-a-version"), None);
    }

    #[test]
    fn the_import_plan_takes_user_data_and_never_the_whole_public_folder() {
        let src = temp("plan");
        for d in [
            "data/default-user/chats",
            "plugins/my-plugin",
            "public/scripts/extensions/third-party/my-ext",
            "node_modules/x",
            "public/css",
        ] {
            std::fs::create_dir_all(src.join(d)).unwrap();
        }
        for f in [
            "config.yaml",
            "plugins/.gitkeep",
            "plugins/package.json",
            "public/scripts/extensions/third-party/.gitkeep",
            "public/index.html",
            "data/default-user/chats/a.jsonl",
        ] {
            std::fs::write(src.join(f), "x").unwrap();
        }
        let plan: Vec<String> = import_plan(&src)
            .iter()
            .map(|p| p.display().to_string().replace('\\', "/"))
            .collect();
        assert_eq!(
            plan,
            vec![
                "data",
                "config.yaml",
                "plugins/my-plugin",
                "public/scripts/extensions/third-party/my-ext",
            ]
        );

        // 复制之后源一个字节都没少。
        let dst = temp("plan-dst");
        let (files, skipped) = copy_plan(&src, &dst, &import_plan(&src)).unwrap();
        assert_eq!(files, 2, "data 里一个聊天 + config.yaml");
        assert!(skipped.is_empty());
        assert!(dst.join("data/default-user/chats/a.jsonl").is_file());
        assert!(src.join("data/default-user/chats/a.jsonl").is_file());
        assert!(!dst.join("public/index.html").exists());
        assert!(!dst.join("node_modules").exists());
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn data_from_a_newer_tavern_is_not_imported_into_an_older_release() {
        assert!(check_import_version(Some("1.19.0"), "1.18.0").is_err());
        assert!(check_import_version(Some("1.18.0"), "1.18.0").is_ok());
        assert!(check_import_version(Some("1.17.2"), "1.18.0").is_ok());
        // 读不出版本：放行，不猜。
        assert!(check_import_version(None, "1.18.0").is_ok());
        assert!(check_import_version(Some("staging"), "1.18.0").is_ok());
    }

    #[test]
    fn swapping_keeps_exactly_one_previous_copy() {
        let root = temp("swap");
        let (cur, stg, prev) = (
            root.join("sillytavern"),
            root.join("sillytavern.staging"),
            root.join("sillytavern.previous"),
        );
        for (d, v) in [(&cur, "current"), (&stg, "new"), (&prev, "older")] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("v"), v).unwrap();
        }
        swap_in(&cur, &stg, &prev).unwrap();
        assert_eq!(std::fs::read_to_string(cur.join("v")).unwrap(), "new");
        // 更早那份（older）没了：只留一份旧版。
        assert_eq!(std::fs::read_to_string(prev.join("v")).unwrap(), "current");
        assert!(!stg.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_swap_puts_the_current_copy_back() {
        let root = temp("swap-fail");
        let (cur, stg, prev) = (
            root.join("sillytavern"),
            root.join("sillytavern.staging"),
            root.join("sillytavern.previous"),
        );
        std::fs::create_dir_all(&cur).unwrap();
        std::fs::write(cur.join("v"), "current").unwrap();
        // 暂存目录不存在 → 第二次改名失败 → 原来那份得回到原位。
        let e = swap_in(&cur, &stg, &prev).unwrap_err().to_string();
        assert!(e.contains("放回去了"), "{e}");
        assert_eq!(std::fs::read_to_string(cur.join("v")).unwrap(), "current");
        assert!(!prev.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstall_backs_up_data_first_and_refuses_what_the_panel_did_not_install() {
        let root = temp("uninstall");
        let cur = root.join("sillytavern");
        std::fs::create_dir_all(cur.join("data/default-user")).unwrap();
        std::fs::write(cur.join("data/default-user/settings.json"), "{}").unwrap();
        std::fs::write(cur.join("config.yaml"), "port: 8000\n").unwrap();
        std::fs::create_dir_all(root.join("sillytavern.previous")).unwrap();
        let backups = root.join("backups");
        // 没有标记：使用者自己的那份，面板不删。
        assert!(uninstall(&cur, &backups).is_err());
        assert!(cur.join("config.yaml").is_file());
        st::write_marker(
            &cur,
            &st::Marker {
                version: "1.18.0".into(),
                tag: "1.18.0".into(),
                installed_at: String::new(),
                archive_sha256: String::new(),
                source: String::new(),
            },
        )
        .unwrap();
        let r = uninstall(&cur, &backups).unwrap();
        assert!(!cur.exists());
        assert!(!root.join("sillytavern.previous").exists());
        let backup = PathBuf::from(&r.backup);
        assert!(backup.join("data/default-user/settings.json").is_file());
        assert!(backup.join("config.yaml").is_file());
        assert_eq!(r.backed_up, 2);
        assert!(r.left.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 一份假的 SillyTavern：`server.js` + `package.json`（名字对、带版本）+ 面板的标记 + 一个聊天。
    fn fake_st(dir: &Path, version: &str, chat: &str) {
        std::fs::create_dir_all(dir.join("data/default-user/chats")).unwrap();
        std::fs::write(dir.join("server.js"), "//").unwrap();
        std::fs::write(
            dir.join("package.json"),
            format!(r#"{{"name":"sillytavern","version":"{version}"}}"#),
        )
        .unwrap();
        std::fs::write(dir.join("data/default-user/chats/a.jsonl"), chat).unwrap();
        st::write_marker(
            dir,
            &st::Marker {
                version: version.into(),
                tag: version.into(),
                installed_at: String::new(),
                archive_sha256: String::new(),
                source: String::new(),
            },
        )
        .unwrap();
    }

    fn chat(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("data/default-user/chats/a.jsonl")).unwrap()
    }

    /// 带着现在的数据换回去：程序换成留着那一版，聊天还是现在的；留着那份原来的数据进了备份；
    /// 换下来的那份整份留着，再换一次就回来。
    #[test]
    fn swapping_back_carries_todays_data_and_keeps_both_copies() {
        let root = temp("swap-back");
        let (cur, prev, backups) = (
            root.join("sillytavern"),
            root.join("sillytavern.previous"),
            root.join("rollback"),
        );
        fake_st(&cur, "1.19.0", "today");
        fake_st(&prev, "1.18.0", "back then");
        let r = swap_with_previous(&cur, true, &backups).unwrap();
        assert_eq!(r.version, "1.18.0");
        assert_eq!(version_of(&cur), "1.18.0");
        assert_eq!(chat(&cur), "today", "聊天要跟着过去");
        assert_eq!(version_of(&prev), "1.19.0", "换下来的那份整份留着");
        assert_eq!(chat(&prev), "today");
        let saved = std::fs::read_dir(&backups)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        assert_eq!(chat(&saved), "back then", "留着那份原来的数据不许丢");
        assert!(!st::staging_of(&cur).exists());
        // 再换一次：回到 1.19.0。
        swap_with_previous(&cur, true, &backups).unwrap();
        assert_eq!(version_of(&cur), "1.19.0");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn swapping_back_with_its_own_data_leaves_todays_data_in_the_other_copy() {
        let root = temp("swap-own");
        let (cur, prev) = (root.join("sillytavern"), root.join("sillytavern.previous"));
        fake_st(&cur, "1.19.0", "today");
        fake_st(&prev, "1.18.0", "back then");
        swap_with_previous(&cur, false, &root.join("rollback")).unwrap();
        assert_eq!(chat(&cur), "back then");
        assert_eq!(chat(&prev), "today");
        assert!(!root.join("rollback").exists(), "不带数据就不用备份");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn swapping_back_needs_a_managed_copy_and_a_kept_one() {
        let root = temp("swap-refuse");
        let cur = root.join("sillytavern");
        fake_st(&cur, "1.19.0", "today");
        assert!(
            swap_with_previous(&cur, true, &root.join("b")).is_err(),
            "没有留着的那一版"
        );
        std::fs::remove_file(cur.join(st::MARKER)).unwrap();
        fake_st(&root.join("sillytavern.previous"), "1.18.0", "x");
        std::fs::remove_file(cur.join(st::MARKER)).ok();
        assert!(
            swap_with_previous(&cur, true, &root.join("b")).is_err(),
            "不是面板装的不换"
        );
        assert_eq!(chat(&cur), "today");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_named_version_may_go_back_but_not_sideways() {
        assert!(
            check_switch("1.19.0", "1.18.0", true).is_ok(),
            "指定了就能降级"
        );
        assert!(check_switch("1.19.0", "1.19.0", true).is_err());
        assert!(check_switch("1.19.0", "v1.19.0", true).is_err());
        assert!(check_switch("1.18.0", "1.19.0", false).is_ok());
        assert!(
            check_switch("1.19.0", "1.18.0", false).is_err(),
            "没指定就只往新了换"
        );
        assert!(check_switch("1.19.0", "1.19.0", false).is_err());
    }

    /// 发布页 Atom（照 2026-09-29 实际回来的形状删短）：标签、日期、更新说明都读得出；
    /// 更新说明里的 HTML 一个标签都不留。
    #[test]
    fn the_release_feed_is_read_into_plain_text() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <link type="text/html" rel="alternate" href="https://github.com/SillyTavern/SillyTavern/releases"/>
  <entry>
    <id>tag:github.com,2008:Repository/599524116/1.19.0</id>
    <updated>2026-09-14T18:05:11Z</updated>
    <link rel="alternate" type="text/html" href="https://github.com/SillyTavern/SillyTavern/releases/tag/1.19.0"/>
    <title>1.19.0</title>
    <content type="html">&lt;h1&gt;SillyTavern 1.19.0&lt;/h1&gt;
&lt;h2&gt;Backends&lt;/h2&gt;
&lt;ul&gt;
&lt;li&gt;Anthropic: Added &lt;code&gt;x&lt;/code&gt; &amp;amp; more &amp;quot;quoted&amp;quot;.&lt;/li&gt;
&lt;li&gt;&lt;script&gt;alert(1)&lt;/script&gt;OpenAI&lt;/li&gt;
&lt;/ul&gt;</content>
  </entry>
  <entry>
    <updated>2026-05-03T15:55:24Z</updated>
    <link rel="alternate" type="text/html" href="https://github.com/SillyTavern/SillyTavern/releases/tag/1.18.0"/>
    <content type="html"></content>
  </entry>
  <entry>
    <link rel="alternate" type="text/html" href="https://github.com/someone/else/releases/tag/9.9.9"/>
  </entry>
</feed>"#;
        let list = parse_releases_atom(xml);
        assert_eq!(list.len(), 2, "别的仓库的链接不认");
        assert_eq!(list[0].tag, "1.19.0");
        assert_eq!(list[0].date, "2026-09-14");
        assert_eq!(list[1].version, "1.18.0");
        let n = &list[0].notes;
        assert!(n.starts_with("SillyTavern 1.19.0\n\nBackends"), "{n}");
        assert!(n.contains("• Anthropic: Added x & more \"quoted\"."), "{n}");
        assert!(
            !n.contains('<') && !n.contains('>'),
            "HTML 标签一个都不留：{n}"
        );
        assert!(list[1].notes.is_empty());
    }

    #[test]
    fn entities_that_are_not_real_stay_as_they_are() {
        assert_eq!(
            unescape("a &amp;&lt; b &#x4E2D;&#25991; &bogus; & c"),
            "a &< b 中文 &bogus; & c"
        );
    }

    #[test]
    fn symlinks_inside_the_data_are_not_followed() {
        let src = temp("links");
        std::fs::create_dir_all(src.join("data")).unwrap();
        std::fs::write(src.join("data/real.txt"), "x").unwrap();
        // 建不了链接（没开发者模式、没权限）就只测普通文件那一半。
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&src, src.join("data/loop")).is_ok();
        #[cfg(not(windows))]
        let linked = std::os::unix::fs::symlink(&src, src.join("data/loop")).is_ok();
        let dst = temp("links-dst");
        let (files, skipped) = copy_plan(&src, &dst, &[PathBuf::from("data")]).unwrap();
        assert_eq!(files, 1);
        assert_eq!(skipped.len(), usize::from(linked));
        assert!(!dst.join("data/loop").exists());
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }
}
