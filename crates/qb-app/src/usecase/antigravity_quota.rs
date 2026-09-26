//! 反重力账户的联网额度（2026-09-23）：Claude / Gemini 两组 × 5 小时 / 每周四格，外加档位与 AI 积分。
//!
//! # 为什么要联网
//!
//! IDE 写在本机 `state.vscdb` 的 `userStatus`（`qb_accounts::antigravity::status`）每个模型只有
//! **一个**剩余比例、**一个**重置时刻，没有「5 小时 / 每周」两个窗口，而且只有 IDE 开着时才更新。
//! 使用者 09-23 的截图：「测试」那个槽位停在 09-21 写下的「剩 100% · 5 天后重置」，而同一时刻
//! Google 那边 Gemini 的周窗口已经用掉 5%。他要「参考 cockpit-tools，把它联网能拿到的条目都做出来」。
//!
//! # 问什么
//!
//! 接口事实（端点名、`bucketId`、字段名）是读 `jlcodes99/cockpit-tools` 的源码核对的 ——
//! 它是 **CC BY-NC-SA 4.0，一行代码都没抄**，这里是照着这些事实自己写的。
//!
//! | # | 调用 | 取什么 |
//! |---|---|---|
//! | 1 | `v1internal:loadCodeAssist` | 档位、`usesGcpTos`、project、AI 积分（`paidTier.availableCredits`） |
//! | 2 | `v1internal:retrieveUserQuotaSummary` | 四格，按 `bucketId` 认：`gemini-5h` / `gemini-weekly` / `3p-5h` / `3p-weekly` |
//! | 3 | `v1internal:fetchAvailableModels`（只在 2 拿不到时） | 各模型的 `quotaInfo`（免费档 Google 不给汇总，回 403） |
//!
//! ⛔ **不发 `onboardUser`。** cockpit 在没有 project 时会发它 —— 那是一个**会改账户状态**的调用
//! （把账户登记到某个档位上）。这里只问、不改：没有 project 就发 `{}`，付费账户这样也答得上来。
//!
//! 主机默认 `daily-cloudcode-pa`（Hub 与 IDE 自己去的那个）；账户按 GCP 服务条款管、又有真实
//! project 时才换 `cloudcode-pa`。请求形状照官方 IDE：`User-Agent: antigravity/<本机 IDE 版本>
//! windows/<arch>`、`metadata.ideType = ANTIGRAVITY`，版本从位置表读 `product.json`，不写死。
//!
//! # 令牌：过期了在内存里换新
//!
//! 用官方客户端自己存在本机的那一份（IDE 槽位的 `state.vscdb` / Hub 的凭据管理器，
//! `qb_accounts::antigravity::token`）。访问令牌只有一小时；过期或五分钟内过期时，拿同一份里的
//! 刷新令牌向 `oauth2.googleapis.com/token` 换一张新的 ——
//! **2026-09-23 使用者拍板，推翻了 0.32.0「不跑 OAuth、不持有 client_id/secret、不刷新令牌」那条。**
//!
//! 换新要带上签发这个令牌的那个客户端的标识。**仓库里一个字都不出现**：要换的那一刻从本机装的
//! 反重力里现读（位置表 `oauth_client_sources`：IDE 的 `main.js`，再不行才是语言服务器），
//! 按出现位置配对，Google 回 `invalid_client` / `unauthorized_client` 就换下一组。
//! 认准的是「哪个文件的第几组」（[`WORKING_CLIENT`]，不含标识本身），标识用完即丢。
//! 换新那一套（回复解析、标识扫描、令牌缓存）在 [`super::google_oauth`]，Gemini CLI 那条也用它。
//!
//! 换来的访问令牌只放在 [`TOKENS`]（内存，过期前五分钟作废，按刷新令牌的指纹认 ——
//! 同一个槽位里换了个账户登录，旧的那张立刻不算数）。**不写回 IDE、不写回凭据管理器、
//! 不进日志 / 事件 / 审计 / 返回值。** 审计只写「换新了一次」。
//!
//! Google 说刷新令牌作废了（`invalid_grant`）时，账户槽位那一笔记进 [`super::login_health`]：
//! 账户行上 IDE 那一半改说「登录已失效」，直到 IDE 自己重新写它的状态库。
//!
//! # 什么时候问
//!
//! **只在使用者点刷新图标时，一次一个账户**（2026-09-23 使用者选的）。打开页面、切页回来、
//! 面板启动都不问，也没有定时器。结果按账户记「最近一次」（[`LAST`]），没有 TTL ——
//! 界面标明是几点查的，要新的就再点一次。
//!
//! # 读不出来就说读不出来
//!
//! 字段名改了、端点换了、令牌换不下来 → 返回一句能照着做的话，**不显示猜出来的数**。
//! 唯一的「推」：某一格有 `resetTime` 却没有 `remainingFraction` 时按 0 算 —— proto3 的 JSON
//! 会把零值字段整个省掉，一格用光了长的就是这个样子。这一格带 `remaining_implied`，
//! 界面悬停要说出来；KNOWN-ISSUES 记着「未实测」。

