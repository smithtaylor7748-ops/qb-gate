//! 酒馆装在哪 —— 跟 [`crate::install::inventory`] 同一个道理的第二张表。
//!
//! # 为什么不能照抄 inventory
//!
//! Claude 的落点是**有限的几种安装器**（原生、winget、Scoop、npm、桌面端自带…），
//! 每一种的目录都可以写死成一条规则。酒馆和桥接不是：它们是使用者自己
//! clone 下来放的，可能在任何一个盘的任何一层。硬编码一份路径表，
//! 在作者机器上永远对、在别人机器上永远错 —— 这正是 v0.12.0 删掉
//! `D:\tools\…` 那几行的原因。
//!
//! 所以这里换一条路：**按证据找**，三档，一档比一档贵：
//!
//! | 档 | 依据 | 代价 |
//! |---|---|---|
//! | [`Evidence::Running`] | 正在跑的 `bridge.py` 的命令行 | 一次进程枚举 |
//! | [`Evidence::PidFile`] | `bridge.pid` 记的那个进程的命令行 | 同上 |
//! | [`Evidence::Scan`] | 从固定盘根与家目录起的限深扫描 | 秒级 |
//!
//! 跑过一次桥接的人，第一档就够了，零扫描。没跑过的才落到第三档。
//!
//! # 认标志物，不认目录名
//!
//! 目录叫什么完全不作数（`claude-code-sillytavern-bridge-v2`、`bridge`、
//! `酒馆桥接` 都有人用）。判据是**这个目录里有没有那个东西**：
//!
//!   - 桥接：`bridge.py`，而且内容里同时有 `--claude` 和 `--data-dir`
//!     两个参数名 —— 面板正是拿这两个参数起它的。别人的 `bridge.py` 没有这两个。
//!   - 酒馆：`server.js` 加上 `package.json` 里 `"name"` 是 sillytavern。
//!     光看 `server.js` 会把一堆 Node 项目认进来。
//!
//! # 备份目录必须排在后面
//!
//! 实机上一个使用者的盘里有九份 `claude-code-sillytavern-bridge*`：一份在用，
//! 八份是 `备份1`…`备份8` 和 `*.reinstall-backup-*`。**随便挑一个就是挑错。**
//! [`score`] 给带备份词的路径扣分、给最近改过的加分、给浅的加分，
//! 排完让人自己确认 —— 面板不替他决定用哪一份。
//!
//! # 只读
//!
//! 这里只 `read_dir` / `metadata` / 读几个文件的前几百 KB，不写盘、不起进程、
//! 不联网。起点全部来自 [`Roots`]，单测自己搭一棵临时目录树喂进来 ——
//! 单测不许碰真实的运行期状态。

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;
use ts_rs::TS;

/// 一条候选是怎么来的。界面上要如实显示 —— 「正在跑的那个」和
/// 「盘里翻出来的」可信度差着量级，合成一句「找到了」等于骗人。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TavernEvidence {
    /// 现在就有一个桥接进程在用这个路径。
    Running,
    /// `bridge.pid` 记的那个进程在用这个路径。
    PidFile,
    /// 扫盘扫出来的。
    Scan,
    /// 已经填在配置里的那一份（用来告诉人「你填的这个还在不在」）。
    Configured,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TavernCandidate {
    pub path: String,
    pub evidence: TavernEvidence,
    /// 一句话说明为什么排在这里（「最近改过」「路径里带『备份』」…）。
    pub note: String,
    pub score: i32,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct TavernSurvey {
    pub bridge: Vec<TavernCandidate>,
    pub sillytavern: Vec<TavernCandidate>,
    pub launcher: Vec<TavernCandidate>,
    pub scanned_dirs: u32,
    /// 预算用完了，下面的结果**可能不全**。界面上必须说出来 ——
    /// 把「没找到」和「没找完」显示成同一句，人会去装一份他其实已经有的东西。
    pub truncated: bool,
}

// ------------------------------------------------------------------ 预算

/// 扫描的上限。没有上限的扫盘在插着几块大硬盘的机器上会跑到天亮。
#[derive(Debug, Clone)]
pub struct Budget {
    pub max_depth: usize,
    pub max_dirs: u32,
    pub seconds: u64,
}

impl Budget {
    /// 默认档：够到「盘根下两三层」这种常见摆法，几秒内回来。
    pub fn quick() -> Self {
        Self {
            max_depth: 4,
            max_dirs: 40_000,
            seconds: 15,
        }
    }

    /// 深扫：默认档没找着时由使用者**显式**点。
    pub fn deep() -> Self {
        Self {
            max_depth: 7,
            max_dirs: 400_000,
            seconds: 90,
        }
    }
}

/// 扫描起点。平时用 [`Roots::current`]，单测自己搭。
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// 从哪些目录开始走。
    pub starts: Vec<PathBuf>,
    /// 已经拿到的桥接进程命令行（[`TavernEvidence::Running`] 的来源）。
    pub running_cmdlines: Vec<String>,
    /// `bridge.pid` 指的那个进程的命令行。
    pub pidfile_cmdline: Option<String>,
    /// 配置里已经填着的三个路径，用来产出 [`TavernEvidence::Configured`]。
    pub configured: Vec<PathBuf>,
}

