//! Claude 桌面端 · 中文界面：**撤掉了**（2026-09-27，使用者定的），这里只剩「清理残留」。
//!
//! 0.25.4–0.25.5 接入过社区项目 [javaht/claude-desktop-zh-cn](https://github.com/javaht/claude-desktop-zh-cn)
//! （MIT）Windows 脚本的安全模式。2026-09-27 查清楚了：**官方账户下它必然不生效**。
//!
//! - 官方账户（claude.ai 登录）模式下，Claude 主窗口加载的是在线的 `https://claude.ai`（2.9939.2 的
//!   `app.asar`：官方模式的 `getMainWindowUrl()` 返回它；只有第三方 API 模式才是本地的 `app://localhost`）。
//!   安全模式只改本地那份前端（`ion-dist`），碰不到在线页；
//! - 在线页每次加载都调 `window.electronIntl.requestLocaleChange(它自己的语言)`，而 claude.ai 没有中文，
//!   推回来的是 en-US —— 主进程随即切回英文、把 `config.json` 的 `locale` 改写掉，连菜单都保不住；
//! - 能把语言锁住的只有上游另外两种模式（改 `app.asar` 并重写 `Claude.exe` 的完整性哈希 / Frida 内存补丁），
//!   都是绕过 Claude 的防篡改：Anthropic 消费者条款不许，使用者也定过「必须符合厂商政策」。
//!
//! 所以入口、插件条目、下载与应用全删了。⛔ **别把「应用」那条路加回来** —— 在官方账户下它只会让人
//! 看到「核验通过」而界面照旧是英文（坑 7.100）。
//!
//! # 清理不跑上游的卸载脚本
//!
//! 上游 `uninstall` 的第三步「移除 zh-CN 语言注册」是按字符串删 `"zh-CN"`，把 Claude **原版**文件里
//! 本来就有的那几处也删了：本机 09-25 卸载之后，有 3 个前端文件跟上游自己的备份对不上，而备份跟旧版
//! 2.9939.1 里同名（文件名带内容哈希）的文件逐字一致 —— 备份才是原版，现在的是被改坏的。
//!
//! 所以清理直接用上游自己留下的备份（`resources\.zh-cn-backups` 里最早那一套 —— 上游卸载的第一步用的
//! 也是它）把文件逐字换回去，再删中文文件与备份：不起 PowerShell、不联网，只写备份里列着的前端文件
//! （`ion-dist\` 下）。备份里出现 `app.asar`、`claude.exe` 之类，说明用过上游改程序文件的模式 ——
//! 那一版不自动还原，如实让使用者到官网重装。
use crate::error::{GateError, Result};
use crate::install::inventory;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 那版插件下载上游副本用的目录名（`state_dir\plugins\<它>`）。清理时整份删掉。
pub const PLUGIN_ID: &str = "claude-desktop-zh-cn";
/// 上游的三种中文。面板只用过 zh-CN，但上游卸载时三种都删 —— 清理也三种都认。
pub const LANGUAGES: [&str; 3] = ["zh-CN", "zh-TW", "zh-HK"];
/// 上游备份目录的名字（在 `app-<版本>\resources\` 下）。
pub const BACKUP_DIR: &str = ".zh-cn-backups";

// ---------------------------------------------------------------- config.json 里的 locale