use super::google_oauth::{self, LocalClients, Refreshed, TokenCache};
use super::login_health;
use crate::error::{GateError, Result};
use qb_accounts::antigravity::status::AntigravityModelQuota;
use qb_accounts::antigravity::token::{self, OAuthToken};
use qb_platform::credentials::Secret;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use ts_rs::TS;

const DAILY_HOST: &str = "https://daily-cloudcode-pa.googleapis.com";
const PROD_HOST: &str = "https://cloudcode-pa.googleapis.com";
/// 换不下来时那两句话（[`google_oauth::refresh_error`]）。
const RELOGIN: &str = "在这个账户的反重力里重新登录一次。";
const SELF_REFRESH: &str = "起一次这个账户的反重力，让它自己换新，再点刷新。";

/// 问谁的额度。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// 面板里的一条反重力账户槽位，用它 IDE 那一半的令牌。
    Account(String),
    /// 反重力 Hub（凭据管理器里那一条）。
    Hub,
}

impl Source {
    fn key(&self) -> String {
        match self {
            Source::Account(id) => format!("account:{id}"),
            Source::Hub => "hub".into(),
        }
    }
}

// ------------------------------------------------------------------ 给界面的形状

/// 哪一组额度。Google 的分组叫「Claude and GPT models」—— Claude 与 GPT-OSS 共用一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[ts(export, rename = "AntigravityQuotaGroup")]
#[serde(rename_all = "kebab-case")]
pub enum QuotaGroup {
    Claude,
    Gemini,
}

/// 哪个窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[ts(export, rename = "AntigravityQuotaSpan")]
#[serde(rename_all = "kebab-case")]
pub enum QuotaSpan {
    FiveHour,
    Weekly,
}

/// 一格额度。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityQuotaWindow {
    pub group: QuotaGroup,
    pub span: QuotaSpan,
    /// 剩余比例 0–1。
    pub remaining: f32,
    /// `true` = Google 没给比例、按 proto3 缺省值算成了 0（见模块头）。界面要说出来。
    pub remaining_implied: bool,
    /// 重置时刻的 Unix 秒。没用过的窗口 Google 给的是「现在 + 窗口长度」，每问一次都往后挪。
    #[ts(type = "number | null")]
    pub reset_epoch: Option<i64>,
    /// 重置时刻，本地 `YYYY-MM-DD HH:MM`。
    pub reset_at: Option<String>,
    /// Google 给这一格的说明（部分用掉时才有），原样。
    pub note: Option<String>,
}

/// 一个账户联网问到的额度。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityOnlineQuota {
    /// 档位 id（`g1-pro-tier` / `free-tier`）与名字，原样。
    pub tier_id: Option<String>,
    pub tier_name: Option<String>,
    /// Google 那边按 GCP 服务条款管的账户（原样转述 `usesGcpTos`）。
    pub gcp_tos: bool,
    /// 可用 AI 积分：`paidTier.availableCredits` 各项 `creditAmount` 之和。
    /// `None` = 回复里没有付费档（免费档没有这一项）。
    pub credits: Option<f64>,
    /// 四格，固定顺序：Claude 5h、Claude 周、Gemini 5h、Gemini 周（缺哪格就没有哪格）。
    pub windows: Vec<AntigravityQuotaWindow>,
    /// 拿不到四格时说为什么（免费档 / 改版）。`None` = 拿到了。
    pub windows_note: Option<String>,
    /// 拿不到四格时的兜底：各模型的剩余比例（跟 IDE 本机那份同一个类型）。
    pub models: Vec<AntigravityModelQuota>,
    /// 这次是什么时候问的，本地 `YYYY-MM-DD HH:MM`。
    pub fetched_at: String,
}

// ------------------------------------------------------------------ 内存里的东西

/// 问到的那一份，连同**问的时候登的是谁**（邮箱；读不出来是 `None`）。
type Asked = (Option<String>, AntigravityOnlineQuota);

/// 「最近一次」问到的，按 [`Source::key`] 存。没有 TTL（见模块头「什么时候问」）。
static LAST: Mutex<Option<HashMap<String, Asked>>> = Mutex::new(None);

/// 换新来的访问令牌，按来源存。**只在内存里。**
static TOKENS: TokenCache = TokenCache::new();

/// 上次认准的客户端标识在哪：`(文件, 第几组)`。**不存标识本身。**
static WORKING_CLIENT: Mutex<Option<(PathBuf, usize)>> = Mutex::new(None);

/// 最近一次问到的。**不联网。**
///
/// ⛔ 问的时候登的是 A、现在登的是 B（Hub 里换了号，或者那个槽位的 IDE 重新登了别的账户）：
/// 那份档位、AI 积分、四格都是 A 的，不许挂在 B 的邮箱底下显示（2026-09-25 查出）。
/// 两边都读得出邮箱、又对不上时就当没问过；读不出来时说不清，照旧给。
pub fn last(source: &Source) -> Option<AntigravityOnlineQuota> {
    let (asked, q) = LAST.lock().ok()?.as_ref()?.get(&source.key()).cloned()?;
    same_account(asked.as_deref(), who(source).as_deref()).then_some(q)
}

