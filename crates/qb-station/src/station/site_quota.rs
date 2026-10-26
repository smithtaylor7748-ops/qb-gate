// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 站点的余额与编程套餐额度（中转站重构阶段 6 · U3，cc-switch 的 `services/balance.rs` + `services/coding_plan.rs`）。
//!
//! **纯函数**：按站点地址认出是哪一家、该问哪个地址带什么头，再把回复解析成 [`SiteQuota`]。
//! 发请求在编排层（`qb-app` 的 `usecase::site_quota_ops`）—— 这个 crate 连 `reqwest` 都没有。
//!
//! # 认得的
//!
//! | 类别 | 站点 | 问哪（都带这个站点那把 Key） |
//! |---|---|---|
//! | 余额 | DeepSeek | `GET api.deepseek.com/user/balance` |
//! | 余额 | 阶跃星辰（StepFun） | `GET api.stepfun.com/v1/accounts` |
//! | 余额 | 硅基流动（国内 / 国际） | `GET api.siliconflow.cn|com/v1/user/info` |
//! | 余额 | OpenRouter | `GET openrouter.ai/api/v1/credits` |
//! | 余额 | Novita AI | `GET api.novita.ai/v3/user/balance` |
//! | 套餐 | Kimi For Coding | `GET api.kimi.com/coding/v1/usages` |
//! | 套餐 | 智谱 GLM（bigmodel.cn / z.ai） | `GET <同一主机>/api/monitor/usage/quota/limit`（**不加** `Bearer`） |
//! | 套餐 | MiniMax（国内 / 国际） | `GET api.minimaxi.com|api.minimax.io/v1/api/openplatform/coding_plan/remains` |
//! | 套餐 | OpenCode Go | `GET opencode.ai/zen/go/v1/usage` |
//!
//! # ⛔ 只认主机名，不认子串
//!
//! cc-switch 大多按 `base_url.contains("api.deepseek.com")` 认。一个中转站要是把地址写成
//! `https://relay.example/api.deepseek.com/v1`，那样认会把**这个中转站的 Key** 发到 DeepSeek 去。
//! 这里只按 URL 的主机名认（[`host_and_path`]），Key 只发给它本来就配给的那一家
//! （MiniMax 国内新域名 `api.minimax.cn` 的额度接口在同一家的旧域名 `api.minimaxi.com` 上，是唯一的跨主机，同 cc-switch）。
//!
//! # 跟 cc-switch 不一样的
//!
//! - **读不出来的格子不给数。** cc-switch 的 Kimi 缺 `limit` 按 1、缺 `remaining` 按 0 算（= 已用 100%），
//!   StepFun / 硅基流动缺余额按 0 算；这里那一格（那一笔）直接不要，一格都没有就报「回复里没有……」。
//! - 没搬：ZenMux（要另填管理地址和管理 Key）、火山方舟（要账号 AK/SK、V4 签名 —— 那是比 API Key 大得多的凭证）、
//!   智谱团队版（要组织 / 项目 ID）、Command Code（四个未公开的 `/alpha` 路由，QB Gate 的预设里也没有它）。

use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

/// 余额还是套餐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SiteQuotaKind {
    Balance,
    Plan,
}

/// 一笔余额。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SiteBalance {
    /// 还剩多少。**可以是负数**（透支）；0 不代表 Key 失效。
    pub remaining: f64,
    /// 一共充了多少（只有 OpenRouter 给）。
    pub total: Option<f64>,
    /// 用掉多少（只有 OpenRouter 给）。
    pub used: Option<f64>,
    /// `CNY` / `USD`。
    pub unit: String,
}

/// 套餐的一个额度窗口。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SitePlanWindow {
    /// `five_hour` / `weekly` / `monthly`。
    pub key: String,
    /// 「5 小时」「每周」「每月」。
    pub label: String,
    /// 已用百分比 0–100。
    pub used_percent: f64,
    /// 重置时刻（RFC3339）；没给就是 `None`。
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SiteQuota {
    /// 认出来是哪一家（界面上的名字）。
    pub provider: String,
    pub kind: SiteQuotaKind,
    pub balances: Vec<SiteBalance>,
    pub windows: Vec<SitePlanWindow>,
    /// 套餐等级（智谱给的 `level`）。
    pub plan: Option<String>,
    /// 一句要紧的话（DeepSeek 说这个账户已不可用之类）。
    pub warning: Option<String>,
    /// 问的时刻（RFC3339，编排层填）。
    pub fetched_at: String,
    /// 用的是哪条线路的 Key（编排层填，界面上说「按哪一组的 Key 问的」）。
    pub route_id: String,
}

