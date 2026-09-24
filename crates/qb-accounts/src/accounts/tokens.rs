//! 槽位用掉的 token：从官方客户端自己写在本机的会话转写里数出来。
//!
//! # 数据从哪来
//!
//! `<槽位目录>\projects\<项目>\<会话>.jsonl`。每一行是一条 JSON 记录，
//! `type` 为 `assistant` 的那些带着 `message.model` 与 `message.usage`：
//!
//! ```text
//! input_tokens · output_tokens · cache_creation_input_tokens · cache_read_input_tokens
//! cache_creation: { ephemeral_5m_input_tokens, ephemeral_1h_input_tokens }
//! ```
//!
//! 零网络请求。跟 [`super::usage`] 同一条口径 —— 面板只是把官方客户端
//! 已经落盘的东西读出来显示，不调任何接口（CLAUDE.md「不许加的功能」第 4 行）。
//!
//! # ⛔ 不去重的话，数出来的是真实用量的两倍
//!
//! 续接会话、自动压缩、`--continue`，都会新开一个转写文件并把之前的消息
//! 原样重放进去，包括 assistant 那几条和它们的 `usage`。
//! 实机 2026-09-16 在 `main` 槽位上数过：
//!
//! | | 条数 | 输出 token |
//! |---|--:|--:|
//! | 不去重 | 6,049 | 6,377,231 |
//! | 去重后 | 3,189 | 2,960,298 |
//!
//! **2.15 倍。** 而且多出来的那一倍长得完全像真数据 —— 没有任何地方会报错，
//! 界面上只会显示一个偏大的数字。去重键是 `message.id` + `requestId`：
//! 带上 `requestId` 是为了万一上游哪天真的为同一条消息发两次请求 ——
//! 那是两次真实开销，不该被合成一条。
//!
//! 同一个键出现多次时**留最完整的那一份**（0.25.1 之后，参照 cc-switch，MIT）：
//! 有用量的胜过四类全 0 的 → 带 `stop_reason` 的胜过没带的 → 输出大的胜过小的 →
//! 都一样时留时刻早的（原件，不是重放）。实机 2026-09-24 数过：一个会话里
//! 490 个键有多行，行与行的用量一个都没差 —— 所以对今天的数据这条是无损的；
//! 它防的是 cc-switch 实测过的另一种形状：子代理的短请求只写了 `message_start`
//! 那一刻的快照（输出 1、没有 `stop_reason`），后面那行才是完整的。
//! 按「先见到的」留，留下的恰好是那份快照。
//!
//! # ⛔ 缓存写分两档，单价差一截
//!
//! `cache_creation_input_tokens` 是合计；`cache_creation` 里分成 5 分钟档与
//! 1 小时档。官方价：5 分钟档写 = 输入 ×1.25，**1 小时档写 = 输入 ×2**。
//! 实机 2026-09-24：一个会话 633 条回复、320 万缓存写 token **全是 1 小时档**，
//! 5 分钟档是 0。拿 5 分钟档的价去乘，这一块会少算 37.5%，而界面上看起来
//! 跟真的一样。所以这里把 1 小时档单独记成 `cache_write_1h`，算钱的那一层
//! （`qb-app::usecase::token_summary`）分两档乘。没有 `cache_creation` 对象的
//! 旧记录全按 5 分钟档 —— 那是 API 的默认 TTL。
//!
//! # ⛔ 四类全 0 的不入账
//!
//! `<synthetic>` 那几条（`isApiErrorMessage: true` 的报错、被打断的请求）
//! 也带 `usage`，只是四类全是 0。0.25.1 之前它们照样算「1 条回复」——
//! 使用者看到的是「今天 1 条回复，输入 0、输出 0」，而那一条其实是一句
//! 「请重新登录」。实机 7 天 26,581 条里有 63 条 `<synthetic>`、65 条全 0。
//!
//! 现在它们不进桶、不算回复，单独数在 [`TokenUsage::empty_replies`] 里，
//! 界面照实说「另有 N 条报错没计入」—— 不瞒，也不冒充用量。
//! （cc-switch 的「任一计费维度 > 0 才导入」是同一条口径。）
//!
//! # 范围：槽位目录 + 默认目录里**认得出归属**的那部分
//!
//! 0.20.0 第一版只数槽位目录。实测那是错的：这台机器上两个槽位的
//! `projects\` **两周没人写过**（最后一条 9-02 / 9-04），而同一天
//! `~\.claude\projects` 里有 780 条回复、2.9 亿 token。原因是
//! 桌面端 Code 页里跑的会话不吃 `CLAUDE_CONFIG_DIR`（档案 §7.26），
//! 转写全落在默认目录。只数槽位目录的结果是：**界面上永远是零**。
//!
//! 所以两处都数。默认目录那部分要判归属，**三级判定，按代价从低到高**：
//!
//! | 级 | 依据 | 来源 |
//! |---|---|---|
//! | ① | [`OwnerIndex`]：桌面端资料目录里 `claude-code-sessions\<accountUuid>\<orgUuid>\local_*.json` 的 `cliSessionId` 对上转写的会话 id | 桌面端每开一个 Code 页会话就写一条，账户 uuid 是它自己写在路径里的 |
//! | ② | 项目目录名里嵌着 `scratch-workspaces-<accountUuid>-<orgUuid>-…` | 桌面端「无文件夹」会话的 cwd 就长这样，转写按 cwd 编码成目录名 |
//! | ③ | 文件里 `bridge-session` 行上的 `ownerAccountUuid` | 旧格式：Claude Code ≤ 2.1.270 写的 |
//!
//! ⛔ **第 ③ 级已经死了一半。** 0.21.0 时它是唯一依据；实测 2026-09-20
//! 这台机器上 Claude Code **2.1.271 起（09-15 之后）的转写一份都不带**
//! `bridge-session` 行 —— 31 份新转写零命中，当天 822 条回复 / 3.3 亿 token
//! 全落进「未归属」，界面上是一个斩钉截铁的 0。留着它是为了旧转写，
//! 别再把它当唯一依据；也别去 assistant 行上找 `ownerAccountUuid`，那里从来没有。
//!
//! 第 ① 级才是现在的主力：同一天 107 份转写里 66 份带旧标记、23 份只能靠它、
//! 7 份只能靠第 ② 级；还剩 11 份（多是终端里直接起的旧会话）三级都归不出。
//!
//! ⛔ **归不出来的那部分不许摊给任何账户。** 它们单独报在 `unattributed` 里，
//! 界面照实说「归不到槽位」。按「默认目录当前登录的是谁」去摊派是很诱人的，
//! 但那是拿一个此刻的快照去认领几个月的历史，换账户过一次就全错了。
//!
//! [`scan_all_slots`] 一次把默认目录分给**所有**槽位（用量明细页的「按账户」表）：
//! 默认目录只扫一遍，归到面板里没有的账户的单独一堆，不并进任何一行。
//!
//! # 子代理转写也要数
//!
//! `projects\<项目>\<会话>\subagents\agent-*.jsonl` 是子代理的转写，形状一样、
//! 用量一样真实（实测一天 27 份）。它归**父会话**（目录名就是父会话 id），
//! 归属判定跟着父会话走。0.25.0 之前 `transcripts()` 只下一层，这些全漏了。
//!
//! Claude Code 的 Workflow 再多套一层：`subagents\workflows\wf_<ID>\*.jsonl`
//! （cc-switch 的文件扫描里写着，漏了它 Workflow 的用量整个不计）。这台机器上
//! 还没有，别人的机器上有 —— 一样归父会话。
//!
//! # 为什么没有增量缓存
//!
//! 想过按文件偏移做增量。实测否掉了：两个槽位合计 90 MB、9,634 条带用量的
//! 记录，一遍扫完 0.46 秒（还是 Python 量的，Rust 只会更快）。
//! 为这点开销换一份要自己维护失效逻辑的磁盘缓存不划算 ——
//! 而缓存算错的样子，正是「显示一个看起来很正常的错数字」。

