//! GPT（Codex 桌面端）界面语言：一键设成中文（2026-09-25，使用者定的）。
//!
//! # 只写它自己的设置项
//!
//! Codex 桌面端在「设置 → General → Language」里选中文，写下的就是
//! `CODEX_HOME\config.toml` 里 `[desktop]` 表的一行 `localeOverride = "zh-CN"`
//! （本机 8 月的 `~\.codex\config.toml.bak_*` 里有这一行；26.924 的 `app.asar` 里设置项的键路径是
//! `desktop.<键>`，`zh` / `zh-hans` 都规范成 `zh-CN`）。
//! 这里写的是**同一行、同一个值** —— 替使用者去点那个下拉框，别的什么都不做：
//!
//! - 不碰程序文件（ChatGPT.exe / app.asar）、不注入翻译 —— OpenAI 使用条款不许 modify 它的服务；
//! - 不碰 `auth.json`，也不碰 `config.toml` 里别的任何一项的值（toml_edit 增量改）—— 唯一的例外见下一节；
//! - ⛔ **不碰 `enable_i18n`**。OpenAI 用这个远端开关按账户 / 机器放中文界面
//!   （openai/codex#19239；26.924 的渲染层默认按「关」处理）：开关没放给这个账户时，设了中文界面照样是英文。
//!   那是 OpenAI 的决定，面板如实说，不提供任何绕过（条款：不得 circumvent restrictions）。
//!
//! # GPT 自己把 config.toml 写坏了：先修好再写（2026-09-27，使用者定的）
//!
//! GPT 桌面端读 `config.toml` 用的是严格解析器（26.924 的 `app.asar`：解析失败就记
//! 「Failed to load desktop settings from config.toml」、当成一张空表）—— **同一张表重复定义一次，
//! 整份桌面设置它就一项都读不到**，语言那一行写得再对也白写。本机 09-26 的槽位里就有 GPT 自己写出来的
//! 重复 `[desktop]` 与重复 `[mcp_servers.node_repl.env]`；09-26 那版只合并 `[desktop]`，写出来的文件
//! GPT 照样读不了，面板却报「已设为中文」（坑 7.101）。
//!
//! 现在 [`repair_duplicate_tables`] 只修「同一个表头出现了多次」这一种：让 toml_edit 自己指出是哪一行
//! 重复了（它认得出那行确实是表头，不会被多行数组里的 `[` 骗到），后面那段的键并进第一段 —— 同键同值丢掉，
//! **同键不同值就不修**（哪个才是 GPT 的意思，面板说不准）；修完再用严格解析器读一遍，每个值都还在它该在的
//! 地方才算。这是「别的一项都不动」的唯一例外：不修，GPT 就读不到这一行。
//!
//! # 改哪几份（使用者选的：全部）
//!
//! 面板的每个 GPT 槽位（`codex-accounts\<id>\home`）+ 默认那份（`CODEX_HOME` 环境变量，
//! 没设就是 `~\.codex`，开始菜单起的 GPT 用它）+ 以后新建的槽位（[`seed`]，
//! 按 `settings.gpt_ui_zh` 预写）。**中转环境不碰**：它们的 `config.toml` 受环境哈希追踪，
//! 改了会被判成「外部修改」、起不来。
//!
//! # GPT 开着的时候
//!
//! 桌面端只在启动那一刻读这一项（26.924：运行中改 `config.toml` 它不重读；在它自己的
//! 「设置 → General → Language」里选中文才是立刻生效 —— 界面上把这条路告诉使用者）。所以：
//! **先空跑一遍算出哪几份真要写**，一份都不用写就谁都不关；要写的那几个槽位里、面板起的那份先关
//! （`codex_accounts::close_official`，确认框里写明任务会停），没有外部窗口时写完再按当前槽位重开
//! （命令层做，那里挂看门狗）；别处起的那份面板不关（2026-09-23 那条规矩），它正开着时默认那份这次不改、
//! 如实说，同时不自动再开面板槽位，避免汉化动作旁边出现第二个独立窗口。
//! Codex 出站插件在跑时它整份接管着槽位的 `config.toml`、停的时候整份恢复 —— 这时不改。
use crate::error::{GateError, Result};
use qb_accounts::codex;
use qb_install::install::codex_desktop;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, Value};
use ts_rs::TS;

/// `config.toml` 里那一项所在的表。
pub const TABLE: &str = "desktop";
/// Codex 桌面端自己的语言设置项。
pub const KEY: &str = "localeOverride";
/// 一键中文写的值 —— 跟在它设置里选「中文（中国）」写下的一模一样。
pub const ZH: &str = "zh-CN";

