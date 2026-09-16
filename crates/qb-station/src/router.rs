//! 本机路由的**纯状态**：这一发请求该打哪条线,以及换上游怎么换。
//!
//! 真正监听端口、转发字节的那一层不在这里 —— 这个 crate 连 `reqwest`
//! 都没有依赖,物理上收发不了任何东西。这里只回答两个问题:
//!
//! 1. 下一发请求绑哪条线;
//! 2. 调度器要求换上游时,已经在飞的请求会不会被影响。
//!
//! # ⛔ 只在两次请求之间切换
//!
//! 换上游**绝不许影响正在飞的请求**。中途把上游换掉,那一发请求会拿着
//! A 站的上下文去打 B 站:轻则报错,重则把半截对话计到另一家的账上,
//! 而使用者看到的是「莫名其妙断了」。
//!
//! 做法是 [`begin`](UpstreamSwitch::begin) 把线路 id **交出去一份拷贝**。
//! 请求全程拿着自己那一份,之后 `current` 怎么变都与它无关 ——
//! 这不是靠自觉遵守,是所有权本身就做不到别的。
//!
//! # 本机路由的意义是「启一次」
//!
//! 每个客户端页顶上只有一处能启动。客户端连的是
//! `127.0.0.1`,之后换上游不需要重启客户端 —— 换的是这里的 `current`,
//! 客户端那一头的地址一个字都不用动。

use qb_contract::domain::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// 上游选择。**换上游只在两次请求之间生效。**
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpstreamSwitch {
    current: Option<String>,
    /// 已经排好队、等下一发请求生效的那条。
    pending: Option<String>,
    in_flight: u32,
    /// 换过几次。迟滞是不是真的在起作用,看这个数 ——
    /// 实测没有迟滞时 200 发请求换了 53 次。
    switches: u64,
}

impl UpstreamSwitch {
    pub fn new() -> Self {
        Self::default()
    }

