//! Codex 桌面端（Microsoft Store 版）直装（0.28.0）：不打开 Store 也能把官方 MSIX 装上。
//!
//! # 为什么要有这个
//!
//! 面板对这个包能检测、按槽位启动、上锁、挂看门狗（`codex_desktop`、`workspace::launch`），
//! 唯独**装不了**：找不到时只会说「请自行到 Store 装」。两条路把这个洞补上：
//!
//! | 路 | 做法 | 什么时候用 |
//! |---|---|---|
//! | A · winget | `winget install --id 9PLM9XGG6VKS --source msstore` —— 走 Store 自己的部署通道，打包服务、依赖、注册都由系统管，不需要提权 | 有 winget、msstore 源可用 |
//! | B · 直连 | DisplayCatalog → FE3 `GetCookie` / `SyncUpdates` / `GetExtendedUpdateInfo2` 取签名过的 CDN 地址 → 下载 `.msix` → 核对 SHA-256（清单里有就核）与 Authenticode 主体含 OpenAI → `Add-AppxPackage` | A 没成（没 winget、msstore 被策略禁、要 Entra 登录、地区拦截） |
//!
//! 两条路装出来的都是**正规注册的 Store 包**：包身份、`codex://`、自动更新、面板按槽位
//! `CODEX_HOME` 启动全部照旧。
//!
//! # 来源与许可
//!
//! 三份 SOAP 信封模板（`codex_store/*.xml`）改自 chrichuang218/codex-windows-cn
//! （MIT，Copyright (c) 2026 vaportail），它又改自 StoreDev/StoreLib（MIT）；
//! 匿名 MSA 设备票据同源。解析与流程是自己写的，逐条记在 ATTRIBUTION.md。
//! **不采用**它们「解压 `app/` 子树、未打包运行」与「`versions/` + junction 多版本」两套做法：
//! 那会丢包身份（`codex://`、自动更新），而面板启动的本来就是正规安装的那份。
//!
//! # 硬条件
//!
//! - 下载地址的主机必须在 `delivery.mp.microsoft.com` 底下 —— FE3 回什么就下什么等于跟着
//!   一个未知地址走；
//! - 装之前核对 Authenticode 主体含 OpenAI；清单里给了 SHA-256 就一并核，对不上就删掉不装；
//! - `Add-AppxPackage` 非提权撞上 `0x80073D28`（包里带打包服务，注册要管理员，跟 §7.42
//!   是同一件事）时改走 UAC 提权，使用者拒绝就如实返回「已取消」；
//! - **Store 包的身份只在 `codex_desktop` 里**（product id / 包名 / 包族名），这里只引用；
//! - 单测不联网、不装真包：解析全是纯函数，拿录下来的报文形状测。
//!
//! # 开源兼容
//!
//! 架构按本机探（x64 / arm64），不写死；winget 缺失、msstore 被禁、地区拦截三种情况都
//! 如实报出各自的原因，并把下下来的 `.msix` 留给使用者手动 `Add-AppxPackage`。

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use std::path::{Path, PathBuf};

use super::codex_desktop::{PACKAGE_NAME, PRODUCT_ID};

const DISPLAY_CATALOG: &str = "https://displaycatalog.mp.microsoft.com/v7.0/products";
const FE3: &str = "https://fe3.delivery.mp.microsoft.com/ClientWebService/client.asmx";
const FE3_SECURED: &str =
    "https://fe3.delivery.mp.microsoft.com/ClientWebService/client.asmx/secured";
/// FE3 只认 Windows Update 客户端的 UA。
const WU_AGENT: &str = "Windows-Update-Agent/10.0.10011.16384 Client-Protocol/1.40";

const GET_COOKIE: &str = include_str!("codex_store/get-cookie.xml");
const SYNC_UPDATES: &str = include_str!("codex_store/sync-updates.xml");
const FILE_URL: &str = include_str!("codex_store/file-url.xml");

/// FE3 的证书链：`*.delivery.mp.microsoft.com` ← `Microsoft Update Secure Server CA 2.1`
/// ← **`Microsoft Root Certificate Authority 2011`**。这个根在每台 Windows 的系统证书库里
/// （Windows Update 自己就靠它），但**不在 Mozilla 的根清单里** —— 面板的 reqwest 走 rustls +
/// webpki 根，于是 `GetCookie` 那一步会被当成未知 CA 拒掉（2026-09-20 实机撞上，curl 走
/// schannel 就能通）。所以把这一份根**只**加给本模块的客户端，别的客户端不受影响。
///
/// 来源两处对得上（字节相同）：`https://www.microsoft.com/pki/certs/MicRooCerAut2011_2011_03_22.crt`
/// 与本机 `Cert:\LocalMachine\Root`。SHA-256 指纹
/// `847df6a78497943f27fc72eb93f9a637320a02b561d0a91b09e87a7807ed7c61`（有测试钉着），
/// 有效期到 2036-03-22。下载 CDN（`tlu.dl.delivery.mp.microsoft.com`）用的是公共 CA，不需要它。
const MICROSOFT_ROOT_2011: &[u8] = include_bytes!("codex_store/microsoft-root-2011.pem");
#[cfg(test)]
const MICROSOFT_ROOT_2011_SHA256: &str =
    "847df6a78497943f27fc72eb93f9a637320a02b561d0a91b09e87a7807ed7c61";

