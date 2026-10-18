// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! Cost Calculator - 计算 API 请求成本
//!
//! 跟 cc-switch 的差别：
//!
//! - 它用 `rust_decimal`；QB Gate 的价目（`qb-station::station::pricing`）全是 `f64`，这里照它的
//!   算法换成 `f64`（同一份数在两边不会因为精度不同对不上）。
//! - 它的 `cost_multiplier`（乘在总价上的供应商倍数）**不搬**：QB Gate 里一条线的价是它自己的
//!   一组四类单价（`StationRates`），不是官方价乘一个数 —— CLAUDE.md「倍率不是一个数」。

use super::parser::TokenUsage;

/// 成本明细（美元）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostBreakdown {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_read_cost: f64,
    pub cache_creation_cost: f64,
    pub total_cost: f64,
}

/// 模型定价信息（美元 / 百万 token）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cache_read_cost_per_million: f64,
    /// 5 分钟档的缓存写入价。1 小时档按它的 1.6 倍算（见 [`CostCalculator`]）。
    pub cache_creation_cost_per_million: f64,
}

/// cc-switch 的 `CACHE_INCLUSIVE_APP_TYPES`：这些客户端（OpenAI Responses / Gemini 协议）
/// 报上来的输入 token **含**缓存读写；Claude / Anthropic 报的是 fresh input。
pub const CACHE_INCLUSIVE_APP_TYPES: &[&str] = &["codex", "gemini", "grokbuild"];

/// `app_type` 的输入 token 是否已包含 cache read/write。
pub fn is_cache_inclusive_app(app_type: &str) -> bool {
    CACHE_INCLUSIVE_APP_TYPES.contains(&app_type)
}

/// 成本计算器
pub struct CostCalculator;

impl CostCalculator {
    /// 计算请求成本（Claude / Anthropic 语义：`input_tokens` 已经不含 `cache_read_tokens`）。
    pub fn calculate(usage: &TokenUsage, pricing: &ModelPricing) -> CostBreakdown {
        Self::calculate_with_cache_semantics(usage, pricing, false)
    }

    /// 按 app_type 选择输入 token 语义后计算成本。
    ///
    /// Codex/OpenAI Responses 与 Gemini 的输入 token 字段包含 cache read 部分；
    /// Claude/Anthropic 的 input_tokens 已经是 fresh input。
    pub fn calculate_for_app(
        app_type: &str,
        usage: &TokenUsage,
        pricing: &ModelPricing,
    ) -> CostBreakdown {
        Self::calculate_with_cache_semantics(usage, pricing, is_cache_inclusive_app(app_type))
    }

    fn calculate_with_cache_semantics(
        usage: &TokenUsage,
        pricing: &ModelPricing,
        input_includes_cache_read: bool,
    ) -> CostBreakdown {
        let million = 1_000_000.0;

        // OpenAI/Gemini 风格的 input_tokens 包含缓存读取和写入，需要扣除后再按输入价计费；
        // Claude/Anthropic 风格的 input_tokens 已经是 fresh input，不能再次扣减。
        let billable_input_tokens = if input_includes_cache_read {
            usage
                .input_tokens
                .saturating_sub(usage.cache_read_tokens)
                .saturating_sub(usage.cache_creation_tokens)
        } else {
            usage.input_tokens
        };

        let input_cost =
            f64::from(billable_input_tokens) * pricing.input_cost_per_million / million;
        let output_cost =
            f64::from(usage.output_tokens) * pricing.output_cost_per_million / million;
        let cache_read_cost =
            f64::from(usage.cache_read_tokens) * pricing.cache_read_cost_per_million / million;
        // Anthropic 5 分钟缓存写入为输入价 1.25 倍、1 小时写入为 2 倍；定价表的
        // cache_creation 单价对应 5 分钟写入，1 小时写入按其 1.6 倍计。
        let cache_creation_1h_tokens = usage
            .cache_creation_1h_tokens
            .min(usage.cache_creation_tokens);
        let cache_creation_cost =
            (f64::from(usage.cache_creation_tokens - cache_creation_1h_tokens)
                + f64::from(cache_creation_1h_tokens) * 1.6)
                * pricing.cache_creation_cost_per_million
                / million;

        let total_cost = input_cost + output_cost + cache_read_cost + cache_creation_cost;

        CostBreakdown {
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
        }
    }

    pub fn try_calculate_for_app(
        app_type: &str,
        usage: &TokenUsage,
        pricing: Option<&ModelPricing>,
    ) -> Option<CostBreakdown> {
        pricing.map(|p| Self::calculate_for_app(app_type, usage, p))
    }
}

