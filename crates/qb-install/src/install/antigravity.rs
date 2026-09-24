//! 反重力（Google Antigravity）装在哪 —— 两个产品**唯一**的一张位置表（0.26.0）。
//!
//! 跟 `inventory`（Claude）与 `codex_desktop`（Codex）同一个道理：检测、启动、上锁、
//! 收进程都从这里拿路径，**不许在别处再拼**。
//!
//! # 两个产品
//!
//! | 产品 | 目录 | 主程序 | 真正往外发请求的进程 |
//! |---|---|---|---|
//! | Hub（[`Product::Hub`]） | `%LOCALAPPDATA%\Programs\antigravity\` | `Antigravity.exe`（Electron，electron-builder 打包） | `resources\bin\language_server.exe`（Go，由主进程拉起，它自己做 Google OAuth） |
//! | IDE（[`Product::Ide`]） | `%LOCALAPPDATA%\Programs\Antigravity IDE\` | `Antigravity IDE.exe`（VS Code 分支） | `resources\app\extensions\antigravity\bin\language_server_*.exe` |
//!
//! 2026-09-20 在实机上核过：Hub v2.15.0 的 `app.asar` → `dist/languageServer.js` 把
//! `--api_server_url` / `--cloud_code_endpoint` **写死**，OAuth 由语言服务器做，
//! 令牌在 Windows 凭据管理器（二进制里 `CredRead/CredWrite` 43 处、`oauth_creds` 0 处）——
//! 所以**没有中转路径，也做不了目录隔离的多槽位**。这两条都写在 KNOWN-ISSUES 里。
//!
//! # 每一份都要锁
//!
//! 两个产品用同一个 Google 账户往外发请求。「每一个完整可执行的副本都必须锁上」在这里的
//! 含义：主程序 + 语言服务器都锁；IDE 目录下若有别的工具留下的 `*.original.exe`
//! （本机就有：第三方汉化工具把真正的 IDE 改名成 `Antigravity IDE.original.exe`、
//! 自己顶上一层壳），那份也是完整可执行的，同样锁。**主程序不是 Claude 桌面端那种
//! `app-*` 运行时副本**：面板启动时先解锁、持租约期间它自己拉子进程都在解锁状态下，
//! 看门狗收的时候是先收进程再上锁，所以给它加 Deny 不会「开新窗口就崩」。
//!
//! ⚠ IDE 的壳 exe 不一定是 Google 签的（本机那份是第三方壳），所以**这一张表按路径认，
//! 签名只核 Hub 的主程序**（`killswitch` 的证据里 Hub 主程序要 Google LLC 签名，
//! 其余按「在这两个目录底下」认）。
//!
//! # 只读
//!
//! 这个模块只做 `exists` / `read_dir` / 读 JSON，不跑任何程序、不联网。
//! 根目录由调用方传进来，单测用临时目录搭假树 —— 单测不许碰真实的运行期状态。

use crate::config_io;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AntigravityProduct")]
#[serde(rename_all = "kebab-case")]
pub enum Product {
    Hub,
    Ide,
}

impl Product {
    pub const ALL: [Product; 2] = [Product::Hub, Product::Ide];

    pub fn key(self) -> &'static str {
        match self {
            Product::Hub => "antigravity",
            Product::Ide => "antigravity-ide",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Product::Hub => "反重力",
            Product::Ide => "反重力 IDE",
        }
    }

    /// 安装目录。`local` = `%LOCALAPPDATA%`。
    pub fn dir(self, local: &Path) -> PathBuf {
        match self {
            Product::Hub => local.join("Programs").join("antigravity"),
            Product::Ide => local.join("Programs").join("Antigravity IDE"),
        }
    }

    /// 面板拿来启动的那份主程序（在不在都返回路径）。
    ///
    /// IDE 那份就是 `Antigravity IDE.exe` —— 使用者装了第三方汉化壳的话它就是那层壳，
    /// 照样起它：壳会自己拉起 `*.original.exe`，汉化跟着走。面板不替使用者绕过他自己装的东西。
    pub fn launcher(self, local: &Path) -> PathBuf {
        match self {
            Product::Hub => self.dir(local).join("Antigravity.exe"),
            Product::Ide => self.dir(local).join("Antigravity IDE.exe"),
        }
    }

    /// 语言服务器把身份与对话放在哪：Hub 是 `~\.gemini\antigravity`（`--app_data_dir antigravity`），
    /// IDE 是 `~\.gemini\antigravity-ide`（Hub 的 `paths.js` 里 `IDE_NEW_DATA_DIR`）。
    /// **只用于显示**，面板一个字不改。
    pub fn data_dir(self, home: &Path) -> PathBuf {
        match self {
            Product::Hub => home.join(".gemini").join("antigravity"),
            Product::Ide => home.join(".gemini").join("antigravity-ide"),
        }
    }

    /// 语言服务器所在目录。
    fn language_server_dir(self, local: &Path) -> PathBuf {
        match self {
            Product::Hub => self.dir(local).join("resources").join("bin"),
            Product::Ide => self
                .dir(local)
                .join("resources")
                .join("app")
                .join("extensions")
                .join("antigravity")
                .join("bin"),
        }
    }
}