/// 匿名 MSA 设备票据。StoreLib 系工具多年共用的同一个值：免费应用不登录 Microsoft 账户
/// 也能调 `GetExtendedUpdateInfo2` 拿到下载地址。它不是任何人的账户凭证，
/// 也不是面板的密钥 —— 只是 Store 客户端匿名会话的样子。
const MSA_TOKEN: &str = "<Device>dAA9AEUAdwBBAHcAQQBzAE4AMwBCAEEAQQBVADEAYgB5AHMAZQBtAGIAZQBEAFYAQwArADMAZgBtADcAbwBXAHkASAA3AGIAbgBnAEcAWQBtAEEAQQBMAGoAbQBqAFYAVQB2AFEAYwA0AEsAVwBFAC8AYwBDAEwANQBYAGUANABnAHYAWABkAGkAegBHAGwAZABjADEAZAAvAFcAeQAvAHgASgBQAG4AVwBRAGUAYwBtAHYAbwBjAGkAZwA5AGoAZABwAE4AawBIAG0AYQBzAHAAVABKAEwARAArAFAAYwBBAFgAbQAvAFQAcAA3AEgAagBzAEYANAA0AEgAdABsAC8AMQBtAHUAcgAwAFMAdQBtAG8AMABZAGEAdgBqAFIANwArADQAcABoAC8AcwA4ADEANgBFAFkANQBNAFIAbQBnAFIAQwA2ADMAQwBSAEoAQQBVAHYAZgBzADQAaQB2AHgAYwB5AEwAbAA2AHoAOABlAHgAMABrAFgAOQBPAHcAYQB0ADEAdQBwAFMAOAAxAEgANgA4AEEASABzAEoAegBnAFQAQQBMAG8AbgBBADIAWQBBAEEAQQBpAGcANQBJADMAUQAvAFYASABLAHcANABBAEIAcQA5AFMAcQBhADEAQgA4AGsAVQAxAGEAbwBLAEEAdQA0AHYAbABWAG4AdwBWADMAUQB6AHMATgBtAEQAaQBqAGgANQBkAEcAcgBpADgAQQBlAEUARQBWAEcAbQBXAGgASQBCAE0AUAAyAEQAVwA0ADMAZABWAGkARABUAHoAVQB0AHQARQBMAEgAaABSAGYAcgBhAGIAWgBsAHQAQQBUAEUATABmAHMARQBGAFUAYQBRAFMASgB4ADUAeQBRADgAagBaAEUAZQAyAHgANABCADMAMQB2AEIAMgBqAC8AUgBLAGEAWQAvAHEAeQB0AHoANwBUAHYAdAB3AHQAagBzADYAUQBYAEIAZQA4AHMAZwBJAG8AOQBiADUAQQBCADcAOAAxAHMANgAvAGQAUwBFAHgATgBEAEQAYQBRAHoAQQBYAFAAWABCAFkAdQBYAFEARQBzAE8AegA4AHQAcgBpAGUATQBiAEIAZQBUAFkAOQBiAG8AQgBOAE8AaQBVADcATgBSAEYAOQAzAG8AVgArAFYAQQBiAGgAcAAwAHAAUgBQAFMAZQBmAEcARwBPAHEAdwBTAGcANwA3AHMAaAA5AEoASABNAHAARABNAFMAbgBrAHEAcgAyAGYARgBpAEMAUABrAHcAVgBvAHgANgBuAG4AeABGAEQAbwBXAC8AYQAxAHQAYQBaAHcAegB5AGwATABMADEAMgB3AHUAYgBtADUAdQBtAHAAcQB5AFcAYwBLAFIAagB5AGgAMgBKAFQARgBKAFcANQBnAFgARQBJADUAcAA4ADAARwB1ADIAbgB4AEwAUgBOAHcAaQB3AHIANwBXAE0AUgBBAFYASwBGAFcATQBlAFIAegBsADkAVQBxAGcALwBwAFgALwB2AGUATAB3AFMAawAyAFMAUwBIAGYAYQBLADYAagBhAG8AWQB1AG4AUgBHAHIAOABtAGIARQBvAEgAbABGADYASgBDAGEAYQBUAEIAWABCAGMAdgB1AGUAQwBKAG8AOQA4AGgAUgBBAHIARwB3ADQAKwBQAEgAZQBUAGIATgBTAEUAWABYAHoAdgBaADYAdQBXADUARQBBAGYAZABaAG0AUwA4ADgAVgBKAGMAWgBhAEYASwA3AHgAeABnADAAdwBvAG4ANwBoADAAeABDADYAWgBCADAAYwBZAGoATAByAC8ARwBlAE8AegA5AEcANABRAFUASAA5AEUAawB5ADAAZAB5AEYALwByAGUAVQAxAEkAeQBpAGEAcABwAGgATwBQADgAUwAyAHQANABCAHIAUABaAFgAVAB2AEMAMABQADcAegBPACsAZgBHAGsAeABWAG0AKwBVAGYAWgBiAFEANQA1AHMAdwBFAD0AJgBwAD0A</Device>";

/// 安装进度的段数，前端进度条按它画。
pub const TOTAL: u32 = 7;

// ---------------------------------------------------------------- 架构

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    /// 本机架构。64 位面板在 ARM64 上跑在模拟层里，进程自己看到的
    /// `PROCESSOR_ARCHITECTURE` 可能是 AMD64，所以先看 `PROCESSOR_ARCHITEW6432`。
    pub fn current() -> Arch {
        let arm = ["PROCESSOR_ARCHITEW6432", "PROCESSOR_ARCHITECTURE"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .any(|v| v.eq_ignore_ascii_case("ARM64"));
        if arm {
            Arch::Arm64
        } else {
            Arch::X64
        }
    }

    /// 包 moniker 里的写法：`OpenAI.Codex_<版本>_<这个>__<hash>`。
    pub fn moniker(self) -> &'static str {
        match self {
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
        }
    }

    /// FE3 设备属性里的写法。
    pub fn device(self) -> &'static str {
        match self {
            Arch::X64 => "AMD64",
            Arch::Arm64 => "ARM64",
        }
    }
}

// ---------------------------------------------------------------- 纯解析

/// SyncUpdates 回的一个可下载的包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub moniker: String,
    pub update_id: String,
    pub revision: String,
}

/// 清单里那个 `.msix` 文件：名字、SHA-1（base64，FE3 用它对应下载地址）、
/// 以及可能有的 SHA-256（十六进制）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    pub name: String,
    pub digest_b64: String,
    pub sha256: Option<String>,
    /// 清单里的 `Size`，给进度条当总数（实机 825 MB，没有它进度条是空转的）。
    pub size: Option<u64>,
}

/// 查到的最新版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub moniker: String,
    pub version: String,
    pub update_id: String,
    pub revision: String,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

/// DisplayCatalog 的回包里找 `WuCategoryId`。`FulfillmentData` 有两种形状：
/// 已经是对象，或者是一段 JSON 字符串 —— 两种都要认。
pub fn category_id_from_catalog(json: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| GateError::Other(format!("DisplayCatalog 的回包不是 JSON：{e}")))?;
    let skus = v
        .pointer("/Product/DisplaySkuAvailabilities")
        .and_then(|x| x.as_array())
        .ok_or_else(|| {
            GateError::Other("DisplayCatalog 的回包里没有 DisplaySkuAvailabilities".into())
        })?;
    for sku in skus {
        let Some(fd) = sku.pointer("/Sku/Properties/FulfillmentData") else {
            continue;
        };
        let inner: serde_json::Value = match fd {
            serde_json::Value::String(s) => match serde_json::from_str(s) {
                Ok(x) => x,
                Err(_) => continue,
            },
            other => other.clone(),
        };
        if let Some(id) = inner.get("WuCategoryId").and_then(|x| x.as_str()) {
            if !id.trim().is_empty() {
                return Ok(id.trim().to_string());
            }
        }
    }
    Err(GateError::Other(
        "DisplayCatalog 的回包里没有 WuCategoryId（这个 product id 在 Store 上下架了？）".into(),
    ))
}

/// 把 SOAP 文本节点里转义过的内层 XML 还原。顺序有讲究：`&amp;` 最后换，
/// 否则 `&amp;lt;` 会被错拆成 `<`。
pub fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `<Tag>文本</Tag>` 里的文本（第一个）。只认没有命名空间前缀的写法 —— FE3 的回包就是这样。
fn element_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim())
}

/// 一个起始标签文本里某个属性的值：`Name="value"` 或 `Name='value'`。
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let i = rest.find(name)?;
        let after = &rest[i + name.len()..];
        // 前面得是空白（不是别的属性名的尾巴），后面得是 `=`。
        let boundary_ok = i == 0
            || rest[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        let after_eq = after.trim_start();
        if boundary_ok && after_eq.starts_with('=') {
            let v = after_eq[1..].trim_start();
            let quote = v.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &v[1..];
                let end = inner.find(quote)?;
                return Some(inner[..end].to_string());
            }
            return None;
        }
        rest = &rest[i + name.len()..];
    }
}