/// 问的时候登的是谁 vs 现在登的是谁。两边都读得出、又对不上 → 不是同一个人。**纯函数。**
fn same_account(asked: Option<&str>, now: Option<&str>) -> bool {
    match (asked, now) {
        (Some(then), Some(now)) => then.eq_ignore_ascii_case(now),
        _ => true,
    }
}

fn remember(source: &Source, q: &AntigravityOnlineQuota) {
    let asked = who(source);
    if let Ok(mut g) = LAST.lock() {
        g.get_or_insert_with(HashMap::new)
            .insert(source.key(), (asked, q.clone()));
    }
}

/// 这个来源此刻登的是谁（邮箱）。**只读本机、不联网**：Hub 从凭据里 `id_token` 的载荷解
/// （状态页每 15 秒本来就这么读一次），账户槽位读 IDE 写在 `state.vscdb` 里的 `userStatus`。
fn who(source: &Source) -> Option<String> {
    use qb_accounts::antigravity::{account, hub, status};
    match source {
        Source::Hub => hub::identity().ok().flatten().and_then(|i| i.email),
        Source::Account(id) => {
            let (_, dir) = account::ide_half(&account::root(), id).ok().flatten()?;
            status::identity(&dir).ok().flatten().and_then(|i| i.email)
        }
    }
}

/// 把这个来源记下的东西（额度、换新来的令牌）全忘掉。移除账户之后调 ——
/// 留着就会有一条「不存在的账户」的额度一直躺在内存里。
pub fn forget(source: &Source) {
    let key = source.key();
    if let Ok(mut g) = LAST.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(&key);
        }
    }
    TOKENS.forget(&key);
    if let Source::Account(id) = source {
        login_health::forget(&login_health::antigravity_ide_key(id));
    }
}

/// 命令层的入口。`refresh = false` 只读「最近一次」，**绝不联网**；`true` 联网问一次。
pub async fn quota(source: Source, refresh: bool) -> Result<Option<AntigravityOnlineQuota>> {
    if !refresh {
        return Ok(last(&source));
    }
    if !crate::settings::antigravity_hub_quota() {
        return Err(GateError::Other(
            "设置里关掉了反重力的联网额度（settings.json 的 antigravity_hub_quota）。".into(),
        ));
    }
    let q = fetch(&source).await?;
    remember(&source, &q);
    Ok(Some(q))
}

// ------------------------------------------------------------------ 联网

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| GateError::Other(format!("联网额度的 HTTP 客户端建不起来：{e}")))
}

fn now() -> i64 {
    google_oauth::now()
}

fn local_minute(secs: i64) -> Option<String> {
    use chrono::TimeZone as _;
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
}

/// `windows/amd64` 这一段，照官方 IDE 的写法。
fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "amd64",
    }
}

/// 请求头里的 `User-Agent`，照官方 IDE 的形状。版本读不到就不写版本。
pub fn user_agent(ide_version: Option<&str>) -> String {
    match ide_version {
        Some(v) => format!("antigravity/{v} windows/{}", arch()),
        None => format!("antigravity windows/{}", arch()),
    }
}

/// `loadCodeAssist` 的 `metadata`，照官方 IDE 的形状。
pub fn metadata(ide_version: Option<&str>) -> Value {
    let mut m = json!({
        "ideName": "antigravity",
        "ideType": "ANTIGRAVITY",
        "platform": if arch() == "arm64" { "WINDOWS_ARM64" } else { "WINDOWS_AMD64" },
        "updateChannel": "stable",
        "pluginType": "GEMINI",
    });
    if let Some(v) = ide_version {
        m["ideVersion"] = Value::String(v.to_string());
    }
    m
}

/// 三个调用拿回来的原文。诊断例子 `antigravity-hub-quota` 拿它看形状（只打印键名）。
pub struct Replies {
    pub load: String,
    pub summary_status: u16,
    pub summary: String,
    /// 只在汇总拿不到时才问：`(状态码, 原文)`。
    pub models: Option<(u16, String)>,
}

/// 问一次，解析好。
pub async fn fetch(source: &Source) -> Result<AntigravityOnlineQuota> {
    let (label, replies) = exchange(source).await?;
    let load = parse_load(&replies.load)?;
    let (windows, windows_note, models) = if (200..300).contains(&replies.summary_status) {
        match parse_summary(&replies.summary) {
            Ok(w) => (w, None, Vec::new()),
            Err(e) => (Vec::new(), Some(e.to_string()), models_of(&replies.models)),
        }
    } else {
        (
            Vec::new(),
            Some(summary_note(
                replies.summary_status,
                &replies.summary,
                &replies.models,
            )),
            models_of(&replies.models),
        )
    };
    crate::audit::write(&format!("问了一次反重力{label}的额度（只读，不记内容）"));
    Ok(AntigravityOnlineQuota {
        tier_id: load.tier_id,
        tier_name: load.tier_name,
        gcp_tos: load.gcp_tos,
        credits: load.credits,
        windows,
        windows_note,
        models,
        fetched_at: local_minute(now()).unwrap_or_default(),
    })
}

