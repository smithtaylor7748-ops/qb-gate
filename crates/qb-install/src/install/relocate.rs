//! 把官方软件单独搬到别的目录（2026-11-05）。按软件分做法：
//!
//! | 软件 | 做法 | 在哪 |
//! |---|---|---|
//! | Claude Code / Codex CLI | 整份搬（或「收编」：托管安装进新目录） | `managed::move_app_dir` + 命令层 |
//! | SillyTavern（面板装的） | 整份搬 + 改酒馆配置里的路径 | `qb-app::usecase::relocate_ops` |
//! | Gemini CLI | `npm install --prefix <新目录>` | 同上 |
//! | 反重力 Hub / IDE | 官方安装器带目录参数重装 | [`installer_dir_arg`] + `antigravity_setup::install_to` |
//! | Claude 桌面端 | 整份复制 → 原位置换成目录联结点 | 本文件 [`relocate_desktop`] / [`restore_desktop`] |
//! | Codex 桌面端（MSIX） | 面板不搬，打开 Windows 设置 | 命令层 |
//!
//! ⛔ 桌面端：`app-*\claude.exe` 永远不加 Deny ACE（加了开新窗口就崩）。搬走之后真实目录仍叫
//! `AnthropicClaude`，所以按 `\anthropicclaude\app-` 认运行时副本的那几道闸照样成立；
//! 按前缀认桌面端进程的那条（`inventory::is_desktop_process_path`）同时认原位置与解析后的真实位置。

use crate::error::{GateError, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 桌面端目录名。搬到别处时目标目录也叫这个名字（见文件头）。
pub const DESKTOP_DIR: &str = "AnthropicClaude";
/// 搬的过程中原目录临时改成这个名字，核对完联结点再删。
pub const MOVING_SUFFIX: &str = ".qbgate-moving";

/// 目录树里的文件数与总字节数（不跟联结点 / 符号链接）。
pub fn tree_stats(dir: &Path) -> (u64, u64) {
    let (mut n, mut size) = (0u64, 0u64);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                n += 1;
                size += m.len();
            }
        }
    }
    (n, size)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let ft = e.file_type()?;
        let to = dst.join(e.file_name());
        if ft.is_symlink() {
            return Err(GateError::Other(format!(
                "{} 是链接，面板不复制链接 —— 没搬",
                e.path().display()
            )));
        }
        if ft.is_dir() {
            copy_tree(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

/// 桌面端目录里要逐个比哈希的关键 exe（相对路径）：存根、`Update.exe`、最新那版 `app-*\claude.exe`。
pub fn desktop_key_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("claude.exe"), PathBuf::from("Update.exe")];
    let mut apps: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("app-"))
        .collect();
    apps.sort_by_key(|n| std::cmp::Reverse(crate::install::inventory::version_key(&n[4..])));
    if let Some(a) = apps.first() {
        out.push(Path::new(a).join("claude.exe"));
    }
    out.retain(|p| dir.join(p).is_file());
    out
}

