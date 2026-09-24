//! 反重力一键安装：A 路 winget，没成走 B 路从 Google 自己的域直下官方安装器（0.29.0）。
//!
//! 0.28.0 之前软件页那张卡只有一个官网链接 —— 面板能检测反重力、能按槽位起、能上锁、
//! 能挂看门狗、能完全卸载，**唯独装不了**。这个模块补的就是那一格。
//!
//! # 「面板不分发 Google 的安装包」这条没有被推翻
//!
//! 变的是「给个链接让人自己去点」→「替人从**同一个官方地址**把**同一个官方安装器**取回来，
//! 核完签名再交给它自己安装」。面板不托管、不镜像、不改包、不重新打包，装出来的那一份
//! 跟使用者自己去官网点下载得到的是同一个文件。跟 0.28.0 Codex 桌面端直装是同一条边界。
//!
//! # 两条路
//!
//! | 路 | 做法 | 什么时候 |
//! |---|---|---|
//! | A · winget | `winget install --id Google.Antigravity`（IDE 是 `Google.AntigravityIDE`）`-e --silent …` | 有 winget 且源里有 |
//! | B · 直下 | 读 `https://antigravity.google/download` → 挑本产品本架构那条 → 域限定 → 下载 → 核 Authenticode 主体含 Google → 按安装器类型静默安装 | A 没成，或 winget 源落后 |
//! | 本地 | 使用者自己给 `.exe` → 只核签名 → 静默安装 | 上面都拿不到时 |
//!
//! **A 路不是「更好」，只是「更省事」。** 2026-09-20 实测 winget 源上 Hub 是 2.12.2、
//! 官网是 2.15.1 —— 落后两个版本。所以 A 路成功之后仍然回读检测，版本落后不算失败
//! （装上就能自己更新），但会如实写进日志。
//!
//! # ⛔ B 路没有官方公布的哈希，闸门是签名
//!
//! `codex_store` 那条链的 SHA-256 来自微软 FE3 的清单（TLS 上拿的），这里没有对应物：
//! 下载页只给地址，不给哈希。所以完整性靠三件事：
//!
//! 1. **地址从 HTTPS 页面上读**（不是写死的、也不是第三方镜像）；
//! 2. **域限定**（[`allowed_host`]）—— 只认 `storage.googleapis.com` / `edgedl.me.gvt1.com`
//!    / `dl.google.com`。页面被改也带不到别的地方去；
//! 3. **Authenticode 主体必须含 Google**，读不出 = 没验成 = 不装（同
//!    `codex_store::require_openai_signature` 那条三态）。
//!
//! 实际算出来的 SHA-256 记进安装日志，只作留痕，**不作判据** —— 没有可信的对照值时，
//! 拿它当判据就是自己跟自己比。别在这里编一个「预期哈希」常量：版本一变就过期，
//! 而过期的形态是「所有人都装不了」。
//!
//! # ⛔ 单测不联网
//!
//! [`pick_download`] 是纯函数，拿录下来的页面形状测。页面改版时先跑
//! `cargo run -p qb-install --example antigravity-resolve -- locate` 看清楚再改解析器。

use super::antigravity::Product;
use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use std::path::{Path, PathBuf};

/// 六段。前端进度条按这个总数画。
pub const TOTAL: u32 = 6;

/// 官方下载页。**这是唯一的地址来源**，别在别处再写一份。
pub const DOWNLOAD_PAGE: &str = "https://antigravity.google/download";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    /// 本机架构。64 位面板在 ARM64 上跑在模拟层里，进程自己看到的
    /// `PROCESSOR_ARCHITECTURE` 可能是 AMD64，所以先看 `PROCESSOR_ARCHITEW6432`。
    pub fn current() -> Arch {
        let arm = ["PROCESSOR_ARCHITEW6432", "PROCESSOR_ARCHITECTURE"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .any(|v| v.eq_ignore_ascii_case("ARM64"));
        if arm {
            Arch::Arm64
        } else {
            Arch::X64
        }
    }
}