/// `GetCookie` 回包里的会话票据。
pub fn cookie_from_response(xml: &str) -> Option<String> {
    element_text(xml, "EncryptedData")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// SyncUpdates 回包里每个 `<UpdateInfo>` 块：第一个 `<UpdateIdentity …/>` 是它自己的身份
/// （后面 Relationships 里的是前置依赖，不算），`<SecuredFragment` 表示这是能下载的叶子，
/// `PackageMoniker` 说明它是哪个包。三样都有才算候选。**传进来的是已经
/// [`decode_entities`] 过的文本。**
pub fn candidates_from_sync(xml: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<UpdateInfo>") {
        let block_start = i + "<UpdateInfo>".len();
        let Some(len) = rest[block_start..].find("</UpdateInfo>") else {
            break;
        };
        let block = &rest[block_start..block_start + len];
        rest = &rest[block_start + len..];
        if !block.contains("<SecuredFragment") {
            continue;
        }
        let identity = block
            .find("<UpdateIdentity")
            .map(|j| &block[j..])
            .and_then(|t| t.find('>').map(|k| &t[..k]));
        let moniker = block
            .find("PackageMoniker")
            .map(|j| &block[j..])
            .and_then(|t| attr(t, "PackageMoniker"));
        let (Some(identity), Some(moniker)) = (identity, moniker) else {
            continue;
        };
        let (Some(update_id), Some(revision)) =
            (attr(identity, "UpdateID"), attr(identity, "RevisionNumber"))
        else {
            continue;
        };
        out.push(Candidate {
            moniker,
            update_id,
            revision,
        });
    }
    out
}

/// `OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0` → (`OpenAI.Codex`, `26.707.3748.0`, `x64`)。
pub fn moniker_parts(moniker: &str) -> Option<(&str, &str, &str)> {
    let mut it = moniker.split('_');
    let name = it.next().filter(|s| !s.is_empty())?;
    let version = it.next().filter(|s| !s.is_empty())?;
    let arch = it.next().filter(|s| !s.is_empty())?;
    Some((name, version, arch))
}

/// 候选里挑本机架构、指定包名、版本最高的那一个。
///
/// FE3 回的顺序是任意的，取第一个会拿到旧版；按点分数字比而不是按字符串比
/// （`26.9` 会排在 `26.10` 后面）。
pub fn pick_candidate<'a>(cands: &'a [Candidate], arch: Arch, name: &str) -> Option<&'a Candidate> {
    cands
        .iter()
        .filter(|c| {
            moniker_parts(&c.moniker)
                .is_some_and(|(n, _, a)| n == name && a.eq_ignore_ascii_case(arch.moniker()))
        })
        .max_by_key(|c| {
            moniker_parts(&c.moniker)
                .map(|(_, v, _)| super::inventory::version_key(v))
                .unwrap_or_default()
        })
}

/// 清单里这个 moniker 对应的 `.msix` 文件，以及它里面带的 `<AdditionalDigest Algorithm="SHA256">`。
///
/// 实机（2026-09-20）的形状是 `<File FileName="<guid>.msix" Digest="…" DigestAlgorithm="SHA1"
/// InstallerSpecificIdentifier="OpenAI.Codex_26.915.4065.0_x64__2p2nqsd0c76g0">` ——
/// **文件名是个 GUID，moniker 在 `InstallerSpecificIdentifier` 里**；同一个更新里还躺着
/// arm64 那份和两个 `.cab` 差分包，所以既要按 moniker 对，也要按扩展名筛。
/// 老一点的清单把 moniker 直接当文件名，两种都认。**传进来的是已经解码过的文本。**
pub fn package_file_from_sync(xml: &str, moniker: &str) -> Option<PackageFile> {
    let mut rest = xml;
    while let Some(i) = rest.find("<File ") {
        let tag_start = i;
        let Some(tag_len) = rest[tag_start..].find('>') else {
            break;
        };
        let tag = &rest[tag_start..tag_start + tag_len];
        let after_tag = &rest[tag_start + tag_len + 1..];
        let name = attr(tag, "FileName").unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        let ident = attr(tag, "InstallerSpecificIdentifier").unwrap_or_default();
        let is_package =
            lower.ends_with(".msix") || lower.ends_with(".appx") || lower.ends_with(".msixbundle");
        let is_ours = is_package
            && (ident.eq_ignore_ascii_case(moniker)
                || lower.starts_with(&moniker.to_ascii_lowercase()));
        if is_ours {
            let digest_b64 = attr(tag, "Digest")?;
            // 只在这个 <File> 自己的范围里找 SHA-256，别拿到下一个文件的。
            let body_end = if tag.ends_with('/') {
                0
            } else {
                after_tag.find("</File>").unwrap_or(0)
            };
            let body = &after_tag[..body_end];
            let sha256 = body
                .find("<AdditionalDigest")
                .map(|j| &body[j..])
                .and_then(|t| {
                    let open_end = t.find('>')?;
                    let open = &t[..open_end];
                    if !attr(open, "Algorithm").is_some_and(|a| a.eq_ignore_ascii_case("SHA256")) {
                        return None;
                    }
                    let inner = &t[open_end + 1..];
                    let end = inner.find("</AdditionalDigest>")?;
                    base64_to_hex(inner[..end].trim())
                });
            return Some(PackageFile {
                name,
                digest_b64,
                sha256,
                size: attr(tag, "Size").and_then(|v| v.parse().ok()),
            });
        }
        rest = after_tag;
    }
    None
}

/// `GetExtendedUpdateInfo2` 回包里的 (文件 SHA-1 base64, 下载地址) 对。
pub fn file_locations(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<FileLocation>") {
        let start = i + "<FileLocation>".len();
        let Some(len) = rest[start..].find("</FileLocation>") else {
            break;
        };
        let block = &rest[start..start + len];
        rest = &rest[start + len..];
        let digest = element_text(block, "FileDigest")
            .unwrap_or_default()
            .to_string();
        if let Some(url) = element_text(block, "Url") {
            out.push((digest, decode_entities(url)));
        }
    }
    out
}

/// 只认微软的分发域。FE3 回什么就下什么等于跟着一个未知地址走。
pub fn allowed_host(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host
        .split('@')
        .next_back()
        .unwrap_or("")
        .to_ascii_lowercase();
    let host = host.split(':').next().unwrap_or("");
    host == "delivery.mp.microsoft.com" || host.ends_with(".delivery.mp.microsoft.com")
}

/// 挑下载地址：优先按文件摘要对上号的那条；对不上就退回「最长的那条」——
/// 一个更新会回好几条地址（blockmap 那条短），包本体那条最长（StoreLib 的经验）。
/// 两条路都只在允许的域里挑。
pub fn pick_url(locations: &[(String, String)], digest: Option<&str>) -> Option<String> {
    let allowed: Vec<&(String, String)> =
        locations.iter().filter(|(_, u)| allowed_host(u)).collect();
    if let Some(d) = digest {
        if let Some((_, u)) = allowed.iter().find(|(x, _)| x == d) {
            return Some(u.clone());
        }
    }
    allowed
        .iter()
        .max_by_key(|(_, u)| u.len())
        .map(|(_, u)| u.clone())
}

