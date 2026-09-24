//! 完全卸载的**采集**（0.19.0）：去真机上把 [`purge::Facts`] 采出来。
//!
//! # 为什么在编排层
//!
//! 一次完整盘点要同时问三个领域：安装清单（`install`）、账户槽位（`accounts`）、
//! 注册表与环境变量（`sysenv`）。三个都是 L2，互不依赖 —— 谁也不该为了盘点
//! 去认识另外两个。跨域收集属于这一层，跟 `install_ops` 把「门禁 + 安装」
//! 拼起来是同一个道理。
//!
//! 判断留在 `install::purge::plan`（纯函数、有单测），这里只负责**把事实取回来**。
//!
//! # ⛔ 只取名字和位置，不取内容
//!
//! 环境变量只记名字、Shell 配置只记文件与行号、凭据只记条目名。
//! 跟 `sysenv::checkup` 的密钥扫描是同一条原则：报出来的东西迟早会被打进日志、
//! 截图或者贴进 issue，所以不是靠「记得别打印」，是让它根本拿不到。
//!
//! 下面那几个匹配函数是纯的，也正是容易出错的地方（少认一个前缀就漏删，
//! 多认一个就误删别人的东西），所以每一个都钉着单测。

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::install::purge::{self, Facts, Target};

/// 这个环境变量名是不是**专门为 Claude / Codex 设的**。
///
/// 只认前缀，不认「名字里带 claude」—— `MY_CLAUDE_NOTES` 是使用者自己的东西。
pub fn is_owned_env(name: &str, target: Target) -> bool {
    let n = name.to_ascii_uppercase();
    match target {
        Target::ClaudeCode | Target::ClaudeDesktop => {
            n.starts_with("ANTHROPIC_") || n.starts_with("CLAUDE_CODE_") || n == "CLAUDE_CONFIG_DIR"
        }
        // Codex 读 OPENAI_*，但那也是别的 OpenAI 工具在读的 —— 删它会误伤，
        // 所以只认 CODEX_ 前缀那一类。
        Target::Codex => n.starts_with("CODEX_"),
        // 桌面端跟 CLI 共用 CODEX_HOME 那一族：归 CLI 那个目标管，卸桌面端不动它。
        Target::CodexDesktop => false,
        // GEMINI_API_KEY 别的 Google 工具也在读，跟 OPENAI_* 同一个理由不碰；
        // GEMINI_CLI_HOME 是 CLI 独有的。
        Target::GeminiCli => n == "GEMINI_CLI_HOME",
        Target::Chrome | Target::Antigravity => false,
    }
}

/// PATH 里的这一项是不是**只服务于**这个软件。
///
/// `~\.local\bin` 不算：uv、pipx 也往那里装东西，摘掉它会把别人的命令一起弄没
/// （`managed::cleanup` 末尾那句 note 记的就是这件事）。
pub fn is_owned_path_entry(entry: &str, target: Target) -> bool {
    let e = entry.to_ascii_lowercase().replace('/', "\\");
    let e = e.trim_end_matches('\\');
    if e.ends_with("\\.local\\bin") {
        return false;
    }
    match target {
        Target::ClaudeCode => {
            e.contains("\\claudeipgate\\apps\\claude-code") || e.ends_with("\\.claude\\bin")
        }
        Target::Codex => e.contains("\\claudeipgate\\apps\\codex"),
        Target::ClaudeDesktop => e.contains("\\anthropicclaude"),
        // Store 包、反重力的安装器、npm 包都不往 PATH 里加自己的项。
        Target::Chrome | Target::CodexDesktop | Target::Antigravity | Target::GeminiCli => false,
    }
}

/// Shell 配置里的这一行，是不是**专门给这个软件设变量**的。
///
/// 只看「有没有在设我们认的那些变量」，**不把行的内容带出去**。
pub fn shell_line_sets_owned_env(line: &str, target: Target) -> bool {
    let l = line.trim();
    if l.starts_with('#') {
        return false;
    }
    // PowerShell 的 `$env:NAME = …`、POSIX 的 `export NAME=…` 与裸 `NAME=…`。
    let candidate = l
        .strip_prefix("$env:")
        .or_else(|| l.strip_prefix("export "))
        .unwrap_or(l);
    let name: String = candidate
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return false;
    }
    // 后面必须真的是赋值，否则一句提到变量名的注释或回显也会被算进来。
    let rest = candidate[name.len()..].trim_start();
    if !rest.starts_with('=') {
        return false;
    }
    is_owned_env(&name, target)
}