/// 修重复表时给重复的那一段临时起的表名（并完就删掉，从不落盘）。
const PLACEHOLDER: &str = "__qb_gate_duplicate_table_";
/// 一份文件里最多修几张重复表。再多就不是「GPT 写重复了一两张」那回事了，不猜。
const MAX_DUPLICATES: usize = 32;

/// 一份 `CODEX_HOME` 的现状。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CodexLocaleHome {
    /// 界面上显示的名字：槽位名，或者「默认」那一份的说明。
    pub label: String,
    /// 要改的那个 `config.toml` 的完整路径（弹窗里逐个列出来）。
    pub config: String,
    /// 现在 `[desktop] localeOverride` 的值 —— **GPT 读得到的那个**。`None` = 没设，
    /// 或者这份 GPT 现在读不了（见 `repair` / `error`）。
    pub current: Option<String>,
    /// 读不出来、也修不了的原因。有值时这一份不改。
    pub error: Option<String>,
    /// GPT 自己把这份写坏了（这几张表重复定义，写法如 `[desktop]、[mcp_servers.x]`），它现在读不到里面
    /// 任何一项桌面设置；点「设为中文」会先把重复段并掉（不改任何值）。
    pub repair: Option<String>,
    /// GPT 上一次用这份资料运行时自己写下的界面语言（`computer-use\config.json` 的 `locale`）。
    ///
    /// 26.924 里它是「`localeOverride` 有就用它，没有就用系统界面语言」：是 zh-CN 说明 GPT **读到了**
    /// 这项设置，不说明界面真的翻译了 —— 那还要看 `enable_i18n`。
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
    /// 面板起的 GPT 桌面端开着几个 —— 要写的那几份先关，改完重开当前槽位。
    pub ours_running: u32,
    /// 别处起的开着几个 —— 开着的时候默认那一份不改。
    pub foreign_running: u32,
    /// 查不到进程时的原因。**查不到不是「没有」**（§7.17），这时不改。
    pub processes_error: Option<String>,
    /// Codex 出站插件在跑：它整份接管着槽位的 `config.toml`，这时不改。
    pub egress_running: bool,
}

/// 点了之后发生了什么。界面按这几个数说成功还是失败（**不去猜那几句话**）；`lines` 原样显示。
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct CodexLocaleOutcome {
    /// 真的写了、而且回读核对过的份数。
    pub changed: u32,
    /// 本来就是（设中文时已是 zh-CN；恢复时没有面板设的 zh-CN），不用写的份数。
    pub already: u32,
    /// 写了的那几份里，顺手修了 GPT 自己写重复的表的有几份。
    pub repaired: u32,
    /// 这次没改成的份数：读不了 / 修不了 / 默认那份被别处起的 GPT 占着 / 写完回读不对。
    pub skipped: u32,
    /// 默认那份因为别处起的 GPT 开着，这次没改。
    pub default_busy: bool,
    /// 逐份说明。每一句都是界面原样显示的纯文本。
    pub lines: Vec<String>,
    /// 重开 GPT 的结果（命令层填）。没关过就是 `None`。
    pub reopened: Option<String>,
    /// 写完要重开哪个槽位（关之前它正开着）。命令层用，不给界面。
    #[serde(skip)]
    #[ts(skip)]
    pub reopen: Option<String>,
}

// ---------------------------------------------------------------- 纯函数：读、写那一项

/// **纯函数**：设（`Some`）或撤（`None`）`[desktop] localeOverride`，别的一个字不动。
/// 只收有效的 TOML —— GPT 自己读不了的文件先过 [`repair_duplicate_tables`]。
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
    Ok(keep_line_endings(text, doc.to_string()))
}