    /// 现在对外宣称用的是哪条线。**这不是在飞的请求用的那条** ——
    /// 那一条在请求自己手里。
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// 排队等生效的那条。`None` = 没有待切换的。
    pub fn pending(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    pub fn in_flight(&self) -> u32 {
        self.in_flight
    }

    pub fn switches(&self) -> u64 {
        self.switches
    }

    /// 调度器要求换上游。
    ///
    /// **不动 `current`** —— 只排队。换成当前这条等于取消排队。
    pub fn request_switch(&mut self, route_id: impl Into<String>) {
        let next = route_id.into();
        if Some(next.as_str()) == self.current.as_deref() {
            self.pending = None;
        } else {
            self.pending = Some(next);
        }
    }

    /// 一发请求开始。返回它这一趟要用的线路 id ——
    /// **拿到的是一份拷贝,之后换上游与它无关。**
    ///
    /// 排队中的切换在这一刻生效:两次请求之间,正是这里。
    pub fn begin(&mut self) -> Option<String> {
        if let Some(next) = self.pending.take() {
            self.current = Some(next);
            self.switches += 1;
        }
        self.in_flight += 1;
        self.current.clone()
    }

    /// 一发请求结束。
    pub fn end(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// 直接落定一条线,不排队。
    ///
    /// 只给「第一次选上游」和「使用者手动指定」用 —— 调度器**不许**走这里,
    /// 它必须走 [`request_switch`](Self::request_switch)。
    pub fn set_now(&mut self, route_id: impl Into<String>) {
        let next = route_id.into();
        if self.current.as_deref() != Some(next.as_str()) {
            self.switches += 1;
        }
        self.current = Some(next);
        self.pending = None;
    }
}

/// 按软件分开的上游选择。**每个软件各记各的当前上游。**
///
/// # 为什么不能只有一个 `current`
///
/// 三个软件（Claude Code / Claude 桌面端 / Codex）同时连着同一个本机路由。
/// 只留一份当前上游的话，在 Codex 页上换一条线会把 Claude Code 正在用的那条
/// **一起换掉** —— 而使用者完全看不出是自己刚才那一下干的：他明明在另一个
/// 分页上操作。症状会表现成「Claude Code 莫名其妙换站了」。
///
/// 每个软件一份 [`UpstreamSwitch`]，所以「换上游只在两次请求之间生效」
/// 那条不变量对每个软件**各自**成立，互不干扰。
///
/// # 没登记过的软件返回 `None`，不回落到别人的上游
///
/// 回落会让一个从没选过上游的软件**悄悄用上别人的 Key**，账记在别人头上。
/// 如实返回 `None`，转发层据此回一句「还没选上游」。
#[derive(Debug, Default)]
pub struct ClientRouter {
    per_client: BTreeMap<Client, UpstreamSwitch>,
}

impl ClientRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 这个软件当前对外宣称用的是哪条线。
    pub fn current(&self, client: Client) -> Option<&str> {
        self.per_client.get(&client)?.current()
    }

    /// 这个软件排队等生效的那条。
    pub fn pending(&self, client: Client) -> Option<&str> {
        self.per_client.get(&client)?.pending()
    }

    /// 三个软件各自的当前上游。界面上「正在路由」按软件分别显示要用它。
    pub fn currents(&self) -> BTreeMap<Client, String> {
        self.per_client
            .iter()
            .filter_map(|(c, s)| s.current().map(|id| (*c, id.to_string())))
            .collect()
    }

    /// 全部软件加起来换过几次手。迟滞有没有起作用看这个数。
    pub fn switches(&self) -> u64 {
        self.per_client.values().map(UpstreamSwitch::switches).sum()
    }

    /// 此刻总共有几发请求在飞。
    pub fn in_flight(&self) -> u32 {
        self.per_client
            .values()
            .map(UpstreamSwitch::in_flight)
            .sum()
    }

    /// 调度器要求给这个软件换上游。**只排队，不动 `current`。**
    pub fn request_switch(&mut self, client: Client, route_id: impl Into<String>) {
        self.per_client
            .entry(client)
            .or_default()
            .request_switch(route_id);
    }

    /// 使用者手动指定，或第一次选上游。
    pub fn set_now(&mut self, client: Client, route_id: impl Into<String>) {
        self.per_client.entry(client).or_default().set_now(route_id);
    }

    /// 这个软件的一发请求开始。返回它这一趟要用的线路 id（一份拷贝）。
    ///
    /// **从没登记过的软件返回 `None`** —— 不建条目、不回落到别人的上游。
    pub fn begin(&mut self, client: Client) -> Option<String> {
        self.per_client.get_mut(&client)?.begin()
    }

    /// 这个软件的一发请求结束。
    pub fn end(&mut self, client: Client) {
        if let Some(s) = self.per_client.get_mut(&client) {
            s.end();
        }
    }
}

/// 一条请求日志。
///
/// **每一项都可能取不到** —— 上游没回、连接断在半路都是常态,
/// 取不到就是 `None`,不拿 0 顶上。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RequestLog {
    pub id: String,
    pub at_ms: i64,
    /// 打的是哪条线。**是请求开始时绑定的那条**,不是现在的 `current`。
    pub route_id: String,
    /// 哪个客户端发来的。
    pub client: String,
    pub model: String,
    pub status: Option<u16>,
    pub first_token_ms: Option<u64>,
    pub total_ms: Option<u64>,
    /// 上游的 `Retry-After`,已经换算成毫秒。
    pub retry_after_ms: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::Outcome;

    #[test]
    fn a_switch_never_disturbs_a_request_that_is_already_in_flight() {
        // 中途换掉,那一发请求会拿着 A 站的上下文去打 B 站。
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        let mine = s.begin();
        assert_eq!(mine.as_deref(), Some("A"));

        s.request_switch("B");
        // 我手里那一份没变,而且 current 也还没变 —— 切换在排队。
        assert_eq!(mine.as_deref(), Some("A"));
        assert_eq!(s.current(), Some("A"));
        assert_eq!(s.pending(), Some("B"));

        s.end();
        // 下一发才换。
        assert_eq!(s.begin().as_deref(), Some("B"));
        assert_eq!(s.current(), Some("B"));
        assert_eq!(s.pending(), None);
    }

    #[test]
    fn several_in_flight_requests_each_keep_their_own_upstream() {
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        let a1 = s.begin();
        let a2 = s.begin();
        s.request_switch("B");
        let a3 = s.begin(); // 前两发还在飞,但这一发是新的,换到 B
        assert_eq!(
            (a1.as_deref(), a2.as_deref(), a3.as_deref()),
            (Some("A"), Some("A"), Some("B"))
        );
        assert_eq!(s.in_flight(), 3);
    }

    #[test]
    fn switching_to_the_current_upstream_cancels_the_queue_instead_of_churning() {
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        s.request_switch("B");
        assert_eq!(s.pending(), Some("B"));
        // 调度器又改主意了,还是 A。
        s.request_switch("A");
        assert_eq!(s.pending(), None);
        let before = s.switches();
        s.begin();
        assert_eq!(s.switches(), before, "没换却记了一次换手");
    }

