//! GPT（Codex 桌面端）界面语言：一键设成中文（2026-09-25，使用者定的）。
//!
//! # 只写它自己的设置项
//!
//! Codex 桌面端在「设置 → General → Language」里选中文，写下的就是
//! `CODEX_HOME\config.toml` 里 `[desktop]` 表的一行 `localeOverride = "zh-CN"`
//! （本机 8 月的 `~\.codex\config.toml.bak_*` 里有这一行；openai/codex#23815 里也是这一行）。
//! 这里写的是**同一行、同一个值** —— 替使用者去点那个下拉框，别的什么都不做：
//!
//! - 不碰程序文件（ChatGPT.exe / app.asar）、不注入翻译 —— OpenAI 使用条款不许 modify 它的服务；
//! - 不碰 `auth.json`，也不碰 `config.toml` 里别的任何一项（toml_edit 增量改）；
//! - ⛔ **不碰 `enable_i18n`**。OpenAI 用这个远端开关按账户 / 机器放中文界面
//!   （openai/codex#19239）：开关没放给这个账户时，设了中文界面照样是英文。
//!   那是 OpenAI 的决定，面板如实说，不提供任何绕过（条款：不得 circumvent restrictions）。
//!
//! # 改哪几份（使用者选的：全部）
//!
//! 面板的每个 GPT 槽位（`codex-accounts\<id>\home`）+ 默认那份（`CODEX_HOME` 环境变量，
//! 没设就是 `~\.codex`，开始菜单起的 GPT 用它）+ 以后新建的槽位（[`seed`]，
//! 按 `settings.gpt_ui_zh` 预写）。**中转环境不碰**：它们的 `config.toml` 受环境哈希追踪，
//! 改了会被判成「外部修改」、起不来。
//!
//! # 开着的 GPT 会把改动冲掉
//!
//! 桌面端开着时只在启动那一刻读这一项，而且它自己也写 `[desktop]` 表 —— 开着改，
//! 要么不生效、要么被它写回去。所以：面板起的那份先关（`codex_accounts::close_ours`，
//! 确认框里写明任务会停），写完再按当前槽位重开（命令层做，那里挂看门狗）；
//! 别处起的那份面板不关（2026-09-23 那条规矩），它正开着时默认那份这次不改、如实说。
//! Codex 出站插件在跑时它整份接管着槽位的 `config.toml`、停的时候整份恢复 —— 这时不改。
use crate::error::{GateError, Result};
use qb_accounts::codex;
use qb_install::install::codex_desktop;
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// `config.toml` 里那一项所在的表。
pub const TABLE: &str = "desktop";
/// Codex 桌面端自己的语言设置项。
pub const KEY: &str = "localeOverride";
/// 一键中文写的值 —— 跟在它设置里选「中文（中国）」写下的一模一样。
pub const ZH: &str = "zh-CN";

/// 一份 `CODEX_HOME` 的现状。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CodexLocaleHome {
    /// 界面上显示的名字：槽位名，或者「默认」那一份的说明。
    pub label: String,
    /// 要改的那个 `config.toml` 的完整路径（弹窗里逐个列出来）。
    pub config: String,
    /// 现在 `[desktop] localeOverride` 的值。`None` = 没设，跟随 GPT 自己的默认。
    pub current: Option<String>,
    /// 读不出来的原因（不是 UTF-8、不是有效 TOML）。有值时这一份不改。
    pub error: Option<String>,
    /// GPT 上一次用这份资料运行时自己写下的界面语言（`computer-use\config.json` 的 `locale`）。
    ///
    /// 它说明 GPT **读到了**这项设置，不说明界面真的翻译了 —— 那还要看 `enable_i18n`。
    pub adopted: Option<String>,
    /// 是不是默认那一份（开始菜单、`codex://` 链接起的 GPT 用它）。
    pub is_default: bool,
}

/// 弹窗要的全部事实。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CodexLocaleStatus {
    /// 使用者上次点的是「设为中文」（`settings.gpt_ui_zh`）。新建槽位按它预写。
    pub enabled: bool,
    pub homes: Vec<CodexLocaleHome>,
    /// 面板起的 GPT 桌面端开着几个 —— 改之前要先关，改完重开当前槽位。
    pub ours_running: u32,
    /// 别处起的开着几个 —— 开着的时候默认那一份不改。
    pub foreign_running: u32,
    /// 查不到进程时的原因。**查不到不是「没有」**（§7.17），这时不改。
    pub processes_error: Option<String>,
    /// Codex 出站插件在跑：它整份接管着槽位的 `config.toml`，这时不改。
    pub egress_running: bool,
}

