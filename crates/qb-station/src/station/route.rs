//! 线路：**一条线路 = 站点 + 分组。一个分组一把 key,没有第三层。**
//!
//! 纯类型,不做 I/O。
//!
//! # 哪些东西在哪一层
//!
//! | 层 | 有什么 |
//! |---|---|
//! | 站点 | 域名、账号、**余额(整站共享)**、后端类型 |
//! | 分组 | 倍率、健康度、那把 key、24h 实扣 |
//!
//! 分组**不是按模型分的,是按通道档次分的** —— 同一个模型会同时出现在
//! 「官方直连组」和「低价组」里。所以按站点显示一个「最低倍率」会骗人:
//! 那个最低值可能来自一个你的令牌根本用不了的分组。
//!
//! 余额是唯一的例外:它是账号级的,**整站共享**。你在 GPT 分组上烧的钱
//! 会吃掉 Claude 分组的余额。界面上必须写明这一点。
//!
//! # 真实倍率
//!
//! 每一轮检验记一个 `mult`(实扣是标称的几倍),**真实倍率 = 标称 × mult**。
//! `mult` 记在每一轮上而不是线路上 —— 历史里才看得出这家是越来越离谱
//! 还是在收敛。
//!
//! ⛔ **造假的站不拉黑。** 真实倍率仍然过线的照样留在池子里,只是拿真实值
//! 参与比较。这是 DISCLAIMER 那条「只检测、只如实报告 —— 不拉黑、
//! 不替你换站」的直接落地。

use qb_contract::domain::Client;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 标称与实测差多少以内算「属实」。
///
/// 这是判断不是实测值:计费有舍入、不同模型的补全倍率也会让实扣浮动,
/// 卡得太死会把每一家正常站点都打成造假。5% 足够放过舍入,
/// 又拦得住「标 ×0.20 实收 ×0.26」那一档(差 30%)。
pub const RATE_TOLERANCE: f64 = 0.05;

/// 倍率这一格该怎么写。**三态,不是两态。**
///
/// 「没检验过」必须独立于「检验过属实」—— 前者是还没有断言,后者是
/// 有证据的断言。合并成一个「看起来没问题」,使用者就分不出
/// 「这站我查过」和「这站我还没查」。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum RateTrust {
    /// 没检验过。界面写「标称 · 未核实」。**不写 0,也不写「属实」。**
    Unverified { nominal: Option<f64> },
    /// 检验过,实扣与标称一致。界面写「已核实」。
    Verified { rate: f64 },
    /// 检验过,实扣与标称对不上。界面把标称划掉,写实测值 + 「实测」。
    ///
    /// 收得比标称**少**也落在这一档:该显示的是量出来的那个数,
    /// 跟方向无关。方向由 [`overcharging`](RateTrust::overcharging) 回答。
    Differs {
        nominal: f64,
        /// 真实倍率 = 标称 × mult。
        real: f64,
        mult: f64,
    },
}

impl RateTrust {
    /// 排序和底线要用的那个数。**优先用真实倍率。**
    ///
    /// 没检验过时只能退回标称 —— 但那是「未核实」的标称,
    /// 界面必须照实说,不能让它看起来像核实过的。
    pub fn rate_for_ranking(self) -> Option<f64> {
        match self {
            RateTrust::Unverified { nominal } => nominal,
            RateTrust::Verified { rate } => Some(rate),
            RateTrust::Differs { real, .. } => Some(real),
        }
    }

    /// 是不是收得比标称多。`None` = 没检验过,答不了。
    pub fn overcharging(self) -> Option<bool> {
        match self {
            RateTrust::Unverified { .. } => None,
            RateTrust::Verified { .. } => Some(false),
            RateTrust::Differs { mult, .. } => Some(mult > 1.0),
        }
    }
}