/// **纯函数**：现在设的是什么。没设 = `None`。GPT 读不了的文件（不是有效 TOML）是 `Err`。
pub fn locale_override(text: &str) -> Result<Option<String>> {
    let doc = parse(text)?;
    Ok(doc
        .get(TABLE)
        .and_then(|i| i.as_table_like())
        .and_then(|t| t.get(KEY))
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

fn parse(text: &str) -> Result<DocumentMut> {
    text.parse::<DocumentMut>().map_err(|e| {
        GateError::Other(format!(
            "config.toml 不是有效 TOML：{}",
            one_line(&e.to_string())
        ))
    })
}

/// toml_edit 的报错是好几行的（行号、那一行原文、箭头、原因）。界面只要一行：位置 + 原因。
fn one_line(error: &str) -> String {
    let lines: Vec<&str> = error
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    match (lines.first(), lines.last()) {
        (Some(first), Some(last)) if first != last => format!("{first}：{last}"),
        (Some(first), _) => first.to_string(),
        _ => error.trim().to_string(),
    }
}

/// 原文件是纯 CRLF 的话，改出来的也用 CRLF（toml_edit 新写的行是 LF）；混着的或 LF 的原样。
fn keep_line_endings(original: &str, out: String) -> String {
    let lf = original.matches('\n').count();
    let crlf = original.matches("\r\n").count();
    if crlf > 0 && crlf == lf {
        out.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        out
    }
}

// ---------------------------------------------------------------- 纯函数：修 GPT 自己写重复的表

/// [`repair_duplicate_tables`] 的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// 本来就是有效 TOML，一个字不用动。
    Clean,
    /// 修好了：`text` 是修过的全文，`tables` 是并掉了重复段的表（`[desktop]` 这样的写法）。
    Repaired { text: String, tables: Vec<String> },
    /// 修不了。原因是界面原样显示的纯文本（不带句号，调用方接着写）。
    Unrepairable { reason: String },
}

fn unrepairable(reason: impl Into<String>) -> Repair {
    Repair::Unrepairable {
        reason: reason.into(),
    }
}

/// **纯函数**：GPT 自己写出了重复的表（同一个 `[a.b]` 表头出现不止一次）时，把后面那几段并进第一段。
///
/// 1. 让 toml_edit 指出重复的那一行（报错的位置），给它换一个临时表名，直到整份解析得过去 ——
///    认表头的是解析器自己，多行数组里长得像表头的那一行不会被误认；
/// 2. 临时表里的键并回它该在的那张表：那张表没有的挪过去（键与值原样），同键同值丢掉，
///    **同键不同值就不修**；
/// 3. 修完的文本用严格解析器再读一遍，拿「并之前每个值在哪」对「并之后每个值在哪」逐项比，
///    一项对不上就不修。
///
/// 别的毛病（缺引号、同一张表里写了两遍同一个键……）不是这里修得了的，照实说读不了。
pub fn repair_duplicate_tables(text: &str) -> Repair {
    let first_error = match text.parse::<DocumentMut>() {
        Ok(_) => return Repair::Clean,
        Err(e) => e,
    };
    let unreadable = || {
        unrepairable(format!(
            "GPT 自己也读不了这份 config.toml（{}），又不是「表写重复了」那一种，面板不猜",
            one_line(&first_error.to_string())
        ))
    };

    let mut work = text.to_string();
    let mut dups: Vec<(Vec<String>, String)> = Vec::new();
    let mut doc = loop {
        match work.parse::<DocumentMut>() {
            Ok(doc) => break doc,
            Err(e) => {
                if dups.len() >= MAX_DUPLICATES {
                    return unreadable();
                }
                let Some(span) = e.span() else {
                    return unreadable();
                };
                let (start, end) = line_bounds(&work, span.start);
                let line = &work[start..end];
                let Some(path) = header_path(line) else {
                    return unreadable();
                };
                let placeholder = format!("{PLACEHOLDER}{}__", dups.len());
                let content = line.trim_end_matches(['\r', '\n']).len();
                work.replace_range(start..start + content, &format!("[{placeholder}]"));
                dups.push((path, placeholder));
            }
        }
    };

    // 并之前：每个值在哪（临时表里的，算到它该在的那张表下）。
    let expected = relocated(&doc, &dups);

    let mut tables: Vec<String> = Vec::new();
    for (path, placeholder) in &dups {
        let name = format!("[{}]", path.join("."));
        let moved: Vec<(toml_edit::Key, Item)> = match doc.get(placeholder).and_then(Item::as_table)
        {
            Some(t) => t
                .iter()
                .filter_map(|(k, _)| t.get_key_value(k))
                .map(|(key, item)| (key.clone(), item.clone()))
                .collect(),
            None => return unreadable(),
        };
        let Some(target) = table_at_mut(&mut doc, path) else {
            return unrepairable(format!(
                "{name} 重复了，可它不是一张普通的表，面板不自动合并"
            ));
        };
        for (key, item) in moved {
            match target.get(key.get()) {
                Some(existing) if canonical_item(existing) == canonical_item(&item) => {}
                Some(_) => {
                    return unrepairable(format!(
                        "{name} 写了两遍，而两遍里 {} 的值不一样 —— 哪个才是 GPT 的意思面板说不准，不修",
                        key.get()
                    ))
                }
                None => {
                    target.insert_formatted(&key, item);
                }
            }
        }
        doc.remove(placeholder);
        if !tables.contains(&name) {
            tables.push(name);
        }
    }

    let out = keep_line_endings(text, doc.to_string());
    let Ok(check) = out.parse::<DocumentMut>() else {
        return unrepairable("修完的文件仍然读不了，没动它");
    };
    let mut actual = BTreeMap::new();
    flatten(&[], check.as_table(), &mut actual);
    if actual != expected {
        return unrepairable("修完之后有值对不上，没动它");
    }
    Repair::Repaired { text: out, tables }
}

/// `pos` 所在那一行：（行首, 行尾含换行符）。
fn line_bounds(text: &str, pos: usize) -> (usize, usize) {
    let pos = pos.min(text.len());
    let start = text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[pos..]
        .find('\n')
        .map(|i| pos + i + 1)
        .unwrap_or(text.len());
    (start, end)
}

/// 一行表头（`[a.b]`，不是 `[[数组表]]`）指的是哪张表：键的路径（引号去掉）。不是表头就是 `None`。
fn header_path(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.starts_with('[') || t.starts_with("[[") {
        return None;
    }
    let doc = format!("{t}\n").parse::<DocumentMut>().ok()?;
    let mut path = Vec::new();
    let mut table = doc.as_table();
    loop {
        let mut items = table.iter();
        let (key, item) = items.next()?;
        if items.next().is_some() {
            return None;
        }
        let child = item.as_table()?;
        path.push(key.to_string());
        if !child.is_implicit() {
            return Some(path);
        }
        table = child;
    }
}

fn table_at_mut<'a>(doc: &'a mut DocumentMut, path: &[String]) -> Option<&'a mut Table> {
    let mut table = doc.as_table_mut();
    for segment in path {
        table = table.get_mut(segment)?.as_table_mut()?;
    }
    Some(table)
}