/// 点了之后发生了什么。每一句都是界面原样显示的纯文本。
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct CodexLocaleOutcome {
    /// 真的改了几个文件。
    pub changed: u32,
    /// 逐份说明：改了 / 本来就是 / 跳过（带原因）。
    pub lines: Vec<String>,
    /// 重开 GPT 的结果（命令层填）。没关过就是 `None`。
    pub reopened: Option<String>,
    /// 写完要重开哪个槽位（关之前它正开着）。命令层用，不给界面。
    #[serde(skip)]
    #[ts(skip)]
    pub reopen: Option<String>,
}

/// **纯函数**：设（`Some`）或撤（`None`）`[desktop] localeOverride`，别的一个字不动。
///
/// - 设：没有 `[desktop]` 表就建一张标准表（不是内联表，§7.11）；原来是别的语言也改成给的这个 ——
///   使用者点的就是「设为中文」。
/// - 撤：**只在它的值是 [`ZH`] 时删** —— 使用者在 GPT 里自己选了别的语言，那是他的决定，不是我们的残留。
///   删完 `[desktop]` 空了就连表一起删。
pub fn set_locale_override(text: &str, locale: Option<&str>) -> Result<String> {
    let mut doc = parse(text)?;
    match locale {
        Some(locale) => {
            let table =
                super::turnstate_ops::ensure_table(doc.entry(TABLE).or_insert(toml_edit::table()));
            table[KEY] = toml_edit::value(locale);
        }
        None => {
            let mut drop_table = false;
            if let Some(table) = doc.get_mut(TABLE).and_then(|i| i.as_table_like_mut()) {
                if table.get(KEY).and_then(|v| v.as_str()) == Some(ZH) {
                    table.remove(KEY);
                    drop_table = table.is_empty();
                }
            }
            if drop_table {
                doc.remove(TABLE);
            }
        }
    }
    Ok(doc.to_string())
}