/// 由标称倍率与最近一轮的 `mult` 得出这一格该怎么写。
///
/// `mult` 为 `None` = 还没检验过。
pub fn rate_trust(nominal: Option<f64>, mult: Option<f64>) -> RateTrust {
    match (nominal, mult) {
        (Some(n), Some(m)) if n.is_finite() && m.is_finite() && n > 0.0 && m > 0.0 => {
            if (m - 1.0).abs() <= RATE_TOLERANCE {
                RateTrust::Verified { rate: n }
            } else {
                RateTrust::Differs {
                    nominal: n,
                    real: n * m,
                    mult: m,
                }
            }
        }
        // mult 存在但不是个有效的数 —— 当作没检验过,不拿坏数据去改写倍率。
        (nominal, _) => RateTrust::Unverified { nominal },
    }
}

/// 一条线路。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Route {
    /// `软件 + 站点 id + 分组名`,见 [`Route::make_id`]。
    pub id: String,
    /// 这条线属于哪个软件。**线路按软件独立,站点不按软件复制。**
    ///
    /// 同一个站点的同一个分组,在 Claude Code 和 Codex 底下是**两行**:
    /// 各自绑各自的 key、各自记各自的健康度。站点(域名)那一层则是共享的 ——
    /// 总站只是个容器,按软件复制三份会让「余额」在三张卡上各写一遍,
    /// 看起来像三笔钱。
    ///
    /// ⛔ **这一维不能用 `protocols` 推。** Claude Code 与 Claude 桌面端
    /// 走的是同一套 Anthropic 协议,协议上分不开 —— 分得开它们的是
    /// 本机路由的路径前缀(`/v1/messages` 对 `/cd/v1/messages`)。
    #[serde(default)]
    pub client: Client,
    pub station_id: String,
    /// 分组名。**空字符串是合法的** —— 有些站点只有一个默认分组。
    pub group: String,
    /// 这个分组绑的那把 key。一个分组一把,没有第三层。
    pub credential_id: Option<String>,
    /// 站点自己标的分组倍率。
    pub nominal_rate: Option<f64>,
    /// 最近一轮检验量出来的 `mult`。`None` = 没检验过。
    ///
    /// **完整的 mult 历史不在这里**,在检验轮次里 —— 这里只缓存最近一轮,
    /// 因为排序每次都要用它。
    pub latest_mult: Option<f64>,
    /// 这条线上一次被检验是什么时候。`None` = 从没检验过。
    pub last_audit_ms: Option<i64>,
    /// 这条线支持哪些协议。
    ///
    /// **线路出现在哪个客户端分页,只看这个** —— 跟「模型是谁家的」正交:
    /// DeepSeek 有 Anthropic 协议端点,所以 ds 模型可以用 Claude Code 跑。
    /// 三个都是 `Option<bool>`:`None` = 还没探过,跟「探过了,没有」是两回事。
    #[serde(default)]
    pub protocols: super::model::Protocols,
    /// 站点公布的计费倍率(`/api/pricing`)。
    ///
    /// **界面要显示「开了计费翻倍」** —— 两家站一个翻倍一个不翻倍时,
    /// 光看「×0.15 对 ×0.4」会得出完全相反的结论。
    #[serde(default)]
    pub rates: super::pricing::StationRates,
    /// 上面那组价是**哪个模型**的。`None` = 还没检验过,不知道。
    ///
    /// ⛔ 少了这一段,公布绝对单价的站点(sub2api 系)就没法比价 ——
    /// 一个「输入 $2.5/百万」换不成倍率,除非知道官方单价是多少,
    /// 而官方单价是按模型查的。倍率那一套(New API 系)不需要它,
    /// 所以早先没有也没出症状:那一套本来就只报站点自己的数。
    ///
    /// **不拿线路上配的模型名顶替。** 那个是发请求用的,可能是别名、
    /// 可能是空;这个是「价目表上那一行叫什么」。两者对不上的时候
    /// 顶替出来的是另一个模型的官方价,算出的倍率会错得看不出来。
    #[serde(default)]
    pub rates_model: Option<String>,
}

