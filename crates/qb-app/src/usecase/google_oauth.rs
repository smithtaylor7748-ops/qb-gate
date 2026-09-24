//! Google 登录令牌过期时在内存里换新 —— 反重力与 Gemini CLI 两条联网额度共用（2026-09-23）。
//!
//! # 从哪来
//!
//! 原来这些都长在 `antigravity_quota` 里。09-23 同一天使用者又说「GPT 和 Gemini 的登录态
//! 对着那个开源项目（cockpit-tools）修一下」：Gemini CLI 的访问令牌也只有一小时，
//! 面板拿着过期的那张去问额度，Google 回 401，界面就说「登录令牌已失效，请重新登录」——
//! 而那个账户登得好好的，CLI 下次跑起来会自己换新。cockpit-tools 在这种时候换一张新的再问；
//! 这里照同一个做法，跟反重力走同一套代码。
//!
//! # ⛔ 只对 Google 成立，别搬去 GPT
//!
//! Google 的刷新令牌换新之后**不作废**，面板拿它换一张访问令牌放在内存里，官方客户端手里那份
//! 照样能用 —— 所以「换了不写回」是安全的。OpenAI 的刷新令牌**每换一次就轮换**：面板替 Codex
//! 换一次，槽位 `auth.json` 里那份立刻作废，桌面端下次换新就被登出、弹出登录页。
//! GPT 那条（`tavern_quota::fetch_gpt`）因此只看本机时钟、过期就如实说，**不换**。
//!
//! # 硬规矩（CLAUDE.md「联网额度」第 3–5 条）
//!
//! 1. **只放内存，不写回。** 换来的访问令牌在调用方的 [`TokenCache`] 里，按刷新令牌的
//!    [`fingerprint`] 认 —— 同一个槽位里换了个人登录，旧的那张立刻不算数。
//! 2. **客户端标识不进仓库。** 换新要带签发那张令牌的客户端的 id / secret：要换的那一刻从
//!    本机装的官方客户端里现读（[`LocalClients::files`]，由位置表给），按出现位置配对，
//!    Google 回 `invalid_client` / `unauthorized_client` 就换下一组。认准的只记
//!    「哪个文件的第几组」（[`LocalClients::working`]），标识用完即丢。
//! 3. **令牌不落任何地方。** 全程装在 [`Secret`] 里；审计只写「换新了一次」。

use crate::error::GateError;
use qb_platform::credentials::Secret;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// 访问令牌离过期不到这么久就换新 —— 发出去的路上过期跟本来就过期一样是 401。
pub const TOKEN_MARGIN_SECS: i64 = 300;
/// 换新时最多试几组客户端标识。试错一组 Google 只回一句 `invalid_client`，没有副作用。
const MAX_CLIENT_TRIES: usize = 6;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

// ------------------------------------------------------------------ 内存里的令牌

/// 一张换新来的访问令牌：`(刷新令牌指纹, 访问令牌, 过期秒)`。
type FreshToken = (String, Secret, i64);

/// 换新来的访问令牌，按调用方给的键存。**只在内存里。**
pub struct TokenCache(Mutex<Option<HashMap<String, FreshToken>>>);

impl Default for TokenCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCache {
    /// `const`：调用方拿它当 `static`（每个产品一份）。
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// 还能用的那张（指纹对得上、离过期还有余量）。
    pub fn get(&self, key: &str, print: &str, now: i64) -> Option<Secret> {
        let g = self.0.lock().ok()?;
        let (p, access, expires) = g.as_ref()?.get(key)?;
        (p == print && expires - TOKEN_MARGIN_SECS > now)
            .then(|| Secret::new(access.as_bytes().to_vec()))
    }

    pub fn put(&self, key: &str, print: &str, access: &Secret, expires: i64) {
        if let Ok(mut g) = self.0.lock() {
            g.get_or_insert_with(HashMap::new).insert(
                key.to_string(),
                (
                    print.to_string(),
                    Secret::new(access.as_bytes().to_vec()),
                    expires,
                ),
            );
        }
    }

    pub fn forget(&self, key: &str) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(m) = g.as_mut() {
                m.remove(key);
            }
        }
    }
}

/// 刷新令牌的指纹：同一个槽位里换了账户登录，旧的那张换新来的访问令牌立刻不算数。
pub fn fingerprint(refresh: Option<&Secret>) -> String {
    use sha2::{Digest, Sha256};
    match refresh {
        Some(r) => hex::encode(&Sha256::digest(r.as_bytes())[..8]),
        None => String::new(),
    }
}