/// 凭据管理器里的这一条，是不是这个软件的。
pub fn is_owned_credential(entry: &str, target: Target) -> bool {
    let e = entry.to_ascii_lowercase();
    match target {
        Target::ClaudeCode | Target::ClaudeDesktop => {
            e.contains("anthropic") || e.contains("claude")
        }
        Target::Codex | Target::CodexDesktop => e.contains("openai") || e.contains("codex"),
        // 反重力的令牌就在凭据管理器里（语言服务器 CredWrite）。只按名字含 antigravity 认 ——
        // 名字不含它的条目面板认不出，界面上要如实说。
        Target::Antigravity => e.contains("antigravity"),
        // Gemini CLI 的凭据是文件（<GEMINI_CLI_HOME>\.gemini\oauth_creds.json），随槽位一起删。
        Target::Chrome | Target::GeminiCli => false,
    }
}

// ------------------------------------------------------------------ 采集

#[cfg(windows)]
async fn reg_query(key: &str) -> Vec<(String, String)> {
    let out = crate::process::hidden_tokio(tokio::process::Command::new("reg"))
        .args(["query", key])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            crate::sysenv::checkup::parse_reg_values(&String::from_utf8_lossy(&o.stdout))
        }
        _ => Vec::new(),
    }
}

#[cfg(not(windows))]
async fn reg_query(_key: &str) -> Vec<(String, String)> {
    Vec::new()
}

/// 凭据管理器里的条目名。**`cmdkey /list` 只解析 `Target:` 那一行，不碰密码。**
#[cfg(windows)]
async fn credential_names() -> Vec<String> {
    let out = crate::process::hidden_tokio(tokio::process::Command::new("cmdkey"))
        .arg("/list")
        .output()
        .await;
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            // 中文系统上是「目标:」，英文是「Target:」。
            t.strip_prefix("Target:")
                .or_else(|| t.strip_prefix("目标:"))
                .map(|v| v.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(not(windows))]
async fn credential_names() -> Vec<String> {
    Vec::new()
}

/// 可能为 Claude / Codex 设过变量的 Shell 配置文件。
fn shell_profiles(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Documents")
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
        home.join("Documents")
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".zshrc"),
    ]
}

/// 扫一遍 Shell 配置，返回 (文件, 行号)。**行的内容不带出去。**
fn scan_shell_lines(home: &Path, target: Target) -> Vec<(PathBuf, u32)> {
    let mut out = Vec::new();
    for p in shell_profiles(home) {
        let Ok(body) = std::fs::read_to_string(&p) else {
            continue;
        };
        for (i, line) in body.lines().enumerate() {
            if shell_line_sets_owned_env(line, target) {
                out.push((p.clone(), i as u32 + 1));
            }
        }
    }
    out
}

/// 存在才算数 —— 盘点报告里不该出现一条「不存在的路径」。
fn existing(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|p| p.exists()).collect()
}