/// `Add-AppxPackage` 报「注册打包服务要管理员」（`0x80073D28`）—— 这时候要换提权那条路。
pub fn looks_like_admin_required(err: &str) -> bool {
    let e = err.to_ascii_uppercase();
    e.contains("0X80073D28") || e.contains("ADMINISTRATOR PRIVILEGES")
}

/// `winget install` 走 Store 源的参数。三个 flag 缺一不可（同 `winget::winget_args`）。
pub fn winget_msstore_args(product_id: &str) -> Vec<String> {
    [
        "install",
        "--id",
        product_id,
        "--source",
        "msstore",
        "-e",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// 标准 base64 → 小写十六进制。解不了回 `None`。
pub fn base64_to_hex(s: &str) -> Option<String> {
    base64_decode(s).map(hex::encode)
}

/// 标准 base64 解码。没有第三方依赖，几十行够用；遇到不认识的字符回 `None`。
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((acc >> bits) & 0xff) as u8);
        }
    }
    if bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

/// PEM 里第一张证书的 DER 字节（测试用来算指纹）。
#[cfg(test)]
fn pem_der(pem: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(pem).ok()?;
    let start = text.find("-----BEGIN CERTIFICATE-----")? + "-----BEGIN CERTIFICATE-----".len();
    let end = text[start..].find("-----END CERTIFICATE-----")? + start;
    base64_decode(&text[start..end])
}

fn timestamps() -> (String, String) {
    let now = chrono::Utc::now();
    (
        now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        (now + chrono::Duration::minutes(5))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    )
}

/// 三份信封的占位符全部换掉。纯函数，测试断言没有占位符漏下。
pub fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let (created, expires) = timestamps();
    let mut s = template
        .replace("{CREATED}", &created)
        .replace("{EXPIRES}", &expires);
    for (k, v) in pairs {
        s = s.replace(k, v);
    }
    s
}

// ---------------------------------------------------------------- 联网

fn http() -> Result<reqwest::Client> {
    let root = reqwest::Certificate::from_pem(MICROSOFT_ROOT_2011)
        .map_err(|e| GateError::Other(format!("内置的 Microsoft 根证书读不出来：{e}")))?;
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(WU_AGENT)
        // 见 MICROSOFT_ROOT_2011 的说明：只给这个客户端加，webpki 的根照旧保留。
        .add_root_certificate(root)
        .build()
        .map_err(|e| GateError::Other(format!("建不了网络客户端：{e}")))
}

async fn soap(c: &reqwest::Client, url: &str, body: String) -> Result<String> {
    c.post(url)
        .header("Content-Type", "application/soap+xml; charset=utf-8")
        .body(body)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| GateError::Other(format!("访问 {url} 失败：{e}")))?
        .text()
        .await
        .map_err(|e| GateError::Other(format!("读 {url} 的回包失败：{e}")))
}

/// DisplayCatalog → GetCookie → SyncUpdates，回（解码过的 SyncUpdates 报文、选中的候选）。
/// `resolve` 与 `locate` 共用；单独查版本的「检查」按钮走 `resolve`，不多跑 `/secured` 那一步。
async fn sync(c: &reqwest::Client, arch: Arch) -> Result<(String, Candidate)> {
    let catalog = c
        .get(format!(
            "{DISPLAY_CATALOG}/{PRODUCT_ID}?market=US&languages=en-US,en,neutral"
        ))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| GateError::Other(format!("查 Store 目录失败：{e}")))?
        .text()
        .await
        .map_err(|e| GateError::Other(format!("读 Store 目录回包失败：{e}")))?;
    let category = category_id_from_catalog(&catalog)?;

    let cookie_xml = soap(c, FE3, fill(GET_COOKIE, &[])).await?;
    let cookie = cookie_from_response(&cookie_xml).ok_or_else(|| {
        GateError::Other("FE3 没有回会话票据（GetCookie 里没有 EncryptedData）".into())
    })?;

    let sync_xml = soap(
        c,
        FE3,
        fill(
            SYNC_UPDATES,
            &[
                ("{COOKIE}", &cookie),
                ("{CATEGORY_ID}", &category),
                ("{MSA_TOKEN}", MSA_TOKEN),
                ("{ARCH}", arch.device()),
            ],
        ),
    )
    .await?;
    let decoded = decode_entities(&sync_xml);
    let cands = candidates_from_sync(&decoded);
    let best = pick_candidate(&cands, arch, PACKAGE_NAME)
        .ok_or_else(|| {
            GateError::Other(format!(
                "Store 回了 {} 个候选包，没有一个是 {} 的 {} 版",
                cands.len(),
                PACKAGE_NAME,
                arch.moniker()
            ))
        })?
        .clone();
    Ok((decoded, best))
}

fn resolved_of(decoded: &str, best: &Candidate) -> Result<(Resolved, Option<PackageFile>)> {
    let (_, version, _) = moniker_parts(&best.moniker)
        .ok_or_else(|| GateError::Other(format!("认不出这个包名的形状：{}", best.moniker)))?;
    let file = package_file_from_sync(decoded, &best.moniker);
    Ok((
        Resolved {
            moniker: best.moniker.clone(),
            version: version.to_string(),
            update_id: best.update_id.clone(),
            revision: best.revision.clone(),
            sha256: file.as_ref().and_then(|f| f.sha256.clone()),
            size: file.as_ref().and_then(|f| f.size),
        },
        file,
    ))
}

/// 调试用：解码过的 SyncUpdates 报文原样（`codex-store-resolve` 例子的 `dump` 模式）。
/// 页面改版时先拿它看清单长什么样，再改解析器。
pub async fn debug_sync_xml(arch: Arch) -> Result<String> {
    let c = http()?;
    Ok(sync(&c, arch).await?.0)
}

/// 查 Store 上现在是哪一版（不下载）。
pub async fn resolve(arch: Arch) -> Result<Resolved> {
    let c = http()?;
    let (decoded, best) = sync(&c, arch).await?;
    Ok(resolved_of(&decoded, &best)?.0)
}

/// 版本 + 签名过的下载地址。`GetExtendedUpdateInfo2` 走 `/secured`。
pub async fn locate(arch: Arch) -> Result<(Resolved, String)> {
    let c = http()?;
    let (decoded, best) = sync(&c, arch).await?;
    let (resolved, file) = resolved_of(&decoded, &best)?;
    let url_xml = soap(
        &c,
        FE3_SECURED,
        fill(
            FILE_URL,
            &[
                ("{UPDATE_ID}", &best.update_id),
                ("{REVISION}", &best.revision),
                ("{MSA_TOKEN}", MSA_TOKEN),
                ("{ARCH}", arch.device()),
            ],
        ),
    )
    .await?;
    let locations = file_locations(&url_xml);
    let url = pick_url(&locations, file.as_ref().map(|f| f.digest_b64.as_str())).ok_or_else(
        || {
            GateError::Other(format!(
                "FE3 回了 {} 条地址，没有一条在 delivery.mp.microsoft.com 底下，面板不跟着别的地址走",
                locations.len()
            ))
        },
    )?;
    Ok((resolved, url))
}