use serde::Serialize;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 「最近请求」最多带几条。用量明细页按区间再筛、再截到 200。
pub const RECENT_CAP: usize = 500;

/// 按小时的桶只留最近这么久 —— 只给「今天」那一档画按小时的柱子用。
/// 取 48 小时而不是「今天」：「今天」是调用方按本地日期定的，这里多留一天，
/// 不管哪边先跨过零点都够用。
const HOURLY_WINDOW_MS: i64 = 48 * 60 * 60 * 1000;

// ------------------------------------------------------------------ 归属索引

/// 桌面端 Code 页留下的「会话 → 账户」索引（文件头第 ① 级）。
///
/// 桌面端每开一个 Code 会话，就在**它自己的资料目录**里写一条记录：
/// `<资料目录>\claude-code-sessions\<accountUuid>\<orgUuid>\local_<id>.json`，
/// 里面 `cliSessionId` 就是转写的会话 id（`projects\<项目>\<会话>.jsonl` 的文件名）。
/// 账户 uuid 直接取路径里那一段 —— 桌面端自己写的，不是猜的。
///
/// 纯函数吃目录列表：哪些目录算桌面端资料目录由 `AccountRoots::desktop_profile_dirs`
/// 决定（「Claude 的目录名怎么拼」全项目只在那一处），单测自己搭临时目录。
#[derive(Debug, Default, Clone)]
pub struct OwnerIndex {
    /// 会话 id（小写）→ 账户 uuid（原样）。
    owners: HashMap<String, String>,
}

impl OwnerIndex {
    /// 一条记录都没有。`for_slot` 用它 —— 只数槽位目录时归属根本不成立。
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn scan(profile_dirs: &[PathBuf]) -> Self {
        let mut owners = HashMap::new();
        for dir in profile_dirs {
            let Ok(accounts) = std::fs::read_dir(dir.join("claude-code-sessions")) else {
                continue;
            };
            for account in accounts.flatten() {
                let uuid = account.file_name().to_string_lossy().to_string();
                // 目录名不像 uuid 的一律跳过：那不是桌面端写的账户目录。
                if !looks_like_uuid(&uuid) {
                    continue;
                }
                let Ok(orgs) = std::fs::read_dir(account.path()) else {
                    continue;
                };
                for org in orgs.flatten() {
                    let Ok(files) = std::fs::read_dir(org.path()) else {
                        continue;
                    };
                    for f in files.flatten() {
                        let name = f.file_name().to_string_lossy().to_string();
                        if !(name.starts_with("local_") && name.ends_with(".json")) {
                            continue;
                        }
                        let Some(cli) = std::fs::read_to_string(f.path())
                            .ok()
                            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                            .and_then(|j| {
                                j.get("cliSessionId")
                                    .and_then(|x| x.as_str())
                                    .map(|s| s.to_ascii_lowercase())
                            })
                        else {
                            continue;
                        };
                        owners.insert(cli, uuid.clone());
                    }
                }
            }
        }
        Self { owners }
    }

    pub fn owner_of_session(&self, session_id: &str) -> Option<&str> {
        self.owners
            .get(&session_id.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.owners.len()
    }

    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

/// `8-4-4-4-12` 的十六进制串。只认形状，不认版本位。
fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// 从转写的项目目录名里认出账户 uuid（文件头第 ② 级）。
///
/// 桌面端「无文件夹」会话的 cwd 是
/// `%APPDATA%\Claude-<标签>\scratch-workspaces\<accountUuid>\<orgUuid>\scratch-…`，
/// 官方把 cwd 里的分隔符换成 `-` 当项目目录名，uuid 只含十六进制和连字符，
/// 原样活了下来 —— 所以不用打开文件就能认。
pub fn owner_from_project_name(project: &str) -> Option<String> {
    const MARK: &str = "scratch-workspaces-";
    let start = project.find(MARK)? + MARK.len();
    let candidate = project.get(start..start + 36)?;
    looks_like_uuid(candidate).then(|| candidate.to_string())
}

/// 一份转写文件：路径、它属于哪个会话、在哪个项目目录下。
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub path: PathBuf,
    /// 会话 id。主转写 = 文件名；子代理转写 = 父会话目录名。
    pub session_id: String,
    /// 项目目录名（官方按 cwd 编码出来的那一段）。
    pub project: String,
}

/// 一条 assistant 回复的用量。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 去重键：`message.id` + `requestId`。见文件头。
    pub key: String,
    /// 这条回复的时刻，毫秒。`i64::MIN` = 这条记录没带时刻。
    pub at: i64,
    pub model: String,
    pub input: i64,
    pub output: i64,
    /// 缓存写合计（5 分钟档 + 1 小时档）。
    pub cache_write: i64,
    /// 其中 1 小时档。两档单价不同，见文件头。
    pub cache_write_1h: i64,
    pub cache_read: i64,
    /// 这一行带没带 `stop_reason`。去重时用来挑最完整的那一份。
    pub stop: bool,
    /// 项目目录名。解析单行时为空，读文件时由调用方填。
    pub project: String,
    /// 会话 id。同上。
    pub session: String,
}

impl Entry {
    /// 四类全 0：报错、被打断、`<synthetic>` —— 不是一次计费。见文件头。
    pub fn is_empty(&self) -> bool {
        self.input == 0 && self.output == 0 && self.cache_write == 0 && self.cache_read == 0
    }
}

/// 一个「日 × 模型」桶。按本机本地时区切天 —— 使用者看的是自己的日历。
#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenBucket {
    /// `YYYY-MM-DD`，本地时区。
    pub day: String,
    /// 官方写下的模型名，原样给出，不做任何美化或归并。
    /// `usage.speed` 不是 `standard` 的回复在后面带 ` (<speed>)` —— 那一档另有价。
    pub model: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    /// 缓存写合计（5 分钟档 + 1 小时档）。
    #[ts(type = "number")]
    pub cache_write: i64,
    /// 其中 1 小时档。单价是 5 分钟档的 1.6 倍（输入 ×2 对 ×1.25），见文件头。
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    /// 这个桶里有几条回复。
    #[ts(type = "number")]
    pub messages: i64,
}

/// 最近 48 小时的「日 × 小时 × 模型」桶。用量明细页「今天」那一档按小时画。
#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenHourBucket {
    /// `YYYY-MM-DD`，本地时区。
    pub day: String,
    /// 本地小时，0–23。
    pub hour: u8,
    pub model: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    #[ts(type = "number")]
    pub messages: i64,
}

/// 一条回复。用量明细页「最近请求」那张表（cc-switch 的请求日志）。
#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenRequest {
    /// `YYYY-MM-DD`，本地时区。
    pub day: String,
    /// `HH:MM:SS`，本地时区。
    pub time: String,
    pub model: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    /// 项目目录名（官方按 cwd 编码出来的那一段）。
    pub project: String,
    /// 会话 id 的前 8 位 —— 认得出是哪一段对话就够了。
    pub session: String,
}

/// 某一天有几条四类全 0、没入账的回复。按天记是为了能跟着时间档一起筛 ——
/// 「今天另有 1 条报错」不能是三个月的累计。
#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenEmptyDay {
    /// `YYYY-MM-DD`，本地时区。
    pub day: String,
    #[ts(type = "number")]
    pub replies: i64,
}