impl Route {
    /// 线路 id 由软件、站点和分组决定 —— 一条线路就是这三样东西。
    ///
    /// 用 `\u{1f}`(单元分隔符)拼接,因为它不可能出现在这三样里;
    /// 用 `/` 或 `:` 的话,`a` + `b/c` 和 `a/b` + `c` 会拼出同一个 id。
    ///
    /// **软件在最前面**:同一个站点的同一个分组在两个软件底下是两条线路,
    /// 少了这一段,给 Codex 加的那条会把 Claude Code 那条原地覆盖掉。
    pub fn make_id(client: Client, station_id: &str, group: &str) -> String {
        // 拿 serde 的 kebab-case 名字当 id 段,跟 `Client` 在 IPC/TS 那边的
        // 写法是同一个字符串 —— 另写一份映射就会漂。
        let c = match client {
            Client::ClaudeCode => "claude-code",
            Client::ClaudeDesktop => "claude-desktop",
            Client::Codex => "codex",
            // 反重力不该有线路（没有中转路径，`route_save` 在入口拒绝）；这里仍给出
            // 与 serde 一致的段，免得哪天有人绕过入口时 id 段是空的、两条线路撞成一条。
            Client::Antigravity => "antigravity",
            Client::AntigravityIde => "antigravity-ide",
        };
        format!("{c}\u{1f}{station_id}\u{1f}{group}")
    }

    pub fn new(client: Client, station_id: impl Into<String>, group: impl Into<String>) -> Self {
        let station_id = station_id.into();
        let group = group.into();
        Self {
            id: Self::make_id(client, &station_id, &group),
            client,
            station_id,
            group,
            ..Default::default()
        }
    }

    /// 这条线的倍率该怎么写。
    pub fn rate_trust(&self) -> RateTrust {
        rate_trust(self.nominal_rate, self.latest_mult)
    }

    /// 排序和底线要用的倍率。**真实倍率优先。**
    pub fn rate_for_ranking(&self) -> Option<f64> {
        self.rate_trust().rate_for_ranking()
    }

