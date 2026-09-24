//! 开发工具：真机跑一遍本机体检，只打印每一项的判定（2026-09-24）。
//!
//! ```powershell
//! cargo run -p qb-sysenv --example checkup
//! ```
//!
//! **会联网**，跟界面上点「体检」发的是同一批请求：两轮出口（绕过 / 跟随系统代理）、
//! 不带任何凭据地问一次 `api.anthropic.com` 与两个 `robots.txt`、解析 `claude.ai`、
//! 问一次只有 IPv6 的回显服务。**不带任何账户凭据**，也不打印环境变量与密钥扫描的结果
//! （那两样只在界面上看）。
//!
//! 什么时候跑：改了 `checkup.rs` / `probe::reach` 之后，或者界面上某一项开始显示「查不了」时。

#[tokio::main]
async fn main() {
    let c = qb_sysenv::sysenv::checkup::scan().await;
    for it in &c.items {
        println!("[{:?}] {} ({})", it.state, it.label, it.id);
        println!("    {}", it.detail);
        if let Some(m) = &it.manual {
            println!("    → {m}");
        }
    }
}
