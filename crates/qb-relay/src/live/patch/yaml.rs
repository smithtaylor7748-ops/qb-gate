// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! YAML 按顶层小节替换（Hermes 的 `~/.hermes/config.yaml`、MiniMax Code 的 `~/.minimax/config.yaml`，中转站重构阶段 4）。
//!
//! 照搬 cc-switch `hermes_config.rs` 的 `find_yaml_section_range` / `replace_yaml_section` 一族：只换掉点名的那一个顶层键
//! 的文本，**别的小节、注释、空行一个字节都不动**（整份 YAML 读进来再写出去会丢掉所有注释）。
//!
//! 跟 cc-switch 不一样的地方：替换前后都整份解析一遍 —— 原文件解析不了就停（跟 [`super`] 顶上那条规矩一样），
//! 替换出来的东西解析不了也不交出去。cc-switch 用 `serde_yaml`，QB Gate 用 workspace 里早就有的 `serde_yaml_ng`。

use crate::error::{GateError, Result};

/// 这一行是不是一个顶层键（从第 0 列开始、不是注释、不是序列项，`:` 后面是空白或行尾）。
///
/// 行尾可能带 `\r`（CRLF 文件按 `\n` 切）：`\r` 也算行尾。cc-switch #3633 就是没认 `\r`，
/// CRLF 的配置每次都找不到小节，替换退化成在末尾追加。
fn is_top_level_key_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    let first_char = line.as_bytes()[0];
    if first_char == b' ' || first_char == b'\t' || first_char == b'#' || first_char == b'-' {
        return false;
    }
    if let Some(colon_pos) = line.find(':') {
        let after_colon = &line[colon_pos + 1..];
        after_colon.is_empty() || after_colon.starts_with([' ', '\t', '\r', '\n'])
    } else {
        false
    }
}

/// 一个顶层小节在原文里的字节范围 `(开始, 结束)`；没有就是 `None`。小节之间的空行和注释算前一个小节的。
fn find_section_range(raw: &str, section_key: &str) -> Option<(usize, usize)> {
    let target = format!("{section_key}:");
    let mut section_start = None;
    let mut offset = 0;
    for line in raw.split('\n') {
        if section_start.is_none() && is_top_level_key_line(line) && line.starts_with(&target) {
            let after_target = &line[target.len()..];
            if after_target.is_empty()
                || after_target.starts_with(' ')
                || after_target.starts_with('\t')
                || after_target.starts_with('\r')
            {
                section_start = Some(offset);
            }
        } else if section_start.is_some() && is_top_level_key_line(line) {
            return Some((section_start.unwrap(), offset));
        }
        offset += line.len() + 1;
    }
    section_start.map(|start| (start, raw.len()))
}

fn serialize_section(key: &str, value: &serde_yaml_ng::Value) -> Result<String> {
    let mut section = serde_yaml_ng::Mapping::new();
    section.insert(serde_yaml_ng::Value::String(key.to_string()), value.clone());
    serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(section))
        .map_err(|e| GateError::Other(format!("写不出 YAML 小节 {key}：{e}")))
}

/// 把同一个键剩下的那些小节都删掉（老版本追加出来的重复小节；cc-switch 的 `remove_all_sections`）。
fn remove_all_sections(raw: &str, section_key: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some((start, end)) = find_section_range(rest, section_key) {
        result.push_str(&rest[..start]);
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

/// 解析整份 YAML；空文件是空映射。顶层不是映射就报错（这几个工具的配置顶层都是映射）。
pub fn parse(raw: &str) -> Result<serde_yaml_ng::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_yaml_ng::Value::Mapping(Default::default()));
    }
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(raw).map_err(|e| {
        GateError::Other(format!(
            "无法解析这份 YAML：{e}。为避免覆盖你的配置，本次没有写入任何文件"
        ))
    })?;
    match value {
        serde_yaml_ng::Value::Mapping(_) => Ok(value),
        serde_yaml_ng::Value::Null => Ok(serde_yaml_ng::Value::Mapping(Default::default())),
        _ => Err(GateError::Other(
            "这份 YAML 的顶层不是映射。为避免覆盖你的配置，本次没有写入任何文件".into(),
        )),
    }
}

/// 读出一个顶层小节现在的值（没有是 `None`）。原文件解析不了就报错。
pub fn section(raw: &str, key: &str) -> Result<Option<serde_yaml_ng::Value>> {
    Ok(parse(raw)?.get(key).cloned())
}