// ------------------------------------------------------------------ 换新

/// 去哪找客户端标识、认准的那一组记在哪、审计里怎么称呼。
pub struct LocalClients {
    /// 位置表给的、本机存在的文件，按优先级排。
    pub files: Vec<PathBuf>,
    /// 上次认准的是 `(文件, 第几组)`。每个产品一份。**不存标识本身。**
    pub working: &'static Mutex<Option<(PathBuf, usize)>>,
    /// 「反重力」「Gemini CLI」。
    pub product: &'static str,
}

/// 换新那一趟的结局。调用方按它说一句能照着做的话。
pub enum Refreshed {
    /// 换到了：`(访问令牌, 过期秒)`。
    Fresh(Secret, i64),
    /// Google 说刷新令牌作废了（`invalid_grant`）：这个账户要回官方客户端重新登录。
    Revoked,
    /// 本机没找到客户端标识（官方客户端没装，或者装的位置位置表不认识）。
    NoClient,
    /// 标识都试过了，Google 一个都不认（`invalid_client`）—— 多半是官方客户端更新换了标识。
    ClientRejected,
    /// 别的（连不上、Google 回了别的错）：一句人话。
    Failed(String),
}

/// 那一次往返的结局。
pub enum RefreshOutcome {
    /// 换到了：`(访问令牌, 过期秒)`。
    Fresh(Secret, i64),
    /// 这组客户端标识不是签发这个令牌的那一组，换下一组。
    WrongClient,
    /// Google 说刷新令牌作废了（`invalid_grant`）。
    Revoked,
    /// 别的：一句人话。
    Other(String),
}

/// 纯函数：`oauth2.googleapis.com/token` 的回复 → 结局。
pub fn parse_token_reply(status: u16, body: &[u8], now: i64) -> RefreshOutcome {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return RefreshOutcome::Other(format!("HTTP {status}，回复不是 JSON"));
    };
    if (200..300).contains(&status) {
        let Some(access) = v
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return RefreshOutcome::Other("回复里没有 access_token".into());
        };
        let expires_in = v
            .get("expires_in")
            .and_then(Value::as_i64)
            .filter(|s| *s > 0)
            .unwrap_or(3600);
        return RefreshOutcome::Fresh(Secret::new(access.as_bytes().to_vec()), now + expires_in);
    }
    match v.get("error").and_then(Value::as_str).unwrap_or("") {
        "invalid_client" | "unauthorized_client" => RefreshOutcome::WrongClient,
        "invalid_grant" => RefreshOutcome::Revoked,
        "" => RefreshOutcome::Other(format!("HTTP {status}")),
        other => RefreshOutcome::Other(format!(
            "HTTP {status}，{}",
            other.chars().take(40).collect::<String>()
        )),
    }
}

/// 拿刷新令牌换一张访问令牌。客户端标识从 `local.files` 里现读，见模块头。
pub async fn refresh(client: &reqwest::Client, refresh: &Secret, local: LocalClients) -> Refreshed {
    let mut files = local.files;
    if files.is_empty() {
        return Refreshed::NoClient;
    }
    let working = local.working.lock().ok().and_then(|g| g.clone());
    if let Some((f, _)) = &working {
        if let Some(pos) = files.iter().position(|x| x == f) {
            let f = files.remove(pos);
            files.insert(0, f);
        }
    }
    let Some(refresh_text) = refresh.as_str() else {
        return Refreshed::Failed("刷新令牌不是文本，形状不认识".into());
    };
    let mut tried = 0usize;
    let mut last_other: Option<String> = None;
    for file in files {
        let scan_path = file.clone();
        let pairs = match tokio::task::spawn_blocking(move || scan_file(&scan_path)).await {
            Ok(p) => p,
            Err(e) => return Refreshed::Failed(format!("读客户端标识的任务异常结束：{e}")),
        };
        let mut order: Vec<usize> = (0..pairs.len()).collect();
        if let Some((f, i)) = &working {
            if *f == file && *i < pairs.len() {
                order.retain(|x| x != i);
                order.insert(0, *i);
            }
        }
        for i in order {
            if tried >= MAX_CLIENT_TRIES {
                break;
            }
            tried += 1;
            let (id, secret) = &pairs[i];
            let secret_text = secret.as_str().unwrap_or_default();
            let form = [
                ("client_id", id.as_str()),
                ("client_secret", secret_text),
                ("refresh_token", refresh_text),
                ("grant_type", "refresh_token"),
            ];
            let resp = match client.post(TOKEN_URL).form(&form).send().await {
                Ok(r) => r,
                Err(e) => return Refreshed::Failed(format!("连不上 Google：{e}")),
            };
            let status = resp.status().as_u16();
            let body = Secret::new(resp.bytes().await.map(|b| b.to_vec()).unwrap_or_default());
            match parse_token_reply(status, body.as_bytes(), now()) {
                RefreshOutcome::Fresh(access, expires) => {
                    if let Ok(mut g) = local.working.lock() {
                        *g = Some((file.clone(), i));
                    }
                    crate::audit::write(&format!(
                        "在内存里换新了一次{}的访问令牌（不写回、不落盘、不记内容）",
                        local.product
                    ));
                    return Refreshed::Fresh(access, expires);
                }
                RefreshOutcome::WrongClient => continue,
                RefreshOutcome::Revoked => return Refreshed::Revoked,
                RefreshOutcome::Other(e) => last_other = Some(e),
            }
        }
        if tried >= MAX_CLIENT_TRIES {
            break;
        }
    }
    match last_other {
        Some(e) => Refreshed::Failed(e),
        None if tried == 0 => Refreshed::NoClient,
        None => Refreshed::ClientRejected,
    }
}