/// 这个路径是目录联结点（或目录符号链接）吗。
pub fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// 建目录联结点 `link` → `target`（不要管理员权限）。路径走环境变量交给 PowerShell，不拼进脚本。
#[cfg(windows)]
pub fn make_junction(link: &Path, target: &Path) -> Result<()> {
    let o = crate::process::powershell_std(
        "$ErrorActionPreference = 'Stop'; \
         New-Item -ItemType Junction -Path $env:QB_LINK -Target $env:QB_TARGET | Out-Null",
    )
    .env("QB_LINK", link)
    .env("QB_TARGET", target)
    .output()
    .map_err(|e| GateError::Other(format!("启动 PowerShell 建联结点失败：{e}")))?;
    if !o.status.success() || !is_link(link) {
        return Err(GateError::Other(format!(
            "建不了联结点 {} → {}：{}",
            link.display(),
            target.display(),
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn make_junction(link: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

/// 拆掉联结点本身（不碰它指向的东西）。
pub fn remove_link(link: &Path) -> Result<()> {
    if !is_link(link) {
        return Err(GateError::Other(format!("{} 不是联结点", link.display())));
    }
    #[cfg(windows)]
    {
        std::fs::remove_dir(link)?;
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(link)?;
    }
    Ok(())
}

/// 去掉 `\\?\` 的规范路径。
fn canonical(p: &Path) -> Option<PathBuf> {
    let s = std::fs::canonicalize(p).ok()?.to_string_lossy().to_string();
    Some(PathBuf::from(match s.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s),
    }))
}

/// 桌面端现在在哪。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "DesktopPlace")]
pub struct DesktopPlace {
    /// `%LOCALAPPDATA%\AnthropicClaude`。
    pub link: PathBuf,
    /// 实际文件所在（没搬过就跟 `link` 一样）。
    pub real: PathBuf,
    pub installed: bool,
    /// 原位置是面板建的联结点。
    pub relocated: bool,
    /// 联结点还在、指向的地方已经没东西了（卸载之后留下的）。
    pub dangling: bool,
}

pub fn desktop_place(local: &Path) -> DesktopPlace {
    let link = local.join(DESKTOP_DIR);
    let relocated = is_link(&link);
    let real = canonical(&link).unwrap_or_else(|| link.clone());
    let installed = link.join("claude.exe").is_file();
    let dangling = relocated
        && (!real.exists()
            || std::fs::read_dir(&link)
                .map(|mut r| r.next().is_none())
                .unwrap_or(true));
    DesktopPlace {
        link,
        real,
        installed,
        relocated,
        dangling,
    }
}

/// 卸载后留下的空联结点收掉（连同它指向的那个空目录）。没有就什么都不做。返回做了什么。
pub fn sweep_dangling_desktop(local: &Path) -> Option<String> {
    let place = desktop_place(local);
    if !place.dangling {
        return None;
    }
    let target = std::fs::read_link(&place.link).ok();
    remove_link(&place.link).ok()?;
    if let Some(t) = target {
        let _ = std::fs::remove_dir(&t);
    }
    crate::install::inventory::refresh_desktop_real_dir();
    Some(format!(
        "收掉了桌面端卸载后留下的空联结点 {}",
        place.link.display()
    ))
}

/// 把桌面端整份搬到 `dest_parent\AnthropicClaude`，原位置换成指向它的目录联结点。
///
/// **调用方先确认桌面端完全退出。** 步骤：复制 → 核对文件数、总大小、关键 exe 的哈希 →
/// 原目录改名 `AnthropicClaude.qbgate-moving` → 原位置建联结点 → 经联结点核对 → 删改名的那份。
/// 任何一步不成都恢复原样（删掉复制出去的、改回原名）。纯文件操作，单测在临时目录里跑。
pub fn relocate_desktop(local: &Path, dest_parent: &Path) -> Result<Vec<String>> {
    let src = local.join(DESKTOP_DIR);
    if is_link(&src) {
        return Err(GateError::Other(
            "桌面端已经搬过了（原位置是联结点）。要换地方先「迁回」，再迁到新目录。".into(),
        ));
    }
    if !src.join("claude.exe").is_file() {
        return Err(GateError::Other(format!(
            "{} 下没有桌面端（claude.exe），没什么可搬的。",
            src.display()
        )));
    }
    let dest = dest_parent.join(DESKTOP_DIR);
    if crate::install::managed::is_inside(&dest, &src)
        || crate::install::managed::is_inside(&src, &dest)
    {
        return Err(GateError::Other(
            "目标不能在桌面端原目录里面，反过来也不行。".into(),
        ));
    }
    if dest.exists() {
        return Err(GateError::Other(format!(
            "{} 已经存在，面板不覆盖它 —— 换一个目录或先把它挪走。",
            dest.display()
        )));
    }
    let moving = local.join(format!("{DESKTOP_DIR}{MOVING_SUFFIX}"));
    if moving.exists() {
        return Err(GateError::Other(format!(
            "{} 还在（上一次没搬完？）。先看一眼里面是什么，删掉或改回原名再迁。",
            moving.display()
        )));
    }
    let mut notes = Vec::new();

    if let Err(e) = copy_tree(&src, &dest) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(GateError::Other(format!("复制没成，原处没动：{e}")));
    }
    let (a, b) = (tree_stats(&src), tree_stats(&dest));
    if a != b {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(GateError::Other(format!(
            "复制完核对不上（原处 {} 个文件 {} 字节，新处 {} 个 {} 字节），已删掉新处，原处没动。",
            a.0, a.1, b.0, b.1
        )));
    }
    for k in desktop_key_files(&src) {
        let (x, y) = (
            crate::install::managed::sha256_file(&src.join(&k)),
            crate::install::managed::sha256_file(&dest.join(&k)),
        );
        if x.is_none() || x != y {
            let _ = std::fs::remove_dir_all(&dest);
            return Err(GateError::Other(format!(
                "{} 复制前后哈希不一样，已删掉新处，原处没动。",
                k.display()
            )));
        }
    }
    notes.push(format!(
        "已复制到 {}（{} 个文件，{} MB，关键程序哈希一致）",
        dest.display(),
        a.0,
        a.1 >> 20
    ));

    if let Err(e) = std::fs::rename(&src, &moving) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(GateError::Other(format!(
            "原目录改不了名（多半桌面端还开着，或有文件被占着），已删掉新处，原处没动：{e}"
        )));
    }
    let undo = |why: String| -> GateError {
        if is_link(&src) {
            let _ = remove_link(&src);
        }
        match std::fs::rename(&moving, &src) {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&dest);
                GateError::Other(format!("{why}。已恢复原样。"))
            }
            Err(e) => GateError::Other(format!(
                "{why}。改回原名也失败了（{e}）：原目录在 {}，复制好的在 {}，两处都没删。",
                moving.display(),
                dest.display()
            )),
        }
    };
    if let Err(e) = make_junction(&src, &dest) {
        return Err(undo(e.to_string()));
    }
    let through = src.join("claude.exe");
    let same = canonical(&src)
        .zip(canonical(&dest))
        .is_some_and(|(x, y)| crate::install::inventory::same_path(&x, &y));
    if !same || !through.is_file() {
        return Err(undo("联结点建好了但经它找不到桌面端".into()));
    }
    notes.push(format!("{} 现在是指向新位置的联结点", src.display()));
    crate::install::inventory::refresh_desktop_real_dir();

    match std::fs::remove_dir_all(&moving) {
        Ok(()) => notes.push("原处那份已删掉".into()),
        Err(e) => notes.push(format!(
            "原处那份（{}）没删干净：{e}。桌面端已经在用新位置了，这份可以以后自己删。",
            moving.display()
        )),
    }
    Ok(notes)
}