/// 一份要上锁的可执行文件，以及它属于哪个产品。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    pub product: Product,
    pub path: PathBuf,
}

/// 目录里所有以 `.exe` 结尾（不分大小写）的文件。
fn exes_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
        })
        .collect();
    out.sort();
    out
}

/// 一个产品名下的**卸载器**。它不是客户端副本，锁它没有意义，反而会让「卸载」也点不动。
fn is_uninstaller(p: &Path) -> bool {
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with("uninstall") || name.starts_with("unins")
}

/// 两个产品下**此刻存在**的、要上锁的可执行文件。
///
/// 规则：安装目录顶层的每个 `*.exe`（主程序、别的工具留下的 `*.original.exe`；
/// 卸载器除外）+ 语言服务器目录下的每个 `*.exe`。顶层枚举而不是拼死名字，
/// 是为了把「壳 + original」这种布局也收进来 —— 漏一份就是一条现成的绕过口。
pub fn executables(local: &Path) -> Vec<Executable> {
    let mut out = Vec::new();
    for product in Product::ALL {
        let dir = product.dir(local);
        if !dir.is_dir() {
            continue;
        }
        for p in exes_in(&dir) {
            if !is_uninstaller(&p) {
                out.push(Executable { product, path: p });
            }
        }
        for p in exes_in(&product.language_server_dir(local)) {
            out.push(Executable { product, path: p });
        }
    }
    out
}

/// 只要路径。给门禁上锁清单用。
pub fn lockable_paths(local: &Path) -> Vec<PathBuf> {
    executables(local).into_iter().map(|e| e.path).collect()
}

/// 这个产品的主程序在不在。**装完之后回读核对用的就是它**。
///
/// 只问「主程序这个文件存在吗」，不问版本、不跑进程 —— 版本读不出来是显示问题，
/// 文件不在才是「没装上」。这两件事分开，是 §7.20 那条教训
/// （「读不出版本」被当成了「没装过」）的另一半。
pub fn product_installed(product: Product) -> bool {
    let local = dirs::data_local_dir().unwrap_or_default();
    product.launcher(&local).is_file()
}

/// 这条路径在不在某个产品的安装目录底下（大小写与斜杠不敏感）。
///
/// 一键关闭 / 看门狗拿它当证据：在这两个目录下的进程就是反重力的进程。
/// 不按进程名认 —— 叫 `language_server.exe` 的东西别的 IDE 也有。
pub fn product_of(path: &Path, local: &Path) -> Option<Product> {
    let p = super::inventory::norm(path);
    Product::ALL.into_iter().find(|product| {
        let mut d = super::inventory::norm(&product.dir(local));
        if !d.ends_with('\\') {
            d.push('\\');
        }
        p.starts_with(&d)
    })
}

/// IDE 的 `product.json`。联网额度的请求形状照官方 IDE，版本号从这里读（2026-09-23）。
pub fn ide_product_json(local: &Path) -> PathBuf {
    Product::Ide
        .dir(local)
        .join("resources")
        .join("app")
        .join("product.json")
}

