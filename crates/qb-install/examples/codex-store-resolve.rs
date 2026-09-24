//! 一次性核对工具：真的去问一遍 Store（DisplayCatalog + FE3），看直装那条链认不认得出最新版。
//! **只读元数据，不下载、不安装。**
//! 用法：cargo run -p qb-install --example codex-store-resolve [-- locate [url] | dump]
//!   不带参数：只查版本（GetCookie / SyncUpdates）；带 `locate`：再问一次 /secured 拿下载地址
//!   （只打印地址的主机与长度，不打印签名参数）。
use qb_install::install::codex_store::{self, Arch};

#[tokio::main]
async fn main() {
    let locate = std::env::args().any(|a| a == "locate");
    let dump = std::env::args().any(|a| a == "dump");
    let arch = Arch::current();
    println!("本机架构 {}", arch.moniker());
    if dump {
        // 把解码过的 SyncUpdates 报文原样打出来（给排查解析器用；里面没有凭据）。
        match codex_store::debug_sync_xml(arch).await {
            Ok(xml) => println!("{xml}"),
            Err(e) => {
                eprintln!("失败：{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if locate {
        match codex_store::locate(arch).await {
            Ok((r, url)) => {
                println!("最新版 {} ({})", r.version, r.moniker);
                println!("UpdateID {} rev {}", r.update_id, r.revision);
                println!(
                    "清单 SHA-256 {}",
                    r.sha256.as_deref().unwrap_or("（清单里没给）")
                );
                println!(
                    "清单大小 {}",
                    r.size
                        .map_or("（清单里没给）".to_string(), |n| format!("{n} 字节"))
                );
                let host = url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or("");
                println!(
                    "下载地址主机 {host}，长度 {}，允许 {}",
                    url.len(),
                    codex_store::allowed_host(&url)
                );
                if std::env::args().any(|a| a == "url") {
                    // 只在明确要求时打印完整地址（带签名参数，几小时后失效）。
                    println!("{url}");
                }
            }
            Err(e) => {
                eprintln!("失败：{e}");
                std::process::exit(1);
            }
        }
    } else {
        match codex_store::resolve(arch).await {
            Ok(r) => {
                println!("最新版 {} ({})", r.version, r.moniker);
                println!("UpdateID {} rev {}", r.update_id, r.revision);
                println!(
                    "清单 SHA-256 {}",
                    r.sha256.as_deref().unwrap_or("（清单里没给）")
                );
            }
            Err(e) => {
                eprintln!("失败：{e}");
                std::process::exit(1);
            }
        }
    }
}
