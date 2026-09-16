//! 升级官方 Claude Code。
//!
//! 移植自 `install-official-claude.ps1`。四条必须保留的行为：
//!
//!   1. **渠道不能写死 `stable`。** 实测 stable=2.1.236 而本机=2.1.258，
//!      照着它装就是降级。默认走 `latest`。
//!   2. **版本号统一取前三段再比。** 本机文件属性读出来是四段的 `2.1.258.0`，
//!      渠道接口返回三段的 `2.1.258`；直接按四段比会得出「渠道比本机旧」的
//!      错误结论 —— 恰好是这里要防的坑。
//!   3. **本机版本从文件属性读，不要运行 exe。** 没有租约时 claude.exe 上挂着
//!      Deny ExecuteFile，升级流程不能依赖「能把这个二进制启动起来」。
//!   4. **装完必须重新上锁。** 官方安装器写的是全新 exe，继承干净 ACL，
//!      门禁那条 Deny 跟着旧文件一起没了。重锁失败要报错，不能默默放过。
//!
//! # v0.7.0：为什么加了「自己补完」这条路
//!
//! 2026-09-10 实机取证，把上一版的诊断推翻了。当时面板报的是
//! 「权限被写坏了，去点应急解锁」，而实际情况是：
//!
//! | 查的东西 | 结果 |
//! |---|---|
//! | `~\.local\bin\claude.exe` 的 ACL | **完全健康** —— `<用户名>:FullControl:Allow`、继承开着、**一条 Deny 都没有** |
//! | bin 那份的 SHA-256 | 与 `versions\2.1.266` **逐字节相同**，仍是旧版 |
//! | `versions\2.1.267` | 220,051,616 字节，01:41:52 就下好了，**一直没被用上** |
//! | 重试那次有没有重新下载 | 没有 —— `versions\2.1.267` 时间戳纹丝不动 |
//!
//! 也就是说：官方安装器**认出目标版本已经在本地了，然后跳过了换 bin 那一步**，
//! 并且照样 `exit 0` 打印 `✓ successfully installed!`。
//! 让使用者去点「应急解锁」对这个形态**毫无作用**，那条处方是错的。
//!
//! 补充一条布局事实（也是上一版记错的）：
//! `~\.local\share\claude\versions\<版本>` 是**文件**，不是目录 ——
//! 每个版本就是一份完整的二进制，`~\.local\bin\claude.exe` 是其中一份的副本。
//!
//! 所以现在多了两件事：
//!
//!   * 跑安装器之前**开一个维护窗口**（`gate::Maintenance::with_unlock`）。
//!     老代码从头到尾一次都没解过锁，而安装器是个黑盒第三方程序。
//!   * 装完**按哈希核对**，没换成就自己把 `versions\<目标>` 复制过去 ——
//!     先验签名主体含 Anthropic、旧文件改名留底、复制完再算一次哈希。

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

const RELEASES_URL: &str = "https://downloads.claude.ai/claude-code-releases";
pub const INSTALLER_URL: &str = "https://claude.ai/install.ps1";

/// 残留副本的文件名前缀。
///
/// **必须与 `gate::targets::stale_copies()` 认的前缀一致。**
/// 名字一漂，我们留的那份备份就永远不会被 `gate::clean_stale_copies()` 清掉 ——
/// 那是一份没有 Deny ACL 的、完整可执行的 claude.exe，也就是一条现成的
/// 绕过门禁的路（档案 §7.9 那条 `claude.exe.old.*` 教训的同一个坑）。
const STALE_PREFIX: &str = "claude.exe.old.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default, TS)]
#[ts(export, rename = "UpgradeChannel")]
pub enum Channel {
    #[default]
    Latest,
    Stable,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Latest => "latest",
            Channel::Stable => "stable",
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UpgradePlan {
    pub installed: Option<String>,
    pub available: Option<String>,
    pub action: Action,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "UpgradeAction")]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 没装过，全新安装
    FreshInstall,
    /// 可以升级
    Upgrade,
    /// 已经是最新
    UpToDate,
    /// 渠道版本更旧，装下去是降级 —— 默认拒绝
    WouldDowngrade,
    /// 查不到渠道版本，交给官方安装器自己判断
    Unknown,
    /// 文件在，版本号读不出来。**这不是「没装」**。
    ///
    /// 分出这一档的理由：`file_version()` 拿不到值时，老代码把它和
    /// 「路径上根本没有 claude.exe」并成同一种情况，一律报 `FreshInstall`。
    /// 于是执行锁一旦把文件权限改坏（见 `gate::acl::unlock` 的说明），
    /// 界面就把装好的 2.1.266 显示成「未安装 / 尚未安装」，
    /// 升级完那句提示也变成「已安装 。重新上锁 4 个可执行文件。」——
    /// 版本号的位置是空的。**读不出来要如实说读不出来。**
    VersionUnreadable,
}