/// **纯函数**：现在设的是什么。没设 = `None`。
pub fn locale_override(text: &str) -> Result<Option<String>> {
    let doc = parse(text)?;
    Ok(doc
        .get(TABLE)
        .and_then(|i| i.as_table_like())
        .and_then(|t| t.get(KEY))
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

fn parse(text: &str) -> Result<toml_edit::DocumentMut> {
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|e| GateError::Other(format!("config.toml 不是有效 TOML：{e}")))
}

/// 一份要处理的 `CODEX_HOME`。
struct Home {
    label: String,
    dir: PathBuf,
    is_default: bool,
}

/// 默认那一份：`CODEX_HOME` 环境变量（使用者自己设过的话，开始菜单起的 GPT 就用它），
/// 没设就是 `~\.codex`。**目录不在就不算** —— 从没在这台机器上用过默认那份的人，
/// 不该因为点了一下就多出一个 `.codex` 目录。
fn default_home() -> Option<PathBuf> {
    let dir = std::env::var_os("CODEX_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))?;
    dir.is_dir().then_some(dir)
}

fn homes() -> Result<Vec<Home>> {
    let root = codex::root();
    let mut out = Vec::new();
    for slot in codex::list(&root)?.slots {
        out.push(Home {
            label: format!("槽位「{}」", slot.label),
            dir: codex::directory(&root, &slot.id)?.join("home"),
            is_default: false,
        });
    }
    if let Some(dir) = default_home() {
        out.push(Home {
            label: format!("默认（{}，开始菜单起的 GPT 用它）", dir.display()),
            dir,
            is_default: true,
        });
    }
    Ok(out)
}

/// GPT 自己写下的界面语言。读不到就是 `None`（没用过电脑操作那项功能的资料里没有这个文件）。
fn adopted(home: &Path) -> Option<String> {
    let bytes = std::fs::read(home.join("computer-use").join("config.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("locale")?.as_str().map(str::to_string)
}

fn describe(home: &Home) -> CodexLocaleHome {
    let config = home.dir.join("config.toml");
    let (current, error) = match read_config(&config) {
        Ok(text) => match locale_override(&text) {
            Ok(v) => (v, None),
            Err(e) => (None, Some(e.to_string())),
        },
        Err(e) => (None, Some(e.to_string())),
    };
    CodexLocaleHome {
        label: home.label.clone(),
        config: config.display().to_string(),
        current,
        error,
        adopted: adopted(&home.dir),
        is_default: home.is_default,
    }
}

/// 文件不在 = 空文本（新槽位、或者默认那份从没写过配置）。
fn read_config(path: &Path) -> Result<String> {
    match crate::config_io::read_optional(path)? {
        None => Ok(String::new()),
        Some(bytes) => String::from_utf8(bytes)
            .map_err(|_| GateError::Other("config.toml 不是 UTF-8 文本".into())),
    }
}

/// 进程数：`(面板起的, 别处起的)`。
fn running(desktop: &codex_desktop::CodexDesktop) -> (u32, u32) {
    let ours = desktop.processes.iter().filter(|p| p.ours).count() as u32;
    let foreign = desktop.processes.iter().filter(|p| !p.ours).count() as u32;
    (ours, foreign)
}

async fn detect() -> Result<codex_desktop::CodexDesktop> {
    tokio::task::spawn_blocking(codex_desktop::detect)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

/// 弹窗要的现状。只读本机文件与进程表，不联网、不改任何东西。
pub async fn status() -> Result<CodexLocaleStatus> {
    let homes = tokio::task::spawn_blocking(|| homes().map(|h| h.iter().map(describe).collect()))
        .await
        .map_err(|e| GateError::Other(e.to_string()))??;
    let (ours_running, foreign_running, processes_error) = match detect().await {
        Ok(d) => {
            let (o, f) = running(&d);
            (o, f, None)
        }
        Err(e) => (0, 0, Some(e.to_string())),
    };
    Ok(CodexLocaleStatus {
        enabled: crate::settings::load().gpt_ui_zh,
        homes,
        ours_running,
        foreign_running,
        processes_error,
        egress_running: crate::plugins::codex_egress::running(),
    })
}

/// 设为中文（`true`）或恢复默认（`false`）。
///
/// 顺序：拦出站插件 → 查进程（查不到就不改）→ 关面板起的那份 → 一次提交改完所有份 →
/// 记下使用者的选择。重开由命令层按返回的 `reopen` 做（那里挂看门狗）。
pub async fn set(enable: bool, gate: &crate::gate::GateState) -> Result<CodexLocaleOutcome> {
    if crate::plugins::codex_egress::running() {
        return Err(GateError::Other(
            "Codex 出站插件正接管着当前槽位的 config.toml（停下时会整份恢复），这时改会被它冲掉。\
             先在扩展中心停掉它，再点一次。"
                .into(),
        ));
    }
    let desktop = detect()
        .await
        .map_err(|e| GateError::Other(format!("查不到 GPT 桌面端有没有在跑，这次没改：{e}")))?;
    let (_, foreign) = running(&desktop);
    // ⛔ 只关、只重开**官方槽位**（以及没槽位时退回的默认那份）起的桌面端（2026-09-25）：改的只是
    // 它们的 config.toml。原来只要面板起的有一份在跑就整份 `close_ours` —— 中转环境起的那份
    // （连同它的任务）也被关掉，事后还按当前槽位重开一个官方的：关掉的是中转、起来的是另一份。
    let root = codex::root();
    let mut official: Vec<(Option<String>, std::path::PathBuf)> = Vec::new();
    for slot in codex::list(&root)?.slots {
        let (_, profile) = crate::workspace::codex_official_dirs(&slot.id)?;
        official.push((Some(slot.id), profile));
    }
    if let Some(home) = dirs::home_dir() {
        official.push((None, home.join(".codex").join("desktop")));
    }
    let running_official: Vec<&(Option<String>, std::path::PathBuf)> = official
        .iter()
        .filter(|(_, profile)| {
            desktop.processes.iter().any(|p| {
                p.ours
                    && p.profile.as_deref().is_some_and(|d| {
                        crate::install::inventory::same_path(std::path::Path::new(d), profile)
                    })
            })
        })
        .collect();
    let reopen = if running_official.is_empty() {
        None
    } else {
        let active = codex::active_id(&root)?;
        // 关掉的里面有当前槽位那一份，才按当前槽位重开 —— 不替使用者起一个他没开着的窗口。
        let was_active = active.as_deref().is_some_and(|a| {
            running_official
                .iter()
                .any(|(id, _)| id.as_deref() == Some(a))
        });
        let profiles = running_official.iter().map(|(_, p)| p.clone()).collect();
        super::codex_accounts::close_official(gate, profiles).await?;
        if was_active {
            active
        } else {
            None
        }
    };
    let mut outcome = tokio::task::spawn_blocking(move || apply(enable, foreign > 0))
        .await
        .map_err(|e| GateError::Other(e.to_string()))??;
    let mut s = crate::settings::load();
    if s.gpt_ui_zh != enable {
        s.gpt_ui_zh = enable;
        super::settings_ops::save(&s)?;
    }
    crate::audit::write(&format!(
        "GPT 界面语言：{}，改了 {} 份 config.toml 的 [desktop] localeOverride",
        if enable {
            "设为中文"
        } else {
            "恢复默认"
        },
        outcome.changed
    ));
    outcome.reopen = reopen;
    Ok(outcome)
}

/// 改所有份。**一次提交**（`config_io::commit` 两阶段、核对读的时候与写的时候是同一份内容），
/// 要么全改、要么一份都不改。
fn apply(enable: bool, skip_default: bool) -> Result<CodexLocaleOutcome> {
    let mut outcome = CodexLocaleOutcome::default();
    let mut edits = Vec::new();
    for home in homes()? {
        if home.is_default && skip_default {
            outcome.lines.push(format!(
                "{}：别处起的 GPT 正开着，这一份这次没改（它开着会把改动写回去）。关掉它再点一次。",
                home.label
            ));
            continue;
        }
        let path = home.dir.join("config.toml");
        let before = crate::config_io::read_optional(&path)?;
        let text = match &before {
            None => String::new(),
            Some(bytes) => match String::from_utf8(bytes.clone()) {
                Ok(t) => t,
                Err(_) => {
                    outcome.lines.push(format!(
                        "{}：config.toml 不是 UTF-8 文本，没改。",
                        home.label
                    ));
                    continue;
                }
            },
        };
        if !enable && before.is_none() {
            outcome
                .lines
                .push(format!("{}：没有设过，不用改。", home.label));
            continue;
        }
        let next = match set_locale_override(&text, enable.then_some(ZH)) {
            Ok(next) => next,
            Err(e) => {
                outcome.lines.push(format!("{}：{e}，没改。", home.label));
                continue;
            }
        };
        if next == text {
            outcome.lines.push(format!(
                "{}：{}",
                home.label,
                if enable {
                    "本来就是中文。"
                } else {
                    "没有面板设的中文，不用改。"
                }
            ));
            continue;
        }
        outcome.lines.push(format!(
            "{}：{}",
            home.label,
            if enable {
                "已设为中文。"
            } else {
                "已撤掉中文，回到 GPT 自己的默认。"
            }
        ));
        edits.push(crate::config_io::Edit {
            path,
            expected: before,
            body: Some(next.into_bytes()),
        });
    }
    outcome.changed = edits.len() as u32;
    if !edits.is_empty() {
        crate::config_io::commit(edits)?;
    }
    Ok(outcome)
}

/// 新建的槽位：使用者选过「设为中文」就预写上（2026-09-25，使用者选的「以后新建的槽位也改」）。
pub fn seed(home: &Path) -> Result<()> {
    if !crate::settings::load().gpt_ui_zh {
        return Ok(());
    }
    let path = home.join("config.toml");
    let text = read_config(&path)?;
    let next = set_locale_override(&text, Some(ZH))?;
    if next != text {
        crate::config_io::replace(&path, Some(next.as_bytes()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `codex::create` 给新槽位写的就是这两行。
    const NEW_SLOT: &str =
        "forced_login_method = \"chatgpt\"\ncli_auth_credentials_store = \"file\"\n";

    /// 本机一份真实槽位 `config.toml` 的形状（值换成了假的）。
    const REAL_SHAPE: &str = r#"# 使用者自己写的注释要留着
forced_login_method = "chatgpt"
model_provider = "custom"

[model_providers.custom]
name = "custom"
base_url = "https://example.invalid/v1"

[desktop]
followUpQueueMode = "queue"
conversationDetailMode = "compact"

[desktop.open-in-target-preferences]
default = "vscode"

[projects.'c:\users\demo\documents\codex']
trust_level = "trusted"
"#;

    fn value(text: &str) -> Option<String> {
        locale_override(text).unwrap()
    }

    #[test]
    fn a_new_slot_gets_a_desktop_table_and_keeps_its_two_lines() {
        let out = set_locale_override(NEW_SLOT, Some(ZH)).unwrap();
        assert!(out.starts_with(NEW_SLOT), "原来那两行一个字不动：{out}");
        assert!(out.contains("[desktop]"));
        assert_eq!(value(&out).as_deref(), Some(ZH));
    }

    #[test]
    fn it_lands_in_the_desktop_table_and_nothing_else_moves() {
        let out = set_locale_override(REAL_SHAPE, Some(ZH)).unwrap();
        assert_eq!(value(&out).as_deref(), Some(ZH));
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        // 落在 [desktop] 本身，不是它的子表。
        assert_eq!(doc["desktop"]["localeOverride"].as_str(), Some(ZH));
        assert!(doc["desktop"]["open-in-target-preferences"]
            .get("localeOverride")
            .is_none());
        // 别的键、注释、表都还在。
        assert!(out.contains("# 使用者自己写的注释要留着"));
        assert_eq!(doc["model_provider"].as_str(), Some("custom"));
        assert_eq!(doc["desktop"]["followUpQueueMode"].as_str(), Some("queue"));
        assert_eq!(
            doc["model_providers"]["custom"]["base_url"].as_str(),
            Some("https://example.invalid/v1")
        );
        assert!(out.contains("[projects.'c:\\users\\demo\\documents\\codex']"));
        // 除了多出来的那一行，逐行都在。
        for line in REAL_SHAPE.lines() {
            assert!(out.contains(line), "丢了一行：{line}");
        }
    }

    #[test]
    fn applying_twice_changes_nothing_the_second_time() {
        let once = set_locale_override(REAL_SHAPE, Some(ZH)).unwrap();
        let twice = set_locale_override(&once, Some(ZH)).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn another_language_is_replaced_when_the_user_asks_for_chinese() {
        let ja = set_locale_override(REAL_SHAPE, Some("ja-JP")).unwrap();
        let out = set_locale_override(&ja, Some(ZH)).unwrap();
        assert_eq!(value(&out).as_deref(), Some(ZH));
    }

    /// 撤只撤我们设的那个值：使用者在 GPT 里自己选的别的语言不动。
    #[test]
    fn restoring_only_removes_our_value() {
        let zh = set_locale_override(REAL_SHAPE, Some(ZH)).unwrap();
        let back = set_locale_override(&zh, None).unwrap();
        assert_eq!(value(&back), None);
        assert!(back.contains("followUpQueueMode"), "[desktop] 里别的键还在");

        let ja = set_locale_override(REAL_SHAPE, Some("ja-JP")).unwrap();
        assert_eq!(set_locale_override(&ja, None).unwrap(), ja);
    }

    #[test]
    fn restoring_a_file_that_never_had_it_is_a_no_op() {
        assert_eq!(set_locale_override(REAL_SHAPE, None).unwrap(), REAL_SHAPE);
        assert_eq!(set_locale_override(NEW_SLOT, None).unwrap(), NEW_SLOT);
    }

    /// 只有我们那一行的 `[desktop]`，撤完连表一起删，文件回到原样。
    #[test]
    fn restoring_drops_the_table_we_created() {
        let zh = set_locale_override(NEW_SLOT, Some(ZH)).unwrap();
        let back = set_locale_override(&zh, None).unwrap();
        assert!(!back.contains("[desktop]"), "{back}");
        assert_eq!(value(&back), None);
    }

    /// §7.11：内联表 `desktop = { … }` 要变成标准表再写，原来的键保留。
    #[test]
    fn an_inline_desktop_table_is_handled() {
        let text = "desktop = { followUpQueueMode = \"queue\" }\n";
        let out = set_locale_override(text, Some(ZH)).unwrap();
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert_eq!(doc["desktop"]["followUpQueueMode"].as_str(), Some("queue"));
        assert_eq!(doc["desktop"]["localeOverride"].as_str(), Some(ZH));
    }

    #[test]
    fn a_broken_file_is_an_error_not_a_rewrite() {
        assert!(set_locale_override("[desktop\nx = ", Some(ZH)).is_err());
        assert!(locale_override("= =").is_err());
    }

    #[test]
    fn an_empty_file_gets_just_our_line() {
        let out = set_locale_override("", Some(ZH)).unwrap();
        assert_eq!(value(&out).as_deref(), Some(ZH));
        assert_eq!(out.trim(), "[desktop]\nlocaleOverride = \"zh-CN\"");
    }
}
