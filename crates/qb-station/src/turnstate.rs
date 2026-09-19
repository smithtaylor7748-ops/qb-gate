//! Codex `X-Codex-Turn-State` 信封的**外形**解析 + active/ready 状态机。
//! **纯函数 / 纯数据,不取系统时间、不联网、不落盘、不解密、不验签。**
//!
//! # 这是什么
//!
//! `X-Codex-Turn-State` 是 OpenAI Codex 服务器在响应头里发回、期望下一轮带上的
//! 一张加密不透明「纸条」。本模块**只读它的外形**:base64url 解开后头字节应为 `0x80`,
//! 紧跟 8 字节大端签发时间,其余按 16 字节一块。个人号约 10 块 / 292 字符,
//! Team 约 12 块 / 332 字符。
//!
//! # ⛔ 外形不是质量,更不是额度
//!
//! 长度**只是经验筛选口径**:符合不代表模型「满血」,不符合也不能断定降智或代理坏了。
//! 它既不解密也不验 HMAC,拿到一张 292 不证明任何服务端事实。这一条必须在界面上照写。
//!
//! # ⛔ 只对 Codex 有意义
//!
//! 这是 OpenAI Codex 的请求头。Anthropic Claude 协议里**没有对应物** ——
//! 本模块只被 `Client::Codex` 的路径使用,Claude Code / Claude 桌面端一律不碰。
//!
//! # active / ready 状态机
//!
//! 响应头只做**观测**,不能直接改 active(否则同一发请求前后可能拿到不同来源的状态,
//! 排查困难)。合格候选先进 ready,不抢正在用的 active;active 失效、连续两次「形状变了」,
//! 或临近续补窗口且 ready 更新,才晋升。飞行中的请求各自拿的是不可变快照。
//!
//! # ⛔ clean-room
//!
//! 信封外形与状态机口径参照 ccodex-sleep-state(GPL-3.0)公开的 README / SECURITY 描述,
//! **用 Rust 逐行自己写**,未复制其源码,未引入 Mihomo 或任何代理出口。见 `ATTRIBUTION.md`。

use serde::Serialize;
use ts_rs::TS;

/// turn-state 请求头名(小写,与 reqwest 归一后的头名一致)。
pub const HEADER: &str = "x-codex-turn-state";

/// 账号规则。**只决定期望块数,不是账号身份的证明。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    /// 个人:10 块,约 292 字符。
    Personal,
    /// Team / Business:12 块,约 332 字符。
    Team,
}

impl AccountKind {
    pub fn blocks(self) -> usize {
        match self {
            AccountKind::Personal => 10,
            AccountKind::Team => 12,
        }
    }

    /// 这个规则下 base64（含 padding）的期望字符长度 —— 只用于界面提示。
    /// 10 块 → 292,12 块 → 332。
    pub fn expected_length(self) -> usize {
        let raw = 57 + 16 * self.blocks();
        raw.div_ceil(3) * 4
    }
}

/// 默认 TTL / 续补窗口（毫秒）。经验值,可被配置覆盖。
pub const DEFAULT_TTL_MS: i64 = 3_600_000;
pub const DEFAULT_REFRESH_MS: i64 = 1_200_000;

/// 采纳一张 turn-state 的规则。
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub blocks: usize,
    pub ttl_ms: i64,
    pub refresh_ms: i64,
}

impl Policy {
    pub fn for_kind(kind: AccountKind) -> Self {
        Self {
            blocks: kind.blocks(),
            ttl_ms: DEFAULT_TTL_MS,
            refresh_ms: DEFAULT_REFRESH_MS,
        }
    }

    pub fn personal() -> Self {
        Self::for_kind(AccountKind::Personal)
    }

    pub fn team() -> Self {
        Self::for_kind(AccountKind::Team)
    }

    /// 这张纸条此刻能不能用。块数要对得上,签发时间不能是未来(留 30s 容差),
    /// 也不能进入最后 30 秒的续补窗口。
    pub fn accept(&self, shape: &Shape, now_ms: i64) -> bool {
        !shape.value.is_empty()
            && shape.blocks == self.blocks
            && shape.issued_ms <= now_ms + 30_000
            && now_ms < shape.issued_ms + self.ttl_ms - 30_000
    }
}