/// 版本号取前三段。取不到就是 `None`。
///
/// 这个函数是第 2 条教训的落点：两边位数不一样，必须归一化。
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for ch in text.trim().chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else {
            if !cur.is_empty() {
                nums.push(cur.parse::<u32>().ok()?);
                cur.clear();
            }
            // 版本号之间可能夹着别的字符，凑够三段就够了
            if nums.len() >= 3 {
                break;
            }
            if ch != '.' && !nums.is_empty() && nums.len() < 3 {
                // 形如 "v2.1.258 (build x)" —— 遇到非分隔符就重新开始找
                if ch.is_whitespace() || ch == '(' {
                    break;
                }
            }
        }
    }
    if !cur.is_empty() && nums.len() < 3 {
        nums.push(cur.parse::<u32>().ok()?);
    }
    if nums.len() >= 3 {
        Some((nums[0], nums[1], nums[2]))
    } else {
        None
    }
}

/// 只比前三段，决定该不该装。纯函数，可单测。
///
/// `present` = 磁盘上到底有没有这个 exe，跟「版本号读没读出来」是两回事。
/// 合成一个参数就会得出「读不出版本 ⇒ 没装过」，那正是本模块修掉的那个坑。
pub fn decide(
    present: bool,
    installed: Option<(u32, u32, u32)>,
    available: Option<(u32, u32, u32)>,
) -> Action {
    match (present, installed, available) {
        (false, _, _) => Action::FreshInstall,
        (true, None, _) => Action::VersionUnreadable,
        (true, Some(_), None) => Action::Unknown,
        (true, Some(i), Some(a)) if a == i => Action::UpToDate,
        (true, Some(i), Some(a)) if a < i => Action::WouldDowngrade,
        _ => Action::Upgrade,
    }
}

// ------------------------------------------------------------------ 布局

/// 官方原生安装器存放各版本的地方。
///
/// ⚠ **`versions\<版本>` 是文件，不是目录。** 每个版本就是一份完整的二进制
/// （2.1.267 = 220,051,616 字节），`~\.local\bin\claude.exe` 是其中一份的副本。
/// 上一版档案把它记成目录，于是「versions 下有 2.1.267」被读成
/// 「bin 下跑的是 2.1.267」—— 这两件事完全不是一回事，排查时按哈希比对。
pub fn versions_dir() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".local")
            .join("share")
            .join("claude")
            .join("versions"),
    )
}

/// 某个版本在 `versions\` 下的那份二进制。
pub fn versions_file(v: &str) -> Option<PathBuf> {
    Some(versions_dir()?.join(v))
}

/// 备份文件名。**前缀必须是 [`STALE_PREFIX`]**，见那里的说明。
pub fn backup_name(stamp: &str) -> String {
    format!("{STALE_PREFIX}{stamp}")
}

pub fn sha256_of(p: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).ok()?;
    Some(hex::encode(h.finalize()))
}

/// 哈希核对不了时的兜底判据：**装完还落在渠道后面，而且版本号跟装之前一模一样。**
///
/// 这是 v0.6.1 的老判据。它抓不到「安装器打印了新版本号但 bin 没换」之外的
/// 形态，所以 v0.7.0 优先按哈希比。但哈希有比不了的时候
/// （`versions@<目标>` 不在、渠道版本查不到），**那时候不能就此什么都不查** ——
/// 不查就等于把「安装器说成功」直接当成功，退回 v0.5.3 之前的样子。
pub fn looks_unchanged(before: Option<&str>, action: Action, after: Option<&str>) -> bool {
    matches!(action, Action::Upgrade) && after.is_some() && before == after
}