// ---------------------------------------------------------------- 安装

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// `winget install --source msstore`。
    Winget,
    /// FE3 直连下载 + `Add-AppxPackage`。
    Direct,
    /// 使用者自己给的 `.msix`。
    LocalFile,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Outcome {
    pub method: Method,
    /// 装完 `codex_desktop::detect()` 读回来的版本。读不出来就是 `None`（不编）。
    pub version: Option<String>,
    pub executable: Option<String>,
    pub log: Vec<String>,
}

/// `.msix` 落在托管目录下自己的子目录里，跟 Claude Code / Codex CLI 的 `.download` 一个形状。
pub fn download_dir() -> PathBuf {
    super::managed::root()
        .join("codex-desktop")
        .join(".download")
}

#[cfg(windows)]
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// `Add-AppxPackage`。先普通权限；撞上「打包服务要管理员」就走 UAC。
///
/// 返回的是**这一步做了什么**的一句人话，失败原因照 PowerShell 的原文报。
#[cfg(windows)]
pub async fn add_appx(msix: &Path, force: bool) -> Result<String> {
    let flags = if force {
        " -ForceApplicationShutdown -ForceUpdateFromAnyVersion"
    } else {
        ""
    };
    let script = format!(
        "$ErrorActionPreference = 'Stop'; Add-AppxPackage -Path {}{flags}",
        quote(&msix.display().to_string())
    );
    let out = crate::process::powershell_tokio(&script)
        .output()
        .await
        .map_err(|e| GateError::Other(format!("启动 PowerShell 失败：{e}")))?;
    if out.status.success() {
        return Ok("Add-AppxPackage 完成（普通权限）".into());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !looks_like_admin_required(&err) {
        return Err(GateError::Other(format!("Add-AppxPackage 失败：{err}")));
    }
    // 包里带打包服务，注册要管理员 —— 跟 codex_desktop::repair_registration 同一个坑、同一条路。
    let inner = format!(
        "try {{ Add-AppxPackage -Path {}{flags} -ErrorAction Stop; exit 0 }} catch {{ exit 1 }}",
        quote(&msix.display().to_string())
    );
    let elevated = format!(
        r#"
$ErrorActionPreference = 'Stop'
$inner = {inner}
try {{
  $p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-Command',$inner
}} catch {{
  Write-Output 'APPX=cancelled'
  exit 0
}}
Write-Output ('APPX=' + $p.ExitCode)
"#,
        inner = quote(&inner)
    );
    let out = crate::process::powershell_tokio(&elevated)
        .output()
        .await
        .map_err(|e| GateError::Other(format!("启动提权 PowerShell 失败：{e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("APPX=0") {
        return Ok("Add-AppxPackage 完成（包里带打包服务，经 UAC 以管理员注册）".into());
    }
    if stdout.contains("APPX=cancelled") {
        return Err(GateError::Other(
            "已取消管理员授权，没有安装。这个包里带打包服务，注册它必须管理员（0x80073D28）。"
                .into(),
        ));
    }
    Err(GateError::Other(format!(
        "以管理员运行 Add-AppxPackage 也失败了。普通权限那次的原因：{err}"
    )))
}

#[cfg(not(windows))]
pub async fn add_appx(_msix: &Path, _force: bool) -> Result<String> {
    Err(GateError::Other("只在 Windows 上可用".into()))
}

/// 签名主体必须含 OpenAI。三态：对 / 不对 / 读不出 —— 后两种都不装，但报的话不一样。
async fn require_openai_signature(msix: &Path, log: &mut Vec<String>) -> Result<()> {
    let signer = crate::signature::signer_of(msix).await;
    match super::winget::signature_matches(signer.as_deref(), "openai") {
        Some(true) => {
            log.push(format!("签名主体：{}", signer.unwrap_or_default()));
            Ok(())
        }
        Some(false) => Err(GateError::Other(format!(
            "拒绝安装：包的签名主体里没有 OpenAI（读到的是 {}）。",
            signer.unwrap_or_default()
        ))),
        None => Err(GateError::Other(
            "拒绝安装：读不出包的数字签名 —— 这只能说「没验成」，面板不会把没验成的包交给系统注册。"
                .into(),
        )),
    }
}

/// 下载 + 核对 + 安装。`.msix` 放在 [`download_dir`]，装成就删，没装成留着给使用者手动装。
pub async fn install_direct(
    arch: Arch,
    force: bool,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<()> {
    rep.phase(2, "查 Store 上的最新版本（FE3 直连）");
    let (resolved, url) = locate(arch).await?;
    log.push(format!(
        "Store 最新版 {}（{}）",
        resolved.version, resolved.moniker
    ));
    rep.log(2, &format!("Store 最新版 {}", resolved.version));

    let dir = download_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let msix = dir.join(format!("{}.msix", resolved.moniker));
    rep.phase(4, &format!("下载 {}.msix", resolved.moniker));
    // 地址照 FE3 给的用（是 `http://`）：分发 CDN 不给这个主机名配证书（实测 https 是
    // Fastly 的兜底证书、主体对不上），Windows Update 自己也是明文下、靠哈希保完整 ——
    // 这里同样：SHA-256 是从 FE3（TLS）拿的清单里来的，下面还要核 Authenticode。
    let got = super::managed::download(&url, &msix, resolved.size, rep, 4).await?;

    rep.phase(5, "核对 SHA-256 与数字签名");
    match &resolved.sha256 {
        Some(want) if want.eq_ignore_ascii_case(&got) => {
            log.push(format!("SHA-256 与清单一致：{got}"))
        }
        Some(want) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(GateError::Other(format!(
                "下载下来的包跟清单里的 SHA-256 对不上（应为 {want}，实际 {got}），已删除，没有装。"
            )));
        }
        None => log.push(format!(
            "清单里没给 SHA-256（实际 {got}），只能靠数字签名核对"
        )),
    }
    if let Err(e) = require_openai_signature(&msix, log).await {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }

    rep.phase(6, "Add-AppxPackage 注册到系统");
    match add_appx(&msix, force).await {
        Ok(line) => {
            log.push(line);
            let _ = std::fs::remove_dir_all(&dir);
            Ok(())
        }
        Err(e) => Err(GateError::Other(format!(
            "{e} 已核对过的包留在 {}，可以以管理员运行 PowerShell 手动执行：Add-AppxPackage -Path \"{}\"",
            msix.display(),
            msix.display()
        ))),
    }
}

/// 使用者自己给的 `.msix`：只核签名（清单里的 SHA-256 拿不到），然后注册。
pub async fn install_local(
    msix: &Path,
    force: bool,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<()> {
    if !msix.is_file() {
        return Err(GateError::Other(format!("找不到 {}", msix.display())));
    }
    rep.phase(5, "核对数字签名");
    require_openai_signature(msix, log).await?;
    rep.phase(6, "Add-AppxPackage 注册到系统");
    log.push(add_appx(msix, force).await?);
    Ok(())
}

/// 装（或更新）Codex 桌面端。**调用方先关掉正在跑的桌面端、拿到使用者确认。**
///
/// 顺序：盘点现状 → 查最新版 → A 路 winget（起得来才算试过）→ 没成走 B 路直连 →
/// 装完用 `codex_desktop::detect()` 回读核对，**不看命令的退出码**。
/// `local` 给了就只装那一份（跳过 A / B）。
pub async fn install(local: Option<&Path>, force: bool, rep: &dyn ProgressSink) -> Result<Outcome> {
    let mut log = Vec::new();
    let arch = Arch::current();

    rep.phase(1, "盘点现状");
    let before = super::codex_desktop::detect().ok();
    match before.as_ref().and_then(|d| d.version.clone()) {
        Some(v) => log.push(format!("已装 {v}")),
        None => log.push("本机没有 Codex 桌面端（Store 包）".into()),
    }
    log.push(format!("本机架构 {}", arch.moniker()));

    let method = if let Some(p) = local {
        install_local(p, force, rep, &mut log).await?;
        Method::LocalFile
    } else {
        // 已经是最新且不是强制重装，就别把使用者的时间花在一次注定失败的注册上。
        rep.phase(2, "查 Store 上的最新版本");
        match resolve(arch).await {
            Ok(r) => {
                log.push(format!("Store 最新版 {}", r.version));
                rep.log(2, &format!("Store 最新版 {}", r.version));
                if !force {
                    if let Some(have) = before.as_ref().and_then(|d| d.version.as_deref()) {
                        if super::inventory::version_key(have)
                            >= super::inventory::version_key(&r.version)
                        {
                            return Err(GateError::Other(format!(
                                "本机的 {have} 已经不比 Store 上的 {} 旧，没有装。要重装（修复注册）就选「强制重装」。",
                                r.version
                            )));
                        }
                    }
                }
            }
            Err(e) => {
                log.push(format!("查最新版没成（{e}），先试 winget"));
                rep.log(2, "查最新版没成，先试 winget");
            }
        }

        rep.phase(3, "winget 从 Store 源安装");
        let winget_ok = if force {
            log.push("强制重装：winget 不支持同版本重装，直接走直连".into());
            false
        } else {
            match super::winget::run_streaming(
                "winget",
                &winget_msstore_args(PRODUCT_ID),
                rep,
                3,
                &mut log,
            )
            .await
            {
                Ok(ok) => ok,
                Err(e) => {
                    log.push(format!("winget 起不来（{e}）"));
                    false
                }
            }
        };
        if winget_ok {
            Method::Winget
        } else {
            log.push("winget 那条没成，改走 FE3 直连下载 + Add-AppxPackage".into());
            install_direct(arch, force, rep, &mut log).await?;
            Method::Direct
        }
    };

    rep.phase(7, "回读核对");
    let after = super::codex_desktop::detect()?;
    let Some(exe) = after.executable.clone() else {
        return Err(GateError::Other(
            "命令报了完成，但 Get-AppxPackage 里仍然找不到 OpenAI.Codex —— 没有装上。".into(),
        ));
    };
    if let (Some(a), Some(b)) = (
        before.as_ref().and_then(|d| d.version.as_deref()),
        after.version.as_deref(),
    ) {
        if a == b && !force && method != Method::LocalFile {
            return Err(GateError::Other(format!(
                "命令报了完成，但装的还是原来那版 {a} —— 没有换成新版。"
            )));
        }
    }
    log.push(format!(
        "装好了：{}（{}）",
        after.version.clone().unwrap_or_else(|| "版本读不出".into()),
        exe
    ));
    Ok(Outcome {
        method,
        version: after.version,
        executable: Some(exe),
        log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNC: &str = r#"<s:Envelope><s:Body><SyncUpdatesResponse><SyncUpdatesResult><NewUpdates>
<UpdateInfo><ID>1</ID><Xml>&lt;UpdateIdentity UpdateID="aaaa-old" RevisionNumber="3"/&gt;&lt;Properties&gt;&lt;SecuredFragment/&gt;&lt;/Properties&gt;&lt;Relationships&gt;&lt;Prerequisites&gt;&lt;UpdateIdentity UpdateID="dep-1"/&gt;&lt;/Prerequisites&gt;&lt;/Relationships&gt;&lt;ApplicabilityRules&gt;&lt;Metadata&gt;&lt;AppxPackageMetadata&gt;&lt;AppxMetadata IsAppxBundle="false" PackageMoniker="OpenAI.Codex_26.623.13972.0_x64__2p2nqsd0c76g0"/&gt;&lt;/AppxPackageMetadata&gt;&lt;/Metadata&gt;&lt;/ApplicabilityRules&gt;</Xml></UpdateInfo>
<UpdateInfo><ID>2</ID><Xml>&lt;UpdateIdentity RevisionNumber="1" UpdateID="bbbb-new"/&gt;&lt;Properties&gt;&lt;SecuredFragment/&gt;&lt;/Properties&gt;&lt;ApplicabilityRules&gt;&lt;Metadata&gt;&lt;AppxPackageMetadata&gt;&lt;AppxMetadata PackageMoniker="OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0"/&gt;&lt;/AppxPackageMetadata&gt;&lt;/Metadata&gt;&lt;/ApplicabilityRules&gt;</Xml></UpdateInfo>
<UpdateInfo><ID>3</ID><Xml>&lt;UpdateIdentity UpdateID="cccc-arm" RevisionNumber="1"/&gt;&lt;Properties&gt;&lt;SecuredFragment/&gt;&lt;/Properties&gt;&lt;ApplicabilityRules&gt;&lt;Metadata&gt;&lt;AppxPackageMetadata&gt;&lt;AppxMetadata PackageMoniker="OpenAI.Codex_26.707.3748.0_arm64__2p2nqsd0c76g0"/&gt;&lt;/AppxPackageMetadata&gt;&lt;/Metadata&gt;&lt;/ApplicabilityRules&gt;</Xml></UpdateInfo>
<UpdateInfo><ID>4</ID><Xml>&lt;UpdateIdentity UpdateID="dddd-nonleaf" RevisionNumber="1"/&gt;&lt;ApplicabilityRules&gt;&lt;Metadata&gt;&lt;AppxPackageMetadata&gt;&lt;AppxMetadata PackageMoniker="OpenAI.Codex_99.0.0.0_x64__2p2nqsd0c76g0"/&gt;&lt;/AppxPackageMetadata&gt;&lt;/Metadata&gt;&lt;/ApplicabilityRules&gt;</Xml></UpdateInfo>
<UpdateInfo><ID>5</ID><Xml>&lt;UpdateIdentity UpdateID="eeee-other" RevisionNumber="1"/&gt;&lt;Properties&gt;&lt;SecuredFragment/&gt;&lt;/Properties&gt;&lt;ApplicabilityRules&gt;&lt;Metadata&gt;&lt;AppxPackageMetadata&gt;&lt;AppxMetadata PackageMoniker="Contoso.Other_99.0.0.0_x64__abcdefabcdef"/&gt;&lt;/AppxPackageMetadata&gt;&lt;/Metadata&gt;&lt;/ApplicabilityRules&gt;</Xml></UpdateInfo>
</NewUpdates><ExtendedUpdateInfo><Updates><Update><ID>2</ID><Xml>&lt;ExtendedProperties PackageIdentityName="OpenAI.Codex"/&gt;&lt;Files&gt;&lt;File FileName="Abm_d0ba78cc.cab" Digest="Y2Fi" DigestAlgorithm="SHA1" Size="9" PatchingType="DynamicMetadata"&gt;&lt;AdditionalDigest Algorithm="SHA256"&gt;/////////////////////////////////////////w==&lt;/AdditionalDigest&gt;&lt;/File&gt;&lt;File FileName="bfebc401.msix" Digest="YXJt" DigestAlgorithm="SHA1" Size="5" InstallerSpecificIdentifier="OpenAI.Codex_26.707.3748.0_arm64__2p2nqsd0c76g0"&gt;&lt;AdditionalDigest Algorithm="SHA256"&gt;ERERERERERERERERERERERERERERERERERERERERERE=&lt;/AdditionalDigest&gt;&lt;/File&gt;&lt;File FileName="d0ba78cc.msix" Digest="c2hhMS1kaWdlc3Q=" DigestAlgorithm="SHA1" Size="123" InstallerSpecificIdentifier="OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0"&gt;&lt;AdditionalDigest Algorithm="SHA256"&gt;AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=&lt;/AdditionalDigest&gt;&lt;/File&gt;&lt;File FileName="OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0.BlockMap" Digest="Ymxv" DigestAlgorithm="SHA1" Size="9"/&gt;&lt;/Files&gt;</Xml></Update></Updates></ExtendedUpdateInfo>
</SyncUpdatesResult></SyncUpdatesResponse></s:Body></s:Envelope>"#;

    /// 五个块：旧版、新版、arm64、没有 SecuredFragment 的非叶子、别人的包。
    /// 只有带 SecuredFragment 的是候选；第二个的属性顺序是反的也要认；
    /// 前置依赖的 UpdateIdentity 不能顶掉自己的。
    #[test]
    fn sync_updates_yields_only_downloadable_leaves_with_their_own_identity() {
        let c = candidates_from_sync(&decode_entities(SYNC));
        let ids: Vec<&str> = c.iter().map(|x| x.update_id.as_str()).collect();
        assert_eq!(ids, vec!["aaaa-old", "bbbb-new", "cccc-arm", "eeee-other"]);
        assert_eq!(c[0].revision, "3");
        assert_eq!(c[1].revision, "1");
        assert!(c[1].moniker.contains("26.707.3748.0"));
    }

    /// 本机架构 + 本包名 + 最高版本。FE3 的顺序是任意的，旧版排在前面不能赢。
    #[test]
    fn the_newest_package_for_this_arch_wins_regardless_of_order() {
        let c = candidates_from_sync(&decode_entities(SYNC));
        let best = pick_candidate(&c, Arch::X64, "OpenAI.Codex").unwrap();
        assert_eq!(best.update_id, "bbbb-new");
        let arm = pick_candidate(&c, Arch::Arm64, "OpenAI.Codex").unwrap();
        assert_eq!(arm.update_id, "cccc-arm");
        assert!(pick_candidate(&c, Arch::X64, "Nobody.Else").is_none());
        // 别人的包版本号再高也不算。
        assert_ne!(best.update_id, "eeee-other");
    }

    #[test]
    fn version_comparison_is_numeric_not_lexical() {
        let c = vec![
            Candidate {
                moniker: "OpenAI.Codex_26.9.0.0_x64__h".into(),
                update_id: "nine".into(),
                revision: "1".into(),
            },
            Candidate {
                moniker: "OpenAI.Codex_26.10.0.0_x64__h".into(),
                update_id: "ten".into(),
                revision: "1".into(),
            },
        ];
        assert_eq!(
            pick_candidate(&c, Arch::X64, "OpenAI.Codex")
                .unwrap()
                .update_id,
            "ten"
        );
    }

    /// 清单里找得到这个 moniker 的 `.msix`（按 `InstallerSpecificIdentifier` 对，文件名是 GUID）、
    /// 它的 SHA-1 摘要，以及 SHA-256（base64 → 十六进制）。同一更新里的 `.cab` 差分包、
    /// arm64 那份、BlockMap 都不能拿错。
    #[test]
    fn the_package_file_and_its_digests_come_from_the_extended_info() {
        let decoded = decode_entities(SYNC);
        let f = package_file_from_sync(&decoded, "OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0")
            .unwrap();
        assert_eq!(f.name, "d0ba78cc.msix");
        assert_eq!(f.digest_b64, "c2hhMS1kaWdlc3Q=");
        assert_eq!(f.size, Some(123));
        assert_eq!(
            f.sha256.as_deref(),
            Some("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
        );
        let arm =
            package_file_from_sync(&decoded, "OpenAI.Codex_26.707.3748.0_arm64__2p2nqsd0c76g0")
                .unwrap();
        assert_eq!(arm.name, "bfebc401.msix");
        assert!(package_file_from_sync(&decoded, "OpenAI.Codex_1.0.0.0_x64__x").is_none());
        // 老形状：moniker 直接当文件名，没有 InstallerSpecificIdentifier。
        let legacy = r#"<Files><File FileName="OpenAI.Codex_1.2.3.0_x64__h.msix" Digest="bGVn" DigestAlgorithm="SHA1"/></Files>"#;
        let l = package_file_from_sync(legacy, "OpenAI.Codex_1.2.3.0_x64__h").unwrap();
        assert_eq!(l.digest_b64, "bGVn");
        assert!(l.sha256.is_none(), "自闭合的 File 没有 SHA-256");
    }

    const URLS: &str = r#"<GetExtendedUpdateInfo2Response><GetExtendedUpdateInfo2Result><FileLocations>
<FileLocation><FileDigest>Ymxv</FileDigest><Url>http://tlu.dl.delivery.mp.microsoft.com/filestreamingservice/files/short?P1=1&amp;P2=2</Url></FileLocation>
<FileLocation><FileDigest>c2hhMS1kaWdlc3Q=</FileDigest><Url>http://tlu.dl.delivery.mp.microsoft.com/filestreamingservice/files/main-package?P1=1&amp;P2=2&amp;P3=3&amp;P4=sig%3d%3d</Url></FileLocation>
<FileLocation><FileDigest>ZXZpbA==</FileDigest><Url>https://evil.example.com/very/very/very/very/very/very/very/very/very/long/path/that/is/longest</Url></FileLocation>
</FileLocations></GetExtendedUpdateInfo2Result></GetExtendedUpdateInfo2Response>"#;

    /// 按摘要对号入座；`&amp;` 要还原；不在微软分发域的地址再长也不要。
    #[test]
    fn the_download_url_is_matched_by_digest_and_confined_to_microsoft() {
        let locs = file_locations(URLS);
        assert_eq!(locs.len(), 3);
        let u = pick_url(&locs, Some("c2hhMS1kaWdlc3Q=")).unwrap();
        assert!(
            u.starts_with("http://tlu.dl.delivery.mp.microsoft.com/"),
            "{u}"
        );
        assert!(u.contains("&P3=3&P4=sig%3d%3d"), "实体没还原：{u}");
        // 没有摘要时退回最长的那条 —— 但只在允许的域里。
        let fallback = pick_url(&locs, None).unwrap();
        assert!(fallback.contains("main-package"), "{fallback}");
        assert!(pick_url(&[("x".into(), "https://evil.example.com/a".into())], None).is_none());
    }

    #[test]
    fn allowed_hosts_are_microsofts_delivery_domain_only() {
        assert!(allowed_host(
            "http://tlu.dl.delivery.mp.microsoft.com/files/x"
        ));
        assert!(allowed_host("https://dl.delivery.mp.microsoft.com/x"));
        assert!(allowed_host("https://delivery.mp.microsoft.com/x?y=1"));
        assert!(!allowed_host(
            "https://delivery.mp.microsoft.com.evil.com/x"
        ));
        assert!(!allowed_host("https://evildelivery.mp.microsoft.com/x"));
        assert!(!allowed_host("ftp://dl.delivery.mp.microsoft.com/x"));
        assert!(!allowed_host(
            "https://user@evil.com/dl.delivery.mp.microsoft.com"
        ));
    }

    /// `FulfillmentData` 两种形状（对象 / JSON 字符串）都要认得出 `WuCategoryId`。
    #[test]
    fn the_category_id_is_read_from_either_fulfillment_shape() {
        let as_string = r#"{"Product":{"DisplaySkuAvailabilities":[{"Sku":{"Properties":{"FulfillmentData":"{\"WuCategoryId\":\"cat-1\",\"PackageFamilyName\":\"OpenAI.Codex_2p2nqsd0c76g0\"}"}}}]}}"#;
        assert_eq!(category_id_from_catalog(as_string).unwrap(), "cat-1");
        let as_object = r#"{"Product":{"DisplaySkuAvailabilities":[{"Sku":{"Properties":{"FulfillmentData":{"WuCategoryId":"cat-2"}}}}]}}"#;
        assert_eq!(category_id_from_catalog(as_object).unwrap(), "cat-2");
        assert!(category_id_from_catalog(r#"{"Product":{}}"#).is_err());
        assert!(category_id_from_catalog("not json").is_err());
    }

    #[test]
    fn the_cookie_is_the_encrypted_data_element() {
        let xml = "<s:Envelope><s:Body><GetCookieResponse><GetCookieResult><Expiration>2045-01-01T00:00:00Z</Expiration><EncryptedData>AbC+/=</EncryptedData></GetCookieResult></GetCookieResponse></s:Body></s:Envelope>";
        assert_eq!(cookie_from_response(xml).as_deref(), Some("AbC+/="));
        assert!(cookie_from_response("<x><EncryptedData></EncryptedData></x>").is_none());
    }

    #[test]
    fn moniker_parts_are_name_version_arch() {
        assert_eq!(
            moniker_parts("OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0"),
            Some(("OpenAI.Codex", "26.707.3748.0", "x64"))
        );
        assert!(moniker_parts("nounderscore").is_none());
        assert!(moniker_parts("a__b").is_none());
    }

    #[test]
    fn base64_decodes_to_lowercase_hex() {
        assert_eq!(base64_to_hex("AAECAw==").as_deref(), Some("00010203"));
        assert_eq!(base64_to_hex("/w==").as_deref(), Some("ff"));
        assert!(base64_to_hex("!!").is_none());
        assert!(base64_to_hex("").is_none());
    }

    /// 内置的那份根证书必须就是 Microsoft Root Certificate Authority 2011 —— 换成别的
    /// 任何一张，FE3 那条 TLS 要么通不了、要么信了不该信的东西。指纹是从微软 PKI 站与
    /// 本机系统证书库两处对出来的。
    #[test]
    fn the_embedded_root_is_microsofts_2011_root_by_fingerprint() {
        use sha2::{Digest, Sha256};
        let der = pem_der(MICROSOFT_ROOT_2011).expect("PEM 解不出 DER");
        assert_eq!(
            hex::encode(Sha256::digest(&der)),
            MICROSOFT_ROOT_2011_SHA256
        );
        assert!(reqwest::Certificate::from_pem(MICROSOFT_ROOT_2011).is_ok());
    }

    /// 三份信封的占位符必须全被换掉 —— 漏一个 FE3 会静默回一个空清单。
    #[test]
    fn every_placeholder_in_every_envelope_is_filled() {
        let cookie = fill(GET_COOKIE, &[]);
        let sync = fill(
            SYNC_UPDATES,
            &[
                ("{COOKIE}", "c"),
                ("{CATEGORY_ID}", "k"),
                ("{MSA_TOKEN}", "t"),
                ("{ARCH}", "AMD64"),
            ],
        );
        let url = fill(
            FILE_URL,
            &[
                ("{UPDATE_ID}", "u"),
                ("{REVISION}", "1"),
                ("{MSA_TOKEN}", "t"),
                ("{ARCH}", "ARM64"),
            ],
        );
        for (name, body) in [
            ("get-cookie", &cookie),
            ("sync-updates", &sync),
            ("file-url", &url),
        ] {
            assert!(
                !body.contains('{'),
                "{name} 里还有占位符没换：{}",
                body.lines().find(|l| l.contains('{')).unwrap_or("")
            );
        }
        assert!(sync.contains("OSArchitecture=AMD64;"));
        assert!(url.contains("OSArchitecture=ARM64;"));
        assert!(sync.contains("<Id>k</Id>"));
        assert!(url.contains("<UpdateID>u</UpdateID>"));
        // 票据不是空的：空票据的匿名会话拿不到 /secured 的地址。
        assert!(MSA_TOKEN.starts_with("<Device>") && MSA_TOKEN.len() > 500);
    }

    #[test]
    fn admin_required_is_recognised_from_the_hresult() {
        assert!(looks_like_admin_required(
            "Add-AppxPackage : 部署失败，HRESULT: 0x80073D28, Administrator privileges are required"
        ));
        assert!(!looks_like_admin_required(
            "0x80073CF3 dependency not found"
        ));
    }

    #[test]
    fn winget_store_args_have_every_non_interactive_flag() {
        let a = winget_msstore_args("9PLM9XGG6VKS");
        for want in [
            "--source",
            "msstore",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
            "9PLM9XGG6VKS",
        ] {
            assert!(a.iter().any(|x| x == want), "缺 {want}：{a:?}");
        }
    }

    #[test]
    fn arch_names_match_store_and_wu_conventions() {
        assert_eq!(Arch::X64.moniker(), "x64");
        assert_eq!(Arch::Arm64.moniker(), "arm64");
        assert_eq!(Arch::X64.device(), "AMD64");
        assert_eq!(Arch::Arm64.device(), "ARM64");
    }
}