/// 一张 turn-state 的外形。**`value` 是敏感值,绝不发给前端。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub value: String,
    /// 稳定短标识,只用于去重与配对,不做安全用途。
    pub fingerprint: String,
    pub issued_ms: i64,
    pub blocks: usize,
}

/// 解析 turn-state 头的外形。认不出返回 `None`,**绝不猜**。
pub fn parse(value: &str) -> Option<Shape> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return None;
    }
    // 内部不允许有空白 —— 一个合法的信封是连续的 base64url。
    if trimmed
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
    {
        return None;
    }
    let core = trimmed.trim_end_matches('=');
    if trimmed.len() - core.len() > 2 {
        return None;
    }
    let raw = decode_base64url(core)?;
    // 头字节 0x80、至少 73 字节、去掉 57 字节固定头之后按 16 字节整块。
    if raw.len() < 73 || raw[0] != 0x80 || (raw.len() - 57) % 16 != 0 {
        return None;
    }
    let issued = u64::from_be_bytes(raw[1..9].try_into().ok()?);
    // 2020-01-01 ~ 2100-01-01,挡住明显不合理的时间戳。
    if !(1_577_836_800..4_102_444_800).contains(&issued) {
        return None;
    }
    Some(Shape {
        fingerprint: fingerprint(trimmed),
        value: trimmed.to_string(),
        issued_ms: (issued as i64).saturating_mul(1000),
        blocks: (raw.len() - 57) / 16,
    })
}

/// 注入时交给路由的一份不可变快照:要塞的值 + 用来和 `observe` 配对的身份。
#[derive(Debug, Clone)]
pub struct Injection {
    pub value: String,
    pub version: u64,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
struct Snapshot {
    shape: Shape,
    version: u64,
}

/// turn-state 的当前值 / 备用值状态机。**一个 (凭证, 模型) 一份。**
#[derive(Debug, Clone)]
pub struct Store {
    policy: Policy,
    active: Option<Snapshot>,
    ready: Option<Snapshot>,
    version: u64,
    strikes: u32,
    observations: u64,
}

impl Store {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            active: None,
            ready: None,
            version: 0,
            strikes: 0,
            observations: 0,
        }
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    fn active_ok(&self, now_ms: i64) -> bool {
        self.active
            .as_ref()
            .is_some_and(|s| self.policy.accept(&s.shape, now_ms))
    }

    fn ready_ok(&self, now_ms: i64) -> bool {
        self.ready
            .as_ref()
            .is_some_and(|s| self.policy.accept(&s.shape, now_ms))
    }