/// 换掉一个顶层小节（没有就追加在末尾），返回新的全文。别的小节、注释照原样。
pub fn replace_section(raw: &str, key: &str, value: &serde_yaml_ng::Value) -> Result<String> {
    parse(raw)?;
    let serialized = serialize_section(key, value)?;
    let out = if let Some((start, end)) = find_section_range(raw, key) {
        let mut result = String::with_capacity(raw.len());
        result.push_str(&raw[..start]);
        result.push_str(&serialized);
        let remainder = remove_all_sections(&raw[end..], key);
        if !serialized.ends_with('\n') && !remainder.is_empty() && !remainder.starts_with('\n') {
            result.push('\n');
        }
        result.push_str(&remainder);
        result
    } else {
        let mut result = raw.to_string();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&serialized);
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result
    };
    // 换完再整份解析一遍：拼坏了就不交出去。
    parse(&out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! 搬自 cc-switch `hermes_config.rs` 的 `find_yaml_section_range` 测试，外加替换、拒写两条。
    use super::*;

    #[test]
    fn find_section_in_multi_section_yaml() {
        let yaml = "model:\n  default: gpt-4\n  provider: openai\nagent:\n  max_turns: 10\ncustom_providers:\n  - name: foo\n";
        let (start, end) = find_section_range(yaml, "agent").unwrap();
        let section = &yaml[start..end];
        assert!(section.starts_with("agent:"));
        assert!(section.contains("max_turns"));
        assert!(!section.contains("custom_providers"));
    }

    #[test]
    fn find_section_at_end_of_file() {
        let yaml = "model:\n  default: gpt-4\nagent:\n  max_turns: 10\n";
        let (start, end) = find_section_range(yaml, "agent").unwrap();
        assert!(yaml[start..end].starts_with("agent:"));
        assert_eq!(end, yaml.len());
    }

    #[test]
    fn find_section_not_found() {
        assert!(find_section_range("model:\n  default: gpt-4\n", "agent").is_none());
    }

    #[test]
    fn find_section_with_comments_between() {
        let yaml = "model:\n  default: gpt-4\n\n# This is a comment\n  # indented comment\n\nagent:\n  max_turns: 10\n";
        let (start, end) = find_section_range(yaml, "model").unwrap();
        let section = &yaml[start..end];
        assert!(section.starts_with("model:"));
        assert!(section.contains("# This is a comment"));
    }

    #[test]
    fn find_section_does_not_match_substring_key() {
        let yaml = "model_extra:\n  foo: bar\nmodel:\n  default: gpt-4\n";
        let (start, _) = find_section_range(yaml, "model").unwrap();
        assert!(yaml[start..].starts_with("model:"));
    }

    #[test]
    fn find_section_handles_crlf() {
        let yaml = "model:\r\n  default: gpt-4\r\nagent:\r\n  max_turns: 10\r\n";
        let (start, end) = find_section_range(yaml, "model").unwrap();
        let section = &yaml[start..end];
        assert!(section.contains("default: gpt-4"));
        assert!(!section.contains("agent:"));
    }

    #[test]
    fn replacing_one_section_keeps_every_other_byte() {
        let yaml = "# my hermes\nmodel:\n  default: gpt-4 # keep\ncustom_providers:\n- name: old\nmcp_servers:\n  fs:\n    command: npx\n";
        let value: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("- name: new\n  base_url: https://x.test\n").unwrap();
        let out = replace_section(yaml, "custom_providers", &value).unwrap();
        assert!(
            out.starts_with("# my hermes\nmodel:\n  default: gpt-4 # keep\ncustom_providers:\n"),
            "{out}"
        );
        assert!(out.contains("name: new"));
        assert!(!out.contains("name: old"));
        assert!(
            out.ends_with("mcp_servers:\n  fs:\n    command: npx\n"),
            "{out}"
        );
        // 没有这个小节就追加在末尾。
        let appended =
            replace_section("model:\n  default: a\n", "custom_providers", &value).unwrap();
        assert!(
            appended.starts_with("model:\n  default: a\ncustom_providers:\n"),
            "{appended}"
        );
    }

    #[test]
    fn a_yaml_file_that_does_not_parse_is_refused() {
        let value = serde_yaml_ng::Value::Null;
        assert!(replace_section("model: [unclosed\n", "x", &value).is_err());
        assert!(replace_section("- just\n- a list\n", "x", &value).is_err());
        assert!(section("", "x").unwrap().is_none());
    }
}
