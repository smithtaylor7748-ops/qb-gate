//! 停用会另起一个看门狗的旧版启动脚本。文件只**移到**带日期的备份目录，从不删除。
//!
//! # ⛔ 这件事一台机器只做一次（0.29.0）
//!
//! 0.28.0 之前它挂在 `lib.rs` 的 setup 里、**每次启动都跑一遍**：枚举使用者的桌面，
//! 按五个写死的文件名把命中的文件 `rename` 进一个隐藏目录，没有任何提示。
//!
//! 两个问题：
//!
//! 1. **杀软会当真。** 「未签名的进程，开机就枚举桌面、按名字搬走文件」正是勒索软件与
//!    受控文件夹访问那两套启发式盯的形状。这台机器 2026-09 连着三次
//!    `Trojan:Win32/Bearfoos.A!ml`（见 `docs/ANTIVIRUS.zh-CN.md`），能减的动作都要减；
//! 2. **它本来就该是一次性的。** 这是 v0.4 时代那批 `.ps1` / `.cmd` 启动器的迁移动作。
//!    搬过一次之后，桌面上再出现同名文件，要么是使用者自己放的（面板凭什么动它），
//!    要么是他从备份里拿回来的（那更不该再搬走一次）。跑第一万次和跑第一次，
//!    唯一的差别是又多了一次「像恶意软件」的机会。
//!
//! 「做过了」记在 [`MARKER`] 里。⚠ **marker 在就直接返回，连桌面目录都不读** ——
//! 「少做一次 rename」没有意义，要减的是**那次枚举**本身。
//!
//! 想重新跑一遍（换机器、从备份恢复了旧脚本）：删掉那个 marker 文件。

use crate::error::Result;

const LEGACY_NAMES: &[&str] = &[
    "ClaudeIpGate.ps1",
    "ClaudeIpGate.cmd",
    "start-claude-pro-rp.ps1",
    "start-claude-pro-rp.cmd",
    "一键启动克劳德桌面.cmd",
];

/// 「这台机器已经迁移过了」。放在面板自己的状态目录下，不碰使用者的任何目录。
const MARKER: &str = "legacy-launchers-migrated";

fn marker_path() -> std::path::PathBuf {
    crate::paths::state_dir().join(MARKER)
}

/// 已经做过这次迁移了吗。
pub fn already_migrated() -> bool {
    marker_path().exists()
}

/// 把桌面上的旧启动脚本搬进备份目录。**每台机器只做一次**，之后直接返回空。
///
/// 返回这一次搬走了哪些文件；已经做过的话返回空 `Vec`（不是错误 —— 什么都不用做
/// 本来就是正常状态）。
pub fn disable_known_launchers() -> Result<Vec<String>> {
    if already_migrated() {
        return Ok(Vec::new());
    }
    let Some(desktop) = dirs::desktop_dir() else {
        return Ok(Vec::new());
    };
    let backup = crate::paths::state_dir().join("legacy-scripts-backup");
    let mut moved = Vec::new();
    for name in LEGACY_NAMES {
        let src = desktop.join(name);
        if !src.is_file() {
            continue;
        }
        std::fs::create_dir_all(&backup)?;
        let mut dst = backup.join(name);
        if dst.exists() {
            dst = backup.join(format!(
                "{}.{}",
                name,
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            ));
        }
        std::fs::rename(&src, &dst)?;
        moved.push(name.to_string());
    }
    if !moved.is_empty() {
        crate::audit::write(&format!(
            "已停用旧启动脚本：{}（搬到 {}，没有删除）",
            moved.join(", "),
            backup.display()
        ));
    }
    // 一个文件都没搬也要落 marker —— 「桌面上本来就没有」跟「搬完了」对后续是同一件事，
    // 而不落的话下次启动又要枚举一遍桌面，减少动作这件事就白做了。
    mark_migrated()?;
    Ok(moved)
}

fn mark_migrated() -> Result<()> {
    let path = marker_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!(
            "旧启动脚本迁移已于 {} 完成。删掉这个文件会让面板下次启动时再扫一次桌面。\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 名单是写死的五个文件名，别长出通配符来 —— 桌面是使用者的地方。
    #[test]
    fn the_name_list_is_exact_and_never_a_pattern() {
        assert_eq!(LEGACY_NAMES.len(), 5);
        for n in LEGACY_NAMES {
            assert!(!n.contains('*'), "{n} 里有通配符");
            assert!(!n.contains('?'), "{n} 里有通配符");
            assert!(n.ends_with(".ps1") || n.ends_with(".cmd"), "{n} 不是脚本");
        }
    }

    /// marker 落在面板自己的状态目录里，不在使用者的桌面或任何别的地方。
    ///
    /// ⛔ 单测不碰真实运行期状态：这里只看路径长什么样，不建文件、不写盘。
    #[test]
    fn the_marker_lives_in_our_own_state_dir() {
        let p = marker_path();
        assert!(p.starts_with(crate::paths::state_dir()));
        assert_eq!(p.file_name().unwrap(), MARKER);
    }
}
