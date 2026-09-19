//! Controlled six-call monitor contract adapted from the user's standalone source.
use super::{station_billing, station_probe};
use qb_station::station::{
    audit::{AuditBatch, AuditSample},
    model::UsageRow,
    pricing,
};

fn identity(row: &UsageRow) -> String {
    if !row.request_id.is_empty() {
        return row.request_id.clone();
    }
    format!(
        "{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}",
        row.at_ms,
        row.model,
        row.group,
        row.input_uncached,
        row.cache_read,
        row.cache_write,
        row.output,
        row.cost
    )
}
fn tokens(row: &UsageRow) -> [Option<u64>; 4] {
    [
        row.input_uncached,
        row.cache_read,
        row.cache_write,
        row.output,
    ]
}
fn small(tokens: [Option<u64>; 4]) -> [Option<u32>; 4] {
    tokens.map(|n| n.and_then(|v| u32::try_from(v).ok()))
}
fn group_rows(rows: Vec<UsageRow>, group: &str) -> Vec<UsageRow> {
    rows.into_iter().filter(|r| r.group == group).collect()
}

/// Match only unused new rows. Token fallback is labeled weaker and must be unique
/// in both directions, so equal-token concurrent requests cannot be assigned by guess.
fn correlate(
    probes: &[station_probe::Probe],
    candidates: &[UsageRow],
    model: &str,
) -> Vec<Option<(usize, &'static str)>> {
    let mut result = vec![None; probes.len()];
    let mut used = std::collections::HashSet::new();
    for (i, probe) in probes.iter().enumerate() {
        let exact: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.model == model
                    && !r.request_id.is_empty()
                    && probe.request_ids.contains(&r.request_id)
            })
            .map(|(j, _)| j)
            .collect();
        if exact.len() == 1 && used.insert(exact[0]) {
            result[i] = Some((exact[0], "request-id"));
        }
    }
    for (i, probe) in probes.iter().enumerate() {
        if result[i].is_some() || probe.tokens.iter().any(Option::is_none) {
            continue;
        }
        let matches =
            |p: &station_probe::Probe, r: &UsageRow| r.model == model && tokens(r) == p.tokens;
        let possible: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter(|(j, r)| !used.contains(j) && matches(probe, r))
            .map(|(j, _)| j)
            .collect();
        if possible.len() == 1 {
            let j = possible[0];
            let contenders = probes
                .iter()
                .enumerate()
                .filter(|(k, p)| result[*k].is_none() && matches(p, &candidates[j]))
                .count();
            if contenders == 1 {
                used.insert(j);
                result[i] = Some((j, "tokens-only"));
            }
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    base: &str,
    key: &str,
    kind: qb_contract::domain::Client,
    group: &str,
    auth: Option<&station_billing::LedgerAuth>,
    model: &str,
    catalog: &pricing::Catalog,
    client: &reqwest::Client,
    cold: bool,
    now_ms: i64,
    notify: &(dyn Fn(u32, u32, &str) + Send + Sync),
) -> (AuditBatch, Vec<String>) {
    let planned = if cold { 7 } else { 6 };
    let mut batch = AuditBatch {
        planned,
        ..Default::default()
    };
    let mut problems = Vec::new();
    notify(0, planned, "检查后台登录与账单");
    let Some(auth) = auth else {
        return (batch, vec!["请先登录后台账单账户。未发送计费请求".into()]);
    };
    let before = match station_billing::usage(base, auth, client).await {
        Ok(rows) => group_rows(rows, group),
        Err(e) => return (batch, vec![format!("{e}。未发送计费请求")]),
    };
    let baseline: std::collections::HashSet<_> = before.iter().map(identity).collect();
    match station_billing::balance(base, auth, client).await {
        Ok((value, currency)) => {
            batch.balance_before = value;
            batch.currency = currency;
        }
        Err(e) => {
            // Authentication failures must stop spending; unavailable balance is merely missing evidence.
            if e.contains("401") || e.contains("403") {
                return (batch, vec![format!("{e}。未发送计费请求")]);
            }
            problems.push(format!("开始余额未取到：{e}"));
        }
    }
    let prefix = include_str!("station_material.txt").replace("{batch}", &crate::config_io::id());
    let mut questions = vec![
        "Review the project structure and state one concrete reliability concern.".to_string(),
    ];
    questions.extend(
        serde_json::from_str::<Vec<String>>(include_str!("station_questions.json"))
            .expect("embedded monitor questions"),
    );
    if cold {
        questions.push(questions[5].clone());
    }
    let parts: Vec<_> = prefix.split("\n\n").collect();
    let cold_prefix = parts
        .iter()
        .skip(1)
        .chain(parts.iter().take(1))
        .copied()
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut probes = Vec::new();
    let mut explicit = kind == qb_contract::domain::Client::Codex && model.starts_with("gpt-5.6");
    for (index, question) in questions.iter().enumerate() {
        let stage = if index == 0 {
            "预热".into()
        } else if index == 6 {
            "冷前缀对照".into()
        } else {
            format!("缓存验证 {index}")
        };
        notify(index as u32, planned, &stage);
        let mut sample = AuditSample {
            stage,
            ..Default::default()
        };
        match station_probe::run(
            kind,
            base,
            key,
            model,
            client,
            if index == 6 { &cold_prefix } else { &prefix },
            question,
            explicit,
        )
        .await
        {
            Ok(probe) => {
                if explicit && probe.automatic_cache {
                    explicit = false;
                    problems.push(
                        "站点明确拒绝显式缓存参数，已按源码退回自动缓存模式；未重放其它失败请求"
                            .into(),
                    );
                }
                sample.request_id = probe
                    .request_ids
                    .last()
                    .cloned()
                    .unwrap_or_default()
                    .chars()
                    .take(160)
                    .collect();
                sample.api_tokens = small(probe.tokens);
                sample.first_token_ms = probe.first_token_ms;
                sample.official = catalog.resolve(model).and_then(|p| {
                    let all: Option<Vec<_>> = probe.tokens.into_iter().collect();
                    all.map(|t| pricing::official_cost([t[0], t[1], t[2], t[3]], &p))
                });
                let incomplete = !probe.usage_complete();
                if incomplete {
                    sample.problem = Some("API 未返回完整的四类用量，已停止后续请求".into());
                }
                if !incomplete && probe.tokens[2].is_none() {
                    sample.problem = Some(
                        "API 未报告缓存写入计量；继续缓存验证，但不假定为零或计算完整价格倍率"
                            .into(),
                    );
                }
                probes.push(probe);
                batch.samples.push(sample);
                if incomplete {
                    break;
                }
            }
            Err(e) => {
                sample.problem = Some(e.clone());
                batch.samples.push(sample);
                problems.push(format!(
                    "第 {} 次请求未完成：{e}。已停止后续请求",
                    index + 1
                ));
                break;
            }
        }
        if index + 1 < questions.len() {
            // Bounded jitter like the reference monitor, without a new random dependency.
            let delay = 350 + (chrono::Utc::now().timestamp_subsec_millis() as u64 % 701);
            tokio::time::sleep(std::time::Duration::from_millis(if cfg!(test) {
                1
            } else {
                delay
            }))
            .await;
        }
    }
    notify(
        batch.samples.len() as u32,
        planned,
        "等待账单入账，核对余额",
    );
    let mut candidates = Vec::new();
    let mut matches = vec![None; probes.len()];
    if !probes.is_empty() {
        // Original monitor: seven 1-second polls, then seven 10-second polls; reads only.
        for attempt in 0..14 {
            tokio::time::sleep(std::time::Duration::from_millis(if cfg!(test) {
                1
            } else if attempt < 7 {
                1000
            } else {
                10000
            }))
            .await;
            match station_billing::usage(base, auth, client).await {
                Ok(rows) => {
                    candidates = group_rows(rows, group)
                        .into_iter()
                        .filter(|r| {
                            !baseline.contains(&identity(r))
                                && (r.at_ms >= now_ms - 5000
                                    || probes.iter().any(|p| p.request_ids.contains(&r.request_id)))
                        })
                        .collect();
                    matches = correlate(&probes, &candidates, model);
                    if matches.iter().all(Option::is_some) {
                        break;
                    }
                }
                Err(e) => {
                    problems.push(e);
                    break;
                }
            }
        }
    }
    for (i, matched) in matches.iter().enumerate() {
        let sample = &mut batch.samples[i];
        if let Some((j, evidence)) = matched {
            let row = &candidates[*j];
            sample.ledger_tokens = small(tokens(row));
            sample.billed = row.cost.filter(|v| v.is_finite() && *v >= 0.0);
            sample.match_kind = (*evidence).into();
            if tokens(row)
                .iter()
                .zip(probes[i].tokens)
                .any(|(b, a)| b.zip(a).is_some_and(|(b, a)| b != a))
            {
                sample.problem = Some("同一请求的 API Token 与账单 Token 不一致".into());
            }
            if *evidence == "tokens-only" {
                sample.problem =
                    Some("仅新账单与 Token 唯一匹配，缺少相同请求 ID，不能独立证明归属".into());
            }
        } else {
            sample.problem = Some("未找到可唯一匹配的新账单；未用旧账单或相邻请求补数".into());
        }
    }
    let complete = probes.len() == planned as usize && matches.iter().all(Option::is_some);
    if complete {
        batch.total_billed = batch.samples.iter().map(|s| s.billed).sum();
        batch.official_cost = batch.samples.iter().map(|s| s.official).sum();
    } else {
        problems.push("本轮请求或账单证据不完整，不计算整轮实扣倍率".into());
    }
    if probes.len() >= 6 {
        let warm = &probes[0];
        let controlled = warm.tokens[2]
            .filter(|v| *v > 0)
            .or(warm.tokens[0])
            .or_else(|| {
                warm.input_total
                    .zip(warm.tokens[1])
                    .and_then(|(t, r)| t.checked_sub(r))
            });
        batch.prefix_reuse = controlled.filter(|v| *v >= 1024).and_then(|prefix_tokens| {
            let baseline = warm.tokens[1]?;
            let rates: Option<Vec<_>> = probes[1..6]
                .iter()
                .map(|p| {
                    p.tokens[1].map(|r| {
                        r.saturating_sub(baseline).min(prefix_tokens) as f64 / prefix_tokens as f64
                    })
                })
                .collect();
            rates.map(|v| v.iter().sum::<f64>() / 5.0)
        });
        if batch.prefix_reuse.is_some_and(|v| v < 0.9) {
            problems.push("五次验证的平均前缀复用低于源码的 90% 参考线；缓存策略、路由变化或用量上报都可能影响结果".into());
        }
    }
    match station_billing::balance(base, auth, client).await {
        Ok((amount, _)) => {
            batch.balance_after = amount;
            batch.balance_delta = batch.balance_before.zip(amount).map(|(b, a)| b - a);
            if batch
                .balance_delta
                .zip(batch.total_billed)
                .is_some_and(|(d, b)| (d - b).abs() > 0.000001_f64.max(b.abs() * 0.02))
            {
                problems.push(
                    "账户余额变化与本轮账单合计不一致；并发消费、延迟入账或充值也会影响余额".into(),
                );
            }
        }
        Err(e) => problems.push(format!("结束余额未取到：{e}")),
    }
    if catalog.resolve(model).is_none() {
        problems.push("此模型没有官方参考价，保留用量证据，不推算价格".into());
    }
    notify(batch.samples.len() as u32, planned, "报告已完成");
    (batch, problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_token_rows_never_get_assigned_by_order() {
        let p = station_probe::Probe {
            tokens: [Some(1000), Some(0), Some(0), Some(10)],
            ..Default::default()
        };
        let r = UsageRow {
            model: "fixture".into(),
            input_uncached: Some(1000),
            cache_read: Some(0),
            cache_write: Some(0),
            output: Some(10),
            ..Default::default()
        };
        assert_eq!(
            correlate(&[p.clone(), p.clone()], std::slice::from_ref(&r), "fixture"),
            vec![None, None]
        );
        assert_eq!(
            correlate(&[p], &[r], "fixture"),
            vec![Some((0, "tokens-only"))]
        );
    }
}