    /// 该不该把 ready 顶成 active。
    fn promote(&mut self, now_ms: i64) {
        if !self.ready_ok(now_ms) {
            return;
        }
        let ready = self.ready.as_ref().unwrap();
        // 已经是同一张,不用动。
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.shape.fingerprint == ready.shape.fingerprint)
        {
            return;
        }
        let active_ok = self.active_ok(now_ms);
        let stale = self.active.as_ref().is_some_and(|a| {
            now_ms + self.policy.refresh_ms > a.shape.issued_ms + self.policy.ttl_ms
                && ready.shape.issued_ms > a.shape.issued_ms
        });
        if !active_ok || self.strikes >= 2 || stale {
            self.version += 1;
            let mut promoted = self.ready.take().unwrap();
            promoted.version = self.version;
            self.active = Some(promoted);
            self.strikes = 0;
        }
    }

    /// 取当前可注入的一份。取不到（没有 / 已失效）返回 `None`。
    pub fn acquire(&mut self, now_ms: i64) -> Option<Injection> {
        self.promote(now_ms);
        let s = self.active.as_ref()?;
        if self.policy.accept(&s.shape, now_ms) {
            Some(Injection {
                value: s.shape.value.clone(),
                version: s.version,
                fingerprint: s.shape.fingerprint.clone(),
            })
        } else {
            None
        }
    }

    /// 采集到一张候选。返回是否被接纳（形状合格）。
    pub fn offer(&mut self, shape: Shape, now_ms: i64) -> bool {
        self.observations += 1;
        if !self.policy.accept(&shape, now_ms) {
            return false;
        }
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.shape.fingerprint == shape.fingerprint)
        {
            return true;
        }
        if !self.active_ok(now_ms) {
            // 没有可用 active,合格候选直接上位。
            self.version += 1;
            self.active = Some(Snapshot {
                shape,
                version: self.version,
            });
            self.strikes = 0;
            return true;
        }
        // active 还健康:更新的候选留在 ready,不抢正在工作的主值。
        if self
            .ready
            .as_ref()
            .is_none_or(|r| shape.issued_ms >= r.shape.issued_ms)
        {
            self.ready = Some(Snapshot { shape, version: 0 });
        }
        self.promote(now_ms);
        true
    }

    /// 观测一发真实请求回来的 turn-state 头。**只计观测与 strikes,绝不直接改 active。**
    /// 返回这张是否「可疑」（缺失或形状变了）。
    pub fn observe(&mut self, value: &str, used: &Injection, now_ms: i64) -> bool {
        if value.trim().is_empty() {
            // 没带回状态头 —— 保留现状,不把「没有信息」硬说成失败。
            return false;
        }
        self.observations += 1;
        let suspect = parse(value).is_none_or(|s| !self.policy.accept(&s, now_ms));
        if let Some(active) = self.active.as_ref() {
            if active.version == used.version && active.shape.fingerprint == used.fingerprint {
                if suspect {
                    self.strikes += 1;
                } else {
                    self.strikes = 0;
                }
            }
        }
        suspect
    }

    /// 该不该再采集一轮。
    pub fn needs_refresh(&mut self, now_ms: i64) -> bool {
        self.promote(now_ms);
        if !self.active_ok(now_ms) || self.strikes >= 2 {
            return true;
        }
        let a = self.active.as_ref().unwrap();
        now_ms + self.policy.refresh_ms > a.shape.issued_ms + self.policy.ttl_ms
    }

    pub fn status(&self, now_ms: i64) -> Status {
        let (version, blocks, length, remaining) = match &self.active {
            Some(a) => {
                let remaining = ((a.shape.issued_ms + self.policy.ttl_ms - now_ms) / 1000).max(0);
                (a.version, a.shape.blocks, a.shape.value.len(), remaining)
            }
            None => (0, 0, 0, 0),
        };
        Status {
            usable: self.active_ok(now_ms),
            version,
            blocks,
            length,
            remaining_seconds: remaining,
            ready: self.ready_ok(now_ms),
            strikes: self.strikes,
            observations: self.observations,
        }
    }
}

/// 一个 model 的 turn-state 状态,给界面按 model 列出来。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "TurnStateModelStatus")]
pub struct ModelStatus {
    pub model: String,
    pub status: Status,
}

/// 发给界面的状态。**不含 turn-state 的值本身**,只有外形与计数。
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export, rename = "TurnStateStatus")]
pub struct Status {
    /// 现在有没有可用的 active。
    pub usable: bool,
    pub version: u64,
    /// active 的块数,0 = 没有。
    pub blocks: usize,
    /// active 的字符长度,0 = 没有。
    pub length: usize,
    pub remaining_seconds: i64,
    /// 备用是否就绪。
    pub ready: bool,
    pub strikes: u32,
    /// 观测过多少次（含采集与真实响应）。
    pub observations: u64,
}