/// 磁盘上那份跟目标版本是不是同一个文件。
///
/// `None` = **判断不了**（哪一头的哈希没拿到）。这个三态不能塌成 bool ——
/// 塌了就会把「没验成」当成「没换成」，然后去做一次根本不必要的覆盖。
pub fn matches_target(bin_hash: Option<&str>, target_hash: Option<&str>) -> Option<bool> {
    match (bin_hash, target_hash) {
        (Some(a), Some(b)) => Some(a.eq_ignore_ascii_case(b)),
        _ => None,
    }
}

/// 读本机版本。**从文件属性读，不运行它。**
///
/// 读不出来会往门禁日志里记一行原因。空手而归又不留痕迹，下一个人只能看到
/// 界面上写着「未安装」，而文件明明就在那 —— §7.17 那条教训的同一类。
///
/// 检测页（`detect::claude_code`）也用这一个，不再自己跑 `--version`。
#[cfg(windows)]
pub(crate) async fn file_version(path: &std::path::Path) -> Option<String> {
    // 单引号在 Windows 文件名里合法，按 PowerShell 的规矩转义成两个。
    let out = crate::process::powershell_tokio(&format!(
        "(Get-Item -LiteralPath '{}').VersionInfo.ProductVersion",
        path.display().to_string().replace('\'', "''")
    ))
    .output()
    .await;
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            crate::audit::write(&format!("读不出 {} 的版本号：{e}", path.display()));
            return None;
        }
    };
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        crate::audit::write(&format!(
            "读不出 {} 的版本号（退出码 {}）：{}",
            path.display(),
            out.status.code().unwrap_or(-1),
            if err.is_empty() {
                "没有任何输出，多半是这个文件的权限被改坏了".into()
            } else {
                err
            }
        ));
        return None;
    }
    Some(s)
}

#[cfg(not(windows))]
pub(crate) async fn file_version(_path: &std::path::Path) -> Option<String> {
    None
}

/// 升级的对象：**官方原生安装器的落点** `~\.local\bin\claude.exe`，不在就是 `None`。
///
/// 原来这里用的是 `find_official_claude()` —— 它先读 `claude-path.txt` 里记住的路径，
/// 再按一张自己的表找。记住的是桌面端带的副本或 winget 那份时，
/// 「装完按哈希核对」核对的就是另一个文件，结论全错。官方安装器只会写这一个位置。
pub fn native_bin() -> Option<PathBuf> {
    super::inventory::native_bin(&super::inventory::Roots::current()).filter(|p| p.is_file())
}

async fn channel_version(ch: Channel) -> Option<String> {
    let c = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .ok()?;
    let text = c
        .get(format!("{RELEASES_URL}/{}", ch.as_str()))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    Some(text.trim().to_string())
}