impl Roots {
    /// 固定盘的根 + 家目录。**不碰网络盘和可移动盘** ——
    /// 一个断了线的映射盘能让 `read_dir` 卡上几十秒。
    pub fn current() -> Self {
        let mut starts = fixed_drive_roots();
        if let Some(h) = dirs::home_dir() {
            if !starts.iter().any(|s| s == &h) {
                starts.insert(0, h);
            }
        }
        Self {
            starts,
            ..Default::default()
        }
    }
}

#[cfg(windows)]
fn fixed_drive_roots() -> Vec<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    /// `DRIVE_FIXED`。windows 0.58 没把这个常量导出到
    /// `Win32::Storage::FileSystem`，所以按 winbase.h 的定义写在这里。
    const DRIVE_FIXED: u32 = 3;

    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = char::from(b'A' + i as u8);
        let root: Vec<u16> = format!("{letter}:\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } == DRIVE_FIXED {
            out.push(PathBuf::from(format!("{letter}:\\")));
        }
    }
    out
}

#[cfg(not(windows))]
fn fixed_drive_roots() -> Vec<PathBuf> {
    Vec::new()
}

// ------------------------------------------------------------------ 标志物

/// 这个目录是不是桥接项目。
///
/// 只认 `bridge.py` 的文件名是不够的 —— 实机上别的项目也有同名文件。
/// 加判内容里有没有 `--claude` 和 `--data-dir`：面板就是拿这两个参数
/// 起它的，没有这两个的 `bridge.py` 起来也是立刻报错。
pub fn is_bridge_root(dir: &Path) -> bool {
    let f = dir.join("bridge.py");
    match read_head(&f, 512 * 1024) {
        Some(t) => t.contains("--claude") && t.contains("--data-dir"),
        None => false,
    }
}

/// 这个目录是不是 SillyTavern。
///
/// `server.js` 一个条件太松（任何 Node 项目都可能有），所以还要
/// `package.json` 的 `"name"` 认下来。
pub fn is_sillytavern_root(dir: &Path) -> bool {
    if !dir.join("server.js").is_file() {
        return false;
    }
    match read_head(&dir.join("package.json"), 64 * 1024) {
        Some(t) => t.to_lowercase().contains("sillytavern"),
        None => false,
    }
}