/// 下载地址里的平台目录名。
///
/// ⚠ **两个产品的 ARM 目录名不一样**：Hub 是 `windows-arm`，IDE 是 `windows-arm64`
/// （2026-09-20 在下载页上逐条核过）。合并成一个常量会让 ARM64 机器上的 Hub 永远找不到。
fn platform_dir(product: Product, arch: Arch) -> &'static str {
    match (product, arch) {
        (_, Arch::X64) => "windows-x64",
        (Product::Hub, Arch::Arm64) => "windows-arm",
        (Product::Ide, Arch::Arm64) => "windows-arm64",
    }
}

/// 这个产品的下载地址长什么样（用来在页面里认出它）。
fn url_marker(product: Product) -> &'static str {
    match product {
        Product::Hub => "/antigravity-public/antigravity-hub/",
        Product::Ide => "/antigravity/stable/",
    }
}

/// winget 上的包 id。
pub fn winget_id(product: Product) -> &'static str {
    match product {
        Product::Hub => "Google.Antigravity",
        Product::Ide => "Google.AntigravityIDE",
    }
}

/// 静默安装的开关。
///
/// Hub 是 NSIS（winget 清单里 `InstallerType: nullsoft`），IDE 是 Inno（`InstallerType: inno`,
/// `Scope: User`, 自定义开关 `/mergetasks=!runcode`）—— 2026-09-20 从 winget-pkgs 的清单上读的。
///
/// ⚠ `/mergetasks=!runcode` 是**不要装完就启动**。少了它，静默安装会在后台把 IDE 拉起来，
/// 而面板紧接着要上锁 —— 上锁时它正跑着，于是「装完之后门禁没生效」而没人说得清为什么。
fn silent_args(product: Product) -> &'static [&'static str] {
    match product {
        Product::Hub => &["/S"],
        Product::Ide => &[
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/SP-",
            "/mergetasks=!runcode",
        ],
    }
}

/// 只认 Google 自己的分发域。页面被改也带不到别处去。
pub fn allowed_host(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        // ⛔ 明文不收。`codex_store` 收 `http://` 是因为那个 CDN 主机名没配证书、
        // 而完整性由 FE3 清单里的 SHA-256 兜着；这里没有那个对照值，只剩 TLS 和签名。
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host == "storage.googleapis.com"
        || host == "edgedl.me.gvt1.com"
        || host == "dl.google.com"
        || host.ends_with(".dl.google.com")
}

/// 页面上挑出来的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub version: String,
    pub url: String,
}

/// 版本号按段比大小。`2.15.1` 要排在 `2.5.5` 前面 —— 按字符串比会反过来。
fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// 从下载页的 HTML 里挑出某个产品、某个架构的 Windows 安装器地址与版本号。
///
/// 地址形状（2026-09-20 实访录下来的）：
///
/// ```text
/// https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-5880727900913664/windows-x64/Antigravity-x64.exe
/// https://edgedl.me.gvt1.com/edgedl/release2/j0qc3/antigravity/stable/2.5.5-4923483625488384/windows-x64/Antigravity%20IDE.exe
/// ```
///
/// 版本号是平台目录**上一段**里第一个 `-` 之前的部分（后面那串是构建号）。
/// 页面上同时挂着 mac / linux 的包，所以必须按平台目录过滤 —— 只按 `.exe` 结尾过滤的话，
/// ARM64 机器上会挑到 x64 那条，而症状是「装上了但起不来」。
///
/// 页面上出现多个版本时取**最大的那个**，不取第一个：顺序是页面的排版决定的，
/// 上游调一次顺序就会悄悄装成旧版（同 `pricing` 那条「靠表格顺序的运气」）。
pub fn pick_download(html: &str, product: Product, arch: Arch) -> Option<Resolved> {
    let marker = url_marker(product);
    let plat = format!("/{}/", platform_dir(product, arch));
    let mut best: Option<Resolved> = None;
    for raw in html.split(|c: char| c == '"' || c == '\'' || c == '\\' || c.is_whitespace()) {
        let url = raw.trim_end_matches(&[',', ')', ';', '<', '>'][..]);
        if !url.starts_with("https://") || !url.contains(marker) || !url.contains(&plat) {
            continue;
        }
        if !url.to_ascii_lowercase().ends_with(".exe") || !allowed_host(url) {
            continue;
        }
        // …/<版本>-<构建号>/<平台>/<文件名>
        let Some(before_plat) = url.split(&plat).next() else {
            continue;
        };
        let Some(seg) = before_plat.rsplit('/').next() else {
            continue;
        };
        let version = seg.split('-').next().unwrap_or("").to_string();
        if version.is_empty() || !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let candidate = Resolved {
            version,
            url: url.to_string(),
        };
        let better = match &best {
            None => true,
            Some(b) => version_key(&candidate.version) > version_key(&b.version),
        };
        if better {
            best = Some(candidate);
        }
    }
    best
}