    #[test]
    fn switches_are_only_counted_when_the_upstream_really_changes() {
        // 换手次数是迟滞有没有起作用的唯一证据,多记一次就看不出来了。
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        assert_eq!(s.switches(), 1);
        s.set_now("A");
        assert_eq!(s.switches(), 1);
        for _ in 0..10 {
            s.begin();
            s.end();
        }
        assert_eq!(s.switches(), 1, "没有待切换时 begin 不该记换手");
    }

    #[test]
    fn ending_more_requests_than_began_does_not_underflow() {
        let mut s = UpstreamSwitch::new();
        s.end();
        s.end();
        assert_eq!(s.in_flight(), 0);
    }

    #[test]
    fn a_fresh_switch_has_no_upstream_and_begin_says_so() {
        // 一条上游都还没选时,begin 要如实返回 None,
        // 不能凭空给一个 id —— 那会让路由打到一条不存在的线上。
        let mut s = UpstreamSwitch::new();
        assert_eq!(s.begin(), None);
        assert_eq!(s.current(), None);
    }

    /// 在一个软件上换线，**绝不许动到别的软件**。
    ///
    /// 只留一份 `current` 时，在 Codex 页上点一下会把 Claude Code
    /// 正在用的那条一起换掉 —— 而使用者在另一个分页上，完全看不出是自己干的。
    #[test]
    fn switching_one_client_never_moves_another_clients_upstream() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "cc-line");
        r.set_now(Client::Codex, "cx-line");

        r.request_switch(Client::Codex, "cx-other");
        assert_eq!(r.begin(Client::Codex).as_deref(), Some("cx-other"));
        assert_eq!(
            r.current(Client::ClaudeCode),
            Some("cc-line"),
            "改 Codex 把 Claude Code 的上游也换了"
        );
    }

    /// 从没登记过的软件返回 `None`，**不回落到别人的上游**。
    ///
    /// 回落会让它悄悄用上别人的 Key，账记在别人头上，而且完全看不出来。
    #[test]
    fn an_unregistered_client_gets_nothing_rather_than_someone_elses_key() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "cc-line");
        assert_eq!(r.begin(Client::Codex), None);
        assert_eq!(r.current(Client::ClaudeDesktop), None);
        // 问一次不该把它登记进去。
        assert!(!r.currents().contains_key(&Client::Codex));
    }

    /// Claude Code 与桌面端**协议一样，但仍然是两条独立的线**。
    #[test]
    fn claude_code_and_desktop_keep_separate_upstreams() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "A");
        r.set_now(Client::ClaudeDesktop, "B");
        let m = r.currents();
        assert_eq!(m.get(&Client::ClaudeCode).map(String::as_str), Some("A"));
        assert_eq!(m.get(&Client::ClaudeDesktop).map(String::as_str), Some("B"));
        assert_eq!(r.switches(), 2);
    }

    /// 每个软件各自的「只在两次请求之间切换」都要成立。
    #[test]
    fn each_client_keeps_the_in_flight_guarantee_on_its_own() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "A");
        let mine = r.begin(Client::ClaudeCode);
        r.request_switch(Client::ClaudeCode, "B");
        assert_eq!(mine.as_deref(), Some("A"), "在飞的那一发被换掉了");
        assert_eq!(r.current(Client::ClaudeCode), Some("A"));
        assert_eq!(r.in_flight(), 1);
        r.end(Client::ClaudeCode);
        assert_eq!(r.in_flight(), 0);
        assert_eq!(r.begin(Client::ClaudeCode).as_deref(), Some("B"));
    }
    #[test]
    fn http_statuses_map_onto_the_three_breaker_tiers() {
        // §4.6 那张表唯一的落地处。
        assert_eq!(
            Outcome::from_status(429, Some(5_000)),
            Outcome::RateLimited {
                retry_after_ms: Some(5_000)
            }
        );
        for s in [401u16, 402, 403] {
            assert_eq!(Outcome::from_status(s, None), Outcome::Rejected, "{s}");
        }
        for s in [500u16, 502, 503, 504] {
            assert_eq!(Outcome::from_status(s, None), Outcome::ServerError, "{s}");
        }
        for s in [200u16, 201, 204, 301] {
            assert_eq!(Outcome::from_status(s, None), Outcome::Ok, "{s}");
        }
        assert_eq!(Outcome::timeout(), Outcome::ServerError);
    }

    #[test]
    fn a_bad_request_is_our_fault_not_the_line_s_fault() {
        // 400 / 404 / 422 是这一发请求自己的问题(参数错、模型名写错)。
        // 算成故障的话,连打五次就会把一条完全健康的线掐掉。
        for s in [400u16, 404, 422] {
            assert_eq!(Outcome::from_status(s, None), Outcome::Ok, "{s}");
        }
    }
}