/// 认出来的那一家。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    DeepSeek,
    StepFun,
    SiliconFlowCn,
    SiliconFlowEn,
    OpenRouter,
    Novita,
    Kimi,
    ZhipuCn,
    ZhipuEn,
    MiniMaxCn,
    MiniMaxEn,
    OpencodeGo,
}

/// Key 怎么放进请求头。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyHeader {
    /// `Authorization: Bearer <key>`。
    Bearer,
    /// `Authorization: <key>`（智谱）。
    Raw,
}

/// 问什么、怎么问。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaRequest {
    pub url: &'static str,
    pub key_header: KeyHeader,
    pub headers: &'static [(&'static str, &'static str)],
}

/// URL 的主机名（小写、去端口）与路径。认不出就是 `None`。
pub fn host_and_path(base_url: &str) -> Option<(String, String)> {
    let rest = base_url.trim().split_once("://")?.1;
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    // 带用户名的写法（`user@host`）不认：没人这样配站点，认了反而可能被拿去骗。
    if authority.contains('@') {
        return None;
    }
    let host = authority
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(authority, |(h, _)| h)
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some((host, path.to_ascii_lowercase()))
}

fn is_or_under(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// 按站点地址认是哪一家。只认主机名（见模块头）。
pub fn detect(base_url: &str) -> Option<Detected> {
    let (host, path) = host_and_path(base_url)?;
    let d = match host.as_str() {
        "api.deepseek.com" => Detected::DeepSeek,
        "api.stepfun.com" | "api.stepfun.ai" => Detected::StepFun,
        "api.siliconflow.cn" => Detected::SiliconFlowCn,
        "api.siliconflow.com" => Detected::SiliconFlowEn,
        "openrouter.ai" | "api.openrouter.ai" => Detected::OpenRouter,
        "api.novita.ai" => Detected::Novita,
        "api.kimi.com" if path.starts_with("/coding") => Detected::Kimi,
        "api.z.ai" => Detected::ZhipuEn,
        "api.minimaxi.com" | "api.minimax.cn" => Detected::MiniMaxCn,
        "api.minimax.io" => Detected::MiniMaxEn,
        // Zen 按量版（`/zen/v1`）没有用量接口，刻意不认（同 cc-switch）。
        "opencode.ai" if path.starts_with("/zen/go") => Detected::OpencodeGo,
        h if is_or_under(h, "bigmodel.cn") => Detected::ZhipuCn,
        _ => return None,
    };
    Some(d)
}

impl Detected {
    pub fn name(self) -> &'static str {
        match self {
            Self::DeepSeek => "DeepSeek",
            Self::StepFun => "阶跃星辰",
            Self::SiliconFlowCn => "硅基流动",
            Self::SiliconFlowEn => "SiliconFlow（国际）",
            Self::OpenRouter => "OpenRouter",
            Self::Novita => "Novita AI",
            Self::Kimi => "Kimi For Coding",
            Self::ZhipuCn => "智谱 GLM 编程套餐",
            Self::ZhipuEn => "Z.ai GLM 编程套餐",
            Self::MiniMaxCn => "MiniMax 编程套餐",
            Self::MiniMaxEn => "MiniMax 编程套餐（国际）",
            Self::OpencodeGo => "OpenCode Go",
        }
    }

    pub fn kind(self) -> SiteQuotaKind {
        match self {
            Self::DeepSeek
            | Self::StepFun
            | Self::SiliconFlowCn
            | Self::SiliconFlowEn
            | Self::OpenRouter
            | Self::Novita => SiteQuotaKind::Balance,
            _ => SiteQuotaKind::Plan,
        }
    }

    pub fn request(self) -> QuotaRequest {
        const JSON: &[(&str, &str)] = &[("accept", "application/json")];
        const ZHIPU: &[(&str, &str)] = &[
            ("accept", "application/json"),
            ("accept-language", "en-US,en"),
        ];
        let (url, key_header, headers) = match self {
            Self::DeepSeek => (
                "https://api.deepseek.com/user/balance",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::StepFun => (
                "https://api.stepfun.com/v1/accounts",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::SiliconFlowCn => (
                "https://api.siliconflow.cn/v1/user/info",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::SiliconFlowEn => (
                "https://api.siliconflow.com/v1/user/info",
                KeyHeader::Bearer,
                JSON,
            ),
            // `/key` 里的 `limit_remaining` 是这把 Key 的花费上限，不是账户余额（同 cc-switch）。
            Self::OpenRouter => (
                "https://openrouter.ai/api/v1/credits",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::Novita => (
                "https://api.novita.ai/v3/user/balance",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::Kimi => (
                "https://api.kimi.com/coding/v1/usages",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::ZhipuCn => (
                "https://open.bigmodel.cn/api/monitor/usage/quota/limit",
                KeyHeader::Raw,
                ZHIPU,
            ),
            Self::ZhipuEn => (
                "https://api.z.ai/api/monitor/usage/quota/limit",
                KeyHeader::Raw,
                ZHIPU,
            ),
            // 额度接口只在 api.minimaxi.com 有公开出处；国内新推理域名 api.minimax.cn 沿用它（同一账号体系与 Key，同 cc-switch）。
            Self::MiniMaxCn => (
                "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
                KeyHeader::Bearer,
                JSON,
            ),
            Self::MiniMaxEn => (
                "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
                KeyHeader::Bearer,
                JSON,
            ),
            // 用量端点只认 `Authorization: Bearer`（推理那边只认 x-api-key，正好相反）。
            Self::OpencodeGo => (
                "https://opencode.ai/zen/go/v1/usage",
                KeyHeader::Bearer,
                JSON,
            ),
        };
        QuotaRequest {
            url,
            key_header,
            headers,
        }
    }

    /// 被 401 / 403 拒绝时该说的话。`None` = 按通用的说。
    pub fn rejected(self, status: u16) -> Option<&'static str> {
        match (self, status) {
            // 工作区的 Key 对 Zen 和 Go 是同一把：403 是「Key 有效、但没订 Go」，不是 Key 坏了。
            (Self::OpencodeGo, 403) => {
                Some("这把 Key 有效，但它所在的工作区没有 OpenCode Go 订阅（HTTP 403）")
            }
            _ => None,
        }
    }
}

/// 数字或数字字符串。
fn num(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|n: &f64| n.is_finite())
}

fn field(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(num)
}

/// 毫秒时间戳 → RFC3339。
fn millis_iso(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339())
}

/// 重置时刻：字符串原样收；数字按秒 / 毫秒认（`< 1e12` 是秒）；0 和负数是「没有」（火山的 -1，同 cc-switch）。
fn reset_time(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string()).filter(|s| !s.trim().is_empty());
    }
    let n = v.as_i64().filter(|n| *n > 0)?;
    millis_iso(if n < 1_000_000_000_000 { n * 1000 } else { n })
}

fn window(key: &str, used_percent: f64, resets_at: Option<String>) -> SitePlanWindow {
    SitePlanWindow {
        key: key.into(),
        label: match key {
            "five_hour" => "5 小时",
            "weekly" => "每周",
            "monthly" => "每月",
            other => other,
        }
        .into(),
        used_percent: used_percent.clamp(0.0, 100.0),
        resets_at,
    }
}

fn balance(remaining: f64, unit: &str) -> SiteBalance {
    SiteBalance {
        remaining,
        total: None,
        used: None,
        unit: unit.into(),
    }
}

/// 把回复解析成 [`SiteQuota`]（`fetched_at` / `route_id` 留空给编排层填）。认不出来就报一句能看懂的话。
pub fn parse(d: Detected, body: &Value) -> Result<SiteQuota, String> {
    let mut q = SiteQuota {
        provider: d.name().into(),
        kind: d.kind(),
        balances: Vec::new(),
        windows: Vec::new(),
        plan: None,
        warning: None,
        fetched_at: String::new(),
        route_id: String::new(),
    };
    match d {
        Detected::DeepSeek => {
            for info in body
                .get("balance_infos")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(total) = field(info, "total_balance") else {
                    continue;
                };
                let unit = info
                    .get("currency")
                    .and_then(Value::as_str)
                    .unwrap_or("CNY");
                q.balances.push(balance(total, unit));
            }
            if body.get("is_available").and_then(Value::as_bool) == Some(false) {
                q.warning = Some(
                    "DeepSeek 说这个账户现在不可用（is_available = false），多半是余额不足".into(),
                );
            }
        }
        Detected::StepFun => {
            if let Some(b) = field(body, "balance") {
                q.balances.push(balance(b, "CNY"));
            }
        }
        Detected::SiliconFlowCn | Detected::SiliconFlowEn => {
            let unit = if d == Detected::SiliconFlowCn {
                "CNY"
            } else {
                "USD"
            };
            if let Some(b) = body
                .get("data")
                .and_then(|data| field(data, "totalBalance"))
            {
                q.balances.push(balance(b, unit));
            }
        }
        Detected::OpenRouter => {
            let data = body.get("data").filter(|v| v.is_object()).unwrap_or(body);
            if let (Some(total), Some(used)) =
                (field(data, "total_credits"), field(data, "total_usage"))
            {
                let remaining = total - used;
                if remaining.is_finite() {
                    q.balances.push(SiteBalance {
                        remaining,
                        total: Some(total),
                        used: Some(used),
                        unit: "USD".into(),
                    });
                }
            }
        }
        Detected::Novita => {
            // 单位是 0.0001 美元。
            if let Some(b) = field(body, "availableBalance").map(|v| v / 10_000.0) {
                q.balances.push(balance(b, "USD"));
            }
        }
        Detected::Kimi => {
            // 有 `limit` 且大于 0、有 `remaining` 才算一格 —— 缺了就是不知道，不是用光了。
            let tier = |v: &Value| {
                let limit = field(v, "limit").filter(|l| *l > 0.0)?;
                let remaining = field(v, "remaining")?;
                Some((
                    ((limit - remaining).max(0.0) / limit) * 100.0,
                    v.get("resetTime").and_then(reset_time),
                ))
            };
            for item in body
                .get("limits")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some((used, reset)) = item.get("detail").and_then(tier) {
                    q.windows.push(window("five_hour", used, reset));
                    break;
                }
            }
            if let Some((used, reset)) = body.get("usage").and_then(tier) {
                q.windows.push(window("weekly", used, reset));
            }
        }
        Detected::ZhipuCn | Detected::ZhipuEn => {
            if body.get("success").and_then(Value::as_bool) == Some(false) {
                let msg = body
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("没说原因");
                return Err(format!("智谱回了业务错误：{}", short(msg)));
            }
            let data = body.get("data").ok_or("智谱的回复里没有 data")?;
            q.windows = zhipu_windows(data);
            q.plan = data
                .get("level")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        Detected::MiniMaxCn | Detected::MiniMaxEn => {
            if let Some(base) = body.get("base_resp") {
                let code = base
                    .get("status_code")
                    .and_then(Value::as_i64)
                    .unwrap_or(-1);
                if code != 0 {
                    let msg = base
                        .get("status_msg")
                        .and_then(Value::as_str)
                        .unwrap_or("没说原因");
                    return Err(format!("MiniMax 回了业务错误（{code}）：{}", short(msg)));
                }
            }
            q.windows = minimax_windows(body);
        }
        Detected::OpencodeGo => {
            for (k, key) in [
                ("rolling", "five_hour"),
                ("weekly", "weekly"),
                ("monthly", "monthly"),
            ] {
                let Some(w) = body.pointer(&format!("/usage/{k}")) else {
                    continue;
                };
                let Some(percent) = field(w, "percent") else {
                    continue;
                };
                // 用量为 0 时上游的 resetsAt 是「现在 + 窗口长度」的占位值，不显示倒计时（同 cc-switch）。
                let reset = (percent > 0.0)
                    .then(|| w.get("resetsAt").and_then(reset_time))
                    .flatten();
                q.windows.push(window(key, percent, reset));
            }
        }
    }
    match q.kind {
        SiteQuotaKind::Balance if q.balances.is_empty() => Err(format!(
            "{} 的回复里没有认得出的余额（接口可能改版了）",
            d.name()
        )),
        SiteQuotaKind::Plan if q.windows.is_empty() => Err(format!(
            "{} 的回复里没有认得出的套餐额度 —— 这把 Key 可能没订编程套餐，或者接口改版了",
            d.name()
        )),
        _ => Ok(q),
    }
}