/// 换不下来时统一的报错：`hint` 是「让官方客户端自己换新」那一句（每个产品说法不同）。
pub fn refresh_error(outcome: Refreshed, product: &str, relogin: &str, hint: &str) -> GateError {
    GateError::Other(match outcome {
        Refreshed::Fresh(..) => unreachable!("换到了就不是错误"),
        Refreshed::Revoked => {
            format!("Google 说这个账户的刷新令牌已经失效（invalid_grant）—— {relogin}")
        }
        Refreshed::NoClient => {
            format!("本机没找到{product}的客户端标识，换不了新令牌 —— {hint}")
        }
        Refreshed::ClientRejected => format!(
            "本机{product}里的客户端标识 Google 都不认（invalid_client），可能是它更新换了标识 —— {hint}"
        ),
        Refreshed::Failed(e) => format!("换新访问令牌没成功（{e}）—— {hint}"),
    })
}

// ------------------------------------------------------------------ 客户端标识

const ID_SUFFIX: &[u8] = b".apps.googleusercontent.com";
const SECRET_PREFIX: &[u8] = b"GOCSPX-";
/// 一次读这么多；相邻两块重叠这么多（远长于任何一条标识）。
const CHUNK: usize = 4 << 20;
const OVERLAP: usize = 512;

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// 在一块字节里找客户端 id。`at` = 这块在整个文件里的起点。
///
/// 只收**起点落在 `[lo, keep)` 里**的：`keep` 之后是重叠区，留给下一块（免得算两次，
/// 也免得被块边界切掉一半的混进来）；除第一块外 `lo = 1` —— 第 0 个字节是上一块留下的
/// 「回看字节」，起点落在它上面的那条上一块已经收过了，而往回走到它就停下的那条，
/// 真正的起点在更前面，是被切掉的半条。
fn find_ids(buf: &[u8], at: u64, lo: usize, keep: usize, out: &mut Vec<(u64, String)>) {
    let mut from = 0;
    while let Some(rel) = find(&buf[from..], ID_SUFFIX) {
        let dot = from + rel;
        from = dot + ID_SUFFIX.len();
        let mut i = dot;
        while i > 0 && (buf[i - 1].is_ascii_lowercase() || buf[i - 1].is_ascii_digit()) {
            i -= 1;
        }
        if dot - i < 20 || i == 0 || buf[i - 1] != b'-' {
            continue;
        }
        let dash = i - 1;
        let mut j = dash;
        while j > 0 && buf[j - 1].is_ascii_digit() {
            j -= 1;
        }
        if dash - j < 6 || j < lo || j >= keep {
            continue;
        }
        let id = String::from_utf8_lossy(&buf[j..dot + ID_SUFFIX.len()]).into_owned();
        out.push((at + j as u64, id));
    }
}

/// 在一块字节里找客户端 secret。`lo` / `keep` 的规矩同 [`find_ids`]。
fn find_secrets(buf: &[u8], at: u64, lo: usize, keep: usize, out: &mut Vec<(u64, Secret)>) {
    let mut from = 0;
    while let Some(rel) = find(&buf[from..], SECRET_PREFIX) {
        let start = from + rel;
        from = start + SECRET_PREFIX.len();
        let mut k = from;
        while k < buf.len() && (buf[k].is_ascii_alphanumeric() || buf[k] == b'_' || buf[k] == b'-')
        {
            k += 1;
        }
        if k - from < 16 || start < lo || start >= keep {
            continue;
        }
        out.push((at + start as u64, Secret::new(buf[start..k].to_vec())));
    }
}