fn models_of(models: &Option<(u16, String)>) -> Vec<AntigravityModelQuota> {
    match models {
        Some((s, body)) if (200..300).contains(s) => parse_models(body).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// 发那几个请求，把原文拿回来。返回 `(给审计用的称呼, 原文)`。
pub async fn exchange(source: &Source) -> Result<(String, Replies)> {
    let label = match source {
        Source::Account(id) => {
            let (label, _) = ide_dir_of(id)?;
            format!("账户「{label}」")
        }
        Source::Hub => " Hub ".to_string(),
    };
    let client = http()?;
    let local = dirs::data_local_dir().unwrap_or_default();
    let version = qb_install::install::antigravity::ide_version(&local);
    let ua = user_agent(version.as_deref());

    let load_body = json!({
        "metadata": metadata(version.as_deref()),
        "mode": "FULL_ELIGIBILITY_CHECK",
    });
    let mut token = access_token(&client, source, false).await?;
    let (mut status, mut load) = post(
        &client,
        DAILY_HOST,
        "loadCodeAssist",
        &token,
        &ua,
        &load_body,
    )
    .await?;
    if status == 401 {
        // 本机时钟说还没过期、Google 说不认 —— 换一张再问一次，只重来这一次。
        token = access_token(&client, source, true).await?;
        (status, load) = post(
            &client,
            DAILY_HOST,
            "loadCodeAssist",
            &token,
            &ua,
            &load_body,
        )
        .await?;
    }
    if !(200..300).contains(&status) {
        return Err(http_error("loadCodeAssist", status, &load));
    }
    let info = parse_load(&load)?;
    let host = if info.gcp_tos && info.project.is_some() {
        PROD_HOST
    } else {
        DAILY_HOST
    };
    let payload = match &info.project {
        Some(p) => json!({ "project": p }),
        None => json!({}),
    };
    let (summary_status, summary) = post(
        &client,
        host,
        "retrieveUserQuotaSummary",
        &token,
        &ua,
        &payload,
    )
    .await?;
    let summary_ok = (200..300).contains(&summary_status) && parse_summary(&summary).is_ok();
    let models = if summary_ok {
        None
    } else {
        Some(post(&client, host, "fetchAvailableModels", &token, &ua, &payload).await?)
    };
    Ok((
        label,
        Replies {
            load,
            summary_status,
            summary,
            models,
        },
    ))
}

async fn post(
    client: &reqwest::Client,
    host: &str,
    method: &str,
    token: &Secret,
    ua: &str,
    body: &Value,
) -> Result<(u16, String)> {
    let bearer = token
        .as_str()
        .ok_or_else(|| GateError::Other("访问令牌不是文本，形状不认识".into()))?;
    let resp = client
        .post(format!("{host}/v1internal:{method}"))
        .bearer_auth(bearer)
        .header(reqwest::header::USER_AGENT, ua)
        .json(body)
        .send()
        .await
        .map_err(|e| GateError::Other(format!("连不上 Google（{method}）：{e}")))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text))
}

/// Google 错误回复里那个机器可读的原因（`SUBSCRIPTION_REQUIRED` 之类）。**只取代号，不取原文** ——
/// 原文里可能带回声，别往界面上贴。
fn google_reason(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let e = v.get("error")?;
    e.get("details")
        .and_then(Value::as_array)
        .and_then(|d| {
            d.iter()
                .find_map(|x| x.get("reason").and_then(Value::as_str))
        })
        .or_else(|| e.get("status").and_then(Value::as_str))
        .filter(|s| {
            !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
        })
        .map(str::to_owned)
}

/// ⛔ 状态码要带上：401 是「去重登」，404 是「等面板更新」，使用者能做的完全不同。
fn http_error(call: &str, status: u16, body: &str) -> GateError {
    let reason = google_reason(body)
        .map(|r| format!("，{r}"))
        .unwrap_or_default();
    GateError::Other(match status {
        401 => format!(
            "Google 不认这个令牌（HTTP 401{reason}），换新之后也一样 —— 去这个账户的反重力里重新登录一次。"
        ),
        403 => format!(
            "Google 拒绝了 {call}（HTTP 403{reason}）。这个账户可能没开通反重力，或者接口要求的东西变了。"
        ),
        404 => format!("{call} 这个接口不在了（HTTP 404）—— Google 换了地址，这一格暂时读不出来，等面板更新。"),
        429 => "问得太频繁，Google 限流了（HTTP 429）。过一会儿再点刷新。".to_string(),
        _ => format!("Google 回了 HTTP {status}{reason}（{call}），这一格读不出来。"),
    })
}

fn summary_note(status: u16, body: &str, models: &Option<(u16, String)>) -> String {
    let head = match (status, google_reason(body).as_deref()) {
        (403, _) => "免费档：Google 不给免费档 5 小时 / 每周的汇总（HTTP 403）".to_string(),
        (404, _) => "汇总接口不在了（HTTP 404）".to_string(),
        (s, Some(r)) => format!("汇总没拿到（HTTP {s}，{r}）"),
        (s, None) => format!("汇总没拿到（HTTP {s}）"),
    };
    match models {
        Some((s, b)) if (200..300).contains(s) && parse_models(b).is_ok_and(|m| !m.is_empty()) => {
            format!("{head}，下面按模型显示。")
        }
        Some((s, _)) => format!("{head}；按模型也没问到（HTTP {s}）。"),
        None => format!("{head}。"),
    }
}