/// 这堆字节是不是 gzip（magic `1f 8b`）。
fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

/// 把下载页抓回来（纯文本）。
///
/// # ⛔ 这里必须自己解 gzip
///
/// 面板的 reqwest **没开 `gzip` / `brotli` 特性**（见根 `Cargo.toml`），所以它不解压。
/// 而这个站点**不管你发什么 `Accept-Encoding` 都压着发** —— 2026-09-20 实测，
/// 连显式 `Accept-Encoding: identity` 回的也是 `content-encoding: gzip`、36,137 字节。
///
/// 症状极具欺骗性：**请求 200、`Content-Type: text/html`、`.text()` 也给得出一个
/// 六万多字符的字符串**，只是那串东西是 gzip 字节被 UTF-8 有损解码之后的替换字符，
/// 里面一条地址都没有。第一版代码就是这么报的：「页面可能改版了」—— 一条会把下一个人
/// 直接带去改解析器的错误结论。跟坑 7.49 同一族：**拿到的不是你以为的那个东西，
/// 而没有任何一层报错。**
///
/// 为什么不给 workspace 的 reqwest 开 `gzip` 特性：那会让**每一个**客户端都自动加
/// `Accept-Encoding: gzip` 并透明解压，其中包括本机路由那条要求逐字节流式转发的上游
/// 连接。为一个下载页去改 SSE 代理的行为，代价和收益完全不成比例（同坑 7.52 的取舍）。
pub async fn fetch_page() -> Result<String> {
    use std::io::Read;

    let bytes = super::managed::http()?
        .get(DOWNLOAD_PAGE)
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .map_err(|e| GateError::Other(format!("打不开反重力下载页：{e}")))?
        .error_for_status()
        .map_err(|e| GateError::Other(format!("反重力下载页返回错误：{e}")))?
        .bytes()
        .await
        .map_err(|e| GateError::Other(format!("读反重力下载页失败：{e}")))?;

    // 按字节认，不按响应头认：头可以缺、可以撒谎，magic 不会。
    let body = if is_gzip(&bytes) {
        let mut out = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut out)
            .map_err(|e| GateError::Other(format!("反重力下载页解 gzip 失败：{e}")))?;
        out
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };

    // 拿到的不是 HTML 就直说，别把一堆二进制交给解析器。
    if !body.contains('<') || body.contains('\u{fffd}') {
        return Err(GateError::Other(format!(
            "反重力下载页拿回来的不是可读的 HTML（{} 字节）。\
             大概率是又换了一种压缩（brotli / zstd）—— 先跑 \
             `cargo run -p qb-install --example antigravity-resolve -- dump` 看清楚。",
            bytes.len()
        )));
    }
    Ok(body)
}

/// 抓下载页、挑地址。**只读元数据，不下载不安装。**
pub async fn resolve(product: Product, arch: Arch) -> Result<Resolved> {
    let body = fetch_page().await?;
    pick_download(&body, product, arch).ok_or_else(|| {
        GateError::Other(format!(
            "反重力下载页上没找到 {} 的 {} 安装包。页面可能改版了 —— \
             先跑 `cargo run -p qb-install --example antigravity-resolve -- dump` 看清楚再改解析器。",
            product.label(),
            platform_dir(product, arch)
        ))
    })
}