/// 只看不装：报告本机版本、渠道版本与建议动作。
///
/// v0.9.0：面板托管的那份在，就比它（版本读安装记录，读不到再读文件属性）；
/// 升级也只升它（`lib.rs::upgrade_execute`）。不在才看官方安装器那份。
pub async fn plan(ch: Channel) -> UpgradePlan {
    use super::managed;
    let managed_exe = managed::exe(managed::App::ClaudeCode);
    let is_managed = managed_exe.is_file();
    let path = if is_managed {
        Some(managed_exe)
    } else {
        native_bin()
    };
    let installed = match &path {
        Some(_) if is_managed => {
            match managed::record_of(&managed::root(), managed::App::ClaudeCode) {
                Some(r) => Some(r.version),
                None => file_version(path.as_deref().unwrap_or(Path::new(""))).await,
            }
        }
        Some(p) => file_version(p).await,
        None => None,
    };
    let available = channel_version(ch).await;

    let action = decide(
        path.is_some(),
        installed.as_deref().and_then(parse_version),
        available.as_deref().and_then(parse_version),
    );

    if is_managed && matches!(action, Action::Upgrade | Action::Unknown) {
        return UpgradePlan {
            detail: format!(
                "面板托管的 Claude Code：{} → {}。升级会从官方源下载新版、核对 SHA-256 与数字签名后替换，旧版留底。",
                installed.clone().unwrap_or_else(|| "版本未知".into()),
                available.clone().unwrap_or_else(|| "查不到".into())
            ),
            installed,
            available,
            action,
        };
    }

    let detail = match action {
        Action::FreshInstall => {
            // 没有原生安装器那份，不等于这台机器没装 Claude Code ——
            // winget / npm / Scoop 装的都不在 `.local\bin`。说清楚这里升的是哪一份。
            let roots = super::inventory::Roots::current();
            match super::inventory::preferred_cli(&roots) {
                Some(i) => format!(
                    "本机没有官方原生安装器的那份（~\\.local\\bin\\claude.exe）。\
                     现在启动用的是 {}。这里走官方安装器，会在 ~\\.local\\bin 另装一份，\
                     之后启动优先用它；原来那份请用它自己的方式升级（例如 winget upgrade）。",
                    i.path.display()
                ),
                None => "本机尚未安装官方 Claude Code，将执行全新安装。".into(),
            }
        }
        Action::VersionUnreadable => format!(
            "{} 在那儿，但读不出它的版本号，所以判断不了该不该升级。\
             常见原因是执行锁把这个文件的权限改坏了（DACL 里只剩一条 Deny，\
             连读都读不了）—— 到「IP 锁」页点一次「应急解锁」，权限会一并修回来。",
            path.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        ),
        Action::UpToDate => format!(
            "已经是 {} 渠道的最新版本（{}），无需升级。",
            ch.as_str(),
            installed.clone().unwrap_or_default()
        ),
        Action::WouldDowngrade => format!(
            "已停止：{} 渠道的 {} 比本机的 {} 更旧，装下去是降级。\
             本机版本跑到渠道前面属于正常现象（早期自动升级留下的）。{}",
            ch.as_str(),
            available.clone().unwrap_or_default(),
            installed.clone().unwrap_or_default(),
            if matches!(ch, Channel::Stable) {
                "要升级请改用 latest 渠道。"
            } else {
                ""
            }
        ),
        Action::Upgrade => format!(
            "可升级：{} → {}",
            installed.clone().unwrap_or_default(),
            available.clone().unwrap_or_default()
        ),
        Action::Unknown => "查不到渠道版本，跳过版本比较，交给官方安装器自己判断。".into(),
    };

    UpgradePlan {
        installed,
        available,
        action,
        detail,
    }
}

// ------------------------------------------------------------------ 诊断

/// 到底是什么挡住了这次替换。
///
/// 替掉老代码里那句写死的「多半是权限被写坏了」——
/// 2026-09-10 实机上 ACL 完全健康，那句话把排查方向带偏了整整一晚。
/// **查到什么说什么，查不到就说查不到，不猜。**
#[cfg(windows)]
pub async fn diagnose_blocker(bin: &Path) -> String {
    let mut bits: Vec<String> = Vec::new();

    // ① 谁占着这个文件。按 ExecutablePath 匹配，**不按进程名** ——
    //    按名字找会把桌面端那十几个 claude.exe 一起算进来，全是噪声。
    let quoted = bin.display().to_string().replace('\'', "''");
    let script = format!(
        "$p = @(Get-CimInstance Win32_Process -Filter \"Name='claude.exe'\" | \
         Where-Object {{ $_.ExecutablePath -eq '{quoted}' }} | \
         ForEach-Object {{ \"$($_.ProcessId)\" }}); ConvertTo-Json -InputObject @($p) -Compress"
    );
    let out = crate::process::powershell_tokio(&script).output().await;
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            match serde_json::from_str::<Vec<String>>(&text) {
                Ok(pids) if !pids.is_empty() => bits.push(format!(
                    "这个文件正被 {} 个进程占用（PID {}）—— 正在运行的 exe 换不掉，先把它们关掉再升级。",
                    pids.len(),
                    pids.join(", ")
                )),
                Ok(_) => bits.push("没有进程占用这个文件。".into()),
                // 解不开就说解不开。空结果当成「没人占用」正是 §7.17 那个坑。
                Err(_) => bits.push("查不出有没有进程占用这个文件（枚举没能解析）。".into()),
            }
        }
        Err(e) => bits.push(format!("查进程占用失败：{e}。")),
    }

    // ② 权限。只有真查出问题才提应急解锁。
    let readable = std::fs::File::open(bin).is_ok();
    let locked = crate::acl::locked(bin).unwrap_or(false);
    if !readable {
        bits.push(
            "当前用户连读都读不了这个文件 —— 这才是「权限被写坏了」的样子，\
             到「IP 锁」页点一次「应急解锁」。"
                .into(),
        );
    } else {
        bits.push(format!(
            "权限正常（读得到{}）。",
            if locked {
                "，执行锁在位，这不影响写入"
            } else {
                ""
            }
        ));
    }

    // ③ 空间。218 MB 的文件，这台机器的 C: 满过一次（档案 §2）。
    if let Some(free) = free_space_mb(bin) {
        bits.push(format!("目标盘剩余空间 {free} MB。"));
        if free < 600 {
            bits.push("空间偏紧 —— 换一份 218 MB 的二进制需要同时放下新旧两份。".into());
        }
    }

    bits.join(" ")
}

