//! 构建脚本。除了 Tauri 自己那套代码生成，还挂上一份**自己写的应用清单**。
//!
//! # 为什么要自带清单（0.29.0）
//!
//! 0.28.0 之前用的是 tauri-winres 的默认清单，里面只有一个 Common-Controls 依赖：
//! 没有 `requestedExecutionLevel`、没有 `supportedOS`、没有 DPI 声明。
//! 本机实测（把 exe 的 `.rsrc` 里那段 XML 读出来）确认过。
//!
//! 后果不只是 DPI 发虚。**「未签名 + 没声明执行级别」是 Defender 那套 ML 启发式
//! （Bearfoos / Wacatac 一族）给可疑度加分的特征之一**，而这个面板本来就在做一堆
//! 长得像木马的事（给别人的 exe 写 Deny ACE、收进程、下载 exe 再执行）——
//! 那些是产品本身删不掉，能补的就只有这类「正规程序都会声明、恶意程序常常懒得写」的东西。
//!
//! 2026-09-12 / 09-19 / 09-20 本机连着三次 `Trojan:Win32/Bearfoos.A!ml`，
//! 装好的 `qb-gate.exe` 与 `*-setup.exe` 都被隔离过。详见 `docs/ANTIVIRUS.zh-CN.md`。
//!
//! ⚠ **清单进没进去要回读核对，别看构建退出码。** 命令见那份文档。
fn main() {
    let mut windows = tauri_build::WindowsAttributes::new();
    windows = windows.app_manifest(include_str!("qb-gate.manifest"));
    // 清单改了要重新构建 —— 少了这一行，改完清单跑 build 会直接命中缓存。
    println!("cargo:rerun-if-changed=qb-gate.manifest");
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("构建 Tauri 应用失败（多半是清单 XML 写坏了）");
}