/// 一个槽位的 token 统计。
///
/// 后面那几个计数不是装饰，它们是「这份统计覆盖了多少」的证据。
/// 只给总数的话，「读了 41 个文件」和「读了 3 个、38 个打不开」
/// 在界面上长得一模一样。
#[derive(Debug, Clone, Default, Serialize, PartialEq, TS)]
#[ts(export)]
pub struct TokenUsage {
    /// 日 × 模型，按 `day` 再按 `model` 升序。
    pub buckets: Vec<TokenBucket>,
    /// 有用量记录的会话数（子代理并进父会话）。
    #[ts(type = "number")]
    pub sessions: i64,
    /// 读成功的转写文件数。
    #[ts(type = "number")]
    pub files_read: i64,
    /// 打不开或读坏了的转写文件数。不为零就要在界面上说。
    #[ts(type = "number")]
    pub files_failed: i64,
    /// 被去重丢掉的条数。见文件头 —— 这个数通常和留下的一样大。
    #[ts(type = "number")]
    pub duplicates: i64,
    /// 没有可用时刻、落不进任何一天的条数。
    #[ts(type = "number")]
    pub undated: i64,
    /// 默认目录里**归不到任何槽位**的用量，同样按日 × 模型分桶。
    ///
    /// 单独一列而不是并进 `buckets`：它不属于这个账户，也不属于别的账户 ——
    /// 摊给谁都是编。界面把它当成一行旁注显示。
    pub unattributed: Vec<TokenBucket>,
    /// 四类全 0、没有入账的回复数（报错、被打断）。见文件头。
    #[ts(type = "number")]
    pub empty_replies: i64,
    /// 同上，按天。没有时刻的不在这里（落不进任何一天）。
    pub empty_days: Vec<TokenEmptyDay>,
    /// 最近 48 小时，日 × 小时 × 模型。
    pub hourly: Vec<TokenHourBucket>,
    /// 最近的回复，新的在前，最多 [`RECENT_CAP`] 条。
    pub recent: Vec<TokenRequest>,
}

// ------------------------------------------------------------------ 解析

/// 解析一行转写。不是带用量的 assistant 记录就回 `None`。
///
/// 调用方应当先粗筛掉不含 `usage` 的行 —— 转写里绝大多数行是用户输入和
/// 工具结果，对每一行都建一棵 `serde_json::Value` 纯属浪费。
pub fn parse_line(line: &str) -> Option<Entry> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let m = v.get("message")?;
    let u = m.get("usage")?.as_object()?;
    let n = |k: &str| u.get(k).and_then(|x| x.as_i64()).unwrap_or(0).max(0);
    // `message.id` 没有时退回这一行自己的 `uuid`：那是每行唯一的，
    // 退回它等于「这一条不参与去重」，而不是把它悄悄丢掉。
    let id = m
        .get("id")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("uuid").and_then(|x| x.as_str()))?;
    let req = v.get("requestId").and_then(|x| x.as_str()).unwrap_or("");
    let cache_write = n("cache_creation_input_tokens");
    // 两档缓存写（文件头）。1 小时档不许超过合计 —— 字段对不上时宁可少算这一档的差价，
    // 也不凭空造出一截不存在的写入。
    let cache_write_1h = u
        .get("cache_creation")
        .and_then(|c| c.get("ephemeral_1h_input_tokens"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0)
        .clamp(0, cache_write);
    let mut model = m
        .get("model")
        .and_then(|x| x.as_str())
        .unwrap_or("未知模型")
        .to_string();
    // 标准档之外的速度档（比如 fast 模式）另有价，不能跟标准档混在一个桶里按标准价算。
    // 带着后缀，价目里认不出就老老实实进「没有官方价」。
    if let Some(speed) = u.get("speed").and_then(|x| x.as_str()) {
        if !speed.is_empty() && speed != "standard" {
            model = format!("{model} ({speed})");
        }
    }
    Some(Entry {
        key: format!("{id}:{req}"),
        at: v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.timestamp_millis())
            .unwrap_or(i64::MIN),
        model,
        input: n("input_tokens"),
        output: n("output_tokens"),
        cache_write,
        cache_write_1h,
        cache_read: n("cache_read_input_tokens"),
        stop: m
            .get("stop_reason")
            .and_then(|x| x.as_str())
            .is_some_and(|s| !s.is_empty()),
        project: String::new(),
        session: String::new(),
    })
}

/// 去重时比较「谁更完整」。见文件头：有用量 > 带 `stop_reason` > 输出大 > 读写大 > 时刻早。
fn completeness(e: &Entry) -> (bool, bool, i64, i64, std::cmp::Reverse<i64>) {
    (
        !e.is_empty(),
        e.stop,
        e.output,
        e.input + e.cache_write + e.cache_read,
        std::cmp::Reverse(e.at),
    )
}

/// 按 `key` 去重，同一个键留最完整的那一份。返回留下的与丢掉的条数。
///
/// 留下的顺序是「每个键第一次出现的位置」—— 结果不依赖谁后来把谁顶掉。
pub fn dedupe(entries: Vec<Entry>) -> (Vec<Entry>, i64) {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut kept: Vec<Entry> = Vec::new();
    let mut dropped = 0;
    for e in entries {
        match at.get(&e.key) {
            None => {
                at.insert(e.key.clone(), kept.len());
                kept.push(e);
            }
            Some(&i) => {
                dropped += 1;
                if completeness(&e) > completeness(&kept[i]) {
                    kept[i] = e;
                }
            }
        }
    }
    (kept, dropped)
}

/// 聚合成「日 × 模型」。返回桶与没有时刻、落不进任何一天的条数。
pub fn buckets_of(entries: &[Entry]) -> (Vec<TokenBucket>, i64) {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, String), TokenBucket> = BTreeMap::new();
    let mut undated = 0;
    for e in entries {
        let Some(day) = local_day(e.at) else {
            undated += 1;
            continue;
        };
        let b = map
            .entry((day.clone(), e.model.clone()))
            .or_insert_with(|| TokenBucket {
                day,
                model: e.model.clone(),
                input: 0,
                output: 0,
                cache_write: 0,
                cache_write_1h: 0,
                cache_read: 0,
                messages: 0,
            });
        b.input += e.input;
        b.output += e.output;
        b.cache_write += e.cache_write;
        b.cache_write_1h += e.cache_write_1h;
        b.cache_read += e.cache_read;
        b.messages += 1;
    }
    // BTreeMap 的键就是 (day, model)，迭代出来已经是那个顺序。
    (map.into_values().collect(), undated)
}

/// `since_ms` 之后的回复，按「日 × 小时 × 模型」聚合，按时间升序。
pub fn hourly_of(entries: &[Entry], since_ms: i64) -> Vec<TokenHourBucket> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, u8, String), TokenHourBucket> = BTreeMap::new();
    for e in entries
        .iter()
        .filter(|e| e.at != i64::MIN && e.at >= since_ms)
    {
        let Some((day, hour, _)) = local_parts(e.at) else {
            continue;
        };
        let b = map
            .entry((day.clone(), hour, e.model.clone()))
            .or_insert_with(|| TokenHourBucket {
                day,
                hour,
                model: e.model.clone(),
                input: 0,
                output: 0,
                cache_write: 0,
                cache_write_1h: 0,
                cache_read: 0,
                messages: 0,
            });
        b.input += e.input;
        b.output += e.output;
        b.cache_write += e.cache_write;
        b.cache_write_1h += e.cache_write_1h;
        b.cache_read += e.cache_read;
        b.messages += 1;
    }
    map.into_values().collect()
}

