//! QB Gate **自己**的更新（0.25.3，使用者定的）。
//!
//! 使用者 2026-09-24 要的是：「我 GitHub 上传了新版本，已经装了老版本的用户打开软件时
//! 弹窗收到更新通知，还有一键更新」。问他时定了两件事：验真用**同一个 Release 里的
//! `SHA256SUMS.txt`**（跟 README 教大家手动核对的是同一件事，不另管一把签名私钥）；
//! 带这个功能的第一版是 0.25.3。
//!
//! 这个模块只管「问、下、核」三件事，全是单步操作；什么时候问、下完怎么退出交给安装包，
//! 归接口层（`src-tauri/src/update.rs`）。
//!
//! # 问哪里
//!
//! 只问本项目自己的发布页：[`manifest_url`] = `github.com/<REPO>/releases/latest/download/update.json`。
//! 这份文件由 `release.yml` 在打标签发版时生成（`scripts/update-manifest.mjs`），
//! 里面是版本号、标签、发布时刻和给人看的更新说明。
//!
//! **不走 `api.github.com`。** 未登录的 API 一个出口 IP 一小时只给 60 次，而用这个面板的人
//! 大多走共用出口（VPN / 机场）—— 同一个出口上别人把额度用完了，他的更新检查就永远失败，
//! 而症状只是「从来没收到过提醒」。Release 附件走的是下载 CDN，不吃这个限额。
//!
//! # 认哪一版
//!
//! 只认**严格的三段数字**（`0.25.3`），而且必须**比当前这份大**（逐段比数字，[`is_newer`]）。
//! 带预发布后缀、格式不对、标签跟版本号对不上的，一律当「没有新版」，不猜。
//!
//! ⚠ 这意味着**以后每一版的版本号都必须比已经发出去的大** —— 0.25.1 曾经在 0.32.0 之后
//! 发布（使用者定的号，见 CHANGELOG），那种「数字变小」的版本，装着更大号的人收不到提醒。
//!
//! # 怎么核
//!
//! 下载前先取同一个标签下的 `SHA256SUMS.txt`，找出固定名安装包那一行的哈希
//! （[`expected_sha256`]）；边下边算（`managed::download`），**对不上就删掉、不装**。
//! 地址全部由这里按常量拼出来（[`asset_url`]），`update.json` 里的任何字段都不会被当成地址用 ——
//! 那份文件只提供版本号与说明。
//!
//! 单测只测纯函数，**不联网**（CLAUDE.md「单测不许碰真实的运行期状态」）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;

/// 本项目的 GitHub 仓库。**改版（fork）发布时要改成自己的** —— 附加条款本来就要求改名，
/// 指着原仓库的话，改版的使用者会被「更新」回原版。
pub const REPO: &str = "smithtaylor7748-ops/qb-gate";
/// `release.yml` 生成的更新清单。
pub const MANIFEST: &str = "update.json";
/// 固定文件名的安装包。README 的「一键下载」也链它，`release.yml` 每版都放一份。
pub const INSTALLER: &str = "QB-Gate-Windows-x64-setup.exe";
/// 同一个 Release 里的哈希清单。
pub const SUMS: &str = "SHA256SUMS.txt";

/// 更新清单最多收多大。正常的只有几 KB；再大就不是我们发的那份。
const MANIFEST_MAX: usize = 256 * 1024;
/// 哈希清单最多收多大。正常的只有两三行。
const SUMS_MAX: usize = 64 * 1024;
/// 给人看的说明最多留多长（字符）。弹窗里放不下更长的，也不该有更长的。
const NOTES_MAX: usize = 20_000;

/// 最新那一版的信息。**只用来显示与比较**，里面没有一个字段会被当成下载地址。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateInfo {
    /// 三段数字，例如 `0.25.4`。
    pub version: String,
    /// `v` + 版本号。
    pub tag: String,
    /// 发布时刻（`update.json` 里写的，RFC 3339）。没写就是 `None`。
    pub published_at: Option<String>,
    /// 给人看的更新说明（中文那一段）。纯文本，按行排：`## ` 开头是小标题、`- ` 开头是条目。
    pub notes: String,
    /// 这一版的 Release 页面，给「在 GitHub 上看」用。
    pub page_url: String,
}