/// 安装包落在托管目录下自己的子目录里，跟 Codex 桌面端一个形状。
pub fn download_dir(product: Product) -> PathBuf {
    super::managed::root().join(product.key()).join(".download")
}

/// 签名主体必须含 Google。三态：对 / 不对 / 读不出 —— 后两种都不装，但报的话不一样。
///
/// ⚠ 核的是**下载回来的官方安装器**，不是磁盘上装好的那些 exe。装好的 IDE 主程序
/// 可能是第三方汉化壳签的（本机就是），所以位置表那边按路径认、不按签名认。
/// 这里核的是「我从网上拿回来的这个文件是不是 Google 发的」，两件事不能混。
async fn require_google_signature(exe: &Path, log: &mut Vec<String>) -> Result<()> {
    let signer = crate::signature::signer_of(exe).await;
    match super::winget::signature_matches(signer.as_deref(), "google") {
        Some(true) => {
            log.push(format!("签名主体：{}", signer.unwrap_or_default()));
            Ok(())
        }
        Some(false) => Err(GateError::Other(format!(
            "拒绝安装：安装包的签名主体里没有 Google（读到的是 {}）。",
            signer.unwrap_or_default()
        ))),
        None => Err(GateError::Other(
            "拒绝安装：读不出安装包的数字签名 —— 这只能说「没验成」，\
             面板不会把没验成的安装器跑起来。"
                .into(),
        )),
    }
}

