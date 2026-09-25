//! 官方 Codex 的 turn-state「识别」—— **直接作用于当前激活的 Codex 账户槽位**。
//!
//! # 为什么不再造「分身」环境、不再复制 OAuth（0.24.0–0.24.6 的做法，0.24.7 起废弃）
//!
//! 老做法是另建一个环境目录 `qb-router-codex-official`，把槽位的 `auth.json` **复制**一份
//! 进去，再起第二个 Codex 走本机路由。问题在复制：ChatGPT 的 OAuth 刷新令牌是**轮换式**的，
//! 复制件和原件谁先刷新，另一份就作废 —— 迟早把槽位登出。档案 §7.29 在 Claude 上踩过
//! 一模一样的坑。所以现在：**不复制凭证、不另起 Codex 实例**，识别 = 在槽位自己的
//! `config.toml` 里临时把 `model_provider` 指到本机路由，`auth.json` 一个字不碰。
//! 这跟 ccodex-sleep-state 接管 Codex 的方式是同一个套路（备份 + 接管 + 恢复）。
//!
//! # 三条硬约束
//!
//! 1. **只改 `config.toml` 的两处**：`model_provider` 与 `[model_providers.qb_turnstate]`，
//!    其余原样保留（toml_edit 增量改）。关闭时**反向恢复这两处**，不整份覆盖 —— Codex 在
//!    接管期间自己写进去的项目授权 / MCP / 偏好都留着。整份盖回备份正是 §7.10 那类事故的形状。
//! 2. **接管了哪个槽位、之前的 `model_provider` 是什么，落盘在 marker 里。** 关闭按 marker
//!    恢复，使用者中途切了槽位也不会恢复错。marker 在，就是「识别开着」。
//! 3. **面板启动时若发现 marker（上次没正常关闭），自动关闭识别并恢复。** 本机路由随面板
//!    消失，留着 marker 会让那个槽位的 Codex 对着一个死端口（503）。这也是「默认关闭、
//!    只在使用者手动开启后生效」那条硬约束的落地。
//!
//! 本机路由那头（`arm_official_codex` / `OAuthPassthrough`）不在这里，在命令层一并编排。

use crate::error::{GateError, Result};
use std::path::{Path, PathBuf};

// marker（`Takeover` / `takeover()` / `PROVIDER_ID`）0.25.0 起住在 `crate::turnstate_marker`
// —— `workspace::launch` 也要读它，放这里会跟 `usecase → workspace` 成环。原样再导出，
// 调用点（`commands/station.rs`、`commands/plugins.rs`、`lib.rs`）不用改。
pub use crate::turnstate_marker::{takeover, Takeover, PROVIDER_ID};
const PROVIDER_NAME: &str = "QB Gate 识别（本机路由）";
const BACKUP_NAME: &str = "config.toml.qb-turnstate-backup";

/// 把 `item` 保证成标准表（不是内联表）。§7.11 的坑：链式索引在空文档上建出来的是内联表，
/// `as_table_mut()` 对它返回 `None`，于是「删掉另一种」那类逻辑会静默失效。
///
/// `pub(crate)`：GPT 界面语言（`codex_locale`）改同一个 `config.toml` 的 `[desktop]` 表，用同一个。
pub(crate) fn ensure_table(item: &mut toml_edit::Item) -> &mut toml_edit::Table {
    if let toml_edit::Item::Value(toml_edit::Value::InlineTable(_)) = item {
        let taken = std::mem::replace(item, toml_edit::table());
        if let toml_edit::Item::Value(toml_edit::Value::InlineTable(inline)) = taken {
            *item = toml_edit::Item::Table(inline.into_table());
        }
    }
    if !item.is_table() {
        *item = toml_edit::table();
    }
    item.as_table_mut().expect("just made it a table")
}