/// 读文件开头若干字节，按 UTF-8 有损转换。读不了就是 `None`。
fn read_head(path: &Path, limit: usize) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(limit as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

// ------------------------------------------------------------------ 排序

/// 路径里带这些词 = 多半是备份 / 旧版 / 停用的那一份。
///
/// 实机上的样本：`备份1`…`备份8`、`claude-code-sillytavern-bridge.reinstall-backup-20260605-004247`、
/// `backup-before-auto-embed-auth-20260822-040010`、`*.legacy-disabled`、
/// `superseded-claude-profile-*`、`migration-backups`。
const STALE_WORDS: &[&str] = &[
    "backup",
    "备份",
    "bak",
    "legacy",
    "superseded",
    "obsolete",
    "deprecated",
    "archive",
    "归档",
    "旧版",
    "old",
    "before-",
    "-copy",
    "副本",
    "recycle",
];

/// 跟桥接同在一个父目录下的加分。见 [`locate`] 里用它的那一段。
///
/// 120 分的量级是算出来的，不是拍的：它要压得过「深一层」（−2）和
/// 「新旧差一档」（±30），又**不能压过备份扣分**（−500）——
/// 一份躺在桥接旁边的备份不该因为挨得近就爬到在用的那份前面。
const NEIGHBOUR_BONUS: i32 = 120;

/// 候选排序用的分数与理由。分数只在同一类候选之间比较，没有绝对含义。
pub fn score(
    path: &Path,
    evidence: TavernEvidence,
    now: Option<std::time::SystemTime>,
) -> (i32, String) {
    let mut score = match evidence {
        // 正在跑的那份是事实，不是推测 —— 必须压过任何扫出来的东西。
        TavernEvidence::Running => 1_000,
        TavernEvidence::PidFile => 800,
        TavernEvidence::Configured => 600,
        TavernEvidence::Scan => 100,
    };
    let mut notes: Vec<String> = Vec::new();

    let lower = path.to_string_lossy().to_lowercase();
    if STALE_WORDS.iter().any(|w| lower.contains(w)) {
        score -= 500;
        notes.push("路径里有备份/旧版字样".into());
    }

    // 浅的优先。深处的那份通常是解压出来的副本。
    let depth = path.components().count() as i32;
    score -= depth * 2;

    if let (Some(now), Ok(meta)) = (now, std::fs::metadata(path)) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = now.duration_since(modified) {
                let days = age.as_secs() / 86_400;
                if days <= 30 {
                    score += 60;
                    notes.push("30 天内改过".into());
                } else if days <= 180 {
                    score += 30;
                    notes.push("半年内改过".into());
                } else {
                    notes.push(format!("上次改动在 {days} 天前"));
                }
            }
        }
    }

    let note = if notes.is_empty() {
        match evidence {
            TavernEvidence::Running => "正在运行".into(),
            TavernEvidence::PidFile => "上次运行记录".into(),
            TavernEvidence::Configured => "当前配置".into(),
            TavernEvidence::Scan => "扫描命中".into(),
        }
    } else {
        notes.join(" · ")
    };
    (score, note)
}

// ------------------------------------------------------------------ 命令行

/// 从一条命令行里抠出 `bridge.py` 所在的目录。
///
/// 命令行长这样（带不带引号都可能）：
/// `"C:\py\python.exe" -u I:\x\bridge-v2\bridge.py --claude … --data-dir …`
///
/// 只取**带目录的**那个 —— 裸的 `bridge.py`（`cwd` 相对路径）给不出位置，
/// 硬拿它去 join 就又回到「路径不存在: bridge.py」那句话了。
pub fn bridge_root_from_cmdline(cmd: &str) -> Option<PathBuf> {
    for raw in tokenize(cmd) {
        let t = raw.trim();
        if !t.to_lowercase().ends_with("bridge.py") {
            continue;
        }
        let p = Path::new(t);
        let parent = p.parent()?;
        if parent.as_os_str().is_empty() {
            continue;
        }
        return Some(parent.to_path_buf());
    }
    None
}

/// 按空格切，但**尊重双引号** —— Windows 的路径里有空格是常态
/// （`C:\Program Files\…`），照空格硬切会把它劈成两半。
fn tokenize(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in cmd.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ------------------------------------------------------------------ 扫描

/// 一律跳过的目录名（小写全等比较）。
///
/// 这张表的作用不是「快一点」，是**能不能在预算内走完**：
/// 少一条 `node_modules`，一个前端仓库就能吃掉几万个目录。
const SKIP_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "windows",
    "winsxs",
    "program files",
    "program files (x86)",
    "programdata",
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    "site-packages",
    "dist",
    "build",
    "obj",
    "bin",
    ".cache",
    ".next",
    ".nuxt",
    "temp",
    "tmp",
    "$windows.~ws",
    "$windows.~bt",
    "recovery",
    "perflogs",
];