// ------------------------------------------------------------------ 令牌

fn ide_dir_of(id: &str) -> Result<(String, PathBuf)> {
    let root = qb_accounts::antigravity::account::root();
    qb_accounts::antigravity::account::ide_half(&root, id)?.ok_or_else(|| {
        GateError::Other(
            "这个账户还没有 IDE 那一半 —— 反重力的联网额度要用 IDE 登录的令牌。".into(),
        )
    })
}

/// 读官方客户端存在本机的那一份令牌。
fn stored_token(source: &Source) -> Result<OAuthToken> {
    match source {
        Source::Account(id) => {
            let (_, dir) = ide_dir_of(id)?;
            token::ide_token(&dir)?.ok_or_else(|| {
                GateError::Other(
                    "这个账户的 IDE 还没登录（它的状态库里没有令牌）—— 先在 IDE 窗口里用 Google 登录。"
                        .into(),
                )
            })
        }
        Source::Hub => token::hub_token()?.ok_or_else(|| {
            GateError::Other("反重力 Hub 还没登录过（凭据管理器里没有它那一条）。".into())
        }),
    }
}

/// 拿一张现在能用的访问令牌。`force_refresh` = 不管本机时钟怎么说，都换一张新的。
async fn access_token(
    client: &reqwest::Client,
    source: &Source,
    force_refresh: bool,
) -> Result<Secret> {
    let stored = stored_token(source)?;
    let print = google_oauth::fingerprint(stored.refresh.as_ref());
    let key = source.key();
    let t = now();
    if !force_refresh {
        if let Some(access) = TOKENS.get(&key, &print, t) {
            return Ok(access);
        }
        if stored.access_usable(t, google_oauth::TOKEN_MARGIN_SECS) {
            if let Some(a) = stored.access.as_ref() {
                return Ok(Secret::new(a.as_bytes().to_vec()));
            }
        }
    }
    let refresh = stored.refresh.as_ref().ok_or_else(|| {
        GateError::Other(
            "访问令牌过期了，而本机那份里没有刷新令牌 —— 起一次这个账户的反重力，让它自己换新，再点刷新。"
                .into(),
        )
    })?;
    let local = dirs::data_local_dir().unwrap_or_default();
    let clients = LocalClients {
        files: qb_install::install::antigravity::oauth_client_sources(&local),
        working: &WORKING_CLIENT,
        product: "反重力",
    };
    match google_oauth::refresh(client, refresh, clients).await {
        Refreshed::Fresh(access, expires) => {
            TOKENS.put(&key, &print, &access, expires);
            Ok(access)
        }
        Refreshed::Revoked => {
            // 登录态跟着改：账户行上 IDE 那一半说「登录已失效」、露出「登录」，
            // 直到 IDE 自己重新写它的状态库（重新登录之后一定会写）。Hub 没有槽位行，不记。
            if let Source::Account(id) = source {
                if let Ok((_, dir)) = ide_dir_of(id) {
                    login_health::record(
                        &login_health::antigravity_ide_key(id),
                        &qb_accounts::antigravity::status::state_db(&dir),
                        "登录已失效：Google 说这个账户的刷新令牌作废了（invalid_grant）· 点 IDE 旁边的「登录」重新登录",
                    );
                }
            }
            Err(google_oauth::refresh_error(
                Refreshed::Revoked,
                "反重力",
                RELOGIN,
                SELF_REFRESH,
            ))
        }
        other => Err(google_oauth::refresh_error(
            other,
            "反重力",
            RELOGIN,
            SELF_REFRESH,
        )),
    }
}

// ------------------------------------------------------------------ 解析（纯函数）

/// `loadCodeAssist` 里我们要的那几样。
#[derive(Debug, Default, PartialEq)]
pub struct LoadInfo {
    pub tier_id: Option<String>,
    pub tier_name: Option<String>,
    pub gcp_tos: bool,
    pub project: Option<String>,
    pub credits: Option<f64>,
}