/// 按位置把 id 和 secret 配成对：每个 id 先配它后面最近的那个 secret（没有就前面最近的），
/// 再把剩下的组合按顺序补上。去重，最多 [`MAX_CLIENT_TRIES`] 组。**纯函数。**
fn pair_up(ids: Vec<(u64, String)>, secrets: Vec<(u64, Secret)>) -> Vec<(String, Secret)> {
    let mut uniq_ids: Vec<(u64, String)> = Vec::new();
    for (p, id) in ids {
        if !uniq_ids.iter().any(|(_, x)| *x == id) {
            uniq_ids.push((p, id));
        }
    }
    let mut uniq_secrets: Vec<(u64, Secret)> = Vec::new();
    for (p, s) in secrets {
        if !uniq_secrets
            .iter()
            .any(|(_, x)| x.as_bytes() == s.as_bytes())
        {
            uniq_secrets.push((p, s));
        }
    }
    let mut order: Vec<(usize, usize)> = Vec::new();
    for (ii, (ip, _)) in uniq_ids.iter().enumerate() {
        let after = uniq_secrets
            .iter()
            .enumerate()
            .filter(|(_, (sp, _))| sp > ip)
            .min_by_key(|(_, (sp, _))| sp - ip)
            .map(|(si, _)| si);
        let before = uniq_secrets
            .iter()
            .enumerate()
            .filter(|(_, (sp, _))| sp < ip)
            .min_by_key(|(_, (sp, _))| ip - sp)
            .map(|(si, _)| si);
        if let Some(si) = after.or(before) {
            if !order.contains(&(ii, si)) {
                order.push((ii, si));
            }
        }
    }
    for ii in 0..uniq_ids.len() {
        for si in 0..uniq_secrets.len() {
            if !order.contains(&(ii, si)) {
                order.push((ii, si));
            }
        }
    }
    order
        .into_iter()
        .take(MAX_CLIENT_TRIES)
        .map(|(ii, si)| {
            (
                uniq_ids[ii].1.clone(),
                Secret::new(uniq_secrets[si].1.as_bytes().to_vec()),
            )
        })
        .collect()
}

/// 一段字节 → 配好的对。**纯函数**，单测只喂编造的字符串（正式路径走分块的 [`scan_reader`]）。
#[cfg(test)]
fn scan_bytes(bytes: &[u8]) -> Vec<(String, Secret)> {
    let mut ids = Vec::new();
    let mut secrets = Vec::new();
    find_ids(bytes, 0, 0, bytes.len(), &mut ids);
    find_secrets(bytes, 0, 0, bytes.len(), &mut secrets);
    pair_up(ids, secrets)
}