fn skipped(name: &str) -> bool {
    let lower = name.to_lowercase();
    // 隐藏/点开头的目录一律不进 —— 里面不会有人放酒馆，却常常很深。
    lower.starts_with('.') || SKIP_DIRS.contains(&lower.as_str())
}

/// 走一遍，把两类标志物都收上来。
///
/// 广度优先：浅的先出，预算用完时留下的是**最可能对的那些**，
/// 而不是某一个盘的某条深巷子。
fn walk(roots: &Roots, budget: &Budget) -> (Vec<PathBuf>, Vec<PathBuf>, u32, bool) {
    let started = Instant::now();
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = roots
        .starts
        .iter()
        .filter(|p| p.is_dir())
        .map(|p| (p.clone(), 0usize))
        .collect();

    let mut bridges = Vec::new();
    let mut taverns = Vec::new();
    let mut seen = 0u32;
    let mut truncated = false;

    while let Some((dir, depth)) = queue.pop_front() {
        if seen >= budget.max_dirs || started.elapsed().as_secs() >= budget.seconds {
            truncated = true;
            break;
        }
        seen += 1;

        // 一次 `read_dir` 同时办两件事：认标志物、收子目录。
        //
        // 分开写的话每个目录要多三次 `stat`（bridge.py / server.js /
        // package.json 各一次），而这一趟要走几万个目录 —— 那三次
        // 乘出来就是预算够不够用的差别。
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let (mut has_bridge_py, mut has_server_js, mut has_pkg) = (false, false, false);
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().to_lowercase();
            if ft.is_file() {
                match name.as_str() {
                    "bridge.py" => has_bridge_py = true,
                    "server.js" => has_server_js = true,
                    "package.json" => has_pkg = true,
                    _ => {}
                }
                continue;
            }
            // 联结点 / 符号链接不进：`C:\Documents and Settings` 这类
            // 指回自己的联结点会把 BFS 变成死循环。
            if ft.is_symlink() || !ft.is_dir() || skipped(&name) {
                continue;
            }
            subdirs.push(e.path());
        }

        // 文件名对上了才去读内容 —— 内容判据（`--claude` / `--data-dir`、
        // `package.json` 里的 name）贵得多，不能每个目录都做一遍。
        if has_bridge_py && is_bridge_root(&dir) {
            bridges.push(dir.clone());
        }
        if has_server_js && has_pkg && is_sillytavern_root(&dir) {
            taverns.push(dir.clone());
        }
        if depth >= budget.max_depth {
            continue;
        }
        for p in subdirs {
            queue.push_back((p, depth + 1));
        }
    }
    (bridges, taverns, seen, truncated)
}

/// 找酒馆的启动脚本。
///
/// 顺序有讲究：**先找外面那个 `.cmd`/`.bat`**（它通常会 `cd` 进去再调
/// `Start.bat`，带着 `chcp 65001` 这类必需的准备动作），找不到才退回
/// 酒馆目录里的 `Start.bat`。反过来的话，在需要外层脚本的部署上
/// 会起出一个乱码的控制台。
fn launchers_for(st_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let leaf = st_root
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if let Some(parent) = st_root.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for e in entries.flatten() {
                let p = e.path();
                let ext = p
                    .extension()
                    .map(|s| s.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if ext != "cmd" && ext != "bat" {
                    continue;
                }
                // 只认真的提到这个酒馆目录的脚本。
                if let Some(t) = read_head(&p, 32 * 1024) {
                    if t.to_lowercase().contains(&leaf) {
                        out.push(p);
                    }
                }
            }
        }
    }
    for name in ["Start.bat", "start.bat", "start.cmd"] {
        let p = st_root.join(name);
        if p.is_file() && !out.iter().any(|q| q == &p) {
            out.push(p);
        }
    }
    out
}

// ------------------------------------------------------------------ 入口