fn text_of(v: Option<&Value>, key: &str) -> Option<String> {
    v.and_then(|t| t.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', "").parse::<f64>().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

fn epoch_of(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    let s = v.as_str()?;
    if let Ok(n) = s.parse::<i64>() {
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp())
}

/// 纯函数。认不出任何一样就报错 —— 返回一个全空的 `LoadInfo` 会让界面以为「查到了、就是没档位」。
pub fn parse_load(body: &str) -> Result<LoadInfo> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| GateError::Other(format!("loadCodeAssist 的回复不是 JSON：{e}")))?;
    let paid = v.get("paidTier").filter(|t| t.is_object());
    let current = v.get("currentTier").filter(|t| t.is_object());
    let default_allowed = v
        .get("allowedTiers")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|t| t.get("isDefault").and_then(Value::as_bool) == Some(true))
        });
    if paid.is_none()
        && current.is_none()
        && default_allowed.is_none()
        && v.get("cloudaicompanionProject").is_none()
    {
        return Err(GateError::Other(
            "loadCodeAssist 的回复里没有认得出的字段 —— 多半是接口改版了，这一格读不出来。".into(),
        ));
    }
    let gcp_tos = current
        .and_then(|t| t.get("usesGcpTos"))
        .and_then(Value::as_bool)
        .or_else(|| {
            default_allowed
                .and_then(|t| t.get("usesGcpTos"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    let project = match v.get("cloudaicompanionProject") {
        Some(Value::String(s)) => Some(s.trim().to_string()),
        Some(o @ Value::Object(_)) => text_of(Some(o), "id"),
        _ => None,
    }
    // 官方 IDE 也把这个当成「没有 project」：拿它去问额度会被当成别人的项目。
    .filter(|p| !p.is_empty() && p != "aicode-consumers");
    // 付费档在就一定说得出积分：proto3 会把空列表整个省掉，省掉就是 0。
    let credits = paid.map(|p| {
        p.get("availableCredits")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| match c.get("creditAmount") {
                        None => Some(0.0),
                        Some(x) => number(x),
                    })
                    .filter(|x| *x >= 0.0)
                    .sum()
            })
            .unwrap_or(0.0)
    });
    Ok(LoadInfo {
        tier_id: text_of(paid, "id").or_else(|| text_of(current, "id")),
        tier_name: text_of(paid, "name").or_else(|| text_of(current, "name")),
        gcp_tos,
        project,
        credits,
    })
}

/// 这一格是哪组哪个窗口。认 `bucketId`（`gemini-5h` / `3p-weekly` …），窗口名兜底。
fn bucket_kind(b: &Value) -> Option<(QuotaGroup, QuotaSpan)> {
    let id = b
        .get("bucketId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let window = b
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let group = if id.starts_with("gemini") {
        QuotaGroup::Gemini
    } else if id.starts_with("3p") || id.starts_with("claude") {
        QuotaGroup::Claude
    } else {
        return None;
    };
    let span = if id.ends_with("5h") || window == "5h" {
        QuotaSpan::FiveHour
    } else if id.ends_with("weekly") || window == "weekly" {
        QuotaSpan::Weekly
    } else {
        return None;
    };
    Some((group, span))
}

/// 纯函数：`retrieveUserQuotaSummary` → 四格。一格都认不出来就报错。
pub fn parse_summary(body: &str) -> Result<Vec<AntigravityQuotaWindow>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| GateError::Other(format!("汇总的回复不是 JSON：{e}")))?;
    let mut out: Vec<AntigravityQuotaWindow> = Vec::new();
    let groups = v.get("groups").and_then(Value::as_array);
    for group in groups.into_iter().flatten() {
        let buckets = group.get("buckets").and_then(Value::as_array);
        for b in buckets.into_iter().flatten() {
            let Some((g, s)) = bucket_kind(b) else {
                continue;
            };
            let reset_epoch = b.get("resetTime").and_then(epoch_of).filter(|x| *x > 0);
            let fraction = b.get("remainingFraction").and_then(number);
            let (remaining, implied) = match fraction {
                Some(f) => (f.clamp(0.0, 1.0) as f32, false),
                // proto3 把 0 省掉了 —— 有重置时刻、没有比例，就是用光了。
                None if reset_epoch.is_some() => (0.0, true),
                None => continue,
            };
            if out.iter().any(|w| w.group == g && w.span == s) {
                continue;
            }
            out.push(AntigravityQuotaWindow {
                group: g,
                span: s,
                remaining,
                remaining_implied: implied,
                reset_epoch,
                reset_at: reset_epoch.and_then(local_minute),
                note: b
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                    .map(|x| x.chars().take(200).collect()),
            });
        }
    }
    if out.is_empty() {
        return Err(GateError::Other(
            "汇总的回复里没有认得出的额度格 —— 多半是接口改版了。".into(),
        ));
    }
    out.sort_by_key(|w| (w.group, w.span));
    Ok(out)
}