/// **纯函数**：把槽位的 `config.toml` 改成走本机路由。返回（新文本，接管前的 `model_provider`）。
///
/// - `model_provider = "qb_turnstate"`；
/// - `[model_providers.qb_turnstate]`：`base_url` 指路由的官方线（**不带 `/v1`**，官方端点是
///   `.../backend-api/codex/responses`）、`wire_api = "responses"`、`requires_openai_auth = true`、
///   不写 `env_key`（没有占位 Key，身份全靠槽位自己的 OAuth）；
/// - 别的一个字不动。
///
/// 幂等：已经是 `qb_turnstate` 时返回的 previous 是 `None`，调用方要用 marker 里记的那个。
pub fn apply_to_config(text: &str, base_url: &str) -> Result<(String, Option<String>)> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| GateError::Other(format!("槽位 config.toml 不是有效 TOML：{e}")))?;
    let previous = doc
        .get("model_provider")
        .and_then(|v| v.as_str())
        .filter(|s| *s != PROVIDER_ID)
        .map(str::to_string);
    doc["model_provider"] = toml_edit::value(PROVIDER_ID);
    let providers = ensure_table(doc.entry("model_providers").or_insert(toml_edit::table()));
    providers.set_implicit(true);
    let p = ensure_table(providers.entry(PROVIDER_ID).or_insert(toml_edit::table()));
    p["name"] = toml_edit::value(PROVIDER_NAME);
    p["base_url"] = toml_edit::value(base_url);
    p["wire_api"] = toml_edit::value("responses");
    p["requires_openai_auth"] = toml_edit::value(true);
    p.remove("env_key");
    Ok((doc.to_string(), previous))
}

/// **纯函数**：反向恢复。删掉 `[model_providers.qb_turnstate]`；`model_provider` 当前若仍是
/// `qb_turnstate` 就改回 `previous`（`None` → 删掉这个键，回到官方默认）；使用者接管期间
/// 自己把它改成了别的，就不动 —— 那是他的新决定，不是我们的残留。
pub fn revert_config(text: &str, previous: Option<&str>) -> Result<String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| GateError::Other(format!("槽位 config.toml 不是有效 TOML：{e}")))?;
    if doc.get("model_provider").and_then(|v| v.as_str()) == Some(PROVIDER_ID) {
        match previous {
            Some(prev) => doc["model_provider"] = toml_edit::value(prev),
            None => {
                doc.remove("model_provider");
            }
        }
    }
    let mut drop_parent = false;
    if let Some(providers) = doc
        .get_mut("model_providers")
        .and_then(|i| i.as_table_mut())
    {
        providers.remove(PROVIDER_ID);
        drop_parent = providers.is_empty();
    }
    if drop_parent {
        doc.remove("model_providers");
    }
    Ok(doc.to_string())
}

fn active_slot() -> Result<(String, String, PathBuf, bool)> {
    let root = qb_accounts::codex::root();
    let accounts = qb_accounts::codex::list(&root)?;
    let active = accounts.slots.iter().find(|s| s.active).ok_or_else(|| {
        GateError::Other(
            "没有激活的 Codex 账户槽位。先在「官方账户 · Codex」里新建并用 ChatGPT 登录一个。"
                .into(),
        )
    })?;
    let home = qb_accounts::codex::directory(&root, &active.id)?.join("home");
    Ok((
        active.id.clone(),
        active.label.clone(),
        home,
        active.logged_in,
    ))
}

