//! 酒馆的控制台（2026-09-29，使用者选的「实时控制台日志」）。
//!
//! # 为什么要有它
//!
//! 面板起酒馆一直是 `CREATE_NO_WINDOW`：**没有窗口**，而它的 stdout / stderr 也没接到任何地方。
//! 起不来时报错文案叫人「到它自己的窗口看最后几行」，那个窗口从来不存在 —— 使用者能看到的只有
//! 「进程在就绪前退出了」。现在面板起酒馆时把它的输出**追加**进 `state_dir\plugins\sillytavern-console.log`
//! （`sessions::OwnedProgram::launch_logged`），账户页「酒馆」弹窗的「控制台」按偏移量增量读它
//! （同一天第二轮它在软件页 SillyTavern 卡上，第三轮对调进了弹窗）。
//!
//! # 边界
//!
//! 1. **只收面板自己起的那份酒馆。** 使用者自己双击起的那份有它自己的窗口，面板读不到也不去读。
//! 2. **每次起一份新的就换一个文件**（[`rotate`]）：上一次的改名成 `.prev.log`（只留一份，界面上能切过去看），
//!    这一次从空文件开始。「这是哪一次的」看旁边那份 `sillytavern-console.json` 的 `run` ——
//!    不拿文件的创建时间当身份：Windows 的「文件隧道」会让 15 秒内删掉又重建的同名文件继承旧的创建时间。
//! 3. **只在本机**：日志里可能有酒馆打印的请求内容，面板只把它显示在自己的界面上，不上传、不进审计。
//! 4. **Claude 那条桥（使用者自己的 `bridge.py`）不收**：它有自己的调用日志与监控（软件页 Claude Code 卡最下面），
//!    而它的输出形状面板说不准 —— 别人那份可能把自己的密钥打印出来，收进来就是把密钥落了盘。
//! 5. 报错提示（[`hints_for`]）只认 SillyTavern / npm / cmd **真的会打印的句子**（照 1.19.0 的源码与实际输出），
//!    认不出来就不说，不猜。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 第一次打开（或换了一次运行）时最多往回读多少：长日志只给最后这一段。
const FIRST_MAX: u64 = 256 * 1024;
/// 之后每次最多读多少。读不完的下一轮接着读（`more`）。
const CHUNK_MAX: u64 = 512 * 1024;
/// 一行最多显示多少个字。酒馆会把整段请求打成一行 JSON，几十万字的一行会把界面拖死。
const LINE_MAX: usize = 4000;

pub fn log_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("sillytavern-console.log")
}

pub fn previous_log_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("sillytavern-console.prev.log")
}

fn run_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("sillytavern-console.json")
}

fn previous_run_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("sillytavern-console.prev.json")
}

/// 这一份日志是哪一次起的。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct RunInfo {
    run: String,
    started_at: String,
    launcher: String,
}

fn read_run(path: &Path) -> Option<RunInfo> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// 起一份新的酒馆之前调：上一次的日志与它的身份改名成 `.prev`（只留一份），这一次从空文件开始。
pub fn rotate(launcher: &Path) -> Result<()> {
    let (log, prev) = (log_path(), previous_log_path());
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if log.is_file() {
        let _ = std::fs::remove_file(&prev);
        std::fs::rename(&log, &prev)?;
        let _ = std::fs::remove_file(previous_run_path());
        let _ = std::fs::rename(run_path(), previous_run_path());
    }
    std::fs::write(&log, b"")?;
    let info = RunInfo {
        run: crate::config_io::id(),
        started_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        launcher: launcher.display().to_string(),
    };
    crate::config_io::replace(&run_path(), Some(&serde_json::to_vec(&info)?))?;
    Ok(())
}

/// 从日志里认出来的一件事，带一句能照着做的话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ConsoleHint {
    /// 稳定的键（前端按它去重）。
    pub id: String,
    pub title: String,
    pub detail: String,
    /// 触发它的那一行（截短）。
    pub line: String,
}