/// 纯函数：`fetchAvailableModels` → 各模型额度（拿不到汇总时的兜底）。
///
/// 只留 Gemini / Claude / GPT 的模型（`chat_*` / `tab_*` 是 IDE 内部用的）；
/// 同名的（Google 有两个 id 都叫「Gemini 3.1 Pro (High)」）合成一条，取剩得少的那个。
pub fn parse_models(body: &str) -> Result<Vec<AntigravityModelQuota>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| GateError::Other(format!("模型清单的回复不是 JSON：{e}")))?;
    let models = v
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| GateError::Other("模型清单的回复里没有 models".into()))?;
    let mut out: Vec<AntigravityModelQuota> = Vec::new();
    for (id, m) in models {
        let lid = id.to_ascii_lowercase();
        if !(lid.contains("gemini") || lid.contains("claude") || lid.contains("gpt")) {
            continue;
        }
        let Some(q) = m.get("quotaInfo") else {
            continue;
        };
        let reset_epoch = q.get("resetTime").and_then(epoch_of).filter(|x| *x > 0);
        let read = q
            .get("remainingFraction")
            .and_then(number)
            .map(|f| f.clamp(0.0, 1.0) as f32);
        // 只有 `resetTime`：proto3 把 0 省掉了，按 0 算 —— 并且记下是推出来的（界面要说）。
        let remaining_implied = read.is_none() && reset_epoch.is_some();
        let remaining = read.or(reset_epoch.map(|_| 0.0));
        if remaining.is_none() {
            continue;
        }
        let label = m
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string();
        let tags = m
            .get("tagTitle")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();
        if let Some(seen) = out.iter_mut().find(|x| x.label == label) {
            if remaining < seen.remaining {
                seen.remaining = remaining;
                seen.reset_epoch = reset_epoch;
                seen.reset_at = reset_epoch.and_then(local_minute);
                seen.remaining_implied = remaining_implied;
            }
            continue;
        }
        out.push(AntigravityModelQuota {
            label,
            model_id: 0,
            remaining,
            reset_at: reset_epoch.and_then(local_minute),
            reset_epoch,
            tags,
            remaining_implied,
        });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ⛔ 单测不联网、不碰真实的运行期状态。下面的报文是照 2026-09-23 核对过的形状编的，
    // 标识与令牌都是现编的字符串。

    #[test]
    fn the_four_windows_are_recognised_by_bucket_id_and_come_out_in_a_fixed_order() {
        let body = r#"{"description":"x","groups":[
          {"displayName":"Gemini Models","buckets":[
            {"bucketId":"gemini-weekly","window":"weekly","remainingFraction":0.9507503,"resetTime":"2026-09-26T11:01:47Z","description":"partly used"},
            {"bucketId":"gemini-5h","window":"5h","remainingFraction":1,"resetTime":"2026-09-23T16:06:39Z"}]},
          {"displayName":"Claude and GPT models","buckets":[
            {"bucketId":"3p-weekly","window":"weekly","remainingFraction":1,"resetTime":"2026-09-30T11:06:39Z"},
            {"bucketId":"3p-5h","window":"5h","remainingFraction":1,"resetTime":"2026-09-23T16:06:39Z"}]}]}"#;
        let w = parse_summary(body).expect("认得出");
        let kinds: Vec<_> = w.iter().map(|x| (x.group, x.span)).collect();
        assert_eq!(
            kinds,
            vec![
                (QuotaGroup::Claude, QuotaSpan::FiveHour),
                (QuotaGroup::Claude, QuotaSpan::Weekly),
                (QuotaGroup::Gemini, QuotaSpan::FiveHour),
                (QuotaGroup::Gemini, QuotaSpan::Weekly),
            ]
        );
        let gemini_week = &w[3];
        assert!((gemini_week.remaining - 0.9507503).abs() < 1e-6);
        assert!(!gemini_week.remaining_implied);
        assert_eq!(gemini_week.reset_epoch, Some(1_790_420_507));
        assert_eq!(gemini_week.note.as_deref(), Some("partly used"));
    }

    #[test]
    fn a_window_with_a_reset_time_but_no_fraction_is_a_used_up_window() {
        // proto3 的 JSON 把 0 省掉了。
        let body = r#"{"groups":[{"buckets":[{"bucketId":"gemini-5h","resetTime":"2026-09-23T16:06:39Z"}]}]}"#;
        let w = parse_summary(body).expect("认得出");
        assert_eq!(w[0].remaining, 0.0);
        assert!(w[0].remaining_implied, "界面要说出这个 0 是推出来的");
    }

    #[test]
    fn a_summary_we_do_not_understand_is_an_error_not_an_empty_list() {
        assert!(parse_summary("{}").is_err());
        assert!(
            parse_summary(r#"{"groups":[{"buckets":[{"bucketId":"something-new"}]}]}"#).is_err()
        );
        assert!(parse_summary("not json").is_err());
    }

    #[test]
    fn aliases_and_the_window_field_are_accepted_too() {
        let body = r#"{"groups":[{"buckets":[
            {"bucketId":"claude:weekly","remainingFraction":"0.5","resetTime":1790420507},
            {"bucketId":"gemini-x","window":"5h","remainingFraction":0.25}]}]}"#;
        let w = parse_summary(body).expect("认得出");
        assert_eq!(
            (w[0].group, w[0].span),
            (QuotaGroup::Claude, QuotaSpan::Weekly)
        );
        assert_eq!(w[0].remaining, 0.5);
        assert_eq!(
            (w[1].group, w[1].span),
            (QuotaGroup::Gemini, QuotaSpan::FiveHour)
        );
    }

    #[test]
    fn load_picks_the_paid_tier_first_and_sums_the_credits() {
        let body = r#"{"currentTier":{"id":"standard-tier","name":"Standard","usesGcpTos":false},
            "paidTier":{"id":"g1-pro-tier","name":"Google AI Pro","availableCredits":[
              {"creditType":"GOOGLE_ONE_AI","creditAmount":"1,000"},
              {"creditType":"OTHER","creditAmount":"25.5"}]},
            "cloudaicompanionProject":"aicode-consumers"}"#;
        let l = parse_load(body).expect("认得出");
        assert_eq!(l.tier_id.as_deref(), Some("g1-pro-tier"));
        assert_eq!(l.tier_name.as_deref(), Some("Google AI Pro"));
        assert_eq!(l.credits, Some(1025.5));
        assert_eq!(l.project, None, "aicode-consumers 当作没有 project");
        assert!(!l.gcp_tos);
    }

    #[test]
    fn a_paid_tier_without_credits_has_zero_and_a_free_tier_has_none() {
        let paid = parse_load(r#"{"paidTier":{"id":"g1-pro-tier"}}"#).expect("认得出");
        assert_eq!(paid.credits, Some(0.0), "proto3 省掉了空列表");
        let free =
            parse_load(r#"{"currentTier":{"id":"free-tier","name":"Antigravity Starter Quota"}}"#)
                .expect("认得出");
        assert_eq!(free.credits, None);
        assert_eq!(free.tier_id.as_deref(), Some("free-tier"));
    }

    #[test]
    fn a_gcp_tos_account_with_a_real_project_is_reported_as_such() {
        let l = parse_load(
            r#"{"currentTier":{"id":"standard-tier","usesGcpTos":true},
                "cloudaicompanionProject":{"id":"my-project-123"}}"#,
        )
        .expect("认得出");
        assert!(l.gcp_tos);
        assert_eq!(l.project.as_deref(), Some("my-project-123"));
    }

    #[test]
    fn a_load_reply_we_do_not_understand_is_an_error() {
        assert!(parse_load("{}").is_err());
        assert!(parse_load(r#"{"somethingElse":1}"#).is_err());
        assert!(parse_load("nope").is_err());
    }

    #[test]
    fn models_keep_only_real_model_families_and_merge_same_names() {
        let body = r#"{"models":{
            "gemini-3.1-pro-high":{"displayName":"Gemini 3.1 Pro (High)","quotaInfo":{"remainingFraction":0.8,"resetTime":"2026-09-28T00:00:00Z"}},
            "gemini-pro-agent":{"displayName":"Gemini 3.1 Pro (High)","quotaInfo":{"remainingFraction":0.6,"resetTime":"2026-09-28T00:00:00Z"}},
            "claude-sonnet-4-6":{"displayName":"Claude Sonnet 4.6","tagTitle":"New","quotaInfo":{"resetTime":"2026-09-28T00:00:00Z"}},
            "tab_flash":{"displayName":"internal","quotaInfo":{"remainingFraction":1}},
            "gpt-oss-120b-medium":{"displayName":"GPT-OSS 120B"}}}"#;
        let m = parse_models(body).expect("认得出");
        let names: Vec<_> = m.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(names, vec!["Claude Sonnet 4.6", "Gemini 3.1 Pro (High)"]);
        assert_eq!(m[1].remaining, Some(0.6), "同名取剩得少的");
        assert_eq!(m[0].remaining, Some(0.0), "没有比例但有重置时刻 = 用光了");
        assert_eq!(m[0].tags, vec!["New".to_string()]);
    }

    #[test]
    fn google_error_reasons_are_codes_only() {
        let body = r#"{"error":{"code":403,"message":"echo of something","status":"PERMISSION_DENIED",
            "details":[{"reason":"SUBSCRIPTION_REQUIRED","domain":"cloudaicompanion.googleapis.com"}]}}"#;
        assert_eq!(
            google_reason(body).as_deref(),
            Some("SUBSCRIPTION_REQUIRED")
        );
        assert_eq!(google_reason("not json"), None);
        let note = summary_note(403, body, &None);
        assert!(note.contains("免费档"), "{note}");
        assert!(!note.contains("echo"), "原文不许贴到界面上：{note}");
    }

    #[test]
    fn user_agent_and_metadata_follow_the_official_ide_shape() {
        assert!(user_agent(Some("2.5.5")).starts_with("antigravity/2.5.5 windows/"));
        assert!(user_agent(None).starts_with("antigravity windows/"));
        let m = metadata(Some("2.5.5"));
        assert_eq!(m["ideType"], "ANTIGRAVITY");
        assert_eq!(m["ideVersion"], "2.5.5");
        assert!(metadata(None).get("ideVersion").is_none());
    }

    #[test]
    fn reading_the_last_result_never_needs_the_network() {
        // `refresh = false` 走的就是这一条：没问过就是 None，不是去问一次。
        let s = Source::Account("never-asked-4f2b".into());
        assert!(last(&s).is_none());
        let q = AntigravityOnlineQuota {
            tier_id: None,
            tier_name: None,
            gcp_tos: false,
            credits: None,
            windows: Vec::new(),
            windows_note: Some("x".into()),
            models: Vec::new(),
            fetched_at: "2026-09-23 07:00".into(),
        };
        remember(&s, &q);
        assert_eq!(last(&s), Some(q));
        forget(&s);
        assert!(last(&s).is_none());
    }

    /// Hub 里换了号：上一个号问到的档位与额度不许挂在新号底下。读不出来时说不清，照旧给。
    #[test]
    fn a_result_asked_for_another_account_is_not_shown() {
        assert!(same_account(Some("a@example.com"), Some("A@Example.com")));
        assert!(!same_account(Some("a@example.com"), Some("b@example.com")));
        assert!(same_account(None, Some("b@example.com")));
        assert!(same_account(Some("a@example.com"), None));
    }
}