#[cfg(not(windows))]
pub async fn diagnose_blocker(_bin: &Path) -> String {
    String::new()
}

#[cfg(windows)]
fn free_space_mb(p: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let root = p.components().next()?;
    let mut dir: std::ffi::OsString = root.as_os_str().to_owned();
    dir.push("\\");
    let wide: Vec<u16> = dir.encode_wide().chain(std::iter::once(0)).collect();
    let mut free: u64 = 0;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free), None, None).ok()? };
    Some(free / 1024 / 1024)
}

#[cfg(not(windows))]
fn free_space_mb(_p: &Path) -> Option<u64> {
    None
}

// ------------------------------------------------------------------ 补完

/// 安装器没换成时，由面板自己把 `versions\<目标版本>` 装到 `bin\claude.exe`。
///
/// 使用者 2026-09-10 明确点头同意面板做这件事。做之前有三道闸，**一道都不能省**：
///
///   1. **签名主体必须含 Anthropic。** `None`（拿不到签名）和 `Some(false)`
///      都拒绝 —— 拿不到只能说「没验成」，不能当成「验过了」。
///      我们要把这份二进制放到一个会被反复执行的位置上，宁可不装。
///   2. **旧文件改名留底，不删。** 名字用 `claude.exe.old.<时间戳>`，
///      正好落进 `gate::clean_stale_copies()` 的清理范围，下次升级开头会被收走。
///   3. **复制完再算一次哈希。** 对不上就把改名回滚 ——
///      一个复制到一半的 218 MB 二进制比不升级危险得多。
pub async fn finish_install(bin: &Path, target: &str, rep: &dyn ProgressSink) -> Result<String> {
    let src = versions_file(target)
        .filter(|p| p.is_file())
        .ok_or_else(|| {
            GateError::Other(format!(
                "官方安装器没换成 {target}，而 versions 目录下也没有这个版本的二进制，\
                 面板无从补起。请重试一次升级。"
            ))
        })?;

    rep.log(6, &format!("找到 {}", src.display()));

    // ---- 闸 1：签名
    let signer = crate::signature::signer_of(&src).await;
    match crate::install::winget::signature_has_anthropic(signer.as_deref()) {
        Some(true) => rep.log(6, "签名主体含 Anthropic，通过"),
        Some(false) => {
            return Err(GateError::Other(format!(
                "拒绝复制：{} 的签名主体里没有 Anthropic（读到的是 {}）。\
                 面板不会把一个来源存疑的二进制放到 claude.exe 上。",
                src.display(),
                signer.unwrap_or_default()
            )))
        }
        None => {
            return Err(GateError::Other(format!(
                "拒绝复制：读不出 {} 的 Authenticode 签名 —— 这只能说「没验成」，\
                 不能当成验过了。面板不会拿一份没验成的二进制去覆盖 claude.exe。",
                src.display()
            )))
        }
    }

    let src_hash = sha256_of(&src)
        .ok_or_else(|| GateError::Other(format!("算不出 {} 的 SHA-256", src.display())))?;

    // ---- 闸 2：旧文件改名留底
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let backup = bin.with_file_name(backup_name(&stamp));
    std::fs::rename(bin, &backup).map_err(|e| {
        GateError::Other(format!(
            "换不动 {}：给旧文件改名就失败了（{e}）。",
            bin.display()
        ))
    })?;
    rep.log(6, &format!("旧文件已留底为 {}", backup.display()));

    // ---- 复制。失败就把改名转回去，别留下一个没有 claude.exe 的机器。
    if let Err(e) = std::fs::copy(&src, bin) {
        let _ = std::fs::remove_file(bin);
        let _ = std::fs::rename(&backup, bin);
        return Err(GateError::Other(format!(
            "复制 {} 失败：{e}。已把旧版还原回去。",
            src.display()
        )));
    }

    // ---- 闸 3：复制完再算一次哈希
    match sha256_of(bin) {
        Some(h) if h.eq_ignore_ascii_case(&src_hash) => {
            rep.log(6, &format!("复制完成并核对通过（SHA-256 {}…）", &h[..16]));
            Ok(format!(
                "官方安装器没换成，已由面板补完：{} → {}。旧版留底在 {}。",
                target,
                bin.display(),
                backup.display()
            ))
        }
        other => {
            let _ = std::fs::remove_file(bin);
            let _ = std::fs::rename(&backup, bin);
            Err(GateError::Other(format!(
                "复制之后哈希对不上（期望 {src_hash}，实际 {}），已把旧版还原回去。",
                other.unwrap_or_else(|| "读不出".into())
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_and_four_segment_versions_the_same() {
        // 这条是本模块存在的理由：两边位数不一样，归一化后必须相等。
        assert_eq!(parse_version("2.1.258.0"), Some((2, 1, 258)));
        assert_eq!(parse_version("2.1.258"), Some((2, 1, 258)));
        assert_eq!(parse_version("2.1.258.0"), parse_version("2.1.258"));
    }

    #[test]
    fn tolerates_surrounding_noise() {
        assert_eq!(parse_version("  v2.1.236\n"), Some((2, 1, 236)));
    }

    #[test]
    fn rejects_unparseable() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("not a version"), None);
        assert_eq!(parse_version("2.1"), None);
    }

    #[test]
    fn four_vs_three_segments_is_not_a_downgrade() {
        // 回归测试：naive [version] 比较会把这个判成 WouldDowngrade。
        let installed = parse_version("2.1.258.0");
        let available = parse_version("2.1.258");
        assert_eq!(decide(true, installed, available), Action::UpToDate);
    }

    #[test]
    fn older_channel_is_refused() {
        let installed = parse_version("2.1.258.0");
        let available = parse_version("2.1.236");
        assert_eq!(decide(true, installed, available), Action::WouldDowngrade);
    }

    #[test]
    fn newer_channel_upgrades() {
        assert_eq!(
            decide(true, parse_version("2.1.236"), parse_version("2.1.258")),
            Action::Upgrade
        );
    }

    #[test]
    fn missing_install_is_fresh() {
        assert_eq!(
            decide(false, None, parse_version("2.1.258")),
            Action::FreshInstall
        );
    }

    #[test]
    fn unknown_channel_version_does_not_block() {
        assert_eq!(
            decide(true, parse_version("2.1.258"), None),
            Action::Unknown
        );
    }

    /// 文件在、版本号读不出来 —— **不许**报成「尚未安装」。
    ///
    /// 回归测试。实机现象：`.local\bin\claude.exe` 218 MB 好端端躺在那儿，
    /// 面板却在「本机」那一栏写「未安装」、建议栏写「尚未安装」，
    /// 升级完的提示是「已安装 。重新上锁 4 个可执行文件。」。
    /// 根因在 `gate::acl::unlock` 把文件权限写坏了，但**报成「没装过」是这里的错**：
    /// 「读不出来」和「不存在」被合成了同一种情况。
    #[test]
    fn present_but_unreadable_is_not_reported_as_missing() {
        let a = decide(true, None, parse_version("2.1.267"));
        assert_eq!(a, Action::VersionUnreadable);
        assert_ne!(a, Action::FreshInstall, "文件在那儿，不能说它没装过");
        // 渠道版本也一起查不到时，仍然是「读不出本机版本」这件事更要紧。
        assert_eq!(decide(true, None, None), Action::VersionUnreadable);
    }

    #[test]
    fn default_channel_is_latest_not_stable() {
        // 写死 stable 会导致降级，见文件头第 1 条。
        assert_eq!(Channel::default(), Channel::Latest);
    }

    /// `versions\<版本>` 是**文件**，路径末段就是版本号本身。
    ///
    /// 上一版档案把它记成目录，于是「versions 下有 2.1.267」被读成
    /// 「bin 下跑的是 2.1.267」。实机核对：
    /// `-a---- …\versions\2.1.267`（220,051,616 字节），是文件。
    #[test]
    fn versions_path_ends_with_the_bare_version_number() {
        let Some(p) = versions_file("2.1.267") else {
            return; // 拿不到 home 的受限环境，跳过而不是判红
        };
        assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("2.1.267"));
        assert!(
            p.parent()
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str())
                == Some("versions"),
            "父目录必须是 versions：{}",
            p.display()
        );
    }

    /// 哈希一样就是「安装器什么都没做」，不一样才是换成了。
    ///
    /// 这是本轮把判据从「版本号字符串」换成「文件哈希」的落点：
    /// 实机上安装器打印的版本号是 2.1.267（它自己下下来的那份），
    /// 而 bin 里躺着的仍然是 2.1.266 —— 只看版本号字符串会被它带着走。
    #[test]
    fn same_hash_means_the_installer_did_nothing() {
        let old = "d2c5f7b3b6a12819097ceb6efbce2a390157166003fcaee32dbde0e6d7b45ef7";
        let new = "23dde2a47cf1d7d9c4a2d96d21fa80ea9bfc872dfde0ee06e9982d2908603350";
        assert_eq!(matches_target(Some(old), Some(old)), Some(true));
        assert_eq!(matches_target(Some(old), Some(new)), Some(false));
        // 大小写不该影响判断 —— PowerShell 的 Get-FileHash 给大写，
        // sha2 给小写，两边混着比过。
        assert_eq!(
            matches_target(Some(&old.to_uppercase()), Some(old)),
            Some(true)
        );
    }

    /// 拿不到哈希要报「判断不了」，**不能塌成「没换成」**。
    ///
    /// 塌了就会在每次 versions 下缺文件时都去跑一遍补完流程，
    /// 而那条流程的第一件事正是拒绝并报错 —— 等于把「没验成」变成了一次失败。
    #[test]
    fn an_unavailable_hash_is_undecidable_not_a_mismatch() {
        let h = "abc";
        assert_eq!(matches_target(None, Some(h)), None);
        assert_eq!(matches_target(Some(h), None), None);
        assert_eq!(matches_target(None, None), None);
    }

    /// 哈希核对不了时，仍然要按版本号查一遍。
    ///
    /// 回归测试。v0.7.0 把判据换成哈希之后，`None`（核对不了）那条分支一度
    /// **什么都不查就往下走** —— 等于退回 v0.5.3 之前「安装器说成功就是成功」，
    /// 而 §7.21 那次「成功的失败」正是这么溜过去的。
    #[test]
    fn an_undecidable_hash_still_falls_back_to_the_version_check() {
        // 装完还落在渠道后面，版本号一个字没变 —— 就是没装上。
        assert!(looks_unchanged(
            Some("2.1.266.0"),
            Action::Upgrade,
            Some("2.1.266.0")
        ));
        // 版本号变了 —— 装上了。
        assert!(!looks_unchanged(
            Some("2.1.266.0"),
            Action::Upgrade,
            Some("2.1.267.0")
        ));
        // 已经是最新 / 查不到渠道版本 —— 什么都断不了，不许报失败。
        assert!(!looks_unchanged(
            Some("2.1.267.0"),
            Action::UpToDate,
            Some("2.1.267.0")
        ));
        assert!(!looks_unchanged(
            Some("2.1.267.0"),
            Action::Unknown,
            Some("2.1.267.0")
        ));
        // 装完读不出版本号：这时候「没变」是假象，别拿它报失败 ——
        // 界面另有 VersionUnreadable 那一档说这件事。
        assert!(!looks_unchanged(None, Action::Upgrade, None));
    }
}