/// 四类全 0 的那些按天数一数。
fn empty_days_of(empty: &[Entry]) -> Vec<TokenEmptyDay> {
    let mut map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for e in empty {
        if let Some(day) = local_day(e.at) {
            *map.entry(day).or_default() += 1;
        }
    }
    map.into_iter()
        .map(|(day, replies)| TokenEmptyDay { day, replies })
        .collect()
}

/// 最近的 `cap` 条回复，新的在前。没有时刻的不列 —— 排不进时间线。
pub fn recent_of(entries: &[Entry], cap: usize) -> Vec<TokenRequest> {
    let mut dated: Vec<&Entry> = entries.iter().filter(|e| e.at != i64::MIN).collect();
    dated.sort_by_key(|e| std::cmp::Reverse(e.at));
    dated
        .into_iter()
        .take(cap)
        .filter_map(|e| {
            let (day, _, time) = local_parts(e.at)?;
            Some(TokenRequest {
                day,
                time,
                model: e.model.clone(),
                input: e.input,
                output: e.output,
                cache_write: e.cache_write,
                cache_write_1h: e.cache_write_1h,
                cache_read: e.cache_read,
                project: e.project.clone(),
                session: e.session.chars().take(8).collect(),
            })
        })
        .collect()
}

/// 毫秒 → 本地日期 `YYYY-MM-DD`。没有时刻的回 `None`。
fn local_day(ms: i64) -> Option<String> {
    local_parts(ms).map(|(day, _, _)| day)
}

/// 毫秒 → 本地（日期、小时、`HH:MM:SS`）。
fn local_parts(ms: i64) -> Option<(String, u8, String)> {
    if ms == i64::MIN {
        return None;
    }
    use chrono::{TimeZone, Timelike};
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => Some((
            t.format("%Y-%m-%d").to_string(),
            t.hour() as u8,
            t.format("%H:%M:%S").to_string(),
        )),
        _ => None,
    }
}

// ------------------------------------------------------------------ 入口

/// 数一个槽位用掉的 token。
///
/// 槽位目录不存在、没有 `projects\`、一个转写都没有，都回一份全零的报告
/// 而不是错误 —— 「这个槽位还没跑过会话」是正常状态，不是故障。
pub fn for_slot(slot_dir: &Path) -> TokenUsage {
    for_slot_and_default(slot_dir, None, None, &OwnerIndex::empty())
}

/// 数一个槽位用掉的 token，可选地把默认目录里归得上的那部分也算进来。
///
/// `default_dir` 是 `~\.claude`，`account_uuid` 是这个槽位的
/// `oauthAccount.accountUuid`，`index` 是桌面端留下的会话归属索引。
/// 没有 uuid 就归不了属 —— 那时候默认目录里归得出主的一个字都不算给它，
/// 否则就是把别人的量算到这个账户头上。
///
/// 槽位目录不存在、没有 `projects\`、一个转写都没有，都回一份全零的报告
/// 而不是错误 —— 「这个槽位还没跑过会话」是正常状态，不是故障。
pub fn for_slot_and_default(
    slot_dir: &Path,
    default_dir: Option<&Path>,
    account_uuid: Option<&str>,
    index: &OwnerIndex,
) -> TokenUsage {
    let slot = SlotRef {
        label: String::new(),
        dir: slot_dir.to_path_buf(),
        account_uuid: account_uuid.map(str::to_string),
    };
    scan_all_slots(std::slice::from_ref(&slot), default_dir, index)
        .slots
        .pop()
        .map(|(_, u)| u)
        .unwrap_or_default()
}

/// [`scan_all_slots`] 的一个槽位。
#[derive(Debug, Clone)]
pub struct SlotRef {
    /// 原样带回结果里，方便调用方对上号。
    pub label: String,
    /// 槽位目录（不是 `claude-profile` 联结点）。
    pub dir: PathBuf,
    /// 槽位的 `oauthAccount.accountUuid`。没有就认领不了默认目录里的任何东西。
    pub account_uuid: Option<String>,
}

/// 所有槽位一起数的结果。
#[derive(Debug, Clone, Default)]
pub struct AllSlotsUsage {
    /// 按传进来的顺序，一个槽位一份；每份的 `unattributed` 都是同一份。
    pub slots: Vec<(String, TokenUsage)>,
    /// 默认目录里归得出主、但那个账户**不是面板里任何一个槽位**的用量。
    /// 不并进任何一行，也不算「未归属」—— 它有主，只是主不在这里。
    pub other_accounts: Vec<TokenBucket>,
}