/// 并之前每个值在哪：临时表里的，算到它该在的那张表下。
fn relocated(doc: &DocumentMut, dups: &[(Vec<String>, String)]) -> BTreeMap<Vec<String>, String> {
    let mut flat = BTreeMap::new();
    flatten(&[], doc.as_table(), &mut flat);
    flat.into_iter()
        .map(|(path, value)| {
            match dups
                .iter()
                .find(|(_, placeholder)| path.first() == Some(placeholder))
            {
                Some((target, _)) => {
                    let mut p = target.clone();
                    p.extend(path.into_iter().skip(1));
                    (p, value)
                }
                None => (path, value),
            }
        })
        .collect()
}

/// 整份文档摊平：每个值的完整键路径 → 规范写法。空表不出现（它没有值）。
fn flatten(prefix: &[String], table: &Table, out: &mut BTreeMap<Vec<String>, String>) {
    for (key, item) in table.iter() {
        let mut path = prefix.to_vec();
        path.push(key.to_string());
        match item {
            Item::Table(t) => flatten(&path, t, out),
            Item::ArrayOfTables(a) => {
                for (i, t) in a.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(format!("[{i}]"));
                    flatten(&p, t, out);
                }
            }
            Item::Value(v) => {
                out.insert(path, canonical_value(v));
            }
            Item::None => {}
        }
    }
}

/// 值的规范写法：只看值本身，不看空白、注释、引号风格 —— 拿来判断两段里的同一个键是不是同一个值。
fn canonical_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{:?}", s.value()),
        Value::Integer(i) => i.value().to_string(),
        Value::Float(f) => format!("{:?}", f.value()),
        Value::Boolean(b) => b.value().to_string(),
        Value::Datetime(d) => format!("datetime {}", d.value()),
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical_value).collect::<Vec<_>>().join(",")
        ),
        Value::InlineTable(t) => {
            let mut parts: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{k:?}={}", canonical_value(v)))
                .collect();
            parts.sort();
            format!("{{{}}}", parts.join(","))
        }
    }
}

fn canonical_item(item: &Item) -> String {
    match item {
        Item::None => String::new(),
        Item::Value(v) => canonical_value(v),
        Item::Table(t) => canonical_table(t),
        Item::ArrayOfTables(a) => format!(
            "[{}]",
            a.iter().map(canonical_table).collect::<Vec<_>>().join(",")
        ),
    }
}

fn canonical_table(t: &Table) -> String {
    let mut parts: Vec<String> = t
        .iter()
        .map(|(k, i)| format!("{k:?}={}", canonical_item(i)))
        .collect();
    parts.sort();
    format!("{{{}}}", parts.join(","))
}

// ---------------------------------------------------------------- 纯函数：一份该怎么办

/// 一份 `config.toml` 这次该怎么办。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// 要写：写成 `next`；`repaired` 是顺手并掉的重复表（空 = 没修）。
    Write { next: String, repaired: Vec<String> },
    /// 本来就是（设中文时已是 zh-CN；恢复时没有面板设的 zh-CN）。
    Already,
    /// 这一份不改，原因是界面原样显示的纯文本（不带句号）。
    Skip(String),
}

