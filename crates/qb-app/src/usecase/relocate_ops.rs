//! 按软件迁移目录（2026-11-05）里**不碰门禁**的两种：SillyTavern（面板装的那份）与 Gemini CLI。
//!
//! Claude Code / Codex CLI / 反重力 / 桌面端要开维护窗口、关进程、重锁，在 `src-tauri` 的命令层编排；
//! 文件动作本身在 `qb-install`（`managed::move_app_dir`、`relocate`）。

use crate::error::{GateError, Result};
use std::path::{Path, PathBuf};

/// 把某个软件的目录记进设置。等于默认位置（`<托管根>\<键>`）时删掉这一条，不把本机路径写死。
pub fn store_app_dir(key: &str, dir: &Path) -> Result<()> {
    let mut s = crate::settings::load();
    let default = s
        .managed_apps_dir
        .clone()
        .unwrap_or_else(crate::settings::default_managed_dir)
        .join(key);
    if crate::install::inventory::same_path(dir, &default) {
        s.app_dirs.remove(key);
    } else {
        s.app_dirs.insert(key.to_string(), dir.to_path_buf());
    }
    crate::usecase::settings_ops::save(&s)
}

/// 新目录能不能给 `key` 这个软件用：实测（本机固定 NTFS 盘、锁得上）+ 不跟别的软件目录互相套住。
pub fn check_target(key: &str, new: &Path) -> crate::install::managed::Probe {
    use crate::install::managed;
    let mut p = managed::probe_dir(new);
    if p.ok {
        let others: Vec<(String, PathBuf)> = managed::SLOT_KEYS
            .iter()
            .map(|k| (k.to_string(), managed::slot_dir(k)))
            .collect();
        if let Some(why) = managed::nesting_conflict(key, new, &others) {
            p.ok = false;
            p.reason = Some(why);
        }
    }
    p
}

/// `p` 在 `old` 下面就换成 `new` 下面的同一个相对位置。纯函数。
pub fn rebase(p: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    if p.as_os_str().is_empty() || !crate::install::managed::is_inside(p, old) {
        return None;
    }
    let rel: PathBuf = p.components().skip(old.components().count()).collect();
    Some(if rel.as_os_str().is_empty() {
        new.to_path_buf()
    } else {
        new.join(rel)
    })
}

/// 酒馆配置里指着旧目录的路径（酒馆根、启动器、桥接）一起改到新目录。纯函数，返回改了几项。
pub fn rebase_tavern(
    cfg: &mut crate::plugins::sillytavern::TavernConfig,
    old: &Path,
    new: &Path,
) -> usize {
    let mut n = 0;
    for p in [
        &mut cfg.sillytavern_root,
        &mut cfg.st_launcher,
        &mut cfg.bridge_root,
    ] {
        if let Some(q) = rebase(p, old, new) {
            *p = q;
            n += 1;
        }
    }
    n
}

/// SillyTavern（面板装的那份）整份搬到 `new`。搬之前把 `data\default-user` 备份到 state_dir。
pub fn relocate_sillytavern(new: &Path) -> Result<Vec<String>> {
    use crate::install::{managed, sillytavern};
    let old = sillytavern::managed_dir();
    if crate::install::inventory::same_path(&old, new) {
        return Ok(vec!["已经在这个目录里了".into()]);
    }
    if !sillytavern::is_managed(&old) {
        return Err(GateError::Other(format!(
            "{} 下没有面板装的 SillyTavern。自己装的那份请直接挪文件夹，再在酒馆设置里重新选目录。",
            old.display()
        )));
    }
    if crate::plugins::sillytavern::tavern_port_busy() {
        return Err(GateError::Other(
            "酒馆正在运行（端口有人在听），先停掉它再迁移。什么都没动。".into(),
        ));
    }
    let probe = check_target(sillytavern::DIR, new);
    if !probe.ok {
        return Err(GateError::Other(probe.reason.unwrap_or_default()));
    }
    let mut notes = Vec::new();
    let user = old.join("data").join("default-user");
    if user.is_dir() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let bak = crate::paths::state_dir()
            .join("relocate-backups")
            .join(format!("sillytavern-default-user-{stamp}"));
        managed::copy_verified(&user, &bak)
            .map_err(|e| GateError::Other(format!("备份 data\\default-user 失败，没搬：{e}")))?;
        notes.push(format!("已把 data\\default-user 备份到 {}", bak.display()));
    }
    managed::move_app_dir(None, &old, new)?;
    if let Err(e) = store_app_dir(sillytavern::DIR, new) {
        return Err(GateError::Other(format!(
            "设置没写进去（{e}），{}",
            match managed::move_app_dir(None, new, &old) {
                Ok(()) => "已搬回原处".to_string(),
                Err(b) => format!("搬回也失败了（{b}），文件在 {}", new.display()),
            }
        )));
    }
    notes.push(format!("SillyTavern 已搬到 {}", new.display()));
    // 上一版留底跟着走，回滚才找得到。
    let (pa, pb) = (
        sillytavern::previous_of(&old),
        sillytavern::previous_of(new),
    );
    if pa.is_dir() && !pb.exists() && managed::move_tree(&pa, &pb).is_ok() {
        notes.push("上一版留底也一起搬了".into());
    }
    let mut cfg = crate::plugins::sillytavern::load_config_checked()?;
    if rebase_tavern(&mut cfg, &old, new) > 0 {
        crate::plugins::sillytavern::save_config(&cfg)?;
        notes.push("酒馆配置里的目录、启动器、桥接路径已改到新位置".into());
    }
    crate::audit::write(&format!(
        "SillyTavern 从 {} 迁到 {}",
        old.display(),
        new.display()
    ));
    Ok(notes)
}