impl ModelPricing {
    pub fn new(input: f64, output: f64, cache_read: f64, cache_creation: f64) -> Self {
        Self {
            input_cost_per_million: input,
            output_cost_per_million: output,
            cache_read_cost_per_million: cache_read,
            cache_creation_cost_per_million: cache_creation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cc-switch 用 `Decimal` 精确比；`f64` 只比到 1e-12 美元。
    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn test_cost_calculation() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::new(3.0, 15.0, 0.3, 3.75);

        let cost = CostCalculator::calculate(&usage, &pricing);

        // Claude/Anthropic 语义：input_tokens 已经不含 cache_read_tokens
        // input: 1000 * 3.0 / 1M = 0.003
        close(cost.input_cost, 0.003);
        // output: 500 * 15.0 / 1M = 0.0075
        close(cost.output_cost, 0.0075);
        // cache_read: 200 * 0.3 / 1M = 0.00006
        close(cost.cache_read_cost, 0.00006);
        // cache_creation: 100 * 3.75 / 1M = 0.000375
        close(cost.cache_creation_cost, 0.000375);
        // total: 0.003 + 0.0075 + 0.00006 + 0.000375 = 0.010935
        close(cost.total_cost, 0.010935);
    }

    #[test]
    fn test_claude_1h_cache_writes_billed_at_1h_rate() {
        // Claude Opus 4.8：5m 缓存写入 $6.25/M，1h 缓存写入 $10/M（输入价的 2 倍）
        let pricing = ModelPricing::new(5.0, 25.0, 0.50, 6.25);
        let usage = serde_json::json!({
            "input_tokens": 2,
            "output_tokens": 1,
            "cache_read_input_tokens": 0,
            "cache_creation_input_tokens": 1000,
            "cache_creation": {
                "ephemeral_5m_input_tokens": 400,
                "ephemeral_1h_input_tokens": 600
            }
        });
        let response = serde_json::json!({ "model": "claude-opus-4-8", "usage": usage });
        let events = [
            serde_json::json!({
                "type": "message_start",
                "message": { "model": "claude-opus-4-8", "usage": usage }
            }),
            serde_json::json!({ "type": "message_delta", "usage": { "output_tokens": 5 } }),
        ];

        for parsed in [
            TokenUsage::from_claude_response(&response),
            TokenUsage::from_claude_stream_events(&events),
        ] {
            let cost = CostCalculator::calculate(&parsed.unwrap(), &pricing);
            // 400 × 6.25 / 1M + 600 × 10 / 1M = 0.0085
            close(cost.cache_creation_cost, 0.0085);
        }
    }

    #[test]
    fn test_cost_calculation_for_cache_inclusive_app() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::new(3.0, 15.0, 0.3, 3.75);

        let cost = CostCalculator::calculate_for_app("codex", &usage, &pricing);

        // Codex/OpenAI 语义：input_tokens 包含 cache read/write，两桶都需扣除。
        close(cost.input_cost, 0.0021);
        close(cost.output_cost, 0.0075);
        close(cost.cache_read_cost, 0.00006);
        close(cost.cache_creation_cost, 0.000375);
        close(cost.total_cost, 0.010035);
    }

    #[test]
    fn grokbuild_does_not_double_bill_cached_input() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 0,
            cache_read_tokens: 600,
            cache_creation_tokens: 0,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };
        let pricing = ModelPricing::new(10.0, 0.0, 1.0, 0.0);

        let cost = CostCalculator::calculate_for_app("grokbuild", &usage, &pricing);

        close(cost.input_cost, 0.004);
        close(cost.cache_read_cost, 0.0006);
        close(cost.total_cost, 0.0046);
    }

    #[test]
    fn claude_apps_are_not_cache_inclusive() {
        // 新客户端默认按 Claude 的「fresh input」算（cc-switch 那张表的立场：漏登记时
        // 看得出来的是缓存命中率偏低，而不是悄悄多扣）。
        for app in ["claude", "claude-desktop", "claude-code"] {
            assert!(!is_cache_inclusive_app(app), "{app}");
        }
        assert!(is_cache_inclusive_app("codex"));
    }

    #[test]
    fn test_small_values_keep_precision() {
        let usage = TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            cache_read_tokens: 1,
            cache_creation_tokens: 1,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::new(0.075, 0.3, 0.01875, 0.075);

        let cost = CostCalculator::calculate(&usage, &pricing);

        assert!(cost.total_cost > 0.0);
        close(
            cost.total_cost,
            (0.075 + 0.3 + 0.01875 + 0.075) / 1_000_000.0,
        );
    }
}