/// **纯函数**：拿文件现在的内容（`None` = 文件不在）决定这一份怎么办。
///
/// ⛔ 恢复默认时，一份 GPT 自己写坏了、里面又没有面板设的中文的文件**不去修** —— 那不是面板弄坏的，
/// 恢复默认也用不着碰它。设中文时要修：不修 GPT 就读不到这一行。
pub fn plan_text(text: Option<&str>, enable: bool) -> Plan {
    let original = text.unwrap_or("");
    if !enable && text.is_none() {
        return Plan::Already;
    }
    let (base, repaired) = match repair_duplicate_tables(original) {
        Repair::Clean => (original.to_string(), Vec::new()),
        Repair::Repaired { text, tables } => (text, tables),
        Repair::Unrepairable { reason } => return Plan::Skip(reason),
    };
    let next = match set_locale_override(&base, enable.then_some(ZH)) {
        Ok(next) => next,
        Err(e) => return Plan::Skip(e.to_string()),
    };
    if next == original || (!enable && next == base) {
        return Plan::Already;
    }
    Plan::Write { next, repaired }
}

// ---------------------------------------------------------------- 本机的那几份

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
    let (current, error, repair) = match read_config(&config) {
        Err(e) => (None, Some(e.to_string()), None),
        Ok(text) => match repair_duplicate_tables(&text) {
            Repair::Clean => match locale_override(&text) {
                Ok(v) => (v, None, None),
                Err(e) => (None, Some(e.to_string()), None),
            },
            // GPT 现在读不了它 —— 里面写着什么都不算数，`current` 如实给 None。
            Repair::Repaired { tables, .. } => (None, None, Some(tables.join("、"))),
            Repair::Unrepairable { reason } => (None, Some(reason), None),
        },
    };
    CodexLocaleHome {
        label: home.label.clone(),
        config: config.display().to_string(),
        current,
        error,
        repair,
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

/// 一份这次该怎么办（读文件那一步在这里，决定在 [`plan_text`]）。
fn plan_home(home: &Home, enable: bool, skip_default: bool) -> Result<(Plan, Option<Vec<u8>>)> {
    if home.is_default && skip_default {
        return Ok((
            Plan::Skip(
                "别处起的 GPT 正开着，这一份这次没改（它只在启动时读这一项，开着改了也不生效）。\
                 完全退出它（托盘里也退）再点一次"
                    .into(),
            ),
            None,
        ));
    }
    let before = crate::config_io::read_optional(&home.dir.join("config.toml"))?;
    let plan = match &before {
        None => plan_text(None, enable),
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => plan_text(Some(text), enable),
            Err(_) => Plan::Skip("config.toml 不是 UTF-8 文本".into()),
        },
    };
    Ok((plan, before))
}