/// Gemini CLI 装进 `new`（`npm install --prefix`）。旧的 npm 全局那份不删。
pub async fn relocate_gemini_cli(new: &Path) -> Result<Vec<String>> {
    use crate::install::{detect, managed, winget};
    const KEY: &str = "gemini-cli";
    let probe = check_target(KEY, new);
    if !probe.ok {
        return Err(GateError::Other(probe.reason.unwrap_or_default()));
    }
    if crate::gemini_bridge::running() {
        return Err(GateError::Other(
            "酒馆的 Gemini 桥接正在用 Gemini CLI，先停掉再迁移。".into(),
        ));
    }
    detect::node_exe().ok_or_else(|| {
        GateError::Other("没找到 Node.js：Gemini CLI 是 npm 包，先装 Node.js LTS。".into())
    })?;
    std::fs::create_dir_all(new)?;
    let mut log = Vec::new();
    let args: Vec<String> = vec![
        "/c".into(),
        "npm".into(),
        "install".into(),
        "--prefix".into(),
        new.display().to_string(),
        "@google/gemini-cli".into(),
    ];
    let ok = winget::run_streaming("cmd", &args, &crate::sink::Silent, 1, &mut log).await?;
    let pkg = new.join("node_modules").join("@google").join("gemini-cli");
    if !detect::gemini_cli_entries(&pkg).iter().any(|p| p.is_file()) {
        return Err(GateError::Other(format!(
            "npm {}，但 {} 下找不到 Gemini CLI 的入口文件。npm 最后几行：{}",
            if ok { "报告成功" } else { "没有成功" },
            pkg.display(),
            log.iter()
                .rev()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        )));
    }
    let old = managed::slot_dir(KEY);
    store_app_dir(KEY, new)?;
    let mut notes = vec![format!(
        "Gemini CLI 已装进 {}，面板（酒馆桥接、检测）以后先用这一份",
        new.display()
    )];
    if !crate::install::inventory::same_path(&old, new) && old.is_dir() {
        notes.push(format!("原来托管目录里那份（{}）没删", old.display()));
    }
    notes.push(
        "终端里敲 gemini 仍是旧的 npm 全局那份；不要了的话在终端里跑 npm uninstall -g @google/gemini-cli"
            .into(),
    );
    crate::audit::write(&format!("Gemini CLI 装进 {}", new.display()));
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tavern_paths_under_the_old_dir_follow_it() {
        let old = Path::new(if cfg!(windows) {
            r"C:\apps\sillytavern"
        } else {
            "/apps/sillytavern"
        });
        let new = Path::new(if cfg!(windows) { r"D:\ST" } else { "/apps/ST" });
        let bridge = Path::new(if cfg!(windows) {
            r"E:\bridge"
        } else {
            "/other/bridge"
        });
        let mut cfg = crate::plugins::sillytavern::TavernConfig {
            sillytavern_root: old.to_path_buf(),
            st_launcher: old.join("Start.bat"),
            bridge_root: bridge.to_path_buf(),
            ..Default::default()
        };
        assert_eq!(rebase_tavern(&mut cfg, old, new), 2);
        assert_eq!(cfg.sillytavern_root, new);
        assert_eq!(cfg.st_launcher, new.join("Start.bat"));
        assert_eq!(cfg.bridge_root, bridge, "不在旧目录下的不动");
        assert_eq!(rebase(Path::new(""), old, new), None);
        assert_eq!(
            rebase(&old.with_file_name("sillytavern2").join("x"), old, new),
            None,
            "前缀相同但不是子目录"
        );
    }
}