    /// 检验过没有。**「从没检验过」是独立行态**,界面画虚线框 + 「—」,
    /// 不写成 0 分 —— 0 是断言,没检验过是还没有断言。
    pub fn audited(&self) -> bool {
        self.last_audit_ms.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unaudited_route_reports_its_rate_as_unverified_not_as_true() {
        // 「没检验过」和「检验过属实」必须分得开,否则使用者分不出
        // 「这站我查过」和「这站我还没查」。
        let r = Route {
            nominal_rate: Some(0.20),
            ..Route::new(Client::ClaudeCode, "s1", "低价组")
        };
        assert_eq!(
            r.rate_trust(),
            RateTrust::Unverified {
                nominal: Some(0.20)
            }
        );
        assert_eq!(r.rate_trust().overcharging(), None);
        assert!(!r.audited());
        // 没检验过时排序只能退回标称 —— 但界面上它是「未核实」的。
        assert_eq!(r.rate_for_ranking(), Some(0.20));
    }

    #[test]
    fn a_mult_within_tolerance_reads_as_verified() {
        // 容差**边界本身不测**:`1.0 + 0.05` 在二进制里是 0.05000000000000004,
        // 卡在边界上的断言测的是浮点表示,不是这条规则。测两边足够。
        for m in [1.0, 1.04, 0.96] {
            assert_eq!(
                rate_trust(Some(0.5), Some(m)),
                RateTrust::Verified { rate: 0.5 },
                "mult={m} 应当算属实"
            );
        }
        for m in [1.06, 0.94] {
            assert!(
                matches!(rate_trust(Some(0.5), Some(m)), RateTrust::Differs { .. }),
                "mult={m} 应当算对不上"
            );
        }
    }

    #[test]
    fn the_documented_overcharging_case_reads_as_measured() {
        // 标 ×0.20 实收 ×0.26 —— 界面把标称划掉,写实测值。
        let t = rate_trust(Some(0.20), Some(1.3));
        match t {
            RateTrust::Differs {
                nominal,
                real,
                mult,
            } => {
                assert_eq!(nominal, 0.20);
                assert!((real - 0.26).abs() < 1e-9, "real={real}");
                assert_eq!(mult, 1.3);
            }
            other => panic!("应当是 Differs,实际是 {other:?}"),
        }
        assert_eq!(t.overcharging(), Some(true));
        // ⛔ 底线按真实倍率比,不按站点自己标的。
        assert!((t.rate_for_ranking().unwrap() - 0.26).abs() < 1e-9);
    }

    #[test]
    fn charging_less_than_advertised_also_shows_the_measured_number() {
        // 该显示的是量出来的那个数,跟方向无关 —— 但方向要答得出来。
        let t = rate_trust(Some(1.0), Some(0.5));
        assert_eq!(t.rate_for_ranking(), Some(0.5));
        assert_eq!(t.overcharging(), Some(false));
    }

    #[test]
    fn a_faking_station_is_never_blacklisted_only_repriced() {
        // DISCLAIMER 那条「只检测、只如实报告 —— 不拉黑、不替你换站」。
        // 真实倍率仍然过线的,照样留在池子里。
        let t = rate_trust(Some(0.20), Some(1.3)); // 造假,真实 0.26
        let real = t.rate_for_ranking().unwrap();
        assert!(real <= 0.30, "真实倍率 {real} 仍在 0.30 的底线之内");
        // 它没有任何「被排除」的表达 —— 这个类型里压根没有拉黑这个概念。
        assert!(t.overcharging().unwrap());
    }

    #[test]
    fn a_nonsense_mult_falls_back_to_unverified_instead_of_rewriting_the_rate() {
        // 拿坏数据去改写倍率,比不改写危险得多。
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                rate_trust(Some(0.5), Some(bad)),
                RateTrust::Unverified { nominal: Some(0.5) },
                "mult={bad}"
            );
        }
    }

    #[test]
    fn a_route_without_a_nominal_rate_stays_unknown_rather_than_zero() {
        let t = rate_trust(None, Some(1.3));
        assert_eq!(t, RateTrust::Unverified { nominal: None });
        assert_eq!(t.rate_for_ranking(), None);
    }

    #[test]
    fn ids_cannot_collide_across_different_station_and_group_splits() {
        // 用 `/` 拼的话,("a", "b/c") 和 ("a/b", "c") 会撞成同一个 id ——
        // 两条不同的线路在池子里合并成一条,倍率和健康度全串。
        let cc = Client::ClaudeCode;
        assert_ne!(
            Route::make_id(cc, "a", "b/c"),
            Route::make_id(cc, "a/b", "c")
        );
        assert_eq!(Route::new(cc, "a", "b").id, Route::make_id(cc, "a", "b"));
    }

    #[test]
    fn the_same_station_and_group_are_two_lines_under_two_clients() {
        // 线路按软件独立。少了 id 里那一段,给 Codex 加的那条会把
        // Claude Code 那条**原地覆盖**掉 —— 库里是 `id` 做主键的。
        let a = Route::new(Client::ClaudeCode, "s1", "低价组");
        let b = Route::new(Client::Codex, "s1", "低价组");
        assert_ne!(a.id, b.id);
        assert_eq!(a.station_id, b.station_id, "站点是共享的,不按软件复制");
        assert_eq!(a.client, Client::ClaudeCode);
        assert_eq!(b.client, Client::Codex);
        // Claude Code 与桌面端协议上分不开,但仍然是两条线。
        assert_ne!(
            Route::new(Client::ClaudeCode, "s1", "g").id,
            Route::new(Client::ClaudeDesktop, "s1", "g").id
        );
    }

    #[test]
    fn a_route_deserialised_from_an_old_pool_lands_on_claude_code() {
        // 0.16.0 之前库里没有这个字段。反序列化补的默认值必须和
        // `repository.rs` 第 3 条迁移写进去的是同一个答案。
        let r: Route = serde_json::from_str(
            r#"{"id":"s1-g","station_id":"s1","group":"g","credential_id":null,
                "nominal_rate":null,"latest_mult":null,"last_audit_ms":null}"#,
        )
        .expect("老线路应当反序列化得出来");
        assert_eq!(r.client, Client::ClaudeCode);
    }

    #[test]
    fn an_empty_group_is_legal() {
        // 有些站点只有一个默认分组。
        let r = Route::new(Client::ClaudeCode, "s1", "");
        assert_eq!(r.group, "");
        assert!(!r.id.is_empty());
    }
}