/// **纯函数**：JSON 对象最外层 `key` 的字符串值。不是对象 / 不是 JSON → 报错。
pub fn top_level_string(text: &str, key: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| GateError::Other(format!("config.json 不是有效 JSON：{e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| GateError::Other("config.json 不是一个 JSON 对象".into()))?;
    Ok(obj.get(key).and_then(|v| v.as_str()).map(str::to_string))
}

/// **纯函数**：把 JSON 对象最外层 `key` 的值换成字符串 `value`，**别的字节一个不动**。
///
/// Claude 的 `config.json` 里装着它自己的登录令牌缓存（`oauth:tokenCache`）。整份解析再序列化
/// 会重排键、改缩进，还等于把令牌读出来又写回去 —— 这里只改那一个值的字节，令牌那几行原样不碰。
/// 没有这个键就插在最前面，照第一项的缩进写。先整份解析一遍：拿不准的文件不写。
pub fn set_top_level_string(text: &str, key: &str, value: &str) -> Result<String> {
    top_level_string(text, key)?; // 校验：是 JSON 对象
    let b = text.as_bytes();
    let json = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let mut i = skip_ws(b, 0);
    if b.get(i) != Some(&b'{') {
        return Err(GateError::Other("config.json 不是一个 JSON 对象".into()));
    }
    let open = i;
    i += 1;
    loop {
        i = skip_ws(b, i);
        match b.get(i) {
            Some(b'}') => break,
            Some(b'"') => {}
            _ => return Err(GateError::Other("config.json 的结构读不懂".into())),
        }
        let key_end = string_end(b, i)?;
        let found: String = serde_json::from_str(&text[i..key_end])
            .map_err(|_| GateError::Other("config.json 的键读不懂".into()))?;
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return Err(GateError::Other("config.json 的结构读不懂".into()));
        }
        let value_start = skip_ws(b, i + 1);
        let value_end = value_end(b, value_start)?;
        if found == key {
            return Ok(format!(
                "{}{}{}",
                &text[..value_start],
                json(value),
                &text[value_end..]
            ));
        }
        i = skip_ws(b, value_end);
        match b.get(i) {
            Some(b',') => i += 1,
            Some(b'}') => break,
            _ => return Err(GateError::Other("config.json 的结构读不懂".into())),
        }
    }
    // 没有这个键：插在 `{` 后面，照第一项前面那段空白（`\n\t` 之类）写。
    let first = skip_ws(b, open + 1);
    let indent = &text[open + 1..first];
    if b.get(first) == Some(&b'}') {
        return Ok(format!(
            "{}{}{}: {}{}{}",
            &text[..=open],
            indent,
            json(key),
            json(value),
            indent,
            &text[first..]
        ));
    }
    Ok(format!(
        "{}{}{}: {},{}",
        &text[..=open],
        indent,
        json(key),
        json(value),
        &text[open + 1..]
    ))
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

/// `b[i]` 是一个字符串的开头引号，返回结尾引号**之后**的位置。
fn string_end(b: &[u8], i: usize) -> Result<usize> {
    let mut j = i + 1;
    while let Some(&c) = b.get(j) {
        match c {
            b'\\' => j += 2,
            b'"' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err(GateError::Other("config.json 里有没闭合的字符串".into()))
}

/// 从 `i` 开始的一个 JSON 值的结尾（之后的位置）。输入已经整份解析过，这里只管走到头。
fn value_end(b: &[u8], i: usize) -> Result<usize> {
    match b.get(i) {
        Some(b'"') => string_end(b, i),
        Some(b'{') | Some(b'[') => {
            let mut depth = 0usize;
            let mut j = i;
            while let Some(&c) = b.get(j) {
                match c {
                    b'"' => {
                        j = string_end(b, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            Err(GateError::Other("config.json 里有没闭合的对象".into()))
        }
        Some(_) => {
            let mut j = i;
            while b
                .get(j)
                .is_some_and(|c| !c.is_ascii_whitespace() && !b",}]".contains(c))
            {
                j += 1;
            }
            Ok(j)
        }
        None => Err(GateError::Other("config.json 读到一半就没了".into())),
    }
}

// ---------------------------------------------------------------- 那版插件留下的记录

/// 0.25.4–0.25.5 那版插件自己记的东西。**只读**（清理完整份删掉）：「汉化过没有、各份资料原来是什么语言」
/// 以它为准，别的一概看文件。旧文件里多出来的字段（上游版本、查新版的时间……）读的时候丢掉。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// 使用者点过「一键汉化」、没点「恢复英文」。
    pub desired: bool,
    /// 上一次成功应用的上游版本。
    pub applied_tag: Option<String>,
    /// 应用之前各份 Claude 资料的 `locale`。键是真实目录（联结点解开，见 [`real_dir`]）。
    pub previous_locales: BTreeMap<String, Option<String>>,
}

pub fn state_path() -> PathBuf {
    crate::paths::state_dir()
        .join("claude-zh")
        .join("state.json")
}

pub fn load_state() -> State {
    std::fs::read(state_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// 那版插件下载的上游副本放在哪。
pub fn packages_root() -> PathBuf {
    crate::paths::state_dir().join("plugins").join(PLUGIN_ID)
}

/// 删面板下载过的上游副本与那版插件的记录（都在面板自己的状态目录下）。返回删了哪几样。
pub fn remove_panel_copy() -> Result<Vec<String>> {
    let mut removed = Vec::new();
    let copy = packages_root();
    if copy.is_dir() {
        std::fs::remove_dir_all(&copy)
            .map_err(|e| GateError::Other(format!("删不掉 {}：{e}", copy.display())))?;
        removed.push(copy.display().to_string());
    }
    let state = state_path();
    if state.is_file() {
        std::fs::remove_file(&state)
            .map_err(|e| GateError::Other(format!("删不掉 {}：{e}", state.display())))?;
        removed.push(state.display().to_string());
        // 目录空了才删得掉；删不掉（里面还有别的）就留着。
        if let Some(parent) = state.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    Ok(removed)
}

// ---------------------------------------------------------------- 各份资料的界面语言

/// 实际的资料目录（联结点解开、去掉 `\\?\` 前缀）。[`State::previous_locales`] 的键就是它：
/// `%APPDATA%\Claude` 是指向当前账户的联结点，换了账户它就指向别处了。
pub fn real_dir(p: &Path) -> PathBuf {
    let real = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = real.display().to_string();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// Claude 桌面端的各份资料（每个账户槽位一份 `%APPDATA%\Claude-<名字>`，当前那份经联结点
/// `%APPDATA%\Claude` 指过去；`accounts::AccountRoots::desktop_profile_dirs` 按真实位置去重）。
/// 改名留底的 `Claude-backup-*` 不算 —— 那是备份，不是在用的资料。只要里面有 `config.json` 的。
pub fn profile_dirs() -> Vec<PathBuf> {
    crate::accounts::AccountRoots::current()
        .desktop_profile_dirs()
        .into_iter()
        .filter(|d| {
            !d.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("Claude-backup-"))
        })
        .filter(|d| d.join("config.json").is_file())
        .collect()
}

/// 一份资料的 `locale`。读不出来就是 `None`（**只读这一个键**，令牌缓存一概不看）。
pub fn profile_locale(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    top_level_string(&text, "locale").ok().flatten()
}

/// 把一份资料的 `locale` 设成 `value`。只改那一个值的字节。返回改没改。
pub fn set_profile_locale(dir: &Path, value: &str) -> Result<bool> {
    let path = dir.join("config.json");
    let before = crate::config_io::read_optional(&path)?
        .ok_or_else(|| GateError::Other(format!("{} 不在", path.display())))?;
    let text = String::from_utf8(before.clone())
        .map_err(|_| GateError::Other(format!("{} 不是 UTF-8 文本", path.display())))?;
    let next = set_top_level_string(&text, "locale", value)?;
    if next == text {
        return Ok(false);
    }
    crate::config_io::commit(vec![crate::config_io::Edit {
        path,
        expected: Some(before),
        body: Some(next.into_bytes()),
    }])?;
    Ok(true)
}

/// 这个界面语言是不是上游的三种中文之一。
pub fn is_zh(locale: &str) -> bool {
    LANGUAGES.iter().any(|l| l.eq_ignore_ascii_case(locale))
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhProfile {
    pub dir: String,
    /// 这份资料 `config.json` 里的 `locale`。读不出来是 `None`。
    pub locale: Option<String>,
}

// ---------------------------------------------------------------- Claude 安装目录那一侧

/// 上游安全模式往一个 `app-<版本>` 里放的中文文件（`resources\zh-*.json`、`ion-dist\i18n\zh-*.json`、
/// `ion-dist\i18n\statsig\zh-*.json`），只列现在还在的。
pub fn zh_files(app_dir: &Path) -> Vec<PathBuf> {
    let res = app_dir.join("resources");
    let i18n = res.join("ion-dist").join("i18n");
    LANGUAGES
        .iter()
        .flat_map(|l| {
            let f = format!("{l}.json");
            [res.join(&f), i18n.join(&f), i18n.join("statsig").join(&f)]
        })
        .filter(|p| p.is_file())
        .collect()
}

/// 上游的备份目录。
pub fn backup_root(app_dir: &Path) -> PathBuf {
    app_dir.join("resources").join(BACKUP_DIR)
}

/// 最早那一套备份。上游按时间戳（`20260925-063045`）给每一套命名，最早的是改之前的原版 ——
/// 上游卸载的第一步也是拿它还原。
fn oldest_set(root: &Path) -> Option<PathBuf> {
    let mut sets: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    sets.sort();
    sets.into_iter().next()
}

/// 一套备份里的每个文件，相对 `resources\` 的路径。遇到链接就不认 —— 面板不跟着链接去写别处。
fn backup_files(set: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![set.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let kind = e.file_type()?;
            if kind.is_symlink() {
                return Err(GateError::Other(format!(
                    "上游的备份里有链接（{}），面板不跟",
                    e.path().display()
                )));
            }
            if kind.is_dir() {
                stack.push(e.path());
            } else if kind.is_file() {
                let rel = e
                    .path()
                    .strip_prefix(set)
                    .map_err(|_| GateError::Other("上游的备份路径读不懂".into()))?
                    .to_path_buf();
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// **纯函数**：这一套备份能不能自动还原 —— 只收 `ion-dist\` 下的前端文件（上游安全模式只改这些）。
/// 出现 `app.asar`、`claude.exe` 或别处的文件，说明用过上游改程序文件的模式，面板不碰。空的一套也算能（没什么要还原）。
pub fn restorable(rels: &[PathBuf]) -> bool {
    rels.iter().all(|r| {
        let mut parts = r.components();
        matches!(parts.next(), Some(std::path::Component::Normal(first)) if first.eq_ignore_ascii_case("ion-dist"))
            && parts.all(|c| matches!(c, std::path::Component::Normal(_)))
            && r.components().count() > 1
    })
}

/// 按大小先看一遍（不读内容）：备份里的原版跟现在的文件大小对不上、或者现在不在的，有几个。
/// 只给界面预先说「要不要关 Claude」用；真换之前逐字节再核（[`differing`]）。
fn quick_changed(resources: &Path, set: &Path, rels: &[PathBuf]) -> u32 {
    rels.iter()
        .filter(|r| {
            match (
                std::fs::metadata(set.join(r)),
                std::fs::metadata(resources.join(r)),
            ) {
                (Ok(b), Ok(c)) => b.len() != c.len(),
                _ => true,
            }
        })
        .count() as u32
}

/// 逐字节比：备份里跟现在不一样（或现在不在）的那些。
fn differing(resources: &Path, set: &Path, rels: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for r in rels {
        let original = std::fs::read(set.join(r))?;
        match std::fs::read(resources.join(r)) {
            Ok(now) if now == original => {}
            Ok(_) => out.push(r.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => out.push(r.clone()),
            Err(e) => {
                return Err(GateError::Other(format!(
                    "读不了 {}：{e}",
                    resources.join(r).display()
                )))
            }
        }
    }
    Ok(out)
}

/// 目录里所有文件加起来多大（字节）。读不了的不算。
fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

/// **纯函数**：一个 `app-*` 算不算汉化留下的。有上游的备份就算（那个目录名只有上游会建）；
/// 只有中文文件、没有备份时，要面板自己记过「汉化了」才算 —— 以后 Claude 自己带了中文也长这样，
/// 别把它当残留删了。
pub fn counts_as_leftover(has_backup: bool, zh_files: u32, applied: bool) -> bool {
    has_backup || (zh_files > 0 && applied)
}

/// 清一个 `app-*` 的结果：给界面的几句话，与是不是全清干净了。
#[derive(Debug, Clone, Default)]
pub struct AppClean {
    pub lines: Vec<String>,
    pub ok: bool,
}

/// 清一个 `app-<版本>`：把上游改过的前端文件逐字换回备份里的原版 → 删上游放的中文文件 → 删上游的备份。
///
/// `may_write`：能不能往这一版里写（Claude **现在用的**那一版要它关着，编排层先关好再传 `true`）。
/// 任何一步不对就停在那一步，**备份留着**（它是唯一的原版）。
pub fn clean_app(app_dir: &Path, version: &str, may_write: bool) -> AppClean {
    let mut out = AppClean::default();
    let resources = app_dir.join("resources");
    let root = backup_root(app_dir);
    let zh = zh_files(app_dir);
    if !root.is_dir() {
        if !zh.is_empty() {
            out.lines.push(format!(
                "Claude {version}：还有 {} 个上游放的中文文件，但上游的备份不在了，它改过的前端文件还不回原样 —— \
                 到 Claude 官网重新下载安装包覆盖安装（会换回全部原版文件）。",
                zh.len()
            ));
            return out;
        }
        out.ok = true;
        return out;
    }
    if let Some(set) = oldest_set(&root) {
        let rels = match backup_files(&set) {
            Ok(r) => r,
            Err(e) => {
                out.lines.push(format!("Claude {version}：{e}"));
                return out;
            }
        };
        if !restorable(&rels) {
            out.lines.push(format!(
                "Claude {version}：上游的备份里有程序文件（app.asar / claude.exe 之类），说明用过上游改程序文件的模式 —— \
                 面板不碰，到 Claude 官网重新下载安装包覆盖安装最稳。"
            ));
            return out;
        }
        let diff = match differing(&resources, &set, &rels) {
            Ok(d) => d,
            Err(e) => {
                out.lines.push(format!("Claude {version}：{e}"));
                return out;
            }
        };
        if !diff.is_empty() {
            if !may_write {
                out.lines.push(format!(
                    "Claude {version}：有 {} 个前端文件跟原版不一样，而 Claude 开着，这次没换。关掉 Claude 桌面端再点一次清理。",
                    diff.len()
                ));
                return out;
            }
            for r in &diff {
                let written = std::fs::read(set.join(r))
                    .map_err(GateError::from)
                    .and_then(|body| crate::config_io::replace(&resources.join(r), Some(&body)));
                if let Err(e) = written {
                    out.lines.push(format!(
                        "Claude {version}：换不回 {}（{e}）。备份留着，到 Claude 官网重新下载安装包覆盖安装最稳。",
                        r.display()
                    ));
                    return out;
                }
            }
            match differing(&resources, &set, &diff) {
                Ok(left) if left.is_empty() => out.lines.push(format!(
                    "Claude {version}：{} 个前端文件换回了原版（上游改过的，也包括它卸载时删多了的）。",
                    diff.len()
                )),
                Ok(left) => {
                    out.lines.push(format!(
                        "Claude {version}：写回之后仍有 {} 个文件跟原版对不上，备份留着。到 Claude 官网重新下载安装包覆盖安装最稳。",
                        left.len()
                    ));
                    return out;
                }
                Err(e) => {
                    out.lines.push(format!("Claude {version}：{e}"));
                    return out;
                }
            }
        }
    }
    if !zh.is_empty() {
        if !may_write {
            out.lines.push(format!(
                "Claude {version}：还有 {} 个上游放的中文文件，而 Claude 开着，这次没删。关掉 Claude 桌面端再点一次清理。",
                zh.len()
            ));
            return out;
        }
        for f in &zh {
            if let Err(e) = std::fs::remove_file(f) {
                out.lines
                    .push(format!("Claude {version}：删不掉 {}（{e}）。", f.display()));
                return out;
            }
        }
        out.lines.push(format!(
            "Claude {version}：删掉了 {} 个上游放的中文文件。",
            zh.len()
        ));
    }
    let mb = dir_bytes(&root).div_ceil(1 << 20);
    match std::fs::remove_dir_all(&root) {
        Ok(()) => out.lines.push(format!(
            "Claude {version}：删掉了上游留下的备份（{mb} MB）。"
        )),
        Err(e) => {
            out.lines.push(format!(
                "Claude {version}：删不掉上游留下的备份 {}（{e}）。",
                root.display()
            ));
            return out;
        }
    }
    out.ok = true;
    out
}

// ---------------------------------------------------------------- 上游建的 Claude-3p

/// **纯函数**：`Claude-3p` 是不是上游建的那种 —— 目录里只有一个 `config.json`，它只写了一个字符串 `locale`。
/// 使用者自己用过第三方 API 模式的话，那里是一整份资料，一样都不碰。
pub fn is_upstream_3p(entries: &[String], config: Option<&str>) -> bool {
    entries.len() == 1
        && entries[0].eq_ignore_ascii_case("config.json")
        && config
            .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
            .and_then(|v| {
                v.as_object()
                    .map(|o| o.len() == 1 && o.get("locale").is_some_and(|l| l.is_string()))
            })
            .unwrap_or(false)
}

/// 上游建的 `%APPDATA%\Claude-3p`（形状见 [`is_upstream_3p`]）。不是那个形状就是 `None`。
fn stray_3p_dir() -> Option<PathBuf> {
    let dir = crate::accounts::AccountRoots::current()
        .appdata?
        .join("Claude-3p");
    let entries: Vec<String> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let config = std::fs::read_to_string(dir.join("config.json")).ok();
    is_upstream_3p(&entries, config.as_deref()).then_some(dir)
}

/// 删上游建的 `Claude-3p`：删之前再核一次形状，只删那一个文件、再删**空**目录 ——
/// 这期间多出任何东西，目录就删不掉，那就不删。
pub fn remove_stray_3p() -> Result<Option<PathBuf>> {
    let Some(dir) = stray_3p_dir() else {
        return Ok(None);
    };
    std::fs::remove_file(dir.join("config.json"))
        .and_then(|_| std::fs::remove_dir(&dir))
        .map_err(|e| GateError::Other(format!("删不掉 {}：{e}", dir.display())))?;
    Ok(Some(dir))
}

// ---------------------------------------------------------------- 给界面的「还剩什么」

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhApp {
    pub version: String,
    pub dir: String,
    /// 这一版是不是 Claude 现在用的那一版（往它里面写要 Claude 关着）。
    pub current: bool,
    /// 上游放的中文文件还在几个。
    pub zh_files: u32,
    /// 上游的备份（`resources\.zh-cn-backups`）多大（MB，向上取整）。没有备份是 `None`。
    pub backup_mb: Option<u32>,
    /// 备份里的原版跟现在的文件大小对不上（或现在不在）的有几个 —— 清理时逐字节再核、换回原版。
    pub changed: u32,
    /// 能不能自动还原：备份里只有前端文件（`ion-dist\`）才行。
    pub restorable: bool,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct ClaudeZhLeftovers {
    /// 一样都没剩。账户页那颗「清理汉化残留」只在它是 `false` 时出现。
    pub empty: bool,
    pub apps: Vec<ClaudeZhApp>,
    /// 界面语言还是中文、而且是汉化留下的资料（写回汉化之前的值，没记就写 en-US）。
    pub zh_profiles: Vec<ClaudeZhProfile>,
    /// 上游建的 `%APPDATA%\Claude-3p`：里面只有一个只写了 `locale` 的 `config.json`。
    pub stray_3p: Option<String>,
    /// 那版插件下载过的上游副本（目录）。
    pub panel_copy: Option<String>,
    /// 那版插件的记录文件。
    pub state_file: Option<String>,
    /// 清理要先关 Claude 桌面端：要往**现在用的那一版**里换回原版文件 / 删中文文件时才要。
    pub needs_close: bool,
    /// Claude 桌面端开着几个进程（编排层填；`None` = 查不到，**不是「没有」**）。
    pub desktop_running: Option<u32>,
}

/// 还剩什么。**只读**：Claude 各个 `app-*` 目录、各份资料的界面语言、`Claude-3p`、面板自己的目录。
/// 不联网、不起进程（桌面端开没开由编排层另查后填进去）。
pub fn leftovers() -> ClaudeZhLeftovers {
    let state = load_state();
    let state_file = state_path()
        .is_file()
        .then(|| state_path().display().to_string());
    let panel_copy = packages_root()
        .is_dir()
        .then(|| packages_root().display().to_string());
    let applied = state.desired || state.applied_tag.is_some();

    let mut apps = Vec::new();
    if let Some(local) = dirs::data_local_dir() {
        for (i, (version, dir)) in inventory::desktop_app_dirs(&local).into_iter().enumerate() {
            let zh = zh_files(&dir).len() as u32;
            let root = backup_root(&dir);
            let has_backup = root.is_dir();
            if !counts_as_leftover(has_backup, zh, applied) {
                continue;
            }
            let (backup_mb, changed, can_restore) = if has_backup {
                let mb = u32::try_from(dir_bytes(&root).div_ceil(1 << 20)).unwrap_or(u32::MAX);
                match oldest_set(&root) {
                    None => (Some(mb), 0, true),
                    Some(set) => match backup_files(&set) {
                        Ok(rels) => (
                            Some(mb),
                            quick_changed(&dir.join("resources"), &set, &rels),
                            restorable(&rels),
                        ),
                        Err(_) => (Some(mb), 0, false),
                    },
                }
            } else {
                (None, 0, false)
            };
            apps.push(ClaudeZhApp {
                version,
                dir: dir.display().to_string(),
                current: i == 0,
                zh_files: zh,
                backup_mb,
                changed,
                restorable: can_restore,
            });
        }
    }
    let patched = !apps.is_empty();

    let zh_profiles: Vec<ClaudeZhProfile> = profile_dirs()
        .into_iter()
        .filter_map(|d| {
            let locale = profile_locale(&d)?;
            if !is_zh(&locale) {
                return None;
            }
            let recorded = state
                .previous_locales
                .contains_key(&real_dir(&d).display().to_string());
            (patched || applied || recorded).then(|| ClaudeZhProfile {
                dir: d.display().to_string(),
                locale: Some(locale),
            })
        })
        .collect();

    // `Claude-3p` 只在确实汉化过（有残留、有记录、有面板副本）时才看 —— 单凭形状不删别人的目录。
    let stray_3p = (patched || state_file.is_some() || panel_copy.is_some())
        .then(stray_3p_dir)
        .flatten()
        .map(|d| d.display().to_string());
    let needs_close = apps
        .iter()
        .any(|a| a.current && (a.zh_files > 0 || a.changed > 0));
    let empty = apps.is_empty()
        && zh_profiles.is_empty()
        && stray_3p.is_none()
        && panel_copy.is_none()
        && state_file.is_none();
    ClaudeZhLeftovers {
        empty,
        apps,
        zh_profiles,
        stray_3p,
        panel_copy,
        state_file,
        needs_close,
        desktop_running: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude 的 config.json 是 tab 缩进、里面有令牌缓存。只改 locale 那一个值的字节。
    const CONFIG: &str = "{\n\t\"updaterLastSeenVersion\": \"2.9939.2\",\n\t\"locale\": \"en-US\",\n\t\"oauth:tokenCache\": \"v10abc\\\"def{}[]\",\n\t\"nested\": {\n\t\t\"locale\": \"fr-FR\",\n\t\t\"list\": [1, \"}\", {\"a\": null}]\n\t},\n\t\"flag\": true\n}";

    #[test]
    fn only_the_top_level_locale_bytes_change() {
        assert_eq!(
            top_level_string(CONFIG, "locale").unwrap().as_deref(),
            Some("en-US")
        );
        let out = set_top_level_string(CONFIG, "locale", "zh-CN").unwrap();
        assert_eq!(
            out,
            CONFIG.replacen("\"locale\": \"en-US\"", "\"locale\": \"zh-CN\"", 1)
        );
        assert_eq!(
            top_level_string(&out, "locale").unwrap().as_deref(),
            Some("zh-CN")
        );
        // 嵌套对象里同名的键不动。
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["nested"]["locale"], "fr-FR");
        assert_eq!(v["oauth:tokenCache"], "v10abc\"def{}[]");
    }

    #[test]
    fn a_missing_locale_is_inserted_with_the_file_s_own_indentation() {
        let text = "{\n\t\"flag\": true,\n\t\"nested\": {\"locale\": \"x\"}\n}";
        let out = set_top_level_string(text, "locale", "zh-CN").unwrap();
        assert_eq!(
            out,
            "{\n\t\"locale\": \"zh-CN\",\n\t\"flag\": true,\n\t\"nested\": {\"locale\": \"x\"}\n}"
        );
        let empty = set_top_level_string("{}", "locale", "zh-CN").unwrap();
        assert_eq!(
            top_level_string(&empty, "locale").unwrap().as_deref(),
            Some("zh-CN")
        );
    }

    #[test]
    fn a_file_we_cannot_read_is_left_alone() {
        for bad in ["", "[1,2]", "{\"a\":", "\u{feff}{\"locale\":\"en-US\"}"] {
            assert!(
                set_top_level_string(bad, "locale", "zh-CN").is_err(),
                "{bad:?}"
            );
        }
    }

    /// 测试用的假 `app-<版本>`，建在系统临时目录里（不碰真实的 Claude 与面板状态）。
    struct FakeApp {
        dir: PathBuf,
    }

    impl FakeApp {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("qb-claude-zh-{tag}-{}", crate::config_io::id()));
            std::fs::create_dir_all(dir.join("resources")).unwrap();
            Self { dir }
        }
        fn put(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
        fn read(&self, rel: &str) -> Option<String> {
            std::fs::read_to_string(self.dir.join(rel)).ok()
        }
    }

    impl Drop for FakeApp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    const JS: &str = r"resources\ion-dist\assets\v1\c8ff922bb-BXg55ahQ.js";
    const JS_BACKUP: &str =
        r"resources\.zh-cn-backups\20260925-063045\ion-dist\assets\v1\c8ff922bb-BXg55ahQ.js";

    #[test]
    fn the_three_languages_are_found_in_all_three_places() {
        let app = FakeApp::new("files");
        assert!(zh_files(&app.dir).is_empty());
        app.put(r"resources\zh-CN.json", "{}");
        app.put(r"resources\ion-dist\i18n\zh-TW.json", "{}");
        app.put(r"resources\ion-dist\i18n\statsig\zh-HK.json", "{}");
        // 官方的语言文件不算。
        app.put(r"resources\ion-dist\i18n\ja-JP.json", "{}");
        assert_eq!(zh_files(&app.dir).len(), 3);
    }

    #[test]
    fn only_frontend_backups_are_restored_automatically() {
        let p = |s: &str| PathBuf::from(s);
        assert!(restorable(&[
            p(r"ion-dist\assets\v1\a.js"),
            p(r"ion-dist\i18n\en-US.json")
        ]));
        assert!(restorable(&[]), "空的一套：没什么要还原");
        // 上游改程序文件的模式留下的备份：面板不碰。
        assert!(!restorable(&[p(r"ion-dist\assets\v1\a.js"), p("app.asar")]));
        assert!(!restorable(&[p(r"..\claude.exe")]));
        assert!(!restorable(&[p("ion-dist")]), "只有目录名、没有文件");
    }

    #[test]
    fn a_leftover_needs_the_upstream_backup_or_our_own_record() {
        assert!(counts_as_leftover(true, 0, false), "有备份就算");
        assert!(counts_as_leftover(false, 3, true), "面板记过汉化了");
        // 没有备份也没有记录：那几个中文文件不是这里来的（将来 Claude 自带中文也长这样）。
        assert!(!counts_as_leftover(false, 3, false));
        assert!(!counts_as_leftover(false, 0, true));
    }

    #[test]
    fn an_upstream_3p_folder_is_only_a_locale_config() {
        let one = ["config.json".to_string()];
        assert!(is_upstream_3p(
            &one,
            Some("{\n    \"locale\":  \"en-US\"\n}")
        ));
        // 使用者自己用过第三方 API 模式：一整份资料，不碰。
        let many = ["config.json".to_string(), "Local Storage".to_string()];
        assert!(!is_upstream_3p(&many, Some("{\"locale\":\"en-US\"}")));
        assert!(!is_upstream_3p(
            &one,
            Some("{\"locale\":\"en-US\",\"oauth:tokenCache\":\"x\"}")
        ));
        assert!(!is_upstream_3p(&one, Some("{\"locale\":1}")));
        assert!(!is_upstream_3p(&one, Some("not json")));
        assert!(!is_upstream_3p(&one, None));
        assert!(!is_upstream_3p(&[], None));
    }

    /// 本机 09-25 的形状：上游卸载删多了一处，现在的文件比备份里的原版短；中文文件已经删了。
    #[test]
    fn cleaning_puts_the_original_back_and_drops_the_backup() {
        let app = FakeApp::new("clean");
        app.put(JS, "[\"en-US\",\"id-ID\"]");
        app.put(JS_BACKUP, "[\"en-US\",\"id-ID\",\"zh-CN\"]");
        app.put(
            r"resources\.zh-cn-backups\20260925-063045\ion-dist\assets\v1\same.js",
            "same",
        );
        app.put(r"resources\ion-dist\assets\v1\same.js", "same");
        // 更晚的一套不是原版，不拿它还原。
        app.put(
            r"resources\.zh-cn-backups\20260925-070000\ion-dist\assets\v1\c8ff922bb-BXg55ahQ.js",
            "patched",
        );
        let done = clean_app(&app.dir, "2.9939.2", true);
        assert!(done.ok, "{:?}", done.lines);
        assert_eq!(
            app.read(JS).as_deref(),
            Some("[\"en-US\",\"id-ID\",\"zh-CN\"]")
        );
        assert_eq!(
            app.read(r"resources\ion-dist\assets\v1\same.js").as_deref(),
            Some("same")
        );
        assert!(!backup_root(&app.dir).exists());
        assert!(done
            .lines
            .iter()
            .any(|l| l.contains("1 个前端文件换回了原版")));
    }

    #[test]
    fn cleaning_removes_the_chinese_files_after_restoring() {
        let app = FakeApp::new("zh");
        app.put(JS, "patched");
        app.put(JS_BACKUP, "original");
        let zh = app.put(r"resources\zh-CN.json", "{}");
        let done = clean_app(&app.dir, "2.9939.2", true);
        assert!(done.ok, "{:?}", done.lines);
        assert_eq!(app.read(JS).as_deref(), Some("original"));
        assert!(!zh.exists());
        assert!(!backup_root(&app.dir).exists());
    }

    /// 现在用的那一版、Claude 开着：一个字节都不写，备份留着。
    #[test]
    fn a_running_current_version_is_not_written() {
        let app = FakeApp::new("running");
        app.put(JS, "patched");
        app.put(JS_BACKUP, "original");
        let zh = app.put(r"resources\zh-CN.json", "{}");
        let done = clean_app(&app.dir, "2.9939.2", false);
        assert!(!done.ok);
        assert_eq!(app.read(JS).as_deref(), Some("patched"));
        assert!(zh.exists());
        assert!(backup_root(&app.dir).exists());
        assert!(done.lines[0].contains("关掉 Claude 桌面端再点一次清理"));
    }

    /// 什么都没改过（或者早就还原好了）：只删备份，Claude 开着也行。
    #[test]
    fn an_untouched_version_only_loses_the_backup() {
        let app = FakeApp::new("untouched");
        app.put(JS, "original");
        app.put(JS_BACKUP, "original");
        let done = clean_app(&app.dir, "2.9939.2", false);
        assert!(done.ok, "{:?}", done.lines);
        assert_eq!(app.read(JS).as_deref(), Some("original"));
        assert!(!backup_root(&app.dir).exists());
    }

    #[test]
    fn a_backup_with_program_files_is_left_for_a_reinstall() {
        let app = FakeApp::new("official");
        app.put(JS, "patched");
        app.put(JS_BACKUP, "original");
        app.put(r"resources\.zh-cn-backups\20260925-063045\app.asar", "asar");
        let done = clean_app(&app.dir, "2.9939.2", true);
        assert!(!done.ok);
        assert_eq!(app.read(JS).as_deref(), Some("patched"));
        assert!(backup_root(&app.dir).exists());
        assert!(done.lines[0].contains("官网重新下载安装包"));
    }

    #[test]
    fn chinese_files_without_a_backup_are_left_for_a_reinstall() {
        let app = FakeApp::new("nobackup");
        let zh = app.put(r"resources\zh-CN.json", "{}");
        let done = clean_app(&app.dir, "2.9939.2", true);
        assert!(!done.ok);
        assert!(zh.exists(), "拿不回原版的时候，只删一半只会更乱");
    }
}