/// 找一遍。**纯读，不写任何配置** —— 采用哪一条由调用方（最终是使用者）决定。
pub fn locate(roots: &Roots, budget: &Budget) -> TavernSurvey {
    let now = Some(std::time::SystemTime::now());
    let mut bridge: Vec<TavernCandidate> = Vec::new();
    let mut sillytavern: Vec<TavernCandidate> = Vec::new();
    let mut launcher: Vec<TavernCandidate> = Vec::new();

    let push = |list: &mut Vec<TavernCandidate>, p: &Path, ev: TavernEvidence| {
        let path = p.to_string_lossy().into_owned();
        if list.iter().any(|c| c.path.eq_ignore_ascii_case(&path)) {
            return;
        }
        let (score, note) = score(p, ev, now);
        list.push(TavernCandidate {
            path,
            evidence: ev,
            note,
            score,
        });
    };

    // ---- 第一档：正在跑的进程。零扫描，也最可信。
    for cmd in &roots.running_cmdlines {
        if let Some(root) = bridge_root_from_cmdline(cmd) {
            if root.is_dir() {
                push(&mut bridge, &root, TavernEvidence::Running);
            }
        }
    }
    // ---- 第二档：PID 文件记的那个。
    if let Some(cmd) = &roots.pidfile_cmdline {
        if let Some(root) = bridge_root_from_cmdline(cmd) {
            if root.is_dir() {
                push(&mut bridge, &root, TavernEvidence::PidFile);
            }
        }
    }
    // ---- 已经填着的：只是为了告诉人「你填的这个还在」。
    for p in &roots.configured {
        if p.as_os_str().is_empty() {
            continue;
        }
        if is_bridge_root(p) {
            push(&mut bridge, p, TavernEvidence::Configured);
        } else if is_sillytavern_root(p) {
            push(&mut sillytavern, p, TavernEvidence::Configured);
        } else if p.is_file() {
            push(&mut launcher, p, TavernEvidence::Configured);
        }
    }

    // ---- 第三档：扫盘。桥接已经由前两档定下来时**也要扫** ——
    // 酒馆目录还没有着落，而两者未必挨在一起。
    let (bridges, taverns, scanned_dirs, truncated) = walk(roots, budget);
    for p in &bridges {
        push(&mut bridge, p, TavernEvidence::Scan);
    }
    for p in &taverns {
        push(&mut sillytavern, p, TavernEvidence::Scan);
    }
    for p in &taverns {
        for l in launchers_for(p) {
            push(&mut launcher, &l, TavernEvidence::Scan);
        }
    }

    // ---- 跟桥接挨在一起的酒馆优先。
    //
    // 这一条在实机上是**决定性**的，不是锦上添花：那台机器上
    // `…\<酒馆目录>\SillyTavern`（在用）和
    // `…\<酒馆目录>\SillyTavern-Launcher\SillyTavern`（启动器自带的另一份）
    // 只差 2 分 —— 差在深度，也就是全靠运气。而人是把整套东西放在
    // 一个目录下的，跟桥接同一个父目录的那一份才是他在用的。
    bridge.sort_by_key(|a| std::cmp::Reverse(a.score));
    if let Some(anchor) = bridge.first().and_then(|c| Path::new(&c.path).parent()) {
        for list in [&mut sillytavern, &mut launcher] {
            for c in list.iter_mut() {
                let p = Path::new(&c.path);
                // **直接兄弟**拿满分，只是「在这棵子树底下」拿一半。
                //
                // 差别是必需的：实机上 `…\<酒馆目录>\SillyTavern` 和
                // `…\<酒馆目录>\SillyTavern-Launcher\SillyTavern` 都在锚点底下，
                // 一视同仁的话两者仍然只差 2 分 —— 加了等于没加。
                let bonus = if p.parent() == Some(anchor) {
                    NEIGHBOUR_BONUS
                } else if p.starts_with(anchor) {
                    NEIGHBOUR_BONUS / 2
                } else {
                    0
                };
                if bonus > 0 {
                    c.score += bonus;
                    c.note = format!("{} · 跟桥接在同一个目录下", c.note);
                }
            }
        }
    }

    for list in [&mut bridge, &mut sillytavern, &mut launcher] {
        list.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        list.truncate(8);
    }

    TavernSurvey {
        bridge,
        sillytavern,
        launcher,
        scanned_dirs,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("qb-tavern-locate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_bridge(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("bridge.py"),
            "import argparse\np.add_argument('--claude')\np.add_argument('--data-dir')\n",
        )
        .unwrap();
    }

    fn make_tavern(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("server.js"), "// st").unwrap();
        std::fs::write(dir.join("package.json"), r#"{"name":"sillytavern"}"#).unwrap();
    }

    /// 同名文件不算数：判据是**内容里那两个参数**。
    ///
    /// 光认文件名的话，盘里任何一个叫 bridge.py 的东西都会被当成桥接，
    /// 起起来立刻报参数错，而面板已经说「找到了」。
    #[test]
    fn a_bridge_py_without_the_expected_flags_is_not_a_bridge_root() {
        let root = tmp("flags");
        let real = root.join("real");
        make_bridge(&real);
        let fake = root.join("fake");
        std::fs::create_dir_all(&fake).unwrap();
        std::fs::write(fake.join("bridge.py"), "print('unrelated project')").unwrap();

        assert!(is_bridge_root(&real));
        assert!(!is_bridge_root(&fake));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 有 `server.js` 不等于是酒馆。
    #[test]
    fn a_plain_node_project_is_not_sillytavern() {
        let root = tmp("node");
        let st = root.join("SillyTavern");
        make_tavern(&st);
        let other = root.join("some-api");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("server.js"), "// express").unwrap();
        std::fs::write(other.join("package.json"), r#"{"name":"some-api"}"#).unwrap();

        assert!(is_sillytavern_root(&st));
        assert!(!is_sillytavern_root(&other));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 备份目录排在在用的那一份**后面**。
    ///
    /// 这是实机上的形状：一份在用，八份 `备份N` 和 `*.reinstall-backup-*`。
    /// 不排序就是九选一，随便挑必然挑错。
    #[test]
    fn backups_rank_below_the_live_copy() {
        let root = tmp("rank");
        let live = root.join("bridge-v2");
        make_bridge(&live);
        for name in ["备份1", "bridge.reinstall-backup-20260605", "old-bridge"] {
            make_bridge(&root.join(name));
        }
        let roots = Roots {
            starts: vec![root.clone()],
            ..Default::default()
        };
        let survey = locate(&roots, &Budget::quick());
        assert_eq!(survey.bridge.len(), 4, "四份都要列出来，不许悄悄丢");
        assert!(
            survey.bridge[0].path.ends_with("bridge-v2"),
            "在用的那份要排第一，实得 {}",
            survey.bridge[0].path
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 正在跑的那条命令行压过任何扫出来的结果。
    #[test]
    fn a_running_bridge_outranks_everything_found_on_disk() {
        let root = tmp("running");
        let scanned = root.join("some-bridge");
        make_bridge(&scanned);
        // 故意让「正在跑的」那份路径里带备份词：证据档位要能压过扣分。
        let running = root.join("backup-but-actually-running");
        make_bridge(&running);

        let roots = Roots {
            starts: vec![root.clone()],
            running_cmdlines: vec![format!(
                "python.exe -u {}\\bridge.py --claude c:\\claude.exe --data-dir d:\\dd --port 5001",
                running.display()
            )],
            ..Default::default()
        };
        let survey = locate(&roots, &Budget::quick());
        assert_eq!(survey.bridge[0].evidence, TavernEvidence::Running);
        assert!(survey.bridge[0]
            .path
            .ends_with("backup-but-actually-running"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_quoted_path_with_spaces_survives_tokenizing() {
        let cmd =
            r#""C:\Program Files\Py\python.exe" -u "D:\my stuff\bridge v2\bridge.py" --port 5001"#;
        assert_eq!(
            bridge_root_from_cmdline(cmd),
            Some(PathBuf::from(r"D:\my stuff\bridge v2"))
        );
    }

    /// 裸 `bridge.py` 给不出位置 —— 认了它就又回到那句
    /// 「路径不存在: bridge.py」。
    #[test]
    fn a_bare_relative_bridge_py_yields_no_root() {
        assert_eq!(
            bridge_root_from_cmdline("python.exe -u bridge.py --port 5001"),
            None
        );
    }

    /// 外层启动脚本优先于酒馆目录里的 `Start.bat`。
    #[test]
    fn the_outer_launcher_wins_over_the_inner_start_bat() {
        let root = tmp("launcher");
        let st = root.join("SillyTavern");
        make_tavern(&st);
        std::fs::write(st.join("Start.bat"), "node server.js").unwrap();
        std::fs::write(
            root.join("start-sillytavern.cmd"),
            "chcp 65001\ncd /d \"%~dp0SillyTavern\"\ncall Start.bat",
        )
        .unwrap();

        let roots = Roots {
            starts: vec![root.clone()],
            ..Default::default()
        };
        let survey = locate(&roots, &Budget::quick());
        assert!(
            survey.launcher[0].path.ends_with("start-sillytavern.cmd"),
            "实得 {}",
            survey.launcher[0].path
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 跟桥接同一个父目录的那份酒馆排在前面。
    ///
    /// 实机形状：`…\<酒馆目录>\SillyTavern`（在用）对上
    /// `…\<酒馆目录>\SillyTavern-Launcher\SillyTavern`（启动器自带的另一份）。
    /// 只按深度和时间排，两者差 2 分 —— 那不叫排序，叫抛硬币。
    #[test]
    fn the_sillytavern_next_to_the_bridge_wins() {
        let root = tmp("neighbour");
        let home = root.join("chat");
        make_bridge(&home.join("bridge-v2"));
        let live = home.join("SillyTavern");
        make_tavern(&live);
        // 另一份也在锚点**底下**，只是深一层 —— 这正是实机形状
        // （`…\SillyTavern-Launcher\SillyTavern`）。一视同仁地加分的话
        // 两者仍然只差深度那 2 分，所以直接兄弟必须拿得更多。
        make_tavern(&home.join("SillyTavern-Launcher").join("SillyTavern"));
        // 再放一份埋在别处、深度更浅的，让「浅的优先」反过来偏向它。
        make_tavern(&root.join("ST"));

        let roots = Roots {
            starts: vec![root.clone()],
            ..Default::default()
        };
        let survey = locate(&roots, &Budget::quick());
        assert!(
            survey.sillytavern[0].path.ends_with(r"chat\SillyTavern"),
            "实得 {}",
            survey.sillytavern[0].path
        );
        assert!(survey.sillytavern[0].note.contains("跟桥接在同一个目录下"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 但「挨着桥接」压不过「这是个备份」。
    ///
    /// 备份往往就躺在在用的那份旁边（`bridge-v2\backup-before-…`），
    /// 邻居加分要是压过了备份扣分，加这一条反而把事情做坏了。
    #[test]
    fn being_next_to_the_bridge_does_not_rescue_a_backup() {
        let root = tmp("nb");
        let home = root.join("chat");
        make_bridge(&home.join("bridge-v2"));
        make_tavern(&home.join("SillyTavern.backup-20260605"));
        let live = root.join("elsewhere").join("SillyTavern");
        make_tavern(&live);

        let roots = Roots {
            starts: vec![root.clone()],
            ..Default::default()
        };
        let survey = locate(&roots, &Budget::quick());
        assert!(
            !survey.sillytavern[0].path.contains("backup"),
            "备份不该排第一，实得 {}",
            survey.sillytavern[0].path
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 预算用完要**说出来**。把「没找到」和「没找完」显示成同一句，
    /// 人会去重装一份他其实已经有的东西。
    #[test]
    fn running_out_of_budget_is_reported_not_hidden() {
        let root = tmp("budget");
        make_bridge(&root.join("a").join("b").join("c").join("d").join("bridge"));
        let roots = Roots {
            starts: vec![root.clone()],
            ..Default::default()
        };
        let survey = locate(
            &roots,
            &Budget {
                max_depth: 9,
                max_dirs: 2,
                seconds: 60,
            },
        );
        assert!(survey.truncated);
        assert!(survey.bridge.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `node_modules` 之类不进去。少一条，一个前端仓库就能吃掉几万个目录。
    #[test]
    fn heavy_directories_are_skipped() {
        assert!(skipped("node_modules"));
        assert!(skipped(".git"));
        assert!(skipped("Windows"));
        assert!(skipped(".vscode"));
        assert!(!skipped("chat"));
        assert!(!skipped("SillyTavern"));
    }
}
