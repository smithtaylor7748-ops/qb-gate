//! 连接桥鉴权用的纯函数：扩展来源、随机口令、常量时间比对、HMAC 证明。
//!
//! # 为什么长期口令不再上线路
//!
//! 配对现在长期有效、端口落盘复用。面板没开着的时候，那个端口可能被**别的程序**拿去监听
//! （系统挑的端口本来就在动态端口段里）—— 插件还照着记下的端口去连。如果连上就把长期口令
//! 发过去，等于把口令交给了不相干的程序；对方也可以回一句「可以放行」。
//!
//! 所以长期口令一辈子只在线路上出现一次（首次配对的回包里），之后双方都只发
//! `HMAC-SHA256(口令, 角色 + 双方随机数)`：桥先证明自己认得口令，插件再证明自己。
//! 证明过之后桥发一个只在这条连接活着时有效的会话口令，HTTP 请求带的是它。

use sha2::{Digest, Sha256};

/// 256 位随机数的 64 位十六进制。
///
/// UUID v4 每个带 122 位随机数；去掉固定的版本 / 变体那两个半字节，取三个拼出 64 个字符，
/// 不为此新引随机数依赖。
pub(super) fn token() -> String {
    (0..3)
        .flat_map(|_| {
            uuid::Uuid::new_v4()
                .simple()
                .to_string()
                .chars()
                .enumerate()
                .filter_map(|(i, c)| (!matches!(i, 12 | 16)).then_some(c))
                .collect::<Vec<_>>()
        })
        .take(64)
        .collect()
}

/// 配对的公开编号（不是秘密）：16 位十六进制。
pub(super) fn pairing_id() -> String {
    token().chars().take(16).collect()
}

pub(super) fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `chrome-extension://<32 个 a–p>`，多一个字符都不认。
pub(super) fn extension_id(origin: &str) -> Option<&str> {
    let id = origin.strip_prefix("chrome-extension://")?;
    valid_extension_id(id).then_some(id)
}

pub(super) fn valid_extension_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// 常量时间比对：不因为前缀对上几位而早退。
pub(super) fn token_matches(expected: &str, supplied: &str) -> bool {
    if supplied.len() != expected.len() {
        return false;
    }
    expected
        .bytes()
        .zip(supplied.bytes())
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}

/// HMAC-SHA256（RFC 2104）。`sha2` 早就在依赖里，为这二十行不另引 `hmac`。
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// 一次证明：`hex(HMAC-SHA256(key = 口令的 ASCII 字节, "qb-link/2 <角色>\n<a>\n<b>"))`。
///
/// 角色把四种用途隔开，一种的证明换不成另一种：
///
/// | 角色 | 谁出 | a | b |
/// |---|---|---|---|
/// | `server` | 桥（WebSocket 的 challenge） | 插件随机数 | 桥随机数 |
/// | `client` | 插件（WebSocket 的 authenticate） | 插件随机数 | 桥随机数 |
/// | `claim` | 插件（认领时证明自己是已配对的那个） | 配对编号 | 认领码 |
/// | `claimed` | 桥（认领回包里证明自己认得口令） | 配对编号 | 认领码 |
pub fn proof(token: &str, role: &str, a: &str, b: &str) -> String {
    hex::encode(hmac_sha256(
        token.as_bytes(),
        format!("qb-link/2 {role}\n{a}\n{b}").as_bytes(),
    ))
}

/// 浏览器叫什么：只认 User-Agent 里的 Edge / Chrome 主版本，别的都叫「Chromium 内核浏览器」。
pub(super) fn browser_label(user_agent: Option<&str>) -> String {
    let ua = user_agent.unwrap_or_default();
    let major = |marker: &str| -> Option<String> {
        let rest = &ua[ua.find(marker)? + marker.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        (!digits.is_empty() && digits.len() <= 4).then_some(digits)
    };
    if let Some(v) = major("Edg/") {
        format!("Edge {v}")
    } else if let Some(v) = major("Chrome/") {
        format!("Chrome {v}")
    } else {
        "Chromium 内核浏览器".into()
    }
}

/// 插件自己报上来的短文字（名字、版本）：去掉控制字符，截到 `max` 个字符。
pub(super) fn short_text(value: Option<&str>, max: usize) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}
