// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 命令行工具管理的版本号那一半（2026-10-27，中转站重构阶段 7 · X3）：比版本、从 npm 的 dist-tags /
//! GitHub 的发布 / PyPI 里挑出「最新」、判断本机 Node 满不满足新版要求的 `engines.node`。
//!
//! 全是纯函数，照搬 cc-switch `commands/misc.rs` 的 `parse_semver` / `compare_semver` / `pick_latest_version` /
//! `github_release_version_from_json` / `drop_latest_behind_local` 与 `commands/tool_node_requirement.rs` 的范围判断，
//! 测试一起搬。改动：`regex` 换成手写的扫描（这个 crate 不带 regex）；联网那几步不在这里（`usecase::cli_tools_ops`）。

use std::cmp::Ordering;

/// `2.1.156` / `2.1.156-beta.1` → (主版本三段, 预发布段)。认不出返回 `None`。
///
/// `+` 后面那段像开发构建（`238.g<sha>`，或带 canary / nightly / dev / dirty / snapshot）时也返回 `None`：
/// 版本号可能更低、内容反而更新，跟正式版不可比（Hermes 的 main 分支自报 `0.22.1+238.g<sha>`）。
pub fn parse_semver(v: &str) -> Option<([u64; 3], Vec<String>)> {
    let v = v.trim();
    if let Some((_, local)) = v.split_once('+') {
        if is_dev_local(local) {
            return None;
        }
    }
    let core_and_pre = v.split('+').next().unwrap_or("");
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core_and_pre, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let pre = pre
        .map(|p| p.split('.').map(str::to_string).collect())
        .unwrap_or_default();
    Some(([major, minor, patch], pre))
}

/// `+` 后面那段是不是开发构建：`<数字>.g<至少四位十六进制>` 开头，或者某一段是 canary / nightly / dev / dirty / snapshot。
fn is_dev_local(local: &str) -> bool {
    let lower = local.to_ascii_lowercase();
    let segs: Vec<&str> = lower.split('.').collect();
    if segs.len() >= 2
        && !segs[0].is_empty()
        && segs[0].chars().all(|c| c.is_ascii_digit())
        && segs[1].len() >= 5
        && segs[1].starts_with('g')
        && segs[1][1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return true;
    }
    segs.iter()
        .any(|s| matches!(*s, "canary" | "nightly" | "dev" | "dirty" | "snapshot"))
}

/// 比两个版本（semver：三段优先；三段相同时有预发布的小；预发布逐段比）。任一认不出返回 `None`。
pub fn compare_semver(a: &str, b: &str) -> Option<Ordering> {
    let (ac, ap) = parse_semver(a)?;
    let (bc, bp) = parse_semver(b)?;
    for i in 0..3 {
        match ac[i].cmp(&bc[i]) {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => return Some(Ordering::Equal),
        (true, false) => return Some(Ordering::Greater),
        (false, true) => return Some(Ordering::Less),
        (false, false) => {}
    }
    for (x, y) in ap.iter().zip(bp.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xv), Ok(yv)) => xv.cmp(&yv),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.as_str().cmp(y.as_str()),
        };
        if ord != Ordering::Equal {
            return Some(ord);
        }
    }
    Some(ap.len().cmp(&bp.len()))
}

/// 从 npm 的 dist-tags 里挑要显示的「最新」：就是 `latest`。（cc-switch 只给 Claude Code 补查 `next` 通道；
/// 这六个工具都只看 `latest`，那段没搬。）
pub fn npm_latest(dist_tags: &serde_json::Value) -> Option<String> {
    dist_tags
        .get("latest")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 输出里第一个像版本号的东西：`\d+\.\d+\.\d+`，后面可以跟 `-预发布` 与 `+构建`。没有就原样返回。
pub fn extract_version(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // 前一个字符是数字就不从这里起（别从「21」的「1」开始读）；`v0.21.0` 这种字母后面紧跟的照认。
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
            if let Some(end) = version_end(b, i) {
                return raw[i..end].to_string();
            }
        }
        i += 1;
    }
    raw.trim().to_string()
}