/// 「迁回」：拆掉联结点，把真实目录搬回 `%LOCALAPPDATA%\AnthropicClaude`（同盘改名，跨盘复制核对再删）。
/// 失败时把联结点建回去。调用方先确认桌面端完全退出。
pub fn restore_desktop(local: &Path) -> Result<Vec<String>> {
    let link = local.join(DESKTOP_DIR);
    if !is_link(&link) {
        return Err(GateError::Other(
            "桌面端没被搬过（原位置不是联结点），不用迁回。".into(),
        ));
    }
    let Some(real) = canonical(&link).filter(|r| r.join("claude.exe").is_file()) else {
        // 指向的地方空了：卸载后留下的，收掉就是。
        return Ok(sweep_dangling_desktop(local).into_iter().collect());
    };
    remove_link(&link)?;
    if let Err(e) = crate::install::managed::move_tree(&real, &link) {
        let back = make_junction(&link, &real);
        crate::install::inventory::refresh_desktop_real_dir();
        return Err(GateError::Other(format!(
            "没搬回来：{e}。{}",
            match back {
                Ok(()) => "联结点已经建回去，桌面端照常能用。".to_string(),
                Err(b) => format!("联结点也没建回去（{b}）：桌面端现在在 {}", real.display()),
            }
        )));
    }
    crate::install::inventory::refresh_desktop_real_dir();
    Ok(vec![format!("桌面端已搬回 {}", link.display())])
}

// ------------------------------------------------------------- 反重力的安装器

/// 官方安装器是哪一种（核对过才带目录参数跑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerKind {
    Nsis,
    Inno,
}

/// 从安装器文件内容里认类型。纯函数：NSIS 的 exe 里有 `Nullsoft`，Inno 的有 `Inno Setup`。
pub fn installer_kind(bytes: &[u8]) -> Option<InstallerKind> {
    let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    if has(b"Inno Setup") {
        Some(InstallerKind::Inno)
    } else if has(b"Nullsoft") || has(b"NullsoftInst") {
        Some(InstallerKind::Nsis)
    } else {
        None
    }
}

/// 读安装器前 8 MB 认类型。
pub fn installer_kind_of(exe: &Path) -> Option<InstallerKind> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(exe)
        .ok()?
        .take(8 << 20)
        .read_to_end(&mut buf)
        .ok()?;
    installer_kind(&buf)
}