/// 业务错误里的那句话：截短，不原样搬整段回复（可能带 HTML 或别的东西）。
fn short(s: &str) -> String {
    s.chars().take(80).collect()
}

/// 智谱 `data.limits[]` 里的 `TOKENS_LIMIT` / `CREDIT_LIMIT`：`unit` 3 是 5 小时、6 是每周（`number` 7 和 1 都见过，
/// 所以只认 `unit`）；`unit` 认不出的按重置时刻兜底（没有重置时刻的先归 5 小时）。不能光按重置时刻排 ——
/// 周期末尾每周那格会比 5 小时那格先重置（cc-switch #3036）。
fn zhipu_windows(data: &Value) -> Vec<SitePlanWindow> {
    type Entry = (Option<i64>, f64, Option<String>);
    let mut five: Option<Entry> = None;
    let mut week: Option<Entry> = None;
    let mut rest: Vec<Entry> = Vec::new();
    for item in data
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
        if !(kind.eq_ignore_ascii_case("TOKENS_LIMIT") || kind.eq_ignore_ascii_case("CREDIT_LIMIT"))
        {
            continue;
        }
        let Some(percent) = field(item, "percentage") else {
            continue;
        };
        let reset_ms = item.get("nextResetTime").and_then(Value::as_i64);
        let entry = (reset_ms, percent, reset_ms.and_then(millis_iso));
        match item.get("unit").and_then(Value::as_i64) {
            Some(3) if five.is_none() => five = Some(entry),
            Some(6) if week.is_none() => week = Some(entry),
            _ => rest.push(entry),
        }
    }
    rest.sort_by_key(|(reset, _, _)| (reset.is_some(), reset.unwrap_or(i64::MIN)));
    for entry in rest {
        if five.is_none() {
            five = Some(entry);
        } else if week.is_none() {
            week = Some(entry);
        }
    }
    [("five_hour", five), ("weekly", week)]
        .into_iter()
        .filter_map(|(key, e)| e.map(|(_, percent, reset)| window(key, percent, reset)))
        .collect()
}