/// `releases/latest/download/update.json`：GitHub 把它重定向到最新那个 Release 的附件。
pub fn manifest_url() -> String {
    format!("https://github.com/{REPO}/releases/latest/download/{MANIFEST}")
}

/// 某个标签下的附件地址。**只接受 [`parse_manifest`] 核过的标签。**
pub fn asset_url(tag: &str, name: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{name}")
}

/// 某个标签的 Release 页面。
pub fn page_url(tag: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/{tag}")
}

/// 下载目录：`%LOCALAPPDATA%\ClaudeIpGate\updates`。每次下载前整个清空。
pub fn download_dir() -> PathBuf {
    qb_foundation::paths::state_dir().join("updates")
}

/// 严格的三段数字。`v0.25.3`、`0.25`、`0.25.3-beta.1`、`0.25.03` 之外的怪写法都不认。
///
/// 前导零不认（`0.25.03`）：semver 本身就禁止，而认了之后 `03` 与 `3` 算同一版，
/// 标签就能跟版本号「看着不一样、比起来一样」。
pub fn parse_version(v: &str) -> Option<[u64; 3]> {
    let mut out = [0u64; 3];
    let mut parts = v.split('.');
    for slot in &mut out {
        let p = parts.next()?;
        if p.is_empty()
            || p.len() > 9
            || !p.bytes().all(|b| b.is_ascii_digit())
            || (p.len() > 1 && p.starts_with('0'))
        {
            return None;
        }
        *slot = p.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(out)
}

/// `candidate` 比 `current` 新吗。任何一边解析不了都算「不新」—— 宁可不提醒，也不乱提醒。
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[derive(Deserialize)]
struct RawManifest {
    version: String,
    tag: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

/// 解析 `update.json`。纯函数。
///
/// 版本号必须是严格三段数字，标签必须恰好是 `v` + 版本号 —— 标签会被拼进下载地址，
/// 所以这里就是那条地址的闸门。说明可以没有（弹窗照样能更新，只是没得读）。
pub fn parse_manifest(json: &str) -> Result<UpdateInfo> {
    let raw: RawManifest = serde_json::from_str(json)
        .map_err(|e| GateError::Other(format!("update.json 读不懂：{e}")))?;
    let version = raw.version.trim().to_string();
    if parse_version(&version).is_none() {
        return Err(GateError::Other(format!(
            "update.json 里的版本号「{version}」不是三段数字"
        )));
    }
    let tag = raw.tag.trim().to_string();
    if tag != format!("v{version}") {
        return Err(GateError::Other(format!(
            "update.json 里的标签「{tag}」跟版本号 {version} 对不上"
        )));
    }
    let notes: String = raw
        .notes
        .unwrap_or_default()
        .replace("\r\n", "\n")
        .trim()
        .chars()
        .take(NOTES_MAX)
        .collect();
    let published_at = raw
        .published_at
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(UpdateInfo {
        page_url: page_url(&tag),
        version,
        tag,
        published_at,
        notes,
    })
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 从 `SHA256SUMS.txt` 里找出固定名安装包那一行的哈希（小写）。纯函数。
///
/// 行的形状是 `sha256sum` 那种：`<64 位十六进制><空白><可选的 *><文件名>`。
///
/// 另外做一次交叉核对：清单里如果也有带版本号的那份安装包（`QB Gate_<版本>_x64-setup.exe`，
/// GitHub 附件名里空格会变成点），它的哈希必须跟固定名那份**一样** —— 固定名那份是
/// `release.yml` 从带版本号那份复制出来的，两者不一致就说明固定名指着的不是这一版。
pub fn expected_sha256(sums: &str, version: &str) -> Result<String> {
    let mut fixed: Option<String> = None;
    let mut versioned: Option<String> = None;
    let versioned_names = [
        format!("QB Gate_{version}_x64-setup.exe"),
        format!("QB.Gate_{version}_x64-setup.exe"),
    ];
    for line in sums.lines() {
        let line = line.trim();
        let Some((hash, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !is_sha256_hex(hash) {
            continue;
        }
        let name = rest.trim_start().trim_start_matches('*').trim();
        let hash = hash.to_ascii_lowercase();
        if name == INSTALLER {
            fixed = Some(hash);
        } else if versioned_names.iter().any(|n| n == name) {
            versioned = Some(hash);
        }
    }
    let fixed = fixed.ok_or_else(|| {
        GateError::Other(format!(
            "{SUMS} 里没有 {INSTALLER} 这一行，没法核对，不下载"
        ))
    })?;
    if let Some(v) = versioned {
        if v != fixed {
            return Err(GateError::Other(format!(
                "{SUMS} 里 {INSTALLER} 跟 {version} 那份安装包的哈希不一样 —— 固定名指着的不是这一版，不下载"
            )));
        }
    }
    Ok(fixed)
}

/// 文件现在的 SHA-256 还等不等于 `expected`。启动安装包之前最后核一次用。
pub fn file_matches(path: &Path, expected: &str) -> bool {
    super::managed::sha256_file(path).is_some_and(|h| h.eq_ignore_ascii_case(expected))
}

async fn get_limited(url: &str, max: usize) -> Result<Vec<u8>> {
    let c = super::managed::http()?;
    let r = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let resp = c.get(url).send().await?;
        let status = resp.status();
        let body = resp.bytes().await?;
        Ok::<_, reqwest::Error>((status, body))
    })
    .await
    .map_err(|_| GateError::Other(format!("访问 {url} 超过 30 秒没有回应")))?
    .map_err(|e| GateError::Other(format!("访问 {url} 失败：{e}")))?;
    let (status, body) = r;
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(GateError::Other(format!(
            "GitHub 上找不到 {url}（最新那一版发布得比更新功能早，或者还在发布中）"
        )));
    }
    if !status.is_success() {
        return Err(GateError::Other(format!("访问 {url} 失败：HTTP {status}")));
    }
    if body.len() > max {
        return Err(GateError::Other(format!(
            "{url} 有 {} 字节，超出正常大小，不读",
            body.len()
        )));
    }
    Ok(body.to_vec())
}

/// 问一次 GitHub：最新那一版是多少。**会联网**，只在启动检查（设置里可关）或手动点「检查更新」时调。
pub async fn fetch_latest() -> Result<UpdateInfo> {
    let bytes = get_limited(&manifest_url(), MANIFEST_MAX).await?;
    let text = String::from_utf8(bytes)
        .map_err(|_| GateError::Other("update.json 不是 UTF-8 文本".into()))?;
    parse_manifest(&text)
}

/// 下载 `info` 那一版的安装包并核对 SHA-256。返回（安装包路径，核对过的哈希）。
///
/// 四段进度：取哈希清单 → 下载 → 核对 → （调用方收尾）。对不上就把文件删掉。
pub async fn download(info: &UpdateInfo, rep: &dyn ProgressSink) -> Result<(PathBuf, String)> {
    // 标签会进地址。`parse_manifest` 已经核过，这里再挡一次：调用方可能拿的是别处来的 info。
    if parse_version(&info.version).is_none() || info.tag != format!("v{}", info.version) {
        return Err(GateError::Other(
            "更新信息里的版本号或标签不合规，不下载".into(),
        ));
    }

    rep.phase(1, &format!("读取 {} 的 {SUMS}", info.tag));
    let sums = get_limited(&asset_url(&info.tag, SUMS), SUMS_MAX).await?;
    let sums = String::from_utf8_lossy(&sums);
    let want = expected_sha256(&sums, &info.version)?;
    rep.log(1, &format!("{INSTALLER} 应为 {want}"));

    let dir = download_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(format!("QB-Gate-{}-setup.exe", info.version));

    rep.phase(2, &format!("下载 QB Gate {}", info.version));
    let got = super::managed::download(&asset_url(&info.tag, INSTALLER), &dest, None, rep, 2)
        .await
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&dest);
        })?;

    rep.phase(3, "核对 SHA-256");
    if !got.eq_ignore_ascii_case(&want) {
        let _ = std::fs::remove_file(&dest);
        return Err(GateError::Other(format!(
            "下载下来的安装包跟 {SUMS} 对不上（算出 {got}，应为 {want}），已删除，没有安装"
        )));
    }
    rep.log(3, &format!("SHA-256 一致：{got}"));
    Ok((dest, want))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "cd3cd7a9f711ec45176ff5b2ef5479bdaabd9780d0078ed1b97006a36a57aedb";
    const H2: &str = "906e6cb914ecaed973cf17886f16593ea40c6c7ac0eb2c2180b96ce9fa090199";

    #[test]
    fn only_strict_three_part_versions_parse() {
        assert_eq!(parse_version("0.25.3"), Some([0, 25, 3]));
        assert_eq!(parse_version("10.0.12"), Some([10, 0, 12]));
        for bad in [
            "",
            "v0.25.3",
            "0.25",
            "0.25.3.1",
            "0.25.3-beta.1",
            "0.25.03",
            "0..3",
            "0.25.x",
            " 0.25.3",
            "1234567890.0.0",
        ] {
            assert_eq!(parse_version(bad), None, "{bad:?} 不该被认成版本号");
        }
    }

    /// 逐段比数字，不是比字符串：`0.25.10` 比 `0.25.9` 新。
    #[test]
    fn newer_compares_numbers_not_strings() {
        assert!(is_newer("0.25.4", "0.25.3"));
        assert!(is_newer("0.25.10", "0.25.9"));
        assert!(is_newer("0.26.0", "0.25.99"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.25.3", "0.25.3"), "同一版不算新");
        assert!(!is_newer("0.25.2", "0.25.3"), "旧版不算新");
    }

    /// 0.25.1 是在 0.32.0 之后发的（使用者定的号）。装着 0.32.0 的机器上，
    /// 数字更小的版本**不提醒** —— 这是按版本号比较的代价，模块头写着。
    #[test]
    fn a_smaller_number_is_never_offered_even_if_it_was_published_later() {
        assert!(!is_newer("0.25.3", "0.32.0"));
        assert!(!is_newer("0.25.1", "0.32.0"));
    }

    #[test]
    fn unparsable_versions_are_never_newer() {
        assert!(!is_newer("0.26.0-beta.1", "0.25.3"));
        assert!(!is_newer("garbage", "0.25.3"));
        assert!(!is_newer("0.26.0", "dev"));
    }

    #[test]
    fn manifest_parses_and_builds_the_page_url_from_the_tag() {
        // `r###`：说明以 `"## ` 开头，`r#"` 与 `r##"` 都会在那里提前收尾。
        let info = parse_manifest(
            r###"{"version":"0.25.4","tag":"v0.25.4","published_at":"2026-10-01T08:00:00Z",
                "notes":"## 更新内容\r\n\r\n- 修了一个问题\r\n","notes_en":"ignored"}"###,
        )
        .unwrap();
        assert_eq!(info.version, "0.25.4");
        assert_eq!(info.tag, "v0.25.4");
        assert_eq!(info.published_at.as_deref(), Some("2026-10-01T08:00:00Z"));
        assert_eq!(info.notes, "## 更新内容\n\n- 修了一个问题");
        assert_eq!(
            info.page_url,
            "https://github.com/smithtaylor7748-ops/qb-gate/releases/tag/v0.25.4"
        );
    }

    #[test]
    fn manifest_without_notes_still_parses() {
        let info = parse_manifest(r#"{"version":"0.25.4","tag":"v0.25.4"}"#).unwrap();
        assert_eq!(info.notes, "");
        assert_eq!(info.published_at, None);
    }

    /// 标签会被拼进下载地址，所以它必须**恰好**是 `v` + 版本号。
    #[test]
    fn a_tag_that_does_not_match_the_version_is_rejected() {
        for json in [
            r#"{"version":"0.25.4","tag":"v0.25.5"}"#,
            r#"{"version":"0.25.4","tag":"0.25.4"}"#,
            r#"{"version":"0.25.4","tag":"v0.25.4/../../evil"}"#,
            r#"{"version":"0.25.4-rc.1","tag":"v0.25.4-rc.1"}"#,
            r#"{"version":"latest","tag":"vlatest"}"#,
            r#"{"tag":"v0.25.4"}"#,
            "not json",
        ] {
            assert!(parse_manifest(json).is_err(), "{json} 不该过");
        }
    }

    #[test]
    fn overly_long_notes_are_cut() {
        let long = "字".repeat(NOTES_MAX + 50);
        let json = format!(r#"{{"version":"0.25.4","tag":"v0.25.4","notes":"{long}"}}"#);
        assert_eq!(
            parse_manifest(&json).unwrap().notes.chars().count(),
            NOTES_MAX
        );
    }

    /// `release.yml` 写出来的就是这个形状：两个空格分隔，带版本号那份的文件名里有空格。
    #[test]
    fn sums_as_the_release_workflow_writes_them() {
        let sums = format!(
            "{H1}  QB Gate_0.25.4_x64-setup.exe\r\n{H1}  QB-Gate-Windows-x64-setup.exe\r\n"
        );
        assert_eq!(expected_sha256(&sums, "0.25.4").unwrap(), H1);
    }

    #[test]
    fn sums_accept_the_binary_marker_uppercase_hex_and_the_dotted_asset_name() {
        let sums = format!(
            "{}  *QB-Gate-Windows-x64-setup.exe\n{H1} QB.Gate_0.25.4_x64-setup.exe\n",
            H1.to_ascii_uppercase()
        );
        assert_eq!(expected_sha256(&sums, "0.25.4").unwrap(), H1);
    }

    #[test]
    fn sums_without_the_fixed_name_line_refuse() {
        let sums = format!("{H1}  QB Gate_0.25.4_x64-setup.exe\n");
        assert!(expected_sha256(&sums, "0.25.4").is_err());
        assert!(expected_sha256("", "0.25.4").is_err());
    }

    /// 固定名那份跟这一版的安装包哈希不一样 = 固定名指着别的版本，不下载。
    #[test]
    fn sums_where_the_fixed_name_points_at_another_build_refuse() {
        let sums =
            format!("{H2}  QB Gate_0.25.4_x64-setup.exe\n{H1}  QB-Gate-Windows-x64-setup.exe\n");
        assert!(expected_sha256(&sums, "0.25.4").is_err());
    }

    /// 别的版本那一行不影响判断（清单里只该有这一版，但多一行也不该误伤）。
    #[test]
    fn sums_lines_for_other_versions_and_garbage_are_ignored() {
        let sums = format!(
            "# comment\n{H2}  QB Gate_0.25.3_x64-setup.exe\nnot-a-hash  QB-Gate-Windows-x64-setup.exe\n{H1}  QB-Gate-Windows-x64-setup.exe\n"
        );
        assert_eq!(expected_sha256(&sums, "0.25.4").unwrap(), H1);
    }

    #[test]
    fn urls_are_built_from_constants_only() {
        assert_eq!(
            manifest_url(),
            "https://github.com/smithtaylor7748-ops/qb-gate/releases/latest/download/update.json"
        );
        assert_eq!(
            asset_url("v0.25.4", INSTALLER),
            "https://github.com/smithtaylor7748-ops/qb-gate/releases/download/v0.25.4/QB-Gate-Windows-x64-setup.exe"
        );
    }
}