/// 把默认目录分给所有槽位：默认目录只扫一遍、每份转写只判一次归属。
///
/// 每个槽位的结果跟单独调 [`for_slot_and_default`] 一样（那个函数就是它的特例）：
/// 自己目录里的全算、默认目录里归到自己的算、归不出的进 `unattributed`。
pub fn scan_all_slots(
    slots: &[SlotRef],
    default_dir: Option<&Path>,
    index: &OwnerIndex,
) -> AllSlotsUsage {
    let mut own: Vec<Vec<Entry>> = vec![Vec::new(); slots.len()];
    let mut files_read = vec![0_i64; slots.len()];
    let mut files_failed = vec![0_i64; slots.len()];

    for (i, s) in slots.iter().enumerate() {
        for t in transcripts(&s.dir.join("projects")) {
            match read_entries(&t) {
                Ok(found) => {
                    files_read[i] += 1;
                    own[i].extend(found);
                }
                Err(_) => files_failed[i] += 1,
            }
        }
    }

    let mut stray: Vec<Entry> = Vec::new();
    let mut other: Vec<Entry> = Vec::new();
    let mut shared_read = 0;
    let mut shared_failed = 0;
    if let Some(home) = default_dir {
        for t in transcripts(&home.join("projects")) {
            let owner = owner_of(&t, index);
            match read_entries(&t) {
                Ok(found) => {
                    shared_read += 1;
                    match owner {
                        Some(o) => match slots.iter().position(|s| {
                            s.account_uuid
                                .as_deref()
                                .is_some_and(|me| me.eq_ignore_ascii_case(&o))
                        }) {
                            // 归得上面板里的某个槽位。
                            Some(i) => own[i].extend(found),
                            // 归到别的账户去了 —— 跟这些槽位都无关。
                            None => other.extend(found),
                        },
                        // 三级都归不出：既不能算给谁，也不能当它不存在。
                        None => stray.extend(found),
                    }
                }
                Err(_) => shared_failed += 1,
            }
        }
    }

    let unattributed = settled_buckets(stray);
    let other_accounts = settled_buckets(other);
    let since = chrono::Local::now().timestamp_millis() - HOURLY_WINDOW_MS;

    let slots_out = slots
        .iter()
        .zip(own)
        .enumerate()
        .map(|(i, (s, entries))| {
            let (kept, duplicates) = dedupe(entries);
            let (kept, empty): (Vec<Entry>, Vec<Entry>) =
                kept.into_iter().partition(|e| !e.is_empty());
            let (buckets, undated) = buckets_of(&kept);
            let sessions = kept
                .iter()
                .map(|e| e.session.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len() as i64;
            let usage = TokenUsage {
                buckets,
                sessions,
                files_read: files_read[i] + shared_read,
                files_failed: files_failed[i] + shared_failed,
                duplicates,
                undated,
                unattributed: unattributed.clone(),
                empty_replies: empty.len() as i64,
                empty_days: empty_days_of(&empty),
                hourly: hourly_of(&kept, since),
                recent: recent_of(&kept, RECENT_CAP),
            };
            (s.label.clone(), usage)
        })
        .collect();

    AllSlotsUsage {
        slots: slots_out,
        other_accounts,
    }
}

/// 去重、丢掉四类全 0 的，再分桶。归不到槽位的那两堆用它 —— 它们只要桶。
fn settled_buckets(entries: Vec<Entry>) -> Vec<TokenBucket> {
    let (kept, _) = dedupe(entries);
    let kept: Vec<Entry> = kept.into_iter().filter(|e| !e.is_empty()).collect();
    buckets_of(&kept).0
}

/// 这份转写属于哪个账户 —— 文件头那三级判定，按代价从低到高。
///
/// 前两级不打开文件；第 ③ 级要整份扫一遍，只在前两级都落空时做。
/// ⛔ 第 ③ 级读的是 `bridge-session` 行上的 `ownerAccountUuid`，**不是 assistant 行**
/// —— assistant 行上没有这个字段（实测 13,143 条，零命中）。
/// 一个文件只会有一个 owner（实测 60 个文件里没有例外），所以读到第一个就停。
pub fn owner_of(t: &Transcript, index: &OwnerIndex) -> Option<String> {
    if let Some(o) = index.owner_of_session(&t.session_id) {
        return Some(o.to_string());
    }
    if let Some(o) = owner_from_project_name(&t.project) {
        return Some(o);
    }
    legacy_owner_marker(&t.path)
}

fn legacy_owner_marker(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    for line in std::io::BufReader::new(file).lines() {
        let line = line.ok()?;
        if !line.contains("ownerAccountUuid") {
            continue;
        }
        if let Some(v) = serde_json::from_str::<serde_json::Value>(&line)
            .ok()
            .and_then(|j| {
                j.get("ownerAccountUuid")
                    .and_then(|x| x.as_str())
                    .map(str::to_string)
            })
        {
            return Some(v);
        }
    }
    None
}

/// `projects\<项目>\<会话>.jsonl`、`projects\<项目>\<会话>\subagents\*.jsonl`
/// 与 `projects\<项目>\<会话>\subagents\workflows\<wf>\*.jsonl`。
///
/// 只认这三种形状 —— 官方就是这么排的，无限递归只会把别的东西卷进来。
/// 子代理与 Workflow 的转写归父会话（目录名），见文件头。
pub fn transcripts(projects: &Path) -> Vec<Transcript> {
    let Ok(rd) = std::fs::read_dir(projects) else {
        return Vec::new();
    };
    let is_jsonl = |p: &Path| p.is_file() && p.extension().is_some_and(|e| e == "jsonl");
    let stem = |p: &Path| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for project in rd.flatten() {
        if !project.path().is_dir() {
            continue;
        }
        let project_name = project.file_name().to_string_lossy().to_string();
        let Ok(entries) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if is_jsonl(&p) {
                out.push(Transcript {
                    session_id: stem(&p),
                    path: p,
                    project: project_name.clone(),
                });
            } else if p.is_dir() {
                let Ok(subs) = std::fs::read_dir(p.join("subagents")) else {
                    continue;
                };
                let parent = e.file_name().to_string_lossy().to_string();
                let mut push = |path: PathBuf| {
                    out.push(Transcript {
                        path,
                        session_id: parent.clone(),
                        project: project_name.clone(),
                    })
                };
                for s in subs.flatten() {
                    let sp = s.path();
                    if is_jsonl(&sp) {
                        push(sp);
                    } else if sp.is_dir() && s.file_name() == "workflows" {
                        let Ok(flows) = std::fs::read_dir(&sp) else {
                            continue;
                        };
                        for wf in flows.flatten() {
                            let Ok(files) = std::fs::read_dir(wf.path()) else {
                                continue;
                            };
                            for f in files.flatten() {
                                if is_jsonl(&f.path()) {
                                    push(f.path());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn read_entries(t: &Transcript) -> std::io::Result<Vec<Entry>> {
    let file = std::fs::File::open(&t.path)?;
    let mut out = Vec::new();
    for line in std::io::BufReader::new(file).lines() {
        // 读坏的那一行跳过，不让它废掉整个文件 —— 转写是追加写的，
        // 正在写的最后一行可能只写了一半。
        let Ok(line) = line else { continue };
        if !line.contains("\"usage\"") {
            continue;
        }
        if let Some(mut e) = parse_line(&line) {
            e.project = t.project.clone();
            e.session = t.session_id.clone();
            out.push(e);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, req: &str, model: &str, out: i64, ts: &str) -> String {
        format!(
            concat!(
                r#"{{"type":"assistant","uuid":"u-{id}","requestId":"{req}","timestamp":"{ts}","#,
                r#""message":{{"id":"{id}","model":"{model}","usage":{{"#,
                r#""input_tokens":3,"output_tokens":{out},"#,
                r#""cache_creation_input_tokens":11,"cache_read_input_tokens":222}}}}}}"#
            ),
            id = id,
            req = req,
            model = model,
            out = out,
            ts = ts
        )
    }

    /// 现在的 Claude Code 写的完整形状：两档缓存写、`stop_reason`、`speed`。
    fn full_line(
        id: &str,
        out: i64,
        w5m: i64,
        w1h: i64,
        stop: Option<&str>,
        speed: &str,
    ) -> String {
        let stop = stop.map_or("null".to_string(), |s| format!("\"{s}\""));
        format!(
            concat!(
                r#"{{"type":"assistant","uuid":"u-{id}","requestId":"r-{id}","timestamp":"2026-09-24T06:00:00Z","#,
                r#""message":{{"id":"{id}","model":"claude-opus-5-5","stop_reason":{stop},"usage":{{"#,
                r#""input_tokens":2,"output_tokens":{out},"cache_creation_input_tokens":{total},"#,
                r#""cache_read_input_tokens":1000,"#,
                r#""cache_creation":{{"ephemeral_5m_input_tokens":{w5m},"ephemeral_1h_input_tokens":{w1h}}},"#,
                r#""speed":"{speed}"}}}}}}"#
            ),
            id = id,
            out = out,
            total = w5m + w1h,
            w5m = w5m,
            w1h = w1h,
            stop = stop,
            speed = speed
        )
    }

    /// 实机那一条 `<synthetic>` 报错的形状（`isApiErrorMessage`，四类全 0）。
    fn synthetic_line(id: &str, ts: &str) -> String {
        format!(
            concat!(
                r#"{{"type":"assistant","uuid":"u-{id}","timestamp":"{ts}","isApiErrorMessage":true,"#,
                r#""message":{{"id":"{id}","model":"<synthetic>","usage":{{"input_tokens":0,"output_tokens":0,"#,
                r#""cache_creation_input_tokens":0,"cache_read_input_tokens":0,"#,
                r#""cache_creation":{{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":0}}}}}}}}"#
            ),
            id = id,
            ts = ts
        )
    }

    #[test]
    fn a_plain_assistant_row_parses() {
        let e = parse_line(&line(
            "m1",
            "r1",
            "claude-opus-5",
            40,
            "2026-09-16T10:00:00.000Z",
        ))
        .expect("这是实机上真实的形状");
        assert_eq!(e.key, "m1:r1");
        assert_eq!(e.model, "claude-opus-5");
        assert_eq!(
            (e.input, e.output, e.cache_write, e.cache_read),
            (3, 40, 11, 222)
        );
        assert_eq!(
            e.cache_write_1h, 0,
            "没有 cache_creation 对象 = 全是 5 分钟档"
        );
        assert!(!e.stop);
    }

    /// 实机 2026-09-24：缓存写全是 1 小时档。合计照旧，1 小时档单独记下来。
    #[test]
    fn the_one_hour_cache_write_is_split_out_of_the_total() {
        let e = parse_line(&full_line(
            "m1",
            40,
            10,
            1_000,
            Some("end_turn"),
            "standard",
        ))
        .unwrap();
        assert_eq!(e.cache_write, 1_010);
        assert_eq!(e.cache_write_1h, 1_000);
        assert!(e.stop);
        assert_eq!(e.model, "claude-opus-5-5", "standard 档不带后缀");
    }

    /// 1 小时档大过合计是字段对不上 —— 不许凭空造出一截写入。
    #[test]
    fn the_one_hour_share_never_exceeds_the_total() {
        let row = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-09-24T06:00:00Z",
            "message":{"id":"m1","model":"m","usage":{"input_tokens":1,"output_tokens":1,
            "cache_creation_input_tokens":5,"cache_creation":{"ephemeral_1h_input_tokens":99}}}}"#;
        let e = parse_line(row).unwrap();
        assert_eq!((e.cache_write, e.cache_write_1h), (5, 5));
    }

    /// 不是标准档的回复另有价，不许跟标准档混进一个桶。
    #[test]
    fn a_non_standard_speed_is_kept_apart_in_the_model_name() {
        let e = parse_line(&full_line("m1", 40, 0, 10, Some("end_turn"), "fast")).unwrap();
        assert_eq!(e.model, "claude-opus-5-5 (fast)");
    }

    #[test]
    fn rows_that_are_not_assistant_usage_are_skipped() {
        assert_eq!(
            parse_line(r#"{"type":"user","message":{"content":"hi"}}"#),
            None
        );
        assert_eq!(
            parse_line(r#"{"type":"assistant","message":{"id":"m"}}"#),
            None
        );
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line(r#"{"type":"queue-operation"}"#), None);
    }

    /// 文件头那条：不去重就是两倍。实机上 6049 条里 2860 条是重放。
    #[test]
    fn replayed_rows_are_counted_once() {
        let rows = vec![
            parse_line(&line(
                "m1",
                "r1",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:00Z",
            ))
            .unwrap(),
            parse_line(&line(
                "m2",
                "r2",
                "claude-opus-5",
                60,
                "2026-09-16T10:01:00Z",
            ))
            .unwrap(),
            // 续接之后重放的同两条
            parse_line(&line(
                "m1",
                "r1",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:00Z",
            ))
            .unwrap(),
            parse_line(&line(
                "m2",
                "r2",
                "claude-opus-5",
                60,
                "2026-09-16T10:01:00Z",
            ))
            .unwrap(),
        ];
        let (kept, dropped) = dedupe(rows);
        assert_eq!(dropped, 2);
        let (buckets, _) = buckets_of(&kept);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].output, 100, "重放的那两条不许再加一遍");
        assert_eq!(buckets[0].messages, 2);
    }

    /// 同一条消息真的发了两次请求 = 两次真实开销，不许合成一条。
    #[test]
    fn the_same_message_with_two_request_ids_stays_two_rows() {
        let rows = vec![
            parse_line(&line(
                "m1",
                "r1",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:00Z",
            ))
            .unwrap(),
            parse_line(&line(
                "m1",
                "r2",
                "claude-opus-5",
                40,
                "2026-09-16T10:00:05Z",
            ))
            .unwrap(),
        ];
        let (kept, dropped) = dedupe(rows);
        assert_eq!((kept.len(), dropped), (2, 0));
    }

    /// 同一个键先来一份 `message_start` 那一刻的快照（输出 1、没有 stop_reason），
    /// 后来才是完整的那一份：留完整的，不留先到的（cc-switch 实测过的形状）。
    #[test]
    fn dedupe_keeps_the_most_complete_copy_not_the_first_one() {
        let snapshot = parse_line(&full_line("m1", 1, 0, 50, None, "standard")).unwrap();
        let complete =
            parse_line(&full_line("m1", 700, 0, 50, Some("tool_use"), "standard")).unwrap();
        let (kept, dropped) = dedupe(vec![snapshot, complete]);
        assert_eq!(dropped, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].output, 700);
        assert!(kept[0].stop);
    }

    /// 完全一样的两份（重放）留时刻早的那份 —— 那是原件。
    #[test]
    fn identical_copies_keep_the_earliest_timestamp() {
        let original = parse_line(&line("m1", "r1", "m", 5, "2026-09-16T10:00:00Z")).unwrap();
        let replay = parse_line(&line("m1", "r1", "m", 5, "2026-09-17T09:00:00Z")).unwrap();
        let (kept, _) = dedupe(vec![replay, original.clone()]);
        assert_eq!(kept[0].at, original.at);
    }

    #[test]
    fn buckets_split_by_day_and_by_model() {
        let rows = vec![
            parse_line(&line(
                "a",
                "1",
                "claude-opus-5",
                10,
                "2026-09-15T10:00:00+08:00",
            ))
            .unwrap(),
            parse_line(&line(
                "b",
                "2",
                "claude-sonnet-5",
                20,
                "2026-09-15T11:00:00+08:00",
            ))
            .unwrap(),
            parse_line(&line(
                "c",
                "3",
                "claude-opus-5",
                30,
                "2026-09-16T11:00:00+08:00",
            ))
            .unwrap(),
        ];
        let (buckets, undated) = buckets_of(&rows);
        assert_eq!(undated, 0);
        assert!(buckets.len() >= 2, "至少按模型和天分开了");
        let days: Vec<_> = buckets.iter().map(|b| b.day.as_str()).collect();
        assert!(days.windows(2).all(|w| w[0] <= w[1]), "{days:?} 不是升序");
    }

    /// 没有时刻的条目落不进任何一天。要有个数，不能悄悄消失。
    #[test]
    fn rows_without_a_timestamp_are_counted_not_dropped_silently() {
        let e = parse_line(
            r#"{"type":"assistant","uuid":"u1","message":{"id":"m1","model":"x",
               "usage":{"input_tokens":1,"output_tokens":2}}}"#,
        )
        .expect("没有 timestamp 也要解得出来");
        assert_eq!(e.at, i64::MIN);
        let (buckets, undated) = buckets_of(&[e]);
        assert!(buckets.is_empty());
        assert_eq!(undated, 1);
    }

    /// 缺字段按 0 算，不是整条丢掉 —— 实机上 `<synthetic>` 那几条就没有
    /// 缓存字段。模型名原样留着，界面自己决定要不要筛掉。
    #[test]
    fn missing_token_fields_count_as_zero_and_the_model_name_is_kept_verbatim() {
        let e = parse_line(
            r#"{"type":"assistant","uuid":"u1","timestamp":"2026-09-16T10:00:00Z",
               "message":{"id":"m1","model":"<synthetic>","usage":{"input_tokens":5}}}"#,
        )
        .unwrap();
        assert_eq!((e.output, e.cache_write, e.cache_read), (0, 0, 0));
        assert_eq!(e.model, "<synthetic>");
    }

    /// 按小时的桶只收窗口里的，按时间升序；最近请求新的在前、有封顶。
    #[test]
    fn hourly_buckets_and_recent_requests() {
        let rows: Vec<Entry> = (0..5)
            .map(|i| {
                parse_line(&line(
                    &format!("m{i}"),
                    "r",
                    "m",
                    10 + i,
                    &format!("2026-09-24T0{i}:30:00Z"),
                ))
                .unwrap()
            })
            .collect();
        let since = rows[2].at;
        let hourly = hourly_of(&rows, since);
        assert_eq!(
            hourly.iter().map(|b| b.messages).sum::<i64>(),
            3,
            "窗口外的两条不进"
        );
        let recent = recent_of(&rows, 2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].output, 14, "新的在前");
        assert_eq!(recent[1].output, 13);
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        // ⛔ 单测不许碰 `%LOCALAPPDATA%\ClaudeIpGate\`。
        let d = std::env::temp_dir().join(format!(
            "qbgate-tokens-{tag}-{}-{}",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 没跑过会话的槽位是正常状态，不是故障。
    #[test]
    fn a_slot_with_no_transcripts_reports_zeroes_not_an_error() {
        let d = scratch("empty");
        let got = for_slot(&d);
        assert_eq!(got.files_read, 0);
        assert_eq!(got.files_failed, 0);
        assert!(got.buckets.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 两个转写文件、其中一条是重放，走完整条 I/O 路径。
    #[test]
    fn a_slot_is_read_across_project_folders() {
        let d = scratch("read");
        let p1 = d.join("projects/proj-a");
        let p2 = d.join("projects/proj-b");
        std::fs::create_dir_all(&p1).unwrap();
        std::fs::create_dir_all(&p2).unwrap();
        std::fs::write(
            p1.join("s1.jsonl"),
            format!(
                "{}\n{}\n{}\n",
                r#"{"type":"user","message":{"content":"hi"}}"#,
                line("m1", "r1", "claude-opus-5", 40, "2026-09-16T10:00:00Z"),
                line("m2", "r2", "claude-opus-5", 60, "2026-09-16T10:01:00Z"),
            ),
        )
        .unwrap();
        // 续接出来的第二个会话，重放了 m2。
        std::fs::write(
            p2.join("s2.jsonl"),
            format!(
                "{}\n{}\n",
                line("m2", "r2", "claude-opus-5", 60, "2026-09-16T10:01:00Z"),
                line("m3", "r3", "claude-opus-5", 5, "2026-09-16T10:02:00Z"),
            ),
        )
        .unwrap();
        // 不是 .jsonl 的不许扫进来。
        std::fs::write(p2.join("notes.txt"), "\"usage\"").unwrap();

        let got = for_slot(&d);
        assert_eq!(got.files_read, 2);
        assert_eq!(got.sessions, 2);
        assert_eq!(got.duplicates, 1, "m2 被重放了一次");
        let total: i64 = got.buckets.iter().map(|b| b.output).sum();
        assert_eq!(total, 105, "40 + 60 + 5，重放的那条不再加");
        assert_eq!(got.recent.len(), 3);
        assert_eq!(got.recent[0].project, "proj-b", "最近请求带着项目目录名");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 截图那一刻的形状：槽位今天只有一条 `<synthetic>` 报错。
    /// 它不许算成「1 条回复」，要单独报出来。
    #[test]
    fn an_all_zero_error_row_is_an_empty_reply_not_a_message() {
        let d = scratch("synthetic");
        let p = d.join("projects/C--Users-x");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            p.join("s1.jsonl"),
            format!(
                "{}\n{}\n",
                synthetic_line("e1", "2026-09-24T06:30:41Z"),
                line("m1", "r1", "claude-opus-5", 40, "2026-09-24T06:40:00Z"),
            ),
        )
        .unwrap();
        let got = for_slot(&d);
        assert_eq!(got.empty_replies, 1);
        assert_eq!(got.empty_days.iter().map(|d| d.replies).sum::<i64>(), 1);
        assert_eq!(got.buckets.iter().map(|b| b.messages).sum::<i64>(), 1);
        assert!(got.buckets.iter().all(|b| b.model != "<synthetic>"));
        assert!(got.recent.iter().all(|r| r.model != "<synthetic>"));
        let _ = std::fs::remove_dir_all(&d);
    }

    // ------------------------------------------------------------ 归属

    // ⛔ 这几个 uuid 一律编造。别从本机的会话目录、桌面端记录里抄真的进来 ——
    // 那是账户与组织的标识，仓库是公开的（2026-09-24 推 GitHub 前查出来换掉过一次）。
    const ME: &str = "0000aaaa-1111-4222-8333-444455556666";
    const OTHER: &str = "0000bbbb-1111-4222-8333-444455556666";
    /// 面板里没有这个账户的槽位。随便编的 uuid —— 只要形状对。
    const STRANGER: &str = "00000000-0000-4000-8000-000000000007";
    const ORG: &str = "0000cccc-1111-4222-8333-444455556666";

    /// 写一份只有一条回复的转写，回输出 token 数好核对。
    fn transcript(dir: &Path, name: &str, id: &str, out: i64, extra_line: Option<&str>) {
        std::fs::create_dir_all(dir).unwrap();
        let mut text = String::new();
        if let Some(l) = extra_line {
            text.push_str(l);
            text.push('\n');
        }
        text.push_str(&line(
            id,
            &format!("r-{id}"),
            "claude-opus-5",
            out,
            "2026-09-19T10:00:00Z",
        ));
        text.push('\n');
        std::fs::write(dir.join(name), text).unwrap();
    }

    /// 桌面端的会话记录：`claude-code-sessions\<账户>\<org>\local_<id>.json`。
    fn desktop_record(profile: &Path, account: &str, cli_session: &str) {
        let d = profile.join("claude-code-sessions").join(account).join(ORG);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(format!("local_{cli_session}.json")),
            format!(
                r#"{{"sessionId":"local_{cli_session}","cliSessionId":"{cli_session}","cwd":"D:\\x","isArchived":false}}"#
            ),
        )
        .unwrap();
    }

    fn output_total(usage: &TokenUsage) -> i64 {
        usage.buckets.iter().map(|b| b.output).sum()
    }
    fn stray_total(usage: &TokenUsage) -> i64 {
        usage.unattributed.iter().map(|b| b.output).sum()
    }

    #[test]
    fn uuid_shape_is_checked_not_trusted() {
        assert!(looks_like_uuid(ME));
        assert!(!looks_like_uuid("not-a-uuid"));
        assert!(!looks_like_uuid("0000aaaa-1111-4222-8333-44445555666g"));
        assert!(!looks_like_uuid("0000aaaax1111-4222-8333-444455556666"));
    }

    /// 第 ② 级：项目目录名就是编码过的 cwd，账户 uuid 原样活在里面。
    #[test]
    fn a_scratch_workspace_project_name_carries_the_account_uuid() {
        let name = format!("C--Users-x-AppData-Roaming-Claude-demo-scratch-workspaces-{ME}-{ORG}-scratch-2026-09-19-d67e6d");
        assert_eq!(owner_from_project_name(&name).as_deref(), Some(ME));
        assert_eq!(owner_from_project_name("D--claude-gate"), None);
        assert_eq!(
            owner_from_project_name("x-scratch-workspaces-not-a-uuid"),
            None
        );
    }

    /// 第 ① 级：桌面端记录里的 `cliSessionId` 对上转写文件名。
    #[test]
    fn the_desktop_session_record_attributes_a_new_format_transcript() {
        let d = scratch("index");
        let profile = d.join("Claude-demo");
        desktop_record(&profile, ME, "S1");
        desktop_record(&profile, OTHER, "S2");
        // 不像 uuid 的目录不算账户目录。
        desktop_record(&profile, "junk", "S3");
        let index = OwnerIndex::scan(&[profile.clone()]);
        assert_eq!(index.len(), 2);
        assert_eq!(index.owner_of_session("s1"), Some(ME), "大小写不敏感");
        assert_eq!(index.owner_of_session("S2"), Some(OTHER));
        assert_eq!(index.owner_of_session("S3"), None);

        let home = d.join("home");
        let slot = d.join("slot");
        std::fs::create_dir_all(slot.join("projects")).unwrap();
        // 三份新格式转写（没有 bridge-session 行）：我的、别人的、谁都归不出的。
        transcript(&home.join("projects/D--proj"), "S1.jsonl", "m1", 10, None);
        transcript(&home.join("projects/D--proj"), "S2.jsonl", "m2", 20, None);
        transcript(&home.join("projects/D--proj"), "S9.jsonl", "m9", 40, None);

        let got = for_slot_and_default(&slot, Some(&home), Some(ME), &index);
        assert_eq!(got.files_read, 3);
        assert_eq!(output_total(&got), 10, "只有 S1 归我");
        assert_eq!(
            stray_total(&got),
            40,
            "S9 三级都归不出，进未归属；S2 是别人的，一个字不提"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 第 ③ 级还活着：旧转写靠 `bridge-session` 行归属。
    #[test]
    fn the_legacy_bridge_session_marker_still_counts() {
        let d = scratch("legacy");
        let home = d.join("home");
        let slot = d.join("slot");
        let marker = format!(
            r#"{{"type":"bridge-session","sessionId":"L1","ownerAccountUuid":"{ME}","ownerOrganizationUuid":"{ORG}"}}"#
        );
        transcript(
            &home.join("projects/D--old"),
            "L1.jsonl",
            "m1",
            7,
            Some(&marker),
        );
        let got = for_slot_and_default(&slot, Some(&home), Some(ME), &OwnerIndex::empty());
        assert_eq!(output_total(&got), 7);
        assert_eq!(stray_total(&got), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 第 ② 级走通整条路径：不用索引、不用旧标记。
    #[test]
    fn a_scratch_session_is_attributed_by_its_project_name_alone() {
        let d = scratch("cwd");
        let home = d.join("home");
        let slot = d.join("slot");
        let project = format!("C--x-scratch-workspaces-{ME}-{ORG}-scratch-1");
        transcript(
            &home.join("projects").join(project),
            "W1.jsonl",
            "m1",
            3,
            None,
        );
        let got = for_slot_and_default(&slot, Some(&home), Some(ME), &OwnerIndex::empty());
        assert_eq!(output_total(&got), 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 子代理转写归父会话：父会话归谁它就归谁，槽位目录里的也要数。
    #[test]
    fn subagent_transcripts_follow_their_parent_session() {
        let d = scratch("subagents");
        let profile = d.join("Claude-demo");
        desktop_record(&profile, ME, "P1");
        let index = OwnerIndex::scan(&[profile]);
        let home = d.join("home");
        let slot = d.join("slot");
        transcript(&home.join("projects/D--p"), "P1.jsonl", "m1", 5, None);
        transcript(
            &home.join("projects/D--p/P1/subagents"),
            "agent-a1.jsonl",
            "m2",
            11,
            None,
        );
        // 槽位自己的子代理转写，整个目录都是它的。
        transcript(
            &slot.join("projects/D--q/Q1/subagents"),
            "agent-b.jsonl",
            "m3",
            100,
            None,
        );
        let found = transcripts(&home.join("projects"));
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|t| t.session_id == "P1"), "{found:?}");
        let got = for_slot_and_default(&slot, Some(&home), Some(ME), &index);
        assert_eq!(output_total(&got), 116, "5 + 11 + 100");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Workflow 的子代理多套一层 `workflows\wf_<ID>\`（cc-switch 的扫描里有这一层）。
    /// 一样归父会话；`journal.jsonl` 这种不带用量的行自然解析不出东西。
    #[test]
    fn workflow_subagent_transcripts_are_counted_under_their_parent() {
        let d = scratch("workflows");
        let slot = d.join("slot");
        let wf = slot.join("projects/D--p/S1/subagents/workflows/wf_abc");
        transcript(&wf, "agent-w1.jsonl", "m1", 9, None);
        std::fs::write(wf.join("journal.jsonl"), "{\"type\":\"journal\"}\n").unwrap();
        let found = transcripts(&slot.join("projects"));
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|t| t.session_id == "S1"), "{found:?}");
        assert_eq!(output_total(&for_slot(&slot)), 9);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 没有账户 uuid 时默认目录一个字都不数 —— 归不了属就不能数。
    #[test]
    fn without_an_account_uuid_the_default_dir_is_never_credited() {
        let d = scratch("nouuid");
        let profile = d.join("Claude-demo");
        desktop_record(&profile, ME, "S1");
        let index = OwnerIndex::scan(&[profile]);
        let home = d.join("home");
        let slot = d.join("slot");
        transcript(&home.join("projects/D--p"), "S1.jsonl", "m1", 9, None);
        let got = for_slot_and_default(&slot, Some(&home), None, &index);
        assert_eq!(output_total(&got), 0);
        assert_eq!(
            stray_total(&got),
            0,
            "归得出属、只是不属于这个槽位：不进未归属"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 一次把默认目录分给所有槽位：各归各的，归不出的进未归属，
    /// 归到面板里没有的账户的单独一堆 —— 跟逐个调 `for_slot_and_default` 结果一致。
    #[test]
    fn scanning_all_slots_splits_the_default_dir_by_owner() {
        let d = scratch("all");
        let profile = d.join("Claude-x");
        desktop_record(&profile, ME, "S1");
        desktop_record(&profile, OTHER, "S2");
        desktop_record(&profile, STRANGER, "S7");
        let index = OwnerIndex::scan(&[profile]);
        let home = d.join("home");
        transcript(&home.join("projects/D--p"), "S1.jsonl", "m1", 10, None);
        transcript(&home.join("projects/D--p"), "S2.jsonl", "m2", 20, None);
        transcript(&home.join("projects/D--p"), "S7.jsonl", "m7", 70, None);
        transcript(&home.join("projects/D--p"), "S9.jsonl", "m9", 40, None);
        let me_dir = d.join("slot-me");
        transcript(&me_dir.join("projects/D--own"), "O1.jsonl", "m0", 1, None);
        let other_dir = d.join("slot-other");

        let slots = vec![
            SlotRef {
                label: "me".into(),
                dir: me_dir.clone(),
                account_uuid: Some(ME.into()),
            },
            SlotRef {
                label: "other".into(),
                dir: other_dir.clone(),
                account_uuid: Some(OTHER.to_ascii_uppercase()),
            },
        ];
        let all = scan_all_slots(&slots, Some(&home), &index);
        assert_eq!(all.slots.len(), 2);
        assert_eq!(all.slots[0].0, "me");
        assert_eq!(
            output_total(&all.slots[0].1),
            11,
            "自己目录 1 + 默认目录 S1 的 10"
        );
        assert_eq!(output_total(&all.slots[1].1), 20, "uuid 大小写不敏感");
        assert_eq!(stray_total(&all.slots[0].1), 40);
        assert_eq!(
            stray_total(&all.slots[1].1),
            40,
            "未归属对每个槽位都是同一份"
        );
        assert_eq!(
            all.other_accounts.iter().map(|b| b.output).sum::<i64>(),
            70,
            "S7 有主，只是主不在面板里"
        );

        let single = for_slot_and_default(&me_dir, Some(&home), Some(ME), &index);
        assert_eq!(
            single.buckets, all.slots[0].1.buckets,
            "特例跟全体扫描一个口径"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