/// 分块读（语言服务器一百多 MB，不整个读进内存）。
fn scan_reader<R: Read>(mut r: R, chunk: usize) -> Vec<(String, Secret)> {
    let mut ids = Vec::new();
    let mut secrets = Vec::new();
    let mut buf: Vec<u8> = Vec::with_capacity(chunk + OVERLAP);
    let mut at: u64 = 0;
    let mut lo = 0usize;
    let mut piece = vec![0u8; chunk];
    loop {
        // 读错了当读完：少扫一段只是少几组候选，不是错误。
        let n = r.read(&mut piece).unwrap_or(0);
        let eof = n == 0;
        buf.extend_from_slice(&piece[..n]);
        if !eof && buf.len() < chunk + OVERLAP {
            continue;
        }
        let keep = if eof { buf.len() } else { buf.len() - OVERLAP };
        find_ids(&buf, at, lo, keep, &mut ids);
        find_secrets(&buf, at, lo, keep, &mut secrets);
        if eof {
            break;
        }
        // 留一个回看字节，见 [`find_ids`]。
        let cut = keep - 1;
        at += cut as u64;
        buf.drain(..cut);
        lo = 1;
    }
    // 读出来的字节里就有标识，抹掉。
    for b in piece.iter_mut().chain(buf.iter_mut()) {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    pair_up(ids, secrets)
}

fn scan_file(path: &Path) -> Vec<(String, Secret)> {
    match std::fs::File::open(path) {
        Ok(f) => scan_reader(std::io::BufReader::new(f), CHUNK),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_endpoint_replies_map_to_the_right_next_step() {
        match parse_token_reply(200, br#"{"access_token":"fake","expires_in":3599}"#, 1_000) {
            RefreshOutcome::Fresh(t, e) => {
                assert_eq!(t.as_str(), Some("fake"));
                assert_eq!(e, 4_599);
            }
            _ => panic!("应当换到了"),
        }
        assert!(matches!(
            parse_token_reply(401, br#"{"error":"invalid_client"}"#, 0),
            RefreshOutcome::WrongClient
        ));
        assert!(matches!(
            parse_token_reply(400, br#"{"error":"unauthorized_client"}"#, 0),
            RefreshOutcome::WrongClient
        ));
        assert!(matches!(
            parse_token_reply(400, br#"{"error":"invalid_grant"}"#, 0),
            RefreshOutcome::Revoked
        ));
        assert!(matches!(
            parse_token_reply(500, b"<html>", 0),
            RefreshOutcome::Other(_)
        ));
    }

    /// 编一个客户端 id / secret。**不是任何真实值。**
    fn fake_id(n: u8) -> String {
        format!("12345678{n}-abcdefghijklmnopqrstuvwxyz0123{n}.apps.googleusercontent.com")
    }
    fn fake_secret(n: u8) -> String {
        format!("GOCSPX-fakefakefakefakefake{n}")
    }

    #[test]
    fn each_client_id_is_paired_with_the_secret_that_follows_it_first() {
        let text = format!(
            r#"var a={{clientId:"{}",clientSecret:"{}"}};var b={{clientId:"{}",clientSecret:"{}"}};"#,
            fake_id(1),
            fake_secret(1),
            fake_id(2),
            fake_secret(2)
        );
        let pairs = scan_bytes(text.as_bytes());
        let shown: Vec<_> = pairs
            .iter()
            .map(|(i, s)| (i.clone(), s.as_str().unwrap().to_string()))
            .collect();
        assert_eq!(shown[0], (fake_id(1), fake_secret(1)));
        assert_eq!(shown[1], (fake_id(2), fake_secret(2)));
        assert_eq!(shown.len(), 4, "配好的两组在前，其余组合补在后面");
    }

    #[test]
    fn a_file_without_client_identities_yields_nothing() {
        assert!(
            scan_bytes(b"no identities here .apps.googleusercontent.com GOCSPX-short").is_empty()
        );
    }

    #[test]
    fn chunk_boundaries_neither_lose_nor_duplicate_an_identity() {
        // 尾巴要够长，才会有「不是最后一块」的那几轮 —— 边界只在那几轮上切。
        let tail = format!(
            "\"{}\",\"{}\"{}",
            fake_id(3),
            fake_secret(3),
            "y".repeat(2000)
        );
        // 块很小、前缀长度一格一格地挪：标识总会在某一次被块边界正好切在各个位置上
        // （第一轮的边界在 688，第二轮在 1288）。重叠区要把它接回来，而且只算一次。
        for pad in 0..1400 {
            let text = format!("{}{}", "z".repeat(pad), tail);
            let pairs = scan_reader(std::io::Cursor::new(text.as_bytes()), 600);
            assert_eq!(pairs.len(), 1, "pad={pad}");
            assert_eq!(pairs[0].0, fake_id(3), "pad={pad}");
            assert_eq!(
                pairs[0].1.as_str(),
                Some(fake_secret(3).as_str()),
                "pad={pad}"
            );
        }
    }

    #[test]
    fn a_cached_token_is_only_handed_out_for_the_same_login_and_before_it_expires() {
        let c = TokenCache::new();
        let t = Secret::new(b"fake-access".to_vec());
        c.put("k", "print-a", &t, 10_000);
        assert!(c.get("k", "print-a", 1_000).is_some());
        assert!(
            c.get("k", "print-b", 1_000).is_none(),
            "换了人登录（刷新令牌指纹变了），旧的那张不算数"
        );
        assert!(
            c.get("k", "print-a", 10_000 - TOKEN_MARGIN_SECS).is_none(),
            "离过期不到余量就当它过期了"
        );
        c.forget("k");
        assert!(c.get("k", "print-a", 1_000).is_none());
    }

    #[test]
    fn a_revoked_refresh_token_tells_the_user_to_log_in_again() {
        let e = refresh_error(
            Refreshed::Revoked,
            "Gemini CLI",
            "重新登录一次。",
            "起一次。",
        );
        assert!(e.to_string().contains("invalid_grant"));
        assert!(e.to_string().contains("重新登录一次"));
        let e = refresh_error(
            Refreshed::NoClient,
            "Gemini CLI",
            "重新登录。",
            "起一次它。",
        );
        assert!(e.to_string().contains("Gemini CLI"));
        assert!(e.to_string().contains("起一次它"));
    }
}