/// 稳定的非加密短指纹（FNV-1a 64）。只用于去重 / 配对,不做安全用途,故不引 sha2。
fn fingerprint(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in value.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 最小 base64url 解码（无 padding；调用方已剥掉 `=`）。认不出返回 `None`。
/// 自己写,不引依赖 —— 只用来读信封外形,输入至多 2KB。
fn decode_base64url(s: &str) -> Option<Vec<u8>> {
    fn sextet(b: u8) -> Option<u32> {
        Some(match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        })
    }
    let bytes = s.as_bytes();
    // n % 4 == 1 在 base64 里是不可能的长度。
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        acc = (acc << 6) | sextet(b)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000; // 2023-11-14 左右,毫秒

    fn encode_base64url(raw: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in raw.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            let chars = chunk.len() + 1;
            for i in 0..chars {
                out.push(A[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            }
        }
        out
    }

    /// 造一张给定签发秒数、给定块数的 turn-state。
    fn token(issued_s: u64, blocks: usize) -> String {
        let mut raw = vec![0u8; 57 + 16 * blocks];
        raw[0] = 0x80;
        raw[1..9].copy_from_slice(&issued_s.to_be_bytes());
        encode_base64url(&raw)
    }

    #[test]
    fn a_personal_token_parses_as_ten_blocks() {
        let s = parse(&token(1_700_000_000, 10)).expect("应认得个人 token");
        assert_eq!(s.blocks, 10);
        assert_eq!(s.issued_ms, 1_700_000_000_000);
    }

    #[test]
    fn a_team_token_parses_as_twelve_blocks() {
        let s = parse(&token(1_700_000_000, 12)).expect("应认得 Team token");
        assert_eq!(s.blocks, 12);
    }

    #[test]
    fn padding_is_tolerated() {
        let raw = token(1_700_000_000, 10);
        assert!(parse(&format!("{raw}==")).is_some());
    }

    #[test]
    fn garbage_and_wrong_prefix_are_rejected() {
        assert!(parse("not-a-token").is_none());
        assert!(parse("").is_none());
        // 头字节不是 0x80。
        let mut raw = vec![0u8; 57 + 16 * 10];
        raw[0] = 0x7f;
        assert!(parse(&encode_base64url(&raw)).is_none());
        // 内部有空格。
        assert!(parse("AA BB").is_none());
    }

    #[test]
    fn expected_lengths_are_292_and_332() {
        assert_eq!(AccountKind::Personal.expected_length(), 292);
        assert_eq!(AccountKind::Team.expected_length(), 332);
    }

    #[test]
    fn policy_rejects_wrong_block_count_and_expiry() {
        let personal = Policy::personal();
        let team_shape = parse(&token(1_700_000_000, 12)).unwrap();
        // 12 块的 token 不满足个人（10 块）规则。
        assert!(!personal.accept(&team_shape, T0));

        let shape = parse(&token(1_700_000_000, 10)).unwrap();
        assert!(personal.accept(&shape, T0));
        // 过期。
        assert!(!personal.accept(&shape, shape.issued_ms + DEFAULT_TTL_MS));
        // 签发时间在未来太多。
        assert!(!personal.accept(&shape, shape.issued_ms - 60_000));
    }

    #[test]
    fn offer_publishes_then_acquire_returns_it() {
        let mut store = Store::new(Policy::personal());
        let shape = parse(&token(1_700_000_000, 10)).unwrap();
        let value = shape.value.clone();
        assert!(store.offer(shape, T0));
        let inj = store.acquire(T0).expect("应能取到刚采纳的");
        assert_eq!(inj.value, value);
        assert_eq!(store.status(T0).blocks, 10);
        assert!(store.status(T0).usable);
    }

    #[test]
    fn a_shape_mismatch_is_not_accepted() {
        let mut store = Store::new(Policy::personal());
        let team = parse(&token(1_700_000_000, 12)).unwrap();
        assert!(!store.offer(team, T0), "12 块不该被个人规则接纳");
        assert!(store.acquire(T0).is_none());
    }

    #[test]
    fn a_healthy_active_is_not_replaced_by_a_new_candidate() {
        // 两张都签发在 now 之前,后者更新。now 取两者都已生效的时刻。
        let now = 1_700_000_200_000; // 1_700_000_200s,两张 token 都早于它
        let mut store = Store::new(Policy::personal());
        let first = parse(&token(1_700_000_000, 10)).unwrap();
        let first_value = first.value.clone();
        store.offer(first, now);
        // 更新的候选进来,active 还健康 —— 不抢。
        let second = parse(&token(1_700_000_100, 10)).unwrap();
        store.offer(second, now);
        assert_eq!(store.acquire(now).unwrap().value, first_value);
        assert!(store.status(now).ready, "更新的应留在 ready");
    }

    #[test]
    fn two_suspect_observations_force_a_refresh() {
        let mut store = Store::new(Policy::personal());
        let shape = parse(&token(1_700_000_000, 10)).unwrap();
        store.offer(shape, T0);
        let used = store.acquire(T0).unwrap();
        assert!(!store.needs_refresh(T0));
        // 连续两次「形状变了」。
        store.observe("garbage", &used, T0);
        store.observe("garbage", &used, T0);
        assert!(store.needs_refresh(T0), "两次可疑观测后应要求重采");
    }

    #[test]
    fn an_empty_observation_is_not_suspect() {
        let mut store = Store::new(Policy::personal());
        let shape = parse(&token(1_700_000_000, 10)).unwrap();
        store.offer(shape, T0);
        let used = store.acquire(T0).unwrap();
        assert!(!store.observe("", &used, T0), "没带状态头不算可疑");
        assert!(!store.needs_refresh(T0));
    }
}