/// 读一次的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ConsoleChunk {
    /// 有没有这份日志（面板还没起过酒馆就是 `false`）。
    pub exists: bool,
    /// 这一份是哪一次起的：变了就是酒馆重起过，前端清空重读。
    pub run: String,
    /// 那一次是什么时候起的（本机时间）。
    pub started_at: String,
    /// 下次从这里接着读。
    #[ts(type = "number")]
    pub offset: u64,
    pub lines: Vec<String>,
    /// 这一次没从头读（太长，只给了最后一段）。
    pub truncated: bool,
    /// 还没读完，马上再读一次。
    pub more: bool,
    /// 日志文件在哪。
    pub path: String,
    /// 面板起的那份酒馆现在还在跑（`false` 时连没换行的最后半行也给出来）。
    pub running: bool,
    pub hints: Vec<ConsoleHint>,
}

/// 读当前（或上一次）那份日志。`run` 是前端手里那份的身份，对不上（或 `offset` 超过文件长度）就从头读。
pub fn read(previous: bool, run: Option<&str>, offset: u64, running: bool) -> Result<ConsoleChunk> {
    let (path, info) = if previous {
        (previous_log_path(), read_run(&previous_run_path()))
    } else {
        (log_path(), read_run(&run_path()))
    };
    let info = info.unwrap_or_default();
    let running = running && !previous;
    let mut out = ConsoleChunk {
        exists: false,
        run: info.run.clone(),
        started_at: info.started_at,
        offset: 0,
        lines: Vec::new(),
        truncated: false,
        more: false,
        path: path.display().to_string(),
        running,
        hints: Vec::new(),
    };
    let Ok(mut file) = std::fs::File::open(&path) else {
        return Ok(out);
    };
    out.exists = true;
    let len = file.metadata()?.len();
    let same_run = run.is_some_and(|r| r == info.run) && offset <= len;
    let (start, truncated) = if same_run {
        (offset, false)
    } else if len > FIRST_MAX {
        (len - FIRST_MAX, true)
    } else {
        (0, false)
    };
    let want = (len - start).min(CHUNK_MAX);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; want as usize];
    file.read_exact(&mut buf)?;
    let window = Window {
        bytes: &buf,
        start,
        mid_line: truncated,
        at_end: start + want == len,
        flush_partial: !running,
    };
    let (lines, consumed) = split_lines(&window);
    out.offset = start + consumed;
    out.more = start + want < len;
    out.truncated = truncated;
    out.hints = hints_for(&lines);
    out.lines = lines;
    Ok(out)
}

/// 读进来的那一段字节，以及它在文件里的位置。
struct Window<'a> {
    bytes: &'a [u8],
    start: u64,
    /// 从文件中间开始读的：第一行多半只有后半截，丢掉。
    mid_line: bool,
    /// 这一段读到了文件末尾。
    at_end: bool,
    /// 进程已经不在了：没换行的最后半行也算一行（它不会再写了）。
    flush_partial: bool,
}

/// 纯函数：一段字节 → 完整的行 + 用掉了多少字节。只按 `\n` 断行；
/// 每行去掉结尾的 `\r`，行内的 `\r`（进度条覆盖）只留最后一段。
fn split_lines(w: &Window) -> (Vec<String>, u64) {
    let mut bytes = w.bytes;
    let mut skipped = 0u64;
    if w.mid_line && w.start > 0 {
        match bytes.iter().position(|&b| b == b'\n') {
            Some(i) => {
                skipped = i as u64 + 1;
                bytes = &bytes[i + 1..];
            }
            None => return (Vec::new(), 0),
        }
    }
    let complete = match bytes.iter().rposition(|&b| b == b'\n') {
        Some(i) => i + 1,
        None => 0,
    };
    let take = if w.at_end && w.flush_partial {
        bytes.len()
    } else {
        complete
    };
    let chunk = &bytes[..take];
    let mut parts: Vec<&[u8]> = if chunk.is_empty() {
        Vec::new()
    } else {
        chunk.split(|&b| b == b'\n').collect()
    };
    // 「a\nb\n」按 \n 切出来是 a、b、空串 —— 结尾那个空串不是一行。
    if chunk.ends_with(b"\n") {
        parts.pop();
    }
    let lines = parts.into_iter().map(clean_line).collect();
    (lines, skipped + take as u64)
}