/// 跑官方安装器（静默）。返回它说了什么，**成没成由调用方回读检测决定**。
#[cfg(windows)]
async fn run_installer(
    product: Product,
    exe: &Path,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<()> {
    let args: Vec<String> = silent_args(product)
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    log.push(format!(
        "跑官方安装器：{} {}",
        exe.display(),
        args.join(" ")
    ));
    let ok = super::winget::run_streaming(&exe.display().to_string(), &args, rep, 6, log).await?;
    if !ok {
        log.push("安装器的退出码不是 0（先记下来，装没装上看回读）".into());
    }
    Ok(())
}

#[cfg(not(windows))]
async fn run_installer(
    _product: Product,
    _exe: &Path,
    _rep: &dyn ProgressSink,
    _log: &mut Vec<String>,
) -> Result<()> {
    Err(GateError::Other("只在 Windows 上可用".into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    Winget,
    Direct,
    LocalFile,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Outcome {
    pub product: Product,
    pub method: Method,
    /// 装完 `detect::antigravity()` 读回来的版本。读不出来就是 `None`（不编）。
    pub version: Option<String>,
    pub log: Vec<String>,
}

/// B 路：下载 + 核签名 + 静默安装。
async fn install_direct(
    product: Product,
    arch: Arch,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<()> {
    rep.phase(2, "查官网上的最新版本");
    let found = resolve(product, arch).await?;
    log.push(format!("官网最新版 {}（{}）", found.version, found.url));
    rep.log(2, &format!("官网最新版 {}", found.version));

    let dir = download_dir(product);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let setup = dir.join(format!("{}-{}-setup.exe", product.key(), found.version));

    rep.phase(4, &format!("下载 {} {}", product.label(), found.version));
    let sha = super::managed::download(&found.url, &setup, None, rep, 4).await?;
    // 留痕，不作判据 —— 官网不公布哈希，没有可信的对照值（模块头说明过）。
    log.push(format!("下载完成，SHA-256：{sha}"));

    rep.phase(5, "核对数字签名");
    if let Err(e) = require_google_signature(&setup, log).await {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }

    rep.phase(6, "跑官方安装器（静默）");
    match run_installer(product, &setup, rep, log).await {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&dir);
            Ok(())
        }
        Err(e) => Err(GateError::Other(format!(
            "{e} 已核对过签名的安装包留在 {}，可以自己双击装。",
            setup.display()
        ))),
    }
}

/// 使用者自己给的安装器：只核签名，然后静默安装。
async fn install_local(
    product: Product,
    setup: &Path,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> Result<()> {
    if !setup.is_file() {
        return Err(GateError::Other(format!("找不到 {}", setup.display())));
    }
    rep.phase(5, "核对数字签名");
    require_google_signature(setup, log).await?;
    rep.phase(6, "跑官方安装器（静默）");
    run_installer(product, setup, rep, log).await
}

/// 装（或更新）一个反重力产品。**调用方先关掉正在跑的那份、开好维护窗口。**
///
/// 顺序：盘点现状 → A 路 winget → 没成走 B 路直下 → 回读 `detect::antigravity()` 核对，
/// **不看安装器的退出码**（§7.21：官方安装器换不动文件也会打印 successfully installed）。
/// `local` 给了就只装那一份（跳过 A / B）。
pub async fn install(
    product: Product,
    local: Option<&Path>,
    force: bool,
    rep: &dyn ProgressSink,
) -> Result<Outcome> {
    let mut log = Vec::new();
    let arch = Arch::current();

    rep.phase(1, "盘点现状");
    let before = current_version(product).await;
    match &before {
        Some(v) => log.push(format!("已装 {} {v}", product.label())),
        None => log.push(format!("本机没有{}", product.label())),
    }

    let method = if let Some(p) = local {
        install_local(product, p, rep, &mut log).await?;
        Method::LocalFile
    } else {
        rep.phase(3, "试 winget");
        let winget_ok = try_winget(product, force, rep, &mut log).await;
        if winget_ok {
            Method::Winget
        } else {
            log.push("winget 这条路没走通，改从官网直下".into());
            install_direct(product, arch, rep, &mut log).await?;
            Method::Direct
        }
    };

    // 回读核对。装没装上看盘，不看谁的退出码。
    let after = current_version(product).await;
    let installed = super::antigravity::product_installed(product);
    if !installed {
        return Err(GateError::Other(format!(
            "{} 的安装流程跑完了，但检测仍然找不到它的主程序（{}）。没有装上。",
            product.label(),
            product.launcher(&local_dir()).display()
        )));
    }
    match (&before, &after) {
        (Some(a), Some(b)) if a == b && !force => {
            log.push(format!("版本没变（仍是 {b}）—— 本来就是最新的"))
        }
        (_, Some(b)) => log.push(format!("回读核对：现在是 {b}")),
        (_, None) => log.push("回读核对：主程序在，但版本号读不出".into()),
    }

    Ok(Outcome {
        product,
        method,
        version: after,
        log,
    })
}

/// A 路。**起不来就是没试过**，跟「试了但失败」要分开记 —— 前者要继续走 B 路，
/// 后者也要（winget 源里没有这个包也算），但日志上是两句不同的话。
async fn try_winget(
    product: Product,
    force: bool,
    rep: &dyn ProgressSink,
    log: &mut Vec<String>,
) -> bool {
    let probe = super::winget::probe().await;
    if !probe.winget_available {
        log.push("本机没有 winget，跳过 A 路".into());
        rep.log(3, "本机没有 winget，改走官网直下");
        return false;
    }
    let mut args: Vec<String> = [
        "install",
        "--id",
        winget_id(product),
        "-e",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    if force {
        args.push("--force".into());
    }
    log.push(format!("winget install --id {}", winget_id(product)));
    match super::winget::run_streaming("winget", &args, rep, 3, log).await {
        Ok(true) => true,
        Ok(false) => {
            log.push("winget 返回了非零退出码".into());
            false
        }
        Err(e) => {
            log.push(format!("winget 起不来：{e}"));
            false
        }
    }
}

fn local_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default()
}

/// 这个产品现在装的是哪一版（读主程序的 `VersionInfo.ProductVersion`）。
async fn current_version(product: Product) -> Option<String> {
    let (hub, ide) = super::detect::antigravity().await;
    match product {
        Product::Hub => hub.version,
        Product::Ide => ide.version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-20 从 `https://antigravity.google/download` 上实访录下来的地址形状。
    /// 页面改版时更新这一段，别改断言去将就解析器。
    const PAGE: &str = r#"
      <a href="https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-5880727900913664/darwin-arm/Antigravity.dmg">mac</a>
      <a href="https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-5880727900913664/linux-x64/Antigravity.tar.gz">linux</a>
      <a href="https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-5880727900913664/windows-x64/Antigravity-x64.exe">win</a>
      <a href="https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-5880727900913664/windows-arm/Antigravity-arm64.exe">win arm</a>
      <a href="https://edgedl.me.gvt1.com/edgedl/release2/j0qc3/antigravity/stable/2.5.5-4923483625488384/windows-x64/Antigravity%20IDE.exe">ide</a>
      <a href="https://edgedl.me.gvt1.com/edgedl/release2/j0qc3/antigravity/stable/2.5.5-4923483625488384/windows-arm64/Antigravity%20IDE.exe">ide arm</a>
      <a href="https://edgedl.me.gvt1.com/edgedl/release2/j0qc3/antigravity/stable/2.5.5-4923483625488384/linux-x64/Antigravity%20IDE.tar.gz">ide linux</a>
    "#;

    #[test]
    fn it_picks_the_windows_installer_for_each_product_and_architecture() {
        let hub = pick_download(PAGE, Product::Hub, Arch::X64).expect("Hub x64");
        assert_eq!(hub.version, "2.15.1");
        assert!(hub.url.ends_with("/windows-x64/Antigravity-x64.exe"));

        let hub_arm = pick_download(PAGE, Product::Hub, Arch::Arm64).expect("Hub arm64");
        // ⚠ Hub 的 ARM 目录叫 `windows-arm`，IDE 的叫 `windows-arm64`。合并就找不到。
        assert!(hub_arm.url.contains("/windows-arm/"));

        let ide = pick_download(PAGE, Product::Ide, Arch::X64).expect("IDE x64");
        assert_eq!(ide.version, "2.5.5");
        assert!(ide.url.contains("/windows-x64/"));
        assert!(ide.url.contains("Antigravity%20IDE.exe"));

        let ide_arm = pick_download(PAGE, Product::Ide, Arch::Arm64).expect("IDE arm64");
        assert!(ide_arm.url.contains("/windows-arm64/"));
    }

    /// 两个产品不许串台 —— 它们在同一个页面上。
    #[test]
    fn the_two_products_never_pick_each_others_installer() {
        let hub = pick_download(PAGE, Product::Hub, Arch::X64).unwrap();
        let ide = pick_download(PAGE, Product::Ide, Arch::X64).unwrap();
        assert!(hub.url.contains("antigravity-hub"));
        assert!(!ide.url.contains("antigravity-hub"));
        assert_ne!(hub.url, ide.url);
    }

    /// 页面上挂着多个版本时取最大的那个，不取先出现的那个。
    #[test]
    fn a_newer_version_wins_no_matter_where_it_sits_on_the_page() {
        let page = "\
          https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-1/windows-x64/Antigravity-x64.exe \
          https://storage.googleapis.com/antigravity-public/antigravity-hub/2.9.9-2/windows-x64/Antigravity-x64.exe";
        assert_eq!(
            pick_download(page, Product::Hub, Arch::X64)
                .unwrap()
                .version,
            "2.15.1"
        );
        // 反过来排也要得到同一个答案。按字符串比会挑中 "2.9.9"。
        let flipped = "\
          https://storage.googleapis.com/antigravity-public/antigravity-hub/2.9.9-2/windows-x64/Antigravity-x64.exe \
          https://storage.googleapis.com/antigravity-public/antigravity-hub/2.15.1-1/windows-x64/Antigravity-x64.exe";
        assert_eq!(
            pick_download(flipped, Product::Hub, Arch::X64)
                .unwrap()
                .version,
            "2.15.1"
        );
    }

    /// 不是 Google 的域一律不收，明文也不收。
    #[test]
    fn only_googles_own_https_hosts_are_accepted() {
        assert!(allowed_host(
            "https://storage.googleapis.com/antigravity-public/x.exe"
        ));
        assert!(allowed_host("https://edgedl.me.gvt1.com/edgedl/x.exe"));
        assert!(allowed_host("https://dl.google.com/x.exe"));
        // 明文：这里没有可信的哈希对照值，只剩 TLS 和签名，所以不收。
        assert!(!allowed_host(
            "http://storage.googleapis.com/antigravity-public/x.exe"
        ));
        // 拿域名当路径 / 当用户名的经典绕法。
        assert!(!allowed_host(
            "https://evil.com/storage.googleapis.com/x.exe"
        ));
        assert!(!allowed_host(
            "https://storage.googleapis.com@evil.com/x.exe"
        ));
        assert!(!allowed_host("https://notgoogleapis.com/x.exe"));
    }

    /// 页面里混进一条别的域的地址，不许被挑中。
    #[test]
    fn a_mirror_url_on_the_page_is_never_picked() {
        let page = "https://mirror.example.com/antigravity-public/antigravity-hub/9.9.9-1/windows-x64/Antigravity-x64.exe";
        assert_eq!(pick_download(page, Product::Hub, Arch::X64), None);
    }

    /// IDE 的静默开关里必须有「装完不要自己启动」。
    ///
    /// 少了它，安装器会在后台把 IDE 拉起来，而面板紧接着要上锁 ——
    /// 症状是「装完之后门禁没生效」，而没有任何地方报错。
    #[test]
    fn the_ide_installer_is_told_not_to_launch_itself() {
        assert!(silent_args(Product::Ide).contains(&"/mergetasks=!runcode"));
        assert!(silent_args(Product::Ide).contains(&"/VERYSILENT"));
        assert_eq!(silent_args(Product::Hub), &["/S"]);
    }

    /// gzip 按 magic 认，不按响应头认。
    ///
    /// 实机踩过（2026-09-20）：这个站点**不管发什么 `Accept-Encoding` 都回
    /// `content-encoding: gzip`**，连显式 `identity` 也一样。而面板的 reqwest 不解压，
    /// `.text()` 于是给出一个六万多字符的替换字符串 —— 200、`text/html`、长度也像个网页，
    /// 只是一条地址都没有。**别把这段判断改成只看响应头。**
    #[test]
    fn a_gzipped_body_is_recognised_by_its_magic_bytes() {
        assert!(is_gzip(&[0x1f, 0x8b, 0x08, 0x00]));
        assert!(!is_gzip(b"<!DOCTYPE html>"));
        assert!(!is_gzip(&[]));
        assert!(!is_gzip(&[0x1f]));
    }

    /// 解析器拿到一堆解不开的字节时，**不许报成「页面改版了」**。
    ///
    /// 那句话会把下一个人直接带去改解析器 —— 而解析器是好的，坏的是传输层。
    #[test]
    fn undecodable_bytes_are_never_reported_as_a_page_redesign() {
        // `pick_download` 对替换字符串本来就挑不出东西；真正的防线在 `fetch_page`
        // 的那一道检查上。这里钉住的是「挑不出来 != 页面改版」这个区分确实存在：
        // 空页面挑不出来（返回 None），而 fetch_page 会在更早的地方就拦下二进制。
        let garbage: String = std::iter::repeat_n('\u{fffd}', 100).collect();
        assert_eq!(pick_download(&garbage, Product::Hub, Arch::X64), None);
        assert!(!garbage.contains('<'));
    }

    #[test]
    fn winget_ids_are_the_official_google_ones() {
        assert_eq!(winget_id(Product::Hub), "Google.Antigravity");
        assert_eq!(winget_id(Product::Ide), "Google.AntigravityIDE");
    }
}