/// 开启识别：接管当前激活槽位的 `config.toml`。
///
/// 备份（`config.toml.qb-turnstate-backup`，每次新接管都重写，反映接管前的状态）→ 增量改 →
/// 写 marker。已经接管着同一个槽位就重放一遍改动（Codex 若把它改回去了，再改过来），
/// 接管着**别的**槽位则拒绝 —— 先关再开，否则 marker 里那份恢复信息就对不上了。
pub fn enable(base_url: &str) -> Result<Takeover> {
    let (slot_id, slot_label, home, logged_in) = active_slot()?;
    if let Some(existing) = takeover() {
        if existing.slot_id != slot_id {
            return Err(GateError::Other(format!(
                "识别正作用于槽位「{}」，而当前激活的是「{}」。先关闭识别（会恢复那个槽位的配置），再对这个槽位开启。",
                existing.slot_label, slot_label
            )));
        }
    }
    if !logged_in {
        return Err(GateError::Other(format!(
            "槽位「{slot_label}」还没用 ChatGPT 登录。先在账户页打开它登录，再开启识别 —— 识别靠槽位自己的登录身份打官方端点。"
        )));
    }
    if !home.is_dir() {
        return Err(GateError::Other(format!(
            "槽位目录不存在：{}",
            home.display()
        )));
    }
    let config = home.join("config.toml");
    let text = crate::config_io::read_text(&config)?;
    let fresh = takeover().is_none();
    if fresh {
        // 接管前的原样，留一份给人看 / 手工兜底；关闭走的是反向恢复，不整份盖回来。
        crate::config_io::replace(&home.join(BACKUP_NAME), Some(text.as_bytes()))?;
    }
    let (next, previous) = apply_to_config(&text, base_url)?;
    let previous = if fresh {
        previous
    } else {
        takeover().and_then(|t| t.previous_model_provider)
    };
    crate::config_io::replace(&config, Some(next.as_bytes()))?;
    let take = Takeover {
        slot_id,
        slot_label,
        home: home.display().to_string(),
        previous_model_provider: previous,
        applied_at: chrono::Utc::now().to_rfc3339(),
    };
    crate::turnstate_marker::write(&take)?;
    crate::audit::write(&format!(
        "识别已开启：槽位「{}」的 config.toml 已指向本机路由（{}）",
        take.slot_label, base_url
    ));
    Ok(take)
}