/// 本机装的反重力 IDE 版本（`product.json` 的 `ideVersion`，没有再退到 `version`）。
///
/// 读不到就是 `None` —— 只影响请求里那个版本字段，不影响任何锁。
/// 跟 [`crate::install::detect::antigravity`] 读的 exe 版本资源不是一回事：那个给软件页显示，
/// 这个是 IDE 自己跟 Google 说话时报的版本。
pub fn ide_version(local: &Path) -> Option<String> {
    let bytes = config_io::read_optional(&ide_product_json(local)).ok()??;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    ["ideVersion", "version"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// 反重力自带的 Google OAuth 客户端标识在哪些文件里，按先后试（2026-09-23）。
///
/// 使用者拍板「令牌过期时面板在内存里换新」，而换新要带上**签发这个令牌的那个客户端**
/// 的标识。面板不内置它（仓库里一个字都不出现），用的时候从本机装的反重力里读：
///
/// 1. IDE 的 Electron 主进程脚本 `resources\app\out\main.js`（IDE 2.5.5 上核过：里面有两组）；
/// 2. 两个产品的语言服务器（Hub 的 OAuth 是它做的）。二进制大，排在后面，读一次就够。
///
/// 只给**此刻存在**的路径，不读内容 —— 扫描在 `qb-app::usecase::antigravity_quota`。
pub fn oauth_client_sources(local: &Path) -> Vec<PathBuf> {
    let mut out = vec![Product::Ide
        .dir(local)
        .join("resources")
        .join("app")
        .join("out")
        .join("main.js")];
    for product in [Product::Hub, Product::Ide] {
        out.extend(exes_in(&product.language_server_dir(local)));
    }
    out.retain(|p| p.is_file());
    out
}

/// Electron 用户数据目录：Hub 是 `%APPDATA%\Antigravity`，IDE 是 `%APPDATA%\Antigravity IDE`
/// （2026-09-20 实机核过两个目录都在）。
/// `DevToolsActivePort`、`app_storage.json`、`logs\` 都在下面。
///
/// ⚠ `app_storage.json` 与语言服务器日志是 **Hub 自己的**概念（IDE 那份 VS Code 分支不写），
/// 所以下面两个函数内部固定传 [`Product::Hub`]，别跟着带产品参数。
pub fn user_data_dir(product: Product, roaming: &Path) -> PathBuf {
    match product {
        Product::Hub => roaming.join("Antigravity"),
        Product::Ide => roaming.join("Antigravity IDE"),
    }
}

/// Chromium 写的调试端口文件。第一行是端口，第二行是浏览器级 WebSocket 路径。
///
/// Hub 的 `main.js` 没传 `--remote-debugging-port` 时自己补 `0`（随机端口），
/// 所以 Hub 这个文件**总会有** —— 汉化引擎从这里读端口，不用像 EasyAG 那样写死 9333。
///
/// ⛔ **IDE 那份不一样**：VS Code 分支默认**不开**调试端口，只有面板起它的时候补上
/// `--remote-debugging-port=0`（`qb-platform::sessions::desktop_arguments`）才会有这个文件。
/// 使用者自己从开始菜单起的 IDE 没有端口，汉化引擎附不上去 —— 界面上要如实这么说，
/// 别让人对着一个空状态猜。
pub fn devtools_port_file(product: Product, roaming: &Path) -> PathBuf {
    user_data_dir(product, roaming).join("DevToolsActivePort")
}

/// 语言服务器的日志。里面「Auth succeeded」那一行是**零凭证**的登录态信号。
pub fn language_server_log(roaming: &Path) -> PathBuf {
    user_data_dir(Product::Hub, roaming)
        .join("logs")
        .join("language_server.log")
}

/// Hub 的键值存储（`StorageManager`，扁平 JSON，值都是字符串）。
pub fn app_storage_path(roaming: &Path) -> PathBuf {
    user_data_dir(Product::Hub, roaming).join("app_storage.json")
}

/// `autoCheckForUpdates` 这个键。Hub 自己的 `SettingKey.AUTO_CHECK_FOR_UPDATES`。
pub const AUTO_UPDATE_KEY: &str = "autoCheckForUpdates";

/// 读 Hub 的「自动检查更新」开关。键不在 = Hub 的默认值 `true`。
///
/// 文件不在或坏了回 `None`（不知道），别把「读不出来」显示成「关着」。
pub fn auto_update_enabled(roaming: &Path) -> Option<bool> {
    let bytes = config_io::read_optional(&app_storage_path(roaming)).ok()??;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let obj = v.as_object()?;
    Some(!matches!(
        obj.get(AUTO_UPDATE_KEY).and_then(|x| x.as_str()),
        Some("false")
    ))
}

/// 把「自动检查更新」写进 Hub 的键值存储。**只并入这一个键，其余原样保留。**
///
/// # 为什么面板要管这个
///
/// electron-updater 在 Hub 退出时静默装新版本，新 exe 没有 Deny ACE，
/// 到下一次上锁之前是一份能绕过门禁的完整副本（跟 Claude Code 的
/// `DISABLE_AUTOUPDATER` 是同一个理由）；而且汉化字典是按界面版本配的，
/// 悄悄升级会让字典对不上。所以软件页给一个开关。
///
/// # 只并入
///
/// 这个文件还装着使用者的窗口布局、置顶对话顺序那些 —— 整份写会把它们抹掉。
/// 走 `config_io::commit`：两阶段提交，期间被外部改过就拒绝。
///
/// Hub **只在启动时读它**（运行中改要重启才生效），界面上要写明。
pub fn set_auto_update(roaming: &Path, enabled: bool) -> Result<()> {
    let path = app_storage_path(roaming);
    let expected = config_io::read_optional(&path)?;
    let mut root = config_io::parse_object(expected.as_deref())?;
    let obj = root.as_object_mut().ok_or_else(|| {
        crate::error::GateError::Other("反重力的 app_storage.json 不是对象".into())
    })?;
    // Hub 的 StorageManager 把值一律存成字符串（`String(value)`），照它的样子写。
    obj.insert(
        AUTO_UPDATE_KEY.into(),
        serde_json::Value::String(if enabled { "true" } else { "false" }.into()),
    );
    config_io::commit(vec![config_io::Edit {
        path,
        expected,
        body: Some(serde_json::to_vec_pretty(&root)?),
    }])?;
    Ok(())
}

/// 语言服务器日志里有没有「登录成功」这一行。**只读日志，不碰凭据。**
///
/// `None` = 日志不在（还没起过 / 目录不对）；`Some(false)` = 有日志但没见到那一行
/// （没登录，或者版本换了措辞 —— 所以界面上只能说「未见登录记录」，不能说「未登录」）。
pub fn login_seen_in_log(roaming: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(language_server_log(roaming)).ok()?;
    Some(text.contains("Auth succeeded"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (tempdir::Dir, PathBuf) {
        let t = tempdir::Dir::new("qb-antigravity");
        let local = t.path().join("Local");
        (t, local)
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// 两个产品的主程序、语言服务器、别的工具留下的 `*.original.exe` 全都要在清单里；
    /// 卸载器不在。
    #[test]
    fn every_executable_copy_is_listed_and_the_uninstaller_is_not() {
        let (_t, local) = tree();
        let hub = Product::Hub.dir(&local);
        touch(&hub.join("Antigravity.exe"));
        touch(&hub.join("Uninstall Antigravity.exe"));
        touch(
            &hub.join("resources")
                .join("bin")
                .join("language_server.exe"),
        );
        touch(&hub.join("resources").join("bin").join("webm_encoder.exe"));
        let ide = Product::Ide.dir(&local);
        touch(&ide.join("Antigravity IDE.exe"));
        touch(&ide.join("Antigravity IDE.original.exe"));
        touch(&ide.join("unins000.exe"));
        touch(
            &ide.join("resources")
                .join("app")
                .join("extensions")
                .join("antigravity")
                .join("bin")
                .join("language_server_windows_x64.exe"),
        );

        let got = lockable_paths(&local);
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        for want in [
            "Antigravity.exe",
            "language_server.exe",
            "Antigravity IDE.exe",
            "Antigravity IDE.original.exe",
            "language_server_windows_x64.exe",
        ] {
            assert!(names.iter().any(|n| n == want), "漏了 {want}：{names:?}");
        }
        assert!(
            !names
                .iter()
                .any(|n| n.starts_with("Uninstall") || n.starts_with("unins")),
            "卸载器不该上锁：{names:?}"
        );
        // 语言服务器目录里的别的工具（录屏编码器）是完整可执行文件，照样在清单里 ——
        // 宁可多锁一个无害的，也不漏。
        assert!(names.iter().any(|n| n == "webm_encoder.exe"));
    }

    #[test]
    fn nothing_installed_means_an_empty_list() {
        let (_t, local) = tree();
        assert!(executables(&local).is_empty());
    }

    #[test]
    fn product_is_recognised_by_directory_not_by_name() {
        let local = Path::new(r"C:\Users\me\AppData\Local");
        assert_eq!(
            product_of(
                Path::new(
                    r"C:\Users\me\AppData\Local\Programs\antigravity\resources\bin\language_server.exe"
                ),
                local
            ),
            Some(Product::Hub)
        );
        assert_eq!(
            product_of(
                Path::new(
                    r"c:/users/me/appdata/local/programs/antigravity ide/Antigravity IDE.original.exe"
                ),
                local
            ),
            Some(Product::Ide)
        );
        // 别的 IDE 也有叫 language_server.exe 的东西，不在这两个目录下就不算。
        assert_eq!(
            product_of(
                Path::new(r"C:\Users\me\AppData\Local\Programs\Windsurf\language_server.exe"),
                local
            ),
            None
        );
        // 前缀相近的目录不算（`antigravity` ≠ `antigravity-foo`）。
        assert_eq!(
            product_of(
                Path::new(r"C:\Users\me\AppData\Local\Programs\antigravity-foo\Antigravity.exe"),
                local
            ),
            None
        );
    }

    /// 写自动更新开关只并入一个键，别的键一个都不许丢。
    #[test]
    fn setting_auto_update_keeps_the_other_keys() {
        let (_t, local) = tree();
        let roaming = local.join("Roaming");
        let p = app_storage_path(&roaming);
        touch(&p);
        std::fs::write(
            &p,
            br#"{"ide-install-wizard-shown":"true","auxPaneWidth":"320"}"#,
        )
        .unwrap();
        assert_eq!(
            auto_update_enabled(&roaming),
            Some(true),
            "键不在 = Hub 默认开"
        );
        set_auto_update(&roaming, false).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(v["autoCheckForUpdates"], "false", "{v}");
        assert_eq!(v["ide-install-wizard-shown"], "true", "别的键丢了：{v}");
        assert_eq!(v["auxPaneWidth"], "320");
        assert_eq!(auto_update_enabled(&roaming), Some(false));
        set_auto_update(&roaming, true).unwrap();
        assert_eq!(auto_update_enabled(&roaming), Some(true));
    }

    #[test]
    fn a_missing_storage_file_reads_as_unknown_not_off() {
        let (_t, local) = tree();
        assert_eq!(auto_update_enabled(&local.join("nope")), None);
    }

    #[test]
    fn paths_have_the_shapes_the_hub_uses() {
        let home = Path::new(r"C:\Users\me");
        let roaming = Path::new(r"C:\Users\me\AppData\Roaming");
        assert!(Product::Hub
            .data_dir(home)
            .ends_with(r".gemini\antigravity"));
        assert!(Product::Ide
            .data_dir(home)
            .ends_with(r".gemini\antigravity-ide"));
        assert!(
            devtools_port_file(Product::Hub, roaming).ends_with(r"Antigravity\DevToolsActivePort")
        );
        // IDE 的 Electron 用户数据目录是另一个（带空格的那个），不是 Hub 那份。
        assert!(devtools_port_file(Product::Ide, roaming)
            .ends_with(r"Antigravity IDE\DevToolsActivePort"));
        assert!(language_server_log(roaming).ends_with(r"Antigravity\logs\language_server.log"));
        assert!(app_storage_path(roaming).ends_with(r"Antigravity\app_storage.json"));
    }

    #[test]
    fn the_ide_version_prefers_ide_version_over_the_vscode_base_version() {
        let (_t, local) = tree();
        assert_eq!(ide_version(&local), None, "没装就是不知道");
        let p = ide_product_json(&local);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, br#"{"version":"1.104.0","ideVersion":"2.5.5"}"#).unwrap();
        assert_eq!(ide_version(&local).as_deref(), Some("2.5.5"));
        std::fs::write(&p, br#"{"version":"1.104.0"}"#).unwrap();
        assert_eq!(ide_version(&local).as_deref(), Some("1.104.0"));
    }

    #[test]
    fn oauth_client_sources_list_the_ide_script_first_and_only_what_exists() {
        let (_t, local) = tree();
        assert!(oauth_client_sources(&local).is_empty());
        let hub_ls = Product::Hub
            .dir(&local)
            .join("resources")
            .join("bin")
            .join("language_server.exe");
        touch(&hub_ls);
        let main_js = Product::Ide
            .dir(&local)
            .join("resources")
            .join("app")
            .join("out")
            .join("main.js");
        touch(&main_js);
        let got = oauth_client_sources(&local);
        assert_eq!(got, vec![main_js, hub_ls], "脚本小、在前；二进制大、在后");
    }

    /// 单测里的临时目录。测完删掉；只在系统临时目录下，绝不碰运行期状态。
    mod tempdir {
        use std::path::{Path, PathBuf};
        pub struct Dir(PathBuf);
        impl Dir {
            pub fn new(tag: &str) -> Self {
                let p = std::env::temp_dir().join(format!(
                    "{tag}-{}-{}",
                    std::process::id(),
                    crate::config_io::id()
                ));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