/// 目录参数。**原样**交给 `raw_arg`，不能再被套一层引号：
/// NSIS 的 `/D=` 必须是最后一个参数、不加引号（加了它认不出来）；Inno 是 `/DIR="<路径>"`。
pub fn installer_dir_arg(kind: InstallerKind, dir: &Path) -> Result<String> {
    let d = dir.display().to_string();
    if d.contains('"') || !dir.is_absolute() {
        return Err(GateError::Other(
            "目录要是一个完整路径，且不能带引号".into(),
        ));
    }
    Ok(match kind {
        InstallerKind::Nsis => format!("/D={d}"),
        InstallerKind::Inno => format!("/DIR=\"{d}\""),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "qbgate-relocate-{tag}-{}-{}",
                std::process::id(),
                chrono::Local::now().format("%H%M%S%f")
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let link = self.0.join("local").join(DESKTOP_DIR);
            if is_link(&link) {
                let _ = remove_link(&link);
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fake_desktop(local: &Path) {
        let d = local.join(DESKTOP_DIR);
        std::fs::create_dir_all(d.join("app-1.2.3").join("resources")).unwrap();
        std::fs::write(d.join("claude.exe"), b"stub").unwrap();
        std::fs::write(d.join("Update.exe"), b"update").unwrap();
        std::fs::write(d.join("app-1.2.3").join("claude.exe"), b"electron").unwrap();
        std::fs::write(
            d.join("app-1.2.3").join("resources").join("app.asar"),
            b"asar",
        )
        .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn desktop_moves_behind_a_junction_and_both_paths_count_as_desktop() {
        use crate::install::inventory::{is_desktop_process_path, is_desktop_runtime_copy};
        let t = Tmp::new("desk");
        let local = t.0.join("local");
        let dest = t.0.join("目标 盘");
        fake_desktop(&local);

        let notes = relocate_desktop(&local, &dest).unwrap();
        assert!(!notes.is_empty());
        let link = local.join(DESKTOP_DIR);
        assert!(is_link(&link), "原位置要换成联结点");
        assert!(!local.join(format!("{DESKTOP_DIR}{MOVING_SUFFIX}")).exists());
        assert_eq!(std::fs::read(link.join("claude.exe")).unwrap(), b"stub");
        let real_exe = dest.join(DESKTOP_DIR).join("app-1.2.3").join("claude.exe");
        assert!(real_exe.is_file());

        // 规范化之后的真实路径与原路径都认成桌面端；运行时副本那道闸在真实路径上照样成立。
        crate::install::inventory::refresh_desktop_real_dir();
        assert!(is_desktop_process_path(&real_exe, Some(&local)));
        assert!(is_desktop_process_path(
            &link.join("app-1.2.3").join("claude.exe"),
            Some(&local)
        ));
        assert!(is_desktop_runtime_copy(&real_exe));
        let place = desktop_place(&local);
        assert!(place.relocated && place.installed && !place.dangling);

        // 已经搬过的不能再搬。
        assert!(relocate_desktop(&local, &t.0.join("again")).is_err());

        // 迁回：联结点拆掉、文件回到原处。
        restore_desktop(&local).unwrap();
        assert!(!is_link(&link));
        assert_eq!(
            std::fs::read(link.join("app-1.2.3").join("claude.exe")).unwrap(),
            b"electron"
        );
        assert!(!dest.join(DESKTOP_DIR).exists());
        crate::install::inventory::refresh_desktop_real_dir();
    }

    #[cfg(windows)]
    #[test]
    fn a_dangling_junction_left_by_an_uninstall_is_swept() {
        let t = Tmp::new("dangle");
        let local = t.0.join("local");
        let target = t.0.join("elsewhere").join(DESKTOP_DIR);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        make_junction(&local.join(DESKTOP_DIR), &target).unwrap();
        assert!(
            desktop_place(&local).dangling,
            "指向空目录的联结点就是卸载后留下的"
        );
        assert!(sweep_dangling_desktop(&local).is_some());
        assert!(!local.join(DESKTOP_DIR).exists());
        assert!(!target.exists());
        assert!(sweep_dangling_desktop(&local).is_none());
    }

    #[test]
    fn nothing_is_touched_when_the_target_already_exists() {
        let t = Tmp::new("exists");
        let local = t.0.join("local");
        fake_desktop(&local);
        let dest = t.0.join("d");
        std::fs::create_dir_all(dest.join(DESKTOP_DIR)).unwrap();
        assert!(relocate_desktop(&local, &dest).is_err());
        assert!(local.join(DESKTOP_DIR).join("claude.exe").is_file());
        assert!(!is_link(&local.join(DESKTOP_DIR)));
    }

    #[test]
    fn installer_kind_and_dir_args() {
        assert_eq!(
            installer_kind(b"MZ....Nullsoft Install System v3"),
            Some(InstallerKind::Nsis)
        );
        assert_eq!(
            installer_kind(b"MZ...Inno Setup Setup Data (6.2.0)"),
            Some(InstallerKind::Inno)
        );
        assert_eq!(installer_kind(b"MZ plain"), None);
        let d = Path::new(r"D:\My Apps\antigravity");
        assert_eq!(
            installer_dir_arg(InstallerKind::Nsis, d).unwrap(),
            r"/D=D:\My Apps\antigravity"
        );
        assert_eq!(
            installer_dir_arg(InstallerKind::Inno, d).unwrap(),
            r#"/DIR="D:\My Apps\antigravity""#
        );
        assert!(installer_dir_arg(InstallerKind::Nsis, Path::new("rel")).is_err());
    }
}