/// 从 `start` 起是不是 `数字.数字.数字`（再带可选的 `-…` / `+…`）；是的话返回结尾。
fn version_end(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    for part in 0..3 {
        let s = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == s {
            return None;
        }
        if part < 2 {
            if i < b.len() && b[i] == b'.' {
                i += 1;
            } else {
                return None;
            }
        }
    }
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'_';
    if i + 1 < b.len() && b[i] == b'-' && word(b[i + 1]) {
        i += 1;
        while i < b.len() && word(b[i]) {
            i += 1;
        }
    }
    if i + 1 < b.len() && b[i] == b'+' && (word(b[i + 1]) || b[i + 1] == b'-') {
        i += 1;
        while i < b.len() && (word(b[i]) || b[i] == b'-') {
            i += 1;
        }
    }
    // 结尾的点不算（「版本 1.2.3.」）。
    while i > start && b[i - 1] == b'.' {
        i -= 1;
    }
    Some(i)
}

/// 只接受能按 semver 读、首段不像年份（< 1000）的字符串当显示版本。
///
/// Hermes 的标签是日历式（`v2026.8.31`），当成版本号的话三段解析能过（2026 > 0），于是永远「有新版本」、
/// 点了升级版本不变 —— cc-switch 踩过（#6475 / #6618 / #7033）。
pub fn release_display_version(candidate: &str) -> Option<String> {
    match parse_semver(candidate) {
        Some(([major, ..], _)) if major < 1000 => Some(candidate.to_string()),
        _ => None,
    }
}

/// GitHub 最新发布的 JSON → 显示用版本：先看发布名里像版本号的那一段（Hermes 是 `Hermes Agent v0.21.0 (v2026.8.31)`），
/// 再看标签（去掉 `v`）。限流时（只有 `message`）是 `None`。
pub fn github_release_version(json: &serde_json::Value) -> Option<String> {
    let from_name = json
        .get("name")
        .and_then(|v| v.as_str())
        .and_then(|name| release_display_version(&extract_version(name)));
    from_name.or_else(|| {
        json.get("tag_name")
            .and_then(|v| v.as_str())
            .and_then(|tag| release_display_version(tag.strip_prefix('v').unwrap_or(tag)))
    })
}

/// PyPI 的 `/pypi/<包>/json` → `info.version`。
pub fn pypi_version(json: &serde_json::Value) -> Option<String> {
    json.get("info")?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

/// 兜底来源给的「最新」已经比本机的还旧，就不显示 —— 显示一个「最新 < 当前」只会让人困惑。
pub fn drop_behind_local(latest: Option<String>, local: Option<&str>) -> Option<String> {
    let latest = latest?;
    let local_ahead = local
        .and_then(|l| compare_semver(l, &latest))
        .is_some_and(|o| o == Ordering::Greater);
    (!local_ahead).then_some(latest)
}

/// 本机这一份比最新的旧吗。读不出来（任一侧认不出）就是 `None`：不说「有新版」，也不说「已是最新」。
pub fn is_outdated(local: &str, latest: &str) -> Option<bool> {
    compare_semver(local, latest).map(|o| o == Ordering::Less)
}

// ------------------------------------------------------------------ Node 版本要求

type Core = [u64; 3];

#[derive(Debug, Clone, Copy)]
enum Bound {
    Ge(Core),
    Gt(Core),
    Le(Core),
    Lt(Core),
}

impl Bound {
    fn admits(self, v: Core) -> bool {
        match self {
            Bound::Ge(b) => v >= b,
            Bound::Gt(b) => v > b,
            Bound::Le(b) => v <= b,
            Bound::Lt(b) => v < b,
        }
    }
}

/// `version` 满不满足 npm 的版本范围 `range`（包的 `engines.node`）。认比较符、`^`、`~`、`x` 通配、`a - b` 与 `||`；
/// 认不出的写法返回 `None` —— 调用方按「不确定」放行，**不因为读不懂就拦人**。
pub fn node_range_satisfied(range: &str, version: &str) -> Option<bool> {
    let (version, _) = parse_semver(version.trim().trim_start_matches('v'))?;
    let mut unknown = false;
    for set in range.split("||") {
        match comparator_set_admits(set, version) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown = true,
        }
    }
    (!unknown).then_some(false)
}

fn comparator_set_admits(set: &str, version: Core) -> Option<bool> {
    let tokens: Vec<&str> = set.split_whitespace().collect();
    let mut bounds = Vec::new();
    if let [low, "-", high] = tokens.as_slice() {
        bounds.extend(comparator(">=", &parse_partial(low)?)?);
        bounds.extend(comparator("<=", &parse_partial(high)?)?);
    } else {
        // `>= 24` 这种比较符与版本之间带空格的写法也合法。
        let mut pending: Option<&str> = None;
        for token in tokens {
            let (op, rest) = split_operator(token);
            let op = match pending.take() {
                Some(_) if !op.is_empty() => return None,
                Some(pending) => pending,
                None => op,
            };
            if rest.is_empty() {
                pending = Some(op);
                continue;
            }
            bounds.extend(comparator(op, &parse_partial(rest)?)?);
        }
        if pending.is_some() {
            return None;
        }
    }
    Some(bounds.into_iter().all(|bound| bound.admits(version)))
}