/// 去真机上采一遍。**只读**：不删任何东西，不改任何设置。
pub async fn collect(target: Target) -> Result<Facts> {
    let home = dirs::home_dir().unwrap_or_default();
    let roaming = dirs::config_dir();
    let local = dirs::data_local_dir();
    let roots = crate::install::inventory::Roots::current();

    let mut f = Facts {
        managed_root: crate::install::managed::root(),
        roaming: roaming.clone(),
        ..Facts::default()
    };

    match target {
        Target::ClaudeCode => {
            f.installs = crate::install::inventory::scan(&roots);
            f.stale = crate::install::inventory::stale_copies(&roots);
            f.config_dirs = existing(vec![home.join(".claude")]);
            f.config_files = existing(vec![
                home.join(".claude.json"),
                home.join(".claude.json.backup"),
            ]);
            f.credential_files = existing(vec![home.join(".claude").join(".credentials.json")]);
            // 账户槽位：面板自己管的那几个目录。标签从 `accounts` 来，
            // 目录怎么拼由 `AccountRoots` 说了算 —— 这里不自己拼。
            let ar = crate::accounts::AccountRoots::current();
            f.account_slots = existing(
                crate::accounts::slots()
                    .into_iter()
                    .map(|s| ar.slot_dir(&s.label))
                    .collect(),
            );
        }
        Target::Codex => {
            f.codex_paths = existing(crate::install::detect::codex_candidates());
            f.config_dirs = existing(vec![home.join(".codex")]);
            f.credential_files = existing(vec![home.join(".codex").join("auth.json")]);
        }
        Target::ClaudeDesktop => {
            // 桌面端的存根与它带的副本仍然来自 inventory。
            f.installs = crate::install::inventory::scan(&roots);
            let ar = crate::accounts::AccountRoots::current();
            let mut dirs_: Vec<PathBuf> = Vec::new();
            if let Some(a) = roaming.as_ref() {
                dirs_.push(a.join("Claude"));
            }
            if let Some(l) = local.as_ref() {
                dirs_.push(l.join("Claude"));
                dirs_.push(l.join("AnthropicClaude"));
            }
            f.config_dirs = existing(dirs_);
            // 每个槽位自己那份桌面端资料。
            f.account_slots = existing(
                crate::accounts::slots()
                    .into_iter()
                    .filter_map(|s| ar.desktop_dir(&s.label))
                    .collect(),
            );
        }
        Target::Chrome => {
            f.browser_data = existing(vec![crate::install::chrome::chrome_user_data()]);
        }
        Target::CodexDesktop => {
            // Store 包在不在：查不到（PowerShell 起不来）就当不在 —— 但这里不是「不在 = 没装」
            // 的降级：规划里没有 AppxRemove 时界面会说「没扫到」，而不是报成功。
            f.codex_desktop_package =
                tokio::task::spawn_blocking(crate::install::codex_desktop::detect)
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .is_some_and(|d| d.executable.is_some());
            let mut dirs_: Vec<PathBuf> = Vec::new();
            // Store 包自己的数据目录。
            if let Some(l) = local.as_ref() {
                dirs_.push(
                    l.join("Packages")
                        .join(crate::install::codex_desktop::PACKAGE_FAMILY),
                );
            }
            // 每个 GPT 槽位下桌面端那份 Electron 资料（`<槽位>\desktop`）；`home` 是 CLI 也在用的
            // 登录身份，**不动**。没有槽位时的默认位置是 `~\.codex\desktop`。
            let root = qb_accounts::codex::root();
            if let Ok(list) = qb_accounts::codex::list(&root) {
                for s in list.slots {
                    if let Ok(dir) = qb_accounts::codex::directory(&root, &s.id) {
                        dirs_.push(crate::workspace::codex_slot_dirs(&dir).1);
                    }
                }
            }
            dirs_.push(home.join(".codex").join("desktop"));
            f.config_dirs = existing(dirs_);
        }
        Target::Antigravity => {
            use crate::install::antigravity::{self, Product};
            if let Some(l) = local.as_ref() {
                f.antigravity_dirs = existing(Product::ALL.iter().map(|p| p.dir(l)).collect());
            }
            let mut dirs_: Vec<PathBuf> = Product::ALL.iter().map(|p| p.data_dir(&home)).collect();
            if let Some(r) = roaming.as_ref() {
                dirs_.extend(
                    Product::ALL
                        .iter()
                        .map(|p| antigravity::user_data_dir(*p, r)),
                );
            }
            f.config_dirs = existing(dirs_);
        }
        Target::GeminiCli => {
            let cands = crate::install::detect::gemini_cli_candidates();
            // 候选表的顺序是 npm 全局在前、托管目录在后（`gemini_cli_candidates` 的说明）。
            f.gemini_npm = cands.first().is_some_and(|p| p.is_file());
            let managed = crate::install::managed::root().join("gemini-cli");
            f.gemini_managed = managed.is_dir().then_some(managed);
            let root = qb_accounts::gemini::root();
            if let Ok(list) = qb_accounts::gemini::list(&root) {
                f.account_slots = existing(
                    list.slots
                        .into_iter()
                        .filter_map(|s| qb_accounts::gemini::directory(&root, &s.id).ok())
                        .collect(),
                );
            }
        }
    }

    // 环境变量与 PATH：只读 HKCU\Environment，**只留名字**。
    let env = reg_query("HKCU\\Environment").await;
    f.env_names = env
        .iter()
        .filter(|(name, _)| is_owned_env(name, target))
        .map(|(name, _)| name.clone())
        .collect();
    f.path_entries = env
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Path"))
        .map(|(_, value)| {
            value
                .split(';')
                .map(str::trim)
                .filter(|e| !e.is_empty() && is_owned_path_entry(e, target))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    f.shell_lines = scan_shell_lines(&home, target);

    f.credentials = credential_names()
        .await
        .into_iter()
        .filter(|e| is_owned_credential(e, target))
        .collect();

    Ok(f)
}

/// 采一遍，然后规划出「要删哪些」。**只读，不动任何东西。**
///
/// 界面拿它列确认框：每一项都带绝对路径与归属依据。
pub async fn plan(target: Target) -> Result<Vec<purge::Item>> {
    Ok(purge::plan(target, &collect(target).await?))
}

// ------------------------------------------------------------------ 执行

/// 从 PATH 里摘掉指定的几项，**别的一个字都不动**。
///
/// 纯函数，因为它写错的代价是「使用者的 PATH 被毁掉」，而这件事完全可以
/// 脱离磁盘测出来。比较时忽略大小写与首尾的反斜杠（`C:\X` 和 `c:/x\` 是同一项），
/// 但**保留原样的分隔与空项**不做「顺手规整」—— 顺手规整就是在动没让你动的东西。
pub fn path_without(current: &str, drop: &[String]) -> String {
    let norm = |s: &str| {
        s.trim()
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    let drop: Vec<String> = drop.iter().map(|d| norm(d)).collect();
    current
        .split(';')
        .filter(|e| e.trim().is_empty() || !drop.contains(&norm(e)))
        .collect::<Vec<_>>()
        .join(";")
}

/// 从文件内容里删掉指定行号（1 起）的那几行，其余原样。
///
/// 同样是纯函数：删错行等于改坏使用者的 shell profile。
pub fn body_without_lines(body: &str, lines: &[u32]) -> String {
    let ends_with_newline = body.ends_with('\n');
    let kept: Vec<&str> = body
        .lines()
        .enumerate()
        .filter(|(i, _)| !lines.contains(&(*i as u32 + 1)))
        .map(|(_, l)| l)
        .collect();
    let mut out = kept.join("\n");
    if ends_with_newline && !out.is_empty() {
        out.push('\n');
    }
    out
}

/// 删这个文件之前要不要先验签名。
///
/// 只有可执行文件才验 —— `.claude.json` 这种配置根本没有签名，
/// 拿「读不出签名就不删」去卡它会让配置永远清不掉。
pub fn needs_signature_check(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// 一次完全卸载的结果。**逐项报，做不到的照实说。**
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, rename = "PurgeReport")]
pub struct Report {
    pub done: Vec<String>,
    pub failed: Vec<String>,
    /// 做不到或者故意没做的，照实说。
    pub notes: Vec<String>,
    /// 执行完**复扫**一遍还剩下的。空 = 真的清干净了。
    ///
    /// 这一项是文档阶段 6 要的：不看「命令报了成功」，看重新扫一遍还剩什么。
    pub left: Vec<purge::Item>,
}

async fn run(program: &str, args: &[&str]) -> std::result::Result<String, String> {
    match crate::process::hidden_tokio(tokio::process::Command::new(program))
        .args(args)
        .output()
        .await
    {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        Ok(o) => Err(format!(
            "{}{}",
            String::from_utf8_lossy(&o.stderr).trim(),
            String::from_utf8_lossy(&o.stdout).trim()
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// 把一项真正执行掉。返回一句人话，或者失败原因。
async fn apply(item: &purge::Item, target: Target) -> std::result::Result<String, String> {
    use purge::Action;
    let subject = item.subject.clone();
    match item.action {
        Action::DeleteFile => {
            let p = PathBuf::from(&subject);
            if needs_signature_check(&p) {
                let signer = crate::signature::signer_of(&p).await;
                match crate::install::winget::signature_matches(signer.as_deref(), target.signer())
                {
                    Some(true) => {}
                    Some(false) => {
                        return Err(format!(
                            "{subject} 的签名不是 {}，不是要清的东西，没删",
                            target.signer()
                        ))
                    }
                    None => return Err(format!("{subject} 读不出签名，没验成就不删")),
                }
            }
            std::fs::remove_file(&p)
                .map(|_| format!("已删除 {subject}"))
                .map_err(|e| format!("{subject} 删不掉（多半正在运行或被占用）：{e}"))
        }
        Action::DeleteDir => std::fs::remove_dir_all(&subject)
            .map(|_| format!("已删除目录 {subject}"))
            .map_err(|e| format!("{subject} 删不掉：{e}")),
        Action::NpmUninstall => run("cmd", &["/d", "/c", "npm", "uninstall", "-g", &subject])
            .await
            .map(|_| format!("已卸载 npm 包 {subject}")),
        Action::WingetUninstall => run(
            "winget",
            &[
                "uninstall",
                "--id",
                &subject,
                "-e",
                "--silent",
                "--disable-interactivity",
                "--accept-source-agreements",
            ],
        )
        .await
        .map(|_| format!("已用 winget 卸载 {subject}")),
        Action::ScoopUninstall => run("cmd", &["/d", "/c", "scoop", "uninstall", &subject])
            .await
            .map(|_| format!("已用 scoop 卸载 {subject}")),
        Action::RegistryDelete => run(
            "reg",
            &[
                "delete",
                &format!(
                    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{subject}"
                ),
                "/f",
            ],
        )
        .await
        .map(|_| format!("已删除卸载登记 {subject}")),
        Action::EnvUnset => run(
            "reg",
            &["delete", "HKCU\\Environment", "/v", &subject, "/f"],
        )
        .await
        .map(|_| format!("已删除环境变量 {subject}")),
        Action::CredentialDelete => run("cmdkey", &[&format!("/delete:{subject}")])
            .await
            .map(|_| format!("已删除凭据条目 {subject}")),
        // Store 包：按包族名找到就卸。包族名是 `Get-AppxPackage` 自己吐出来的常量，
        // 不含引号；这里仍按 PowerShell 单引号串的规矩转义，不留拼接口子。
        Action::AppxRemove => {
            let family = subject.replace('\'', "''");
            let script = format!(
                "$ErrorActionPreference = 'Stop'; \
                 $p = @(Get-AppxPackage | Where-Object {{ $_.PackageFamilyName -eq '{family}' }}); \
                 if ($p.Count -eq 0) {{ throw 'Get-AppxPackage 里没有这个包' }}; \
                 $p | Remove-AppxPackage -ErrorAction Stop"
            );
            let out = crate::process::powershell_tokio(&script).output().await;
            match out {
                Ok(o) if o.status.success() => Ok(format!("已卸载 Store 包 {subject}")),
                Ok(o) => Err(format!(
                    "{subject} 卸不掉：{}",
                    String::from_utf8_lossy(&o.stderr).trim()
                )),
                Err(e) => Err(format!("{subject} 卸不掉（PowerShell 起不来）：{e}")),
            }
        }
        Action::StartupRemove => {
            // 别名是一个文件，计划任务要走 schtasks —— 按对象自己的形态处理。
            let p = PathBuf::from(&subject);
            if p.is_file() {
                std::fs::remove_file(&p)
                    .map(|_| format!("已删除 {subject}"))
                    .map_err(|e| format!("{subject} 删不掉：{e}"))
            } else {
                run("schtasks", &["/delete", "/tn", &subject, "/f"])
                    .await
                    .map(|_| format!("已删除计划任务 {subject}"))
            }
        }
        // 这两项要改一份**别人也在用**的东西（PATH、shell profile），
        // 所以不在这里逐项做，由 `apply_merged` 合并成一次改写 —— 逐项改会把
        // 同一个文件读写好几遍，中途失败就留下改了一半的 profile。
        //
        // ⛔ 这里**不能返回 Ok**：那等于在真正动手之前就报成功，而且同一项
        // 会被报两遍（合并那边还会再报一次）。走到这里就是调用方漏了过滤，
        // 当场报错比静默撒谎好查得多。
        Action::PathEntryRemove | Action::ShellLineRemove => Err(format!(
            "{subject} 属于合并处理的那两类，不该逐项执行（调用方漏了过滤）"
        )),
    }
}

/// PATH 与 shell profile 合并改写：**一个文件只读写一次**。
async fn apply_merged(items: &[purge::Item], rep: &mut Report) {
    use purge::Action;
    // ---- PATH：一次读、一次写。
    let drops: Vec<String> = items
        .iter()
        .filter(|i| i.action == Action::PathEntryRemove)
        .map(|i| i.subject.clone())
        .collect();
    if !drops.is_empty() {
        let cur = reg_query("HKCU\\Environment")
            .await
            .into_iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("Path"))
            .map(|(_, v)| v);
        match cur {
            Some(cur) => {
                let next = path_without(&cur, &drops);
                match run(
                    "reg",
                    &[
                        "add",
                        "HKCU\\Environment",
                        "/v",
                        "Path",
                        "/t",
                        "REG_EXPAND_SZ",
                        "/d",
                        &next,
                        "/f",
                    ],
                )
                .await
                {
                    Ok(_) => rep.done.push(format!("已从 PATH 摘掉 {} 项", drops.len())),
                    Err(e) => rep.failed.push(format!("改 PATH 失败：{e}")),
                }
            }
            None => rep.notes.push("读不到用户 PATH，这一项没动。".into()),
        }
    }

    // ---- shell profile：按文件分组，一个文件一次改写。
    let mut by_file: std::collections::BTreeMap<PathBuf, Vec<u32>> = Default::default();
    for i in items.iter().filter(|i| i.action == Action::ShellLineRemove) {
        // subject 是 `路径:行号`，行号在最后一个冒号之后。
        if let Some((file, line)) = i.subject.rsplit_once(':') {
            if let Ok(n) = line.parse::<u32>() {
                by_file.entry(PathBuf::from(file)).or_default().push(n);
            }
        }
    }
    for (file, lines) in by_file {
        match std::fs::read_to_string(&file) {
            Ok(body) => {
                let next = body_without_lines(&body, &lines);
                match std::fs::write(&file, next) {
                    Ok(()) => {
                        rep.done
                            .push(format!("已从 {} 删掉 {} 行", file.display(), lines.len()))
                    }
                    Err(e) => rep.failed.push(format!("{} 写不回去：{e}", file.display())),
                }
            }
            Err(e) => rep.failed.push(format!("{} 读不出来：{e}", file.display())),
        }
    }
}

/// **执行**一次完全卸载。调用方必须已经拿到使用者的确认（界面上要输确认词）。
///
/// 顺序是定死的：
///   1. Claude Code / 桌面端：先关掉全部 Claude —— 正在跑的 exe 删不掉；
///   2. 开维护窗口把执行锁摘掉 —— 带 Deny ACE 的文件删不掉；
///   3. 逐项执行，PATH 与 shell profile 合并成一次改写；
///   4. **复扫**，把还剩下的照实列出来。
///
/// 第 4 步是这个函数存在的意义的一半：不看命令报了什么，看重新扫一遍还剩什么。
pub async fn execute(target: Target, state: &crate::gate::GateState) -> Result<Report> {
    let plan = purge::plan(target, &collect(target).await?);
    let mut rep = Report::default();
    if plan.is_empty() {
        rep.notes.push("没扫到要清的东西。".into());
        return Ok(rep);
    }

    // 先清场。关不掉就别往下走：正在跑的 exe 删不掉，删一半比不删更糟。
    match target {
        Target::ClaudeCode => match crate::killswitch::execute().await {
            Ok(k) => rep
                .notes
                .push(format!("先关掉了 {} 个 Claude 进程。", k.killed.len())),
            Err(e) => {
                return Err(crate::error::GateError::Other(format!(
                    "没能先关闭 Claude，什么都没动：{e}"
                )));
            }
        },
        // ⛔ 只收桌面端那一侧（`Role::Desktop`：桌面端本身 + 它 Code 页拉起的会话）。
        // 0.28.0 之前这里跟 Claude Code 一样 `execute()` 全收 —— 卸个桌面端把终端里
        // 正在写的 Claude Code 会话也关了，而界面上写的是「开着的桌面端会先被关掉」。
        Target::ClaudeDesktop => match crate::killswitch::execute_desktop().await {
            Ok(k) => rep
                .notes
                .push(format!("先关掉了 {} 个桌面端进程。", k.killed.len())),
            Err(e) => {
                return Err(crate::error::GateError::Other(format!(
                    "没能先关闭 Claude 桌面端，什么都没动：{e}"
                )));
            }
        },
        // Store 包正在跑时 Remove-AppxPackage 会失败。按官方包的 exe 路径认进程，不按名字。
        Target::CodexDesktop => {
            if let Err(e) = tokio::task::spawn_blocking(crate::install::codex_desktop::close)
                .await
                .map_err(|e| crate::error::GateError::Other(e.to_string()))
                .and_then(|r| r)
            {
                return Err(crate::error::GateError::Other(format!(
                    "没能先关闭 Codex 桌面端，什么都没动：{e}"
                )));
            }
            rep.notes
                .push("先关掉了正在跑的 Codex 桌面端（如果有）。".into());
        }
        // Electron 单实例 + 托盘常驻，跑着的时候整个目录删不掉。两个产品一起收。
        Target::Antigravity => match crate::killswitch::execute_antigravity(None).await {
            Ok(k) => rep
                .notes
                .push(format!("先关掉了 {} 个反重力进程。", k.killed.len())),
            Err(e) => {
                return Err(crate::error::GateError::Other(format!(
                    "没能先关闭反重力，什么都没动：{e}"
                )));
            }
        },
        // Codex CLI 不清场（面板不按进程名杀）；Gemini CLI 是按请求起的短命进程；Chrome 走它自己的流程。
        Target::Codex | Target::GeminiCli | Target::Chrome => {}
    }

    let guard = crate::gate::Maintenance::with_unlock(state);
    for item in &plan {
        // PATH 与 shell profile 合并成一次改写（见 `apply_merged`）。
        // 在这里也报一次的话，就是**在真正动手之前先说成功**，而且同一项报两遍。
        if matches!(
            item.action,
            purge::Action::PathEntryRemove | purge::Action::ShellLineRemove
        ) {
            continue;
        }
        match apply(item, target).await {
            Ok(msg) => rep.done.push(msg),
            Err(e) => rep.failed.push(e),
        }
    }
    apply_merged(&plan, &mut rep).await;
    let done = guard.finish(state).await;
    rep.notes.push(done.detail);

    // ---- 复扫。
    rep.left = purge::plan(target, &collect(target).await?);
    if rep.left.is_empty() {
        rep.notes
            .push("复扫：没有剩下的了。重启之后建议再扫一次，确认没有自动重建。".into());
    } else {
        rep.notes.push(format!(
            "复扫：还剩 {} 处，逐条列在下面（多半是正被占用）。",
            rep.left.len()
        ));
    }
    for line in &rep.done {
        crate::audit::write(&format!("完全卸载 {}：{line}", target.label()));
    }
    for line in &rep.failed {
        crate::audit::write(&format!("完全卸载 {} 失败：{line}", target.label()));
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只认前缀。名字里带 claude 的私人变量不许动。
    #[test]
    fn only_variables_set_for_this_software_count() {
        assert!(is_owned_env("ANTHROPIC_AUTH_TOKEN", Target::ClaudeCode));
        assert!(is_owned_env("anthropic_base_url", Target::ClaudeCode));
        assert!(is_owned_env("CLAUDE_CONFIG_DIR", Target::ClaudeCode));
        assert!(!is_owned_env("MY_CLAUDE_NOTES", Target::ClaudeCode));
        assert!(!is_owned_env("PATH", Target::ClaudeCode));
        // OPENAI_* 是别的 OpenAI 工具也在读的，删它会误伤。
        assert!(!is_owned_env("OPENAI_API_KEY", Target::Codex));
        assert!(is_owned_env("CODEX_HOME", Target::Codex));
    }

    /// `~\.local\bin` 永远不摘 —— uv、pipx 也往那里装东西。
    #[test]
    fn the_shared_local_bin_is_never_removed_from_path() {
        assert!(!is_owned_path_entry(
            r"C:\Users\me\.local\bin",
            Target::ClaudeCode
        ));
        assert!(!is_owned_path_entry(
            r"C:/Users/me/.local/bin/",
            Target::ClaudeCode
        ));
        assert!(is_owned_path_entry(
            r"C:\Users\me\AppData\Local\ClaudeIpGate\apps\claude-code",
            Target::ClaudeCode
        ));
    }

    /// 提到变量名不算，真的赋值才算。
    #[test]
    fn only_real_assignments_count_as_shell_lines() {
        assert!(shell_line_sets_owned_env(
            "$env:ANTHROPIC_BASE_URL = \"https://x\"",
            Target::ClaudeCode
        ));
        assert!(shell_line_sets_owned_env(
            "export ANTHROPIC_AUTH_TOKEN=abc",
            Target::ClaudeCode
        ));
        assert!(!shell_line_sets_owned_env(
            "# ANTHROPIC_BASE_URL 以前设在这里",
            Target::ClaudeCode
        ));
        assert!(!shell_line_sets_owned_env(
            "echo $env:ANTHROPIC_BASE_URL",
            Target::ClaudeCode
        ));
    }

    #[test]
    fn credentials_are_matched_per_software() {
        assert!(is_owned_credential(
            "LegacyGeneric:target=Anthropic Claude Code",
            Target::ClaudeCode
        ));
        assert!(!is_owned_credential(
            "git:https://github.com",
            Target::ClaudeCode
        ));
        assert!(is_owned_credential("openai-codex", Target::Codex));
    }

    /// Shell 扫描的结果里**只有文件与行号**，没有行的内容。
    #[test]
    fn shell_scan_reports_positions_not_contents() {
        let t = std::env::temp_dir().join(format!("qbpurge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(t.join(".bashrc").parent().unwrap()).unwrap();
        std::fs::write(
            t.join(".bashrc"),
            "# 注释\nexport ANTHROPIC_AUTH_TOKEN=sk-secret-value\necho hi\n",
        )
        .unwrap();
        let hits = scan_shell_lines(&t, Target::ClaudeCode);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, 2, "行号从 1 数");
        let dumped = format!("{hits:?}");
        assert!(
            !dumped.contains("sk-secret-value"),
            "位置可以报，内容不许带：{dumped}"
        );
        let _ = std::fs::remove_dir_all(&t);
    }

    /// ⛔ 摘 PATH **只摘点名的那几项**，别的一个字不动。
    ///
    /// 这个函数写错的代价是使用者的 PATH 被毁掉，所以比较要能吃下
    /// 大小写、正反斜杠、结尾多一个斜杠这三种写法。
    #[test]
    fn removing_path_entries_leaves_everything_else_untouched() {
        let cur = r"C:\Windows;C:\Users\me\AppData\Local\ClaudeIpGate\apps\claude-code;C:\Users\me\.local\bin;C:\Tools";
        let next = path_without(
            cur,
            &["c:/users/me/appdata/local/claudeipgate/apps/claude-code/".to_string()],
        );
        assert_eq!(next, r"C:\Windows;C:\Users\me\.local\bin;C:\Tools");
    }

    /// 一项都没命中时**原样返回** —— 不做「顺手规整」。
    /// 顺手规整就是在动没让你动的东西：空项、结尾斜杠都可能是别人依赖的写法。
    #[test]
    fn path_is_not_silently_reformatted() {
        let cur = r"C:\Windows;;C:\Tools\";
        assert_eq!(path_without(cur, &[]), cur);
    }

    /// 删行按行号，其余原样，末尾换行保持。
    #[test]
    fn only_the_named_lines_are_removed() {
        let body = "a\nexport ANTHROPIC_AUTH_TOKEN=x\nb\n";
        assert_eq!(body_without_lines(body, &[2]), "a\nb\n");
        assert_eq!(body_without_lines(body, &[]), body);
    }

    /// 配置文件没有签名。拿「读不出签名就不删」去卡它，`.claude.json`
    /// 这类东西就永远清不掉 —— 而使用者要的是「一丝不留」。
    #[test]
    fn only_executables_need_a_signature_check() {
        assert!(needs_signature_check(Path::new(r"C:\x\claude.exe")));
        assert!(!needs_signature_check(Path::new(
            r"C:\Users\me\.claude.json"
        )));
    }
}