/// 设为中文（`true`）或恢复默认（`false`）。
///
/// 顺序：拦出站插件 → 查进程（查不到就不改）→ **空跑一遍，算出哪几份真要写** →
/// 只关要写的那几个槽位里面板起的那份 → 一次提交改完 → 回读核对 → 记下使用者的选择。
/// 重开由命令层按返回的 `reopen` 做（那里挂看门狗）。
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
    let skip_default = foreign > 0;

    // 空跑：只读，算出哪几份真要写。一份都不用写就谁都不关 —— 使用者多半正在 GPT 里干活。
    let to_write: Vec<PathBuf> = tokio::task::spawn_blocking(move || -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for home in homes()? {
            if matches!(
                plan_home(&home, enable, skip_default)?.0,
                Plan::Write { .. }
            ) {
                out.push(home.dir);
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| GateError::Other(e.to_string()))??;
    let writes = |dir: &Path| {
        to_write
            .iter()
            .any(|d| crate::install::inventory::same_path(d, dir))
    };

    // ⛔ 只关、只重开**官方槽位**（以及没槽位时退回的默认那份）起的桌面端，而且只关**要写的那几份**的
    // （2026-09-27）：中转环境起的那份（连同它的任务）不关（2026-09-25）；本来就是中文、用不着写的那几份
    // 起的 GPT 也不关。
    let root = codex::root();
    let mut official: Vec<(Option<String>, PathBuf)> = Vec::new();
    for slot in codex::list(&root)?.slots {
        let (home, profile) = crate::workspace::codex_official_dirs(&slot.id)?;
        if writes(&home) {
            official.push((Some(slot.id), profile));
        }
    }
    if let Some(home) = dirs::home_dir() {
        let default = home.join(".codex");
        if writes(&default) {
            official.push((None, default.join("desktop")));
        }
    }
    let running_official: Vec<&(Option<String>, PathBuf)> = official
        .iter()
        .filter(|(_, profile)| {
            desktop.processes.iter().any(|p| {
                p.ours
                    && p.profile.as_deref().is_some_and(|d| {
                        crate::install::inventory::same_path(Path::new(d), profile)
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
        // 外部/默认 GPT 正在使用另一份资料时，不要因为汉化动作又自动拉起
        // 面板槽位。那会在使用者当前对话旁边凭空出现第二个独立窗口；
        // 关闭外部窗口后，使用者可以从账户页手动打开槽位。
        if was_active && foreign == 0 {
            active
        } else {
            None
        }
    };
    let mut outcome = tokio::task::spawn_blocking(move || apply(enable, skip_default))
        .await
        .map_err(|e| GateError::Other(e.to_string()))??;
    if foreign > 0 && !running_official.is_empty() {
        outcome.lines.push(
            "检测到别处打开的 GPT：设置已写入，这次没有自动重新打开面板槽位；关闭外部 GPT 后，再从账户页手动启动槽位。"
                .into(),
        );
    }
    let mut s = crate::settings::load();
    if s.gpt_ui_zh != enable {
        s.gpt_ui_zh = enable;
        super::settings_ops::save(&s)?;
    }
    crate::audit::write(&format!(
        "GPT 界面语言：{}，改了 {} 份 config.toml 的 [desktop] localeOverride{}{}",
        if enable {
            "设为中文"
        } else {
            "恢复默认"
        },
        outcome.changed,
        if outcome.repaired > 0 {
            format!("（其中 {} 份顺手修了 GPT 写重复的表）", outcome.repaired)
        } else {
            String::new()
        },
        if outcome.skipped > 0 {
            format!("；{} 份没改", outcome.skipped)
        } else {
            String::new()
        }
    ));
    outcome.reopen = reopen;
    Ok(outcome)
}

/// 改所有份。**一次提交**（`config_io::commit` 两阶段、核对读的时候与写的时候是同一份内容），
/// 要么全改、要么一份都不改；写完每份用严格解析器回读，值不对就如实算「没改成」。
fn apply(enable: bool, skip_default: bool) -> Result<CodexLocaleOutcome> {
    let mut outcome = CodexLocaleOutcome::default();
    let mut edits = Vec::new();
    let mut written: Vec<(String, PathBuf)> = Vec::new();
    for home in homes()? {
        let label = home.label.clone();
        let (plan, before) = plan_home(&home, enable, skip_default)?;
        match plan {
            Plan::Skip(reason) => {
                outcome.skipped += 1;
                if home.is_default && skip_default {
                    outcome.default_busy = true;
                }
                outcome.lines.push(format!("{label}：{reason}。"));
            }
            Plan::Already => {
                outcome.already += 1;
                outcome.lines.push(format!(
                    "{label}：{}",
                    if enable {
                        "本来就是中文。"
                    } else {
                        "没有面板设的中文，不用改。"
                    }
                ));
            }
            Plan::Write { next, repaired } => {
                if !repaired.is_empty() {
                    outcome.repaired += 1;
                }
                outcome.lines.push(format!(
                    "{label}：{}{}",
                    if enable {
                        "已设为中文。"
                    } else {
                        "已撤掉中文，回到 GPT 自己的默认。"
                    },
                    if repaired.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "顺手并掉了 GPT 自己写重复的 {}（它原来因此读不到这份里的任何桌面设置），没改任何值。",
                            repaired.join("、")
                        )
                    }
                ));
                let path = home.dir.join("config.toml");
                written.push((label, path.clone()));
                edits.push(crate::config_io::Edit {
                    path,
                    expected: before,
                    body: Some(next.into_bytes()),
                });
            }
        }
    }
    if !edits.is_empty() {
        crate::config_io::commit(edits)?;
    }
    // 回读：GPT 用严格解析器读，这里也用严格的读一遍，值必须对。
    for (label, path) in &written {
        let read = read_config(path).and_then(|t| locale_override(&t));
        let ok = match &read {
            Ok(v) if enable => v.as_deref() == Some(ZH),
            Ok(v) => v.as_deref() != Some(ZH),
            Err(_) => false,
        };
        if ok {
            outcome.changed += 1;
        } else {
            outcome.skipped += 1;
            outcome.lines.push(format!(
                "{label}：写完回读不对（{}），GPT 可能读不到这一行。",
                match read {
                    Ok(v) => format!("读到的是 {}", v.as_deref().unwrap_or("没设")),
                    Err(e) => e.to_string(),
                }
            ));
        }
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
    match plan_text(Some(&text), true) {
        Plan::Write { next, .. } => crate::config_io::replace(&path, Some(next.as_bytes())),
        Plan::Already => Ok(()),
        Plan::Skip(reason) => Err(GateError::Other(reason)),
    }
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

    /// 09-26 本机 slot 3 那份的形状：CRLF、GPT 自己写出来的空 `[desktop]` + 整段 `[desktop]`、
    /// 两段一模一样的 `[mcp_servers.node_repl.env]`。值换成了假的（路径不是本机的，坑 7.88）。
    const SLOT_0926: &str = "forced_login_method = \"chatgpt\"\r\ncli_auth_credentials_store = \"file\"\r\n\r\n[plugins.\"template-creator@openai-primary-runtime\"]\r\nenabled = true\r\n\r\n[desktop]\r\n\r\n[desktop]\r\nfollowUpQueueMode = \"steer\"\r\nconversationDetailMode = \"STEPS_COMMANDS\"\r\nsansFontSize = 14\r\ncodeFontSize = 13\r\nambient-suggestions-enabled = true\r\n\r\n[mcp_servers.node_repl]\r\nargs = []\r\nstartup_timeout_sec = 120\r\n\r\n[mcp_servers.node_repl.env]\r\nNODE_REPL_NATIVE_PIPE_CONNECT_TIMEOUT_MS = \"1000\"\r\nNODE_REPL_NODE_MODULE_DIRS = 'C:\\Users\\demo\\AppData\\Local\\OpenAI\\Codex\\runtimes\\cua_node\\0000\\bin\\node_modules'\r\n\r\n[mcp_servers.node_repl.env]\r\nNODE_REPL_NATIVE_PIPE_CONNECT_TIMEOUT_MS = \"1000\"\r\nNODE_REPL_NODE_MODULE_DIRS = 'C:\\Users\\demo\\AppData\\Local\\OpenAI\\Codex\\runtimes\\cua_node\\0000\\bin\\node_modules'\r\n\r\n[windows]\r\nsandbox = \"elevated\"\r\n";

    fn value(text: &str) -> Option<String> {
        locale_override(text).unwrap()
    }

    fn repaired(text: &str) -> (String, Vec<String>) {
        match repair_duplicate_tables(text) {
            Repair::Repaired { text, tables } => (text, tables),
            other => panic!("没修成：{other:?}"),
        }
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
        // GPT 读不了的文件（表写重复了）也不许当成「没设」—— 它读不到里面任何一项。
        assert!(locale_override("[desktop]\n[desktop]\nlocaleOverride = \"zh-CN\"\n").is_err());
    }

    #[test]
    fn an_empty_file_gets_just_our_line() {
        let out = set_locale_override("", Some(ZH)).unwrap();
        assert_eq!(value(&out).as_deref(), Some(ZH));
        assert_eq!(out.trim(), "[desktop]\nlocaleOverride = \"zh-CN\"");
    }

    #[test]
    fn a_crlf_file_stays_crlf() {
        let text = "model = \"x\"\r\n\r\n[desktop]\r\nfollowUpQueueMode = \"queue\"\r\n";
        let out = set_locale_override(text, Some(ZH)).unwrap();
        assert_eq!(value(&out).as_deref(), Some(ZH));
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "{out:?}"
        );
    }

    #[test]
    fn a_valid_file_needs_no_repair() {
        assert_eq!(repair_duplicate_tables(REAL_SHAPE), Repair::Clean);
        assert_eq!(repair_duplicate_tables(""), Repair::Clean);
    }

    /// GPT 写出来的那种：先一个空的 `[desktop]`，再一整段 `[desktop]`。
    #[test]
    fn an_empty_duplicate_header_is_merged_away() {
        let text = "[desktop]\n\n[desktop]\nfollowUpQueueMode = \"steer\"\nsansFontSize = 14\n";
        let (out, tables) = repaired(text);
        assert_eq!(tables, ["[desktop]"]);
        assert_eq!(out.matches("[desktop]").count(), 1, "{out}");
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert_eq!(doc["desktop"]["followUpQueueMode"].as_str(), Some("steer"));
        assert_eq!(doc["desktop"]["sansFontSize"].as_integer(), Some(14));
    }

    #[test]
    fn identical_duplicate_blocks_collapse_into_one() {
        let text = "[mcp_servers.node_repl]\nargs = []\n\n[mcp_servers.node_repl.env]\nA = \"1\"\n\n[mcp_servers.node_repl.env]\nA = \"1\"\n";
        let (out, tables) = repaired(text);
        assert_eq!(tables, ["[mcp_servers.node_repl.env]"]);
        assert_eq!(
            out.matches("[mcp_servers.node_repl.env]").count(),
            1,
            "{out}"
        );
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert_eq!(
            doc["mcp_servers"]["node_repl"]["env"]["A"].as_str(),
            Some("1")
        );
    }

    #[test]
    fn keys_only_the_second_block_has_move_into_the_first() {
        let text = "[desktop]\na = 1\n\n[x]\ny = 2\n\n[desktop]\nb = \"two\"\n";
        let (out, _) = repaired(text);
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert_eq!(doc["desktop"]["a"].as_integer(), Some(1));
        assert_eq!(doc["desktop"]["b"].as_str(), Some("two"));
        assert_eq!(doc["x"]["y"].as_integer(), Some(2), "别的表原样");
    }

    /// 两段里同一个键的值不一样：哪个才是 GPT 的意思面板说不准 —— 不修。
    #[test]
    fn a_key_with_two_different_values_is_not_guessed() {
        let text =
            "[desktop]\nfollowUpQueueMode = \"queue\"\n[desktop]\nfollowUpQueueMode = \"steer\"\n";
        match repair_duplicate_tables(text) {
            Repair::Unrepairable { reason } => {
                assert!(reason.contains("followUpQueueMode"), "{reason}")
            }
            other => panic!("不该修：{other:?}"),
        }
    }

    #[test]
    fn a_file_broken_in_another_way_is_not_touched() {
        for bad in ["[desktop\nx = ", "a = 1\na = 2\n", "[desktop]\nx = \n"] {
            assert!(
                matches!(repair_duplicate_tables(bad), Repair::Unrepairable { .. }),
                "{bad:?}"
            );
        }
    }

    /// 多行数组里长得像表头的一行（`[ "a" ]`）不会被当成表头 —— 认表头的是解析器自己。
    #[test]
    fn a_line_inside_an_array_is_never_taken_for_a_header() {
        let text = "x = [\n  [ \"a\" ]\n]\n\n[desktop]\na = 1\n\n[desktop]\nb = 2\n";
        let (out, tables) = repaired(text);
        assert_eq!(tables, ["[desktop]"]);
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert_eq!(doc["x"].as_array().map(|a| a.len()), Some(1));
        assert_eq!(doc["desktop"]["b"].as_integer(), Some(2));
    }

    /// 本机 09-26 那一份：修完 GPT 读得了、写得进语言，而且 CRLF、单引号路径、别的表逐字都在。
    #[test]
    fn the_0926_slot_is_repaired_and_takes_the_locale() {
        let (out, tables) = repaired(SLOT_0926);
        assert_eq!(tables, ["[desktop]", "[mcp_servers.node_repl.env]"]);
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "还是 CRLF"
        );
        assert_eq!(out.matches("[desktop]").count(), 1);
        assert_eq!(out.matches("[mcp_servers.node_repl.env]").count(), 1);
        for kept in [
            "[plugins.\"template-creator@openai-primary-runtime\"]\r\nenabled = true\r\n",
            "NODE_REPL_NODE_MODULE_DIRS = 'C:\\Users\\demo\\AppData\\Local\\OpenAI\\Codex\\runtimes\\cua_node\\0000\\bin\\node_modules'",
            "[windows]\r\nsandbox = \"elevated\"\r\n",
            "[mcp_servers.node_repl]\r\nargs = []\r\nstartup_timeout_sec = 120\r\n",
        ] {
            assert!(out.contains(kept), "丢了：{kept}\n{out}");
        }
        let zh = set_locale_override(&out, Some(ZH)).unwrap();
        assert_eq!(value(&zh).as_deref(), Some(ZH));
        let doc: toml_edit::DocumentMut = zh.parse().unwrap();
        assert_eq!(doc["desktop"]["codeFontSize"].as_integer(), Some(13));
    }

    #[test]
    fn what_to_do_with_each_file() {
        // 设中文：GPT 写坏的那份先修再写。
        match plan_text(Some("[desktop]\n\n[desktop]\nsansFontSize = 14\n"), true) {
            Plan::Write { next, repaired } => {
                assert_eq!(repaired, ["[desktop]"]);
                assert_eq!(value(&next).as_deref(), Some(ZH));
            }
            other => panic!("{other:?}"),
        }
        // 恢复默认：写坏了、又没有我们的中文 —— 不是面板弄坏的，不碰。
        assert_eq!(
            plan_text(Some("[desktop]\n\n[desktop]\nsansFontSize = 14\n"), false),
            Plan::Already
        );
        // 恢复默认：写坏了、里面有我们的中文 —— 修了才删得掉。
        assert!(matches!(
            plan_text(
                Some("[desktop]\n\n[desktop]\nlocaleOverride = \"zh-CN\"\n"),
                false
            ),
            Plan::Write { .. }
        ));
        // 修不了就不改，原因照实给。
        assert!(matches!(
            plan_text(Some("[desktop]\na = 1\n[desktop]\na = 2\n"), true),
            Plan::Skip(_)
        ));
        // 本来就是中文 / 文件不在时恢复默认：什么都不用写。
        let zh = set_locale_override(REAL_SHAPE, Some(ZH)).unwrap();
        assert_eq!(plan_text(Some(&zh), true), Plan::Already);
        assert_eq!(plan_text(None, false), Plan::Already);
        // 新槽位（文件还不在）设中文：要写。
        assert!(matches!(plan_text(None, true), Plan::Write { .. }));
    }
}