fn split_operator(token: &str) -> (&str, &str) {
    for op in [">=", "<=", ">", "<", "=", "^", "~"] {
        if let Some(rest) = token.strip_prefix(op) {
            return (op, rest);
        }
    }
    ("", token)
}

/// `24` / `24.16` / `24.16.0` / `24.x` / `*` → 写出来的那几段；`v` 前缀与预发布段忽略。
fn parse_partial(raw: &str) -> Option<Vec<u64>> {
    let core = raw.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = Vec::new();
    for segment in core.split('.') {
        if matches!(segment, "x" | "X" | "*") {
            break;
        }
        parts.push(segment.parse().ok()?);
    }
    (parts.len() <= 3).then_some(parts)
}

fn comparator(op: &str, parts: &[u64]) -> Option<Vec<Bound>> {
    let at = |i: usize| parts.get(i).copied().unwrap_or(0);
    let floor = [at(0), at(1), at(2)];
    // 最后写出的那段 +1、其后归零：`24` → 25.0.0，`24.16` → 24.17.0。
    let next = || {
        let mut core = floor;
        core[parts.len() - 1] += 1;
        for slot in core.iter_mut().skip(parts.len()) {
            *slot = 0;
        }
        core
    };
    let exact = parts.len() == 3;
    if parts.is_empty() {
        return matches!(op, "" | "=" | ">=" | "<=" | "^" | "~").then(Vec::new);
    }
    Some(match op {
        "" | "=" if exact => vec![Bound::Ge(floor), Bound::Le(floor)],
        "" | "=" => vec![Bound::Ge(floor), Bound::Lt(next())],
        ">=" => vec![Bound::Ge(floor)],
        ">" if exact => vec![Bound::Gt(floor)],
        ">" => vec![Bound::Ge(next())],
        "<" => vec![Bound::Lt(floor)],
        "<=" if exact => vec![Bound::Le(floor)],
        "<=" => vec![Bound::Lt(next())],
        "~" if parts.len() == 1 => vec![Bound::Ge(floor), Bound::Lt([at(0) + 1, 0, 0])],
        "~" => vec![Bound::Ge(floor), Bound::Lt([at(0), at(1) + 1, 0])],
        "^" => {
            let upper = if at(0) > 0 || parts.len() == 1 {
                [at(0) + 1, 0, 0]
            } else if at(1) > 0 || parts.len() == 2 {
                [0, at(1) + 1, 0]
            } else {
                [0, 0, at(2) + 1]
            };
            vec![Bound::Ge(floor), Bound::Lt(upper)]
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semver_orders_like_npm_and_refuses_dev_builds() {
        assert_eq!(compare_semver("1.2.3", "1.2.4"), Some(Ordering::Less));
        assert_eq!(compare_semver("1.10.0", "1.9.9"), Some(Ordering::Greater));
        assert_eq!(
            compare_semver("1.2.3-beta.1", "1.2.3"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_semver("1.2.3-beta.2", "1.2.3-beta.10"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_semver("1.2.3-alpha", "1.2.3-1"),
            Some(Ordering::Greater),
            "数字段 < 非数字段"
        );
        assert_eq!(
            compare_semver("1.2.3+linux.1", "1.2.3"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_semver("0.22.1+238.g1a2b3c4", "0.21.0"),
            None,
            "开发构建跟正式版不可比"
        );
        assert_eq!(
            compare_semver("0.1.2505172116", "0.1.1"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_semver("not", "1.0.0"), None);
        assert_eq!(compare_semver("1.2", "1.2.0"), None, "少一段不认");
    }

    #[test]
    fn versions_are_pulled_out_of_noisy_output() {
        assert_eq!(extract_version("opencode 0.15.8"), "0.15.8");
        assert_eq!(
            extract_version("Hermes Agent v0.21.0 (v2026.8.31)"),
            "0.21.0"
        );
        assert_eq!(
            extract_version("pi 2.3.4-beta.2+build.7\n"),
            "2.3.4-beta.2+build.7"
        );
        assert_eq!(extract_version("version 1.2.3."), "1.2.3");
        assert_eq!(extract_version("abc1.2.3"), "1.2.3");
        assert_eq!(extract_version("build 12.3 v4.5.6"), "4.5.6", "两段的跳过");
        assert_eq!(extract_version("no version"), "no version");
    }

    #[test]
    fn calendar_tags_never_become_a_display_version() {
        assert_eq!(
            github_release_version(
                &json!({"name": "Hermes Agent v0.21.0 (v2026.8.31)", "tag_name": "v2026.8.31"})
            ),
            Some("0.21.0".into())
        );
        assert_eq!(
            github_release_version(
                &json!({"name": "Release v2026.8.31", "tag_name": "v2026.8.31"})
            ),
            None
        );
        assert_eq!(
            github_release_version(&json!({"name": "", "tag_name": "v1.4.0"})),
            Some("1.4.0".into())
        );
        assert_eq!(
            github_release_version(&json!({"message": "API rate limit exceeded"})),
            None
        );
    }

    #[test]
    fn npm_and_pypi_shapes() {
        assert_eq!(
            npm_latest(&json!({"latest": "0.15.8", "beta": "0.16.0-beta.1"})),
            Some("0.15.8".into())
        );
        assert_eq!(
            npm_latest(&json!("Not Found")),
            None,
            "包不存在时回一个字符串"
        );
        assert_eq!(npm_latest(&json!({"latest": " "})), None);
        assert_eq!(
            pypi_version(&json!({"info": {"version": "0.19.0"}})),
            Some("0.19.0".into())
        );
    }

    #[test]
    fn a_fallback_latest_older_than_local_is_hidden() {
        assert_eq!(
            drop_behind_local(Some("0.19.0".into()), Some("0.21.0")),
            None
        );
        assert_eq!(
            drop_behind_local(Some("0.22.0".into()), Some("0.21.0")),
            Some("0.22.0".into())
        );
        assert_eq!(
            drop_behind_local(Some("0.19.0".into()), None),
            Some("0.19.0".into())
        );
        assert_eq!(is_outdated("0.21.0", "0.22.0"), Some(true));
        assert_eq!(is_outdated("0.22.0", "0.22.0"), Some(false));
        assert_eq!(is_outdated("dev", "0.22.0"), None);
    }

    #[test]
    fn openclaw_range_rejects_node_22_and_admits_24_and_26() {
        let range = ">=24.16.0 <25 || >=26.1.0";
        assert_eq!(node_range_satisfied(range, "22.23.2"), Some(false));
        assert_eq!(node_range_satisfied(range, "v24.15.9"), Some(false));
        assert_eq!(node_range_satisfied(range, "24.16.0"), Some(true));
        assert_eq!(node_range_satisfied(range, "25.0.0"), Some(false));
        assert_eq!(node_range_satisfied(range, "26.1.0"), Some(true));
    }

    #[test]
    fn common_engine_ranges() {
        assert_eq!(node_range_satisfied(">=20", "20.0.0"), Some(true));
        assert_eq!(node_range_satisfied(">=20", "18.20.8"), Some(false));
        assert_eq!(node_range_satisfied(">= 22.0.0", "22.1.0"), Some(true));
        assert_eq!(
            node_range_satisfied("^20.19.0 || >=22.12.0", "21.0.0"),
            Some(false)
        );
        assert_eq!(
            node_range_satisfied("^20.19.0 || >=22.12.0", "20.19.4"),
            Some(true)
        );
        assert_eq!(node_range_satisfied("~18.17", "18.18.0"), Some(false));
        assert_eq!(node_range_satisfied("18.x", "18.20.8"), Some(true));
        assert_eq!(node_range_satisfied("18 - 20", "20.9.0"), Some(true));
        assert_eq!(node_range_satisfied("18 - 20", "21.0.0"), Some(false));
        assert_eq!(node_range_satisfied(">18", "18.9.0"), Some(false));
        assert_eq!(node_range_satisfied("<=20", "20.9.0"), Some(true));
        assert_eq!(node_range_satisfied("*", "4.0.0"), Some(true));
        assert_eq!(node_range_satisfied("^0.2.3", "0.3.0"), Some(false));
    }

    #[test]
    fn unknown_syntax_never_blocks() {
        assert_eq!(node_range_satisfied("node >= 20", "18.0.0"), None);
        assert_eq!(node_range_satisfied(">=", "18.0.0"), None);
        assert_eq!(node_range_satisfied("lts/* || >=18", "20.0.0"), Some(true));
        assert_eq!(node_range_satisfied(">=20", "not-a-version"), None);
    }
}