fn clean_line(raw: &[u8]) -> String {
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    // 行内的 \r：进度条靠它原地刷新，只留最后一段。
    let raw = match raw.iter().rposition(|&b| b == b'\r') {
        Some(i) => &raw[i + 1..],
        None => raw,
    };
    let text = crate::process::decode_console_bytes(raw);
    if text.chars().count() > LINE_MAX {
        let head: String = text.chars().take(LINE_MAX).collect();
        format!("{head}…（这一行太长，只显示前 {LINE_MAX} 个字）")
    } else {
        text
    }
}

// ------------------------------------------------------------------ 报错提示

/// 去掉 ANSI 颜色码再匹配（SillyTavern 的 `color.red(...)` 不管输出到哪都带着）。
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

struct Rule {
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    /// 任何一组里的**全部**片段都在这一行里（不分大小写）就算命中。
    any_of: &'static [&'static [&'static str]],
}

/// 认得出来的几件事。每一条都照 SillyTavern 1.19.0 / npm / cmd 真的会打印的句子写。
const RULES: &[Rule] = &[
    Rule {
        id: "node-missing",
        title: "没找到 Node.js",
        detail: "SillyTavern 是 Node.js 程序，Start.bat 第一步就要 npm。装 Node.js LTS（软件页 SillyTavern 卡的安装 / 更新确认框里可以勾选用 winget 装，或到 nodejs.org 下载），装完再起一次酒馆。",
        any_of: &[
            &["'npm'", "不是内部或外部命令"],
            &["'node'", "不是内部或外部命令"],
            &["'npm' is not recognized"],
            &["'node' is not recognized"],
        ],
    },
    Rule {
        id: "port-in-use",
        title: "酒馆端口被占了",
        detail: "别的程序（多半是另一份酒馆）占着这个端口。关掉它；或者换一个端口 —— 它自己 config.yaml 里的 port 与面板这边「位置与端口」里的酒馆端口要一起改。",
        any_of: &[&["is already in use"], &["EADDRINUSE"]],
    },
    Rule {
        id: "port-reserved",
        title: "这个端口被系统保留了",
        detail: "Windows 会把一段端口留给 Hyper-V / WSL（listen EACCES）。换一个端口（比如 8123）：它自己 config.yaml 里的 port 与面板这边的酒馆端口一起改。",
        any_of: &[&["listen", "EACCES"]],
    },
    Rule {
        id: "npm-network",
        title: "npm 连不上软件源",
        detail: "Start.bat 每次启动都先跑 npm install，它要联网拉依赖。检查网络；npm 不走 Windows 的系统代理设置，要走代理得给 npm 单独配（npm config set proxy …）或设 HTTPS_PROXY。网络好了再起一次酒馆。",
        any_of: &[
            &["npm", "ETIMEDOUT"],
            &["npm", "ECONNRESET"],
            &["npm", "ENOTFOUND"],
            &["npm", "EAI_AGAIN"],
            &["npm", "ECONNREFUSED"],
            &["npm", "certificate"],
        ],
    },
    Rule {
        id: "module-missing",
        title: "依赖没装全",
        detail: "node_modules 里缺包。Start.bat 每次启动都会先跑 npm install —— 往上看 npm 那几行多半有联网或权限报错；那个问题解决了再起一次酒馆。",
        any_of: &[
            &["ERR_MODULE_NOT_FOUND"],
            &["Cannot find module"],
            &["Cannot find package"],
        ],
    },
    Rule {
        id: "file-locked",
        title: "文件被占用或没有权限",
        detail: "多半是杀毒软件正在扫 node_modules，或者另一份酒馆 / 编辑器开着里面的文件。关掉它们再起一次；反复出现就把酒馆目录加进杀软的排除项（在杀软自己的设置里做）。",
        any_of: &[&["EPERM"], &["EBUSY"], &["resource busy or locked"]],
    },
    Rule {
        id: "config-broken",
        title: "config.yaml 写坏了",
        detail: "它自己的 config.yaml 有语法错误（报错那几行里有行号）。改好它；或者先备份再删掉，下次启动会重新生成一份默认的。",
        any_of: &[&["YAMLException"]],
    },
    Rule {
        id: "node-old",
        title: "Node.js 可能太旧",
        detail: "npm 报这一版酒馆要求的 Node.js 比你装的新。装新的 Node.js LTS（软件页 SillyTavern 卡的确认框里可以勾选用 winget 装）。",
        any_of: &[&["EBADENGINE"], &["Unsupported engine"]],
    },
    Rule {
        id: "out-of-memory",
        title: "内存不够",
        detail: "Node.js 用光了它能用的内存（多半是很大的聊天或角色卡）。关掉别的程序再试；还不行就把那个特别大的聊天挪出去。",
        any_of: &[&["heap out of memory"]],
    },
    Rule {
        id: "no-network-stack",
        title: "IPv4 与 IPv6 都没起来",
        detail: "酒馆两种协议都没能监听。检查它 config.yaml 里 protocol 那一段是不是把两种都关了。",
        any_of: &[&["Both IPv6 and IPv4 are"]],
    },
];

fn short(line: &str) -> String {
    let t = line.trim();
    if t.chars().count() > 200 {
        format!("{}…", t.chars().take(200).collect::<String>())
    } else {
        t.to_string()
    }
}

/// 纯函数：这几行里认得出哪些事。同一件事只报一次（第一次出现的那一行）。
pub fn hints_for(lines: &[String]) -> Vec<ConsoleHint> {
    let mut out: Vec<ConsoleHint> = Vec::new();
    for line in lines {
        let text = plain(line);
        let lower = text.to_lowercase();
        for rule in RULES {
            if out.iter().any(|h| h.id == rule.id) {
                continue;
            }
            let hit = rule.any_of.iter().any(|group| {
                group
                    .iter()
                    .all(|needle| lower.contains(&needle.to_lowercase()))
            });
            if hit {
                out.push(ConsoleHint {
                    id: rule.id.into(),
                    title: rule.title.into(),
                    detail: rule.detail.into(),
                    line: short(&text),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        hints_for(&lines(v)).into_iter().map(|h| h.id).collect()
    }

    fn split(bytes: &[u8], start: u64, mid: bool, at_end: bool, flush: bool) -> (Vec<String>, u64) {
        split_lines(&Window {
            bytes,
            start,
            mid_line: mid,
            at_end,
            flush_partial: flush,
        })
    }

    /// 只交出完整的行：没换行的半行留到下一次（进程还在写）。
    #[test]
    fn a_half_written_line_waits_for_its_newline() {
        let (l, used) = split(b"one\r\ntwo\nthr", 0, false, true, false);
        assert_eq!(l, ["one", "two"]);
        assert_eq!(used, 9);
    }

    /// 进程已经不在了：最后那半行不会再写完，照样给出来。
    #[test]
    fn the_last_half_line_is_flushed_once_the_process_is_gone() {
        let (l, used) = split(b"one\nlast words", 0, false, true, true);
        assert_eq!(l, ["one", "last words"]);
        assert_eq!(used, 14);
    }

    /// 从文件中间开始读（只给最后一段）：第一行的后半截丢掉，用掉的字节把它算进去。
    #[test]
    fn reading_from_the_middle_drops_the_torn_first_line() {
        let (l, used) = split(b"rn line\nwhole\n", 100, true, true, false);
        assert_eq!(l, ["whole"]);
        assert_eq!(used, 14);
    }

    /// 进度条靠行内的 \r 原地刷新：只留最后一段。
    #[test]
    fn carriage_return_progress_keeps_only_the_final_frame() {
        let (l, _) = split(b"10%\r50%\r100%\n", 0, false, true, false);
        assert_eq!(l, ["100%"]);
    }

    /// 空行照样是一行（酒馆拿空行排版），但结尾那个换行不会多出一个空行。
    #[test]
    fn blank_lines_survive_but_no_phantom_line_at_the_end() {
        let (l, _) = split(b"a\n\nb\n", 0, false, true, true);
        assert_eq!(l, ["a", "", "b"]);
    }

    #[test]
    fn a_giant_line_is_cut_with_a_note() {
        let long = "x".repeat(LINE_MAX + 10);
        let t = clean_line(long.as_bytes());
        assert!(t.starts_with(&"x".repeat(LINE_MAX)));
        assert!(t.contains("这一行太长"), "{t}");
    }

    /// cmd 的中文与英文两种说法都认（`Start.bat` 第一步就是 npm）。
    #[test]
    fn missing_node_is_recognized_in_chinese_and_english() {
        assert_eq!(
            ids(&["'npm' 不是内部或外部命令，也不是可运行的程序"]),
            ["node-missing"]
        );
        assert_eq!(
            ids(&["'node' is not recognized as an internal or external command,"]),
            ["node-missing"]
        );
    }

    /// 1.19.0 `server-startup.js` 的原话，前面还带着 `color.red` 的颜色码。
    #[test]
    fn sillytavern_own_port_message_is_recognized_through_its_colors() {
        let line = "\u{1b}[31mAddress 127.0.0.1:8000 is already in use. Another SillyTavern instance may already be running. Stop the other process or change \"port\" in config.yaml.\u{1b}[39m";
        let h = hints_for(&lines(&[line]));
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].id, "port-in-use");
        assert!(!h[0].line.contains('\u{1b}'), "{:?}", h[0].line);
    }

    #[test]
    fn a_reserved_port_is_not_mistaken_for_a_busy_one() {
        assert_eq!(
            ids(&["Error: listen EACCES: permission denied 127.0.0.1:8000"]),
            ["port-reserved"]
        );
    }

    #[test]
    fn npm_network_failures_are_told_apart_from_missing_modules() {
        assert_eq!(
            ids(&[
                "npm error code ETIMEDOUT",
                "npm error network request to https://registry.npmjs.org/ failed"
            ]),
            ["npm-network"]
        );
        assert_eq!(
            ids(&["Error [ERR_MODULE_NOT_FOUND]: Cannot find package 'express' imported from C:\\x\\server.js"]),
            ["module-missing"]
        );
    }

    /// 同一件事刷一百行也只报一次；不相干的行一条都不报。
    #[test]
    fn each_hint_is_reported_once_and_normal_lines_stay_quiet() {
        assert_eq!(
            ids(&["EADDRINUSE", "EADDRINUSE", "EADDRINUSE"]),
            ["port-in-use"]
        );
        assert!(ids(&[
            "Node version: v24.14.1. Running in production environment.",
            "SillyTavern 1.19.0",
            "SillyTavern is listening on IPv4: 127.0.0.1:8000",
            "Warning: failed to start server on IPv6",
            "Go to: http://127.0.0.1:8000/ to open SillyTavern",
        ])
        .is_empty());
    }

    #[test]
    fn every_rule_has_a_unique_id_and_something_to_match() {
        let mut seen = std::collections::BTreeSet::new();
        for r in RULES {
            assert!(seen.insert(r.id), "重复的 id：{}", r.id);
            assert!(!r.any_of.is_empty() && r.any_of.iter().all(|g| !g.is_empty()));
            assert!(!r.detail.contains("**"), "文案里不写 Markdown：{}", r.id);
        }
    }
}