/// MiniMax `model_remains[]` 里 `model_name == "general"` 那一条（编程套餐；video 那些跳过）。
/// 给的是**剩余**百分比，换成已用；每周那格只在 `current_weekly_status == 1` 时有（3 = 这档没有周限额）。
fn minimax_windows(body: &Value) -> Vec<SitePlanWindow> {
    let Some(item) = body
        .get("model_remains")
        .and_then(Value::as_array)
        .and_then(|all| {
            all.iter()
                .find(|i| i.get("model_name").and_then(Value::as_str) == Some("general"))
        })
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(left) = field(item, "current_interval_remaining_percent") {
        let reset = item
            .get("end_time")
            .and_then(Value::as_i64)
            .and_then(millis_iso);
        out.push(window("five_hour", 100.0 - left, reset));
    }
    if item.get("current_weekly_status").and_then(Value::as_i64) == Some(1) {
        if let Some(left) = field(item, "current_weekly_remaining_percent") {
            let reset = item
                .get("weekly_end_time")
                .and_then(Value::as_i64)
                .and_then(millis_iso);
            out.push(window("weekly", 100.0 - left, reset));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_host_decides_never_a_substring_in_the_path() {
        assert_eq!(
            detect("https://api.deepseek.com/anthropic"),
            Some(Detected::DeepSeek)
        );
        assert_eq!(
            detect("https://API.DeepSeek.com:443/v1"),
            Some(Detected::DeepSeek)
        );
        // 中转站把官方域名写进路径：不认 —— 认了就会把中转站的 Key 发到 DeepSeek 去。
        assert_eq!(detect("https://relay.example/api.deepseek.com/v1"), None);
        assert_eq!(detect("https://api.deepseek.com.evil.example/v1"), None);
        assert_eq!(detect("https://user@api.deepseek.com/v1"), None);
        assert_eq!(
            detect("https://open.bigmodel.cn/api/anthropic"),
            Some(Detected::ZhipuCn)
        );
        assert_eq!(detect("https://bigmodel.cn.example/api"), None);
        assert_eq!(
            detect("https://api.z.ai/api/anthropic"),
            Some(Detected::ZhipuEn)
        );
        assert_eq!(detect("https://api.kimi.com/coding/"), Some(Detected::Kimi));
        assert_eq!(
            detect("https://api.kimi.com/v1"),
            None,
            "Kimi 按量的不是编程套餐"
        );
        assert_eq!(
            detect("https://api.minimax.cn/anthropic"),
            Some(Detected::MiniMaxCn)
        );
        assert_eq!(
            detect("https://api.minimax.io/v1"),
            Some(Detected::MiniMaxEn)
        );
        assert_eq!(
            detect("https://opencode.ai/zen/go/v1"),
            Some(Detected::OpencodeGo)
        );
        assert_eq!(detect("https://opencode.ai/zen/v1"), None);
        assert_eq!(
            detect("https://api.siliconflow.com/v1"),
            Some(Detected::SiliconFlowEn)
        );
        assert_eq!(detect("not a url"), None);
        assert_eq!(detect("https://my-new-api.example/v1"), None);
    }

    /// Key 只发给那一家自己的主机：每个问额度的地址拿来当站点地址，认出来的还是同一家。
    #[test]
    fn every_quota_request_goes_to_a_host_of_the_same_provider() {
        for d in [
            Detected::DeepSeek,
            Detected::StepFun,
            Detected::SiliconFlowCn,
            Detected::SiliconFlowEn,
            Detected::OpenRouter,
            Detected::Novita,
            Detected::Kimi,
            Detected::ZhipuCn,
            Detected::ZhipuEn,
            Detected::MiniMaxCn,
            Detected::MiniMaxEn,
            Detected::OpencodeGo,
        ] {
            let url = d.request().url;
            assert!(url.starts_with("https://"), "{url}");
            assert_eq!(detect(url), Some(d), "{url}");
        }
        assert_eq!(Detected::ZhipuCn.request().key_header, KeyHeader::Raw);
    }

    #[test]
    fn balances_keep_negative_and_zero_but_drop_what_was_not_given() {
        let q = parse(
            Detected::DeepSeek,
            &json!({"is_available": false, "balance_infos": [
                {"currency": "CNY", "total_balance": "0.00"},
                {"currency": "USD"},
                {"currency": "USD", "total_balance": 1.5}
            ]}),
        )
        .unwrap();
        assert_eq!(q.balances.len(), 2, "没给余额的那一笔不要");
        assert_eq!(q.balances[0].remaining, 0.0);
        assert_eq!(q.balances[1].unit, "USD");
        assert!(q.warning.is_some());

        let q = parse(
            Detected::OpenRouter,
            &json!({"data": {"total_credits": 100, "total_usage": "105"}}),
        )
        .unwrap();
        assert_eq!(q.balances[0].remaining, -5.0);
        assert_eq!(q.balances[0].total, Some(100.0));

        let q = parse(Detected::Novita, &json!({"availableBalance": "255000"})).unwrap();
        assert_eq!(q.balances[0].remaining, 25.5);

        let q = parse(
            Detected::SiliconFlowEn,
            &json!({"data": {"totalBalance": "3.2"}}),
        )
        .unwrap();
        assert_eq!(
            (q.balances[0].remaining, q.balances[0].unit.as_str()),
            (3.2, "USD")
        );

        // cc-switch 在这些情况下按 0 显示；这里报错，不给数。
        for (d, body) in [
            (Detected::StepFun, json!({"object": "account"})),
            (
                Detected::SiliconFlowCn,
                json!({"data": {"status": "normal"}}),
            ),
            (
                Detected::OpenRouter,
                json!({"data": {"total_credits": 100}}),
            ),
            (
                Detected::OpenRouter,
                json!({"data": {"total_credits": "NaN", "total_usage": 0}}),
            ),
            (Detected::Novita, json!({"cashBalance": 1})),
            (Detected::DeepSeek, json!({"balance_infos": []})),
        ] {
            assert!(parse(d, &body).is_err(), "{d:?} {body}");
        }
    }

    #[test]
    fn kimi_windows_need_both_limit_and_remaining() {
        let q = parse(
            Detected::Kimi,
            &json!({
                "limits": [{"detail": {"limit": 200, "remaining": 150, "resetTime": "2026-10-26T12:00:00Z"}}],
                "usage": {"limit": "1000", "remaining": "100", "resetTime": 1793000000}
            }),
        )
        .unwrap();
        assert_eq!(q.windows.len(), 2);
        assert_eq!(q.windows[0].label, "5 小时");
        assert_eq!(q.windows[0].used_percent, 25.0);
        assert_eq!(q.windows[1].used_percent, 90.0);
        assert!(q.windows[1]
            .resets_at
            .as_deref()
            .unwrap()
            .starts_with("2026-"));
        // 缺 remaining / limit 为 0：那一格不要（cc-switch 会画成用光）。
        let err = parse(
            Detected::Kimi,
            &json!({"limits": [{"detail": {"limit": 200}}], "usage": {"limit": 0, "remaining": 0}}),
        )
        .unwrap_err();
        assert!(err.contains("没有认得出的套餐额度"));
    }

    #[test]
    fn zhipu_windows_go_by_unit_and_fall_back_to_reset_order() {
        let q = parse(
            Detected::ZhipuCn,
            &json!({"success": true, "data": {"level": "pro", "limits": [
                {"type": "TOKENS_LIMIT", "unit": 6, "number": 7, "percentage": 40, "nextResetTime": 1793000000000_i64},
                {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 12, "nextResetTime": 1793100000000_i64},
                {"type": "TIME_LIMIT", "unit": 5, "percentage": 99}
            ]}}),
        )
        .unwrap();
        // 每周那格先重置也不会被标成 5 小时（按 unit 认，不按重置时刻排）。
        assert_eq!(q.windows[0].key, "five_hour");
        assert_eq!(q.windows[0].used_percent, 12.0);
        assert_eq!(q.windows[1].key, "weekly");
        assert_eq!(q.plan.as_deref(), Some("pro"));

        let q = parse(
            Detected::ZhipuEn,
            &json!({"data": {"limits": [
                {"type": "tokens_limit", "percentage": 5, "nextResetTime": 1793100000000_i64},
                {"type": "TOKENS_LIMIT", "percentage": 50}
            ]}}),
        )
        .unwrap();
        assert_eq!(q.windows[0].used_percent, 50.0, "没有重置时刻的先归 5 小时");
        assert_eq!(q.windows[1].used_percent, 5.0);

        let err = parse(
            Detected::ZhipuCn,
            &json!({"success": false, "msg": "令牌已过期"}),
        )
        .unwrap_err();
        assert!(err.contains("令牌已过期"));
    }

    #[test]
    fn minimax_flips_remaining_to_used_and_skips_absent_weekly() {
        let body = json!({"base_resp": {"status_code": 0}, "model_remains": [
            {"model_name": "video", "current_interval_remaining_percent": 1},
            {"model_name": "general", "current_interval_remaining_percent": 70, "end_time": 1793000000000_i64,
             "current_weekly_status": 3, "current_weekly_remaining_percent": 100}
        ]});
        let q = parse(Detected::MiniMaxCn, &body).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].used_percent, 30.0);
        let err = parse(
            Detected::MiniMaxEn,
            &json!({"base_resp": {"status_code": 1004, "status_msg": "invalid key"}}),
        )
        .unwrap_err();
        assert!(err.contains("1004"));
    }

    #[test]
    fn opencode_go_hides_the_placeholder_reset_of_an_idle_window() {
        let q = parse(
            Detected::OpencodeGo,
            &json!({"usage": {
                "rolling": {"status": "ok", "percent": 0, "resetsAt": "2026-10-26T05:00:00Z"},
                "weekly": {"status": "ok", "percent": 37, "resetsAt": "2026-10-30T00:00:00Z"},
                "monthly": {"percent": "x"}
            }}),
        )
        .unwrap();
        assert_eq!(q.windows.len(), 2);
        assert_eq!(q.windows[0].resets_at, None);
        assert_eq!(
            q.windows[1].resets_at.as_deref(),
            Some("2026-10-30T00:00:00Z")
        );
        assert!(Detected::OpencodeGo.rejected(403).is_some());
        assert!(Detected::Kimi.rejected(403).is_none());
    }
}