/// 关闭识别：按 marker 反向恢复那个槽位的 `config.toml`，再删 marker。
/// 没有 marker 就什么都不做（`Ok(None)`）。恢复失败时**保留 marker**，让使用者能再点一次。
pub fn disable() -> Result<Option<Takeover>> {
    let Some(take) = takeover() else {
        return Ok(None);
    };
    let config = Path::new(&take.home).join("config.toml");
    match crate::config_io::read_optional(&config)? {
        Some(bytes) => {
            let text = String::from_utf8(bytes)
                .map_err(|e| GateError::Other(format!("槽位 config.toml 不是 UTF-8：{e}")))?;
            let reverted = revert_config(&text, take.previous_model_provider.as_deref())?;
            crate::config_io::replace(&config, Some(reverted.as_bytes()))?;
        }
        None => {
            // 文件没了（槽位被删 / 移走）：没什么可恢复的，只把 marker 清掉。
        }
    }
    crate::turnstate_marker::clear()?;
    crate::audit::write(&format!(
        "识别已关闭：槽位「{}」的 config.toml 已恢复",
        take.slot_label
    ));
    Ok(Some(take))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://127.0.0.1:15721/codex";

    /// 接管只改两处，其余原样；base 不带 /v1；不写 env_key / 占位 Key。
    #[test]
    fn takeover_touches_only_model_provider_and_its_table() {
        let src = "forced_login_method = \"chatgpt\"\ncli_auth_credentials_store = \"file\"\nmodel = \"gpt-5.6-sol\"\n\n[projects.'c:\\\\users\\\\me']\ntrust_level = \"trusted\"\n";
        let (out, previous) = apply_to_config(src, BASE).unwrap();
        assert_eq!(previous, None, "原来没写 model_provider");
        assert!(out.contains("model_provider = \"qb_turnstate\""), "{out}");
        assert!(out.contains("[model_providers.qb_turnstate]"), "{out}");
        assert!(
            out.contains("base_url = \"http://127.0.0.1:15721/codex\""),
            "{out}"
        );
        assert!(!out.contains("/codex/v1"), "官方 base 不许带 /v1：{out}");
        assert!(out.contains("requires_openai_auth = true"), "{out}");
        assert!(!out.contains("env_key"), "{out}");
        assert!(!out.contains("forced_login_method = \"api\""), "{out}");
        // 原有内容一个字不少。
        for keep in [
            "forced_login_method = \"chatgpt\"",
            "cli_auth_credentials_store = \"file\"",
            "model = \"gpt-5.6-sol\"",
            "trust_level = \"trusted\"",
        ] {
            assert!(out.contains(keep), "丢了：{keep}\n{out}");
        }
    }

    /// 记住接管前的 provider；反向恢复把它改回去、把我们的表删掉，别的不动。
    #[test]
    fn revert_restores_previous_provider_and_keeps_codex_edits() {
        let src = "model_provider = \"myrelay\"\n\n[model_providers.myrelay]\nname = \"mine\"\nbase_url = \"https://relay.example/v1\"\n";
        let (taken, previous) = apply_to_config(src, BASE).unwrap();
        assert_eq!(previous.as_deref(), Some("myrelay"));
        // 接管期间 Codex 自己加了东西 —— 恢复时必须留着。
        let during = format!("{taken}\n[mcp_servers.node_repl]\ncommand = 'node_repl.exe'\n");
        let back = revert_config(&during, previous.as_deref()).unwrap();
        assert!(back.contains("model_provider = \"myrelay\""), "{back}");
        assert!(!back.contains("qb_turnstate"), "{back}");
        assert!(
            back.contains("[model_providers.myrelay]"),
            "使用者自己的 provider 不能被删：{back}"
        );
        assert!(
            back.contains("[mcp_servers.node_repl]"),
            "Codex 后写的东西不能被删：{back}"
        );
    }

    /// 原来没写 model_provider 的，恢复后也不该多出一个键；空掉的 model_providers 一并收走。
    #[test]
    fn revert_to_no_provider_leaves_no_residue() {
        let src = "model = \"gpt-5.6-sol\"\n";
        let (taken, previous) = apply_to_config(src, BASE).unwrap();
        let back = revert_config(&taken, previous.as_deref()).unwrap();
        assert!(!back.contains("model_provider"), "{back}");
        assert!(!back.contains("model_providers"), "{back}");
        assert!(back.contains("model = \"gpt-5.6-sol\""), "{back}");
    }

    /// 使用者接管期间自己把 provider 改成了别的：那是他的新决定，恢复时不动它，只删我们的表。
    #[test]
    fn revert_does_not_clobber_a_provider_the_user_changed_meanwhile() {
        let src = "model_provider = \"old\"\n[model_providers.old]\nname = \"o\"\n";
        let (taken, previous) = apply_to_config(src, BASE).unwrap();
        let changed = taken.replace(
            "model_provider = \"qb_turnstate\"",
            "model_provider = \"newer\"",
        );
        let back = revert_config(&changed, previous.as_deref()).unwrap();
        assert!(back.contains("model_provider = \"newer\""), "{back}");
        assert!(!back.contains("[model_providers.qb_turnstate]"), "{back}");
    }

    /// 幂等：接管两次不重复、不破坏；内联表（§7.11）也能正确升级成标准表。
    #[test]
    fn applying_twice_is_idempotent_and_inline_tables_are_upgraded() {
        let src = "model_providers = { other = { name = \"x\" } }\n";
        let (once, _) = apply_to_config(src, BASE).unwrap();
        let (twice, prev2) = apply_to_config(&once, BASE).unwrap();
        assert_eq!(prev2, None, "第二次看到的是 qb_turnstate，不算 previous");
        assert_eq!(
            once.matches("qb_turnstate]").count(),
            twice.matches("qb_turnstate]").count()
        );
        assert!(twice.contains("[model_providers.qb_turnstate]"), "{twice}");
        assert!(
            twice.contains("other"),
            "内联表里原有的 provider 不能丢：{twice}"
        );
        // 能被 toml 再解析（没写坏）。
        twice.parse::<toml_edit::DocumentMut>().unwrap();
    }
}
