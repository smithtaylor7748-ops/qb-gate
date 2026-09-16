//! 面板窗口的插件权限（`capabilities/default.json`）里 opener 那一项。
//!
//! # 为什么要有这条测试
//!
//! 0.22.6 之前那一项写的是光秃秃的 `"opener:allow-open-url"`：它只放行
//! `open_url` 这个**命令**，没给任何**网址范围**。而插件（2.5.5 的 `scope.rs`）
//! 对空范围的处理是一个都不放 —— 面板里每一处 `openUrl` 都在报
//! `Not allowed to open url …`。
//!
//! 外链写的是 `void openUrl(href)`，错误被吞掉，看起来只是「点了没反应」；
//! 酒馆启动那一处把它弹了出来，而弹出来的那一刻酒馆其实已经起来了。
//! 前端的测试全在浏览器里跑演示数据，没有 Tauri IPC，这条链一次都没走到过 ——
//! 所以只能在这里对着权限文件本身钉。
//!
//! # 匹配口径
//!
//! 跟插件一致：每条 `url` 是一个 `glob::Pattern`，用默认选项 `matches`
//! （`*` 可以跨过 `/`）。插件哪天换了匹配方式，这里要跟着换。

use std::path::Path;

/// 给 `opener:allow-open-url` 配的网址范围。写成光秃秃的字符串时是空的。
fn open_url_scope() -> Vec<glob::Pattern> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let text = std::fs::read_to_string(&path).expect("读不到 capabilities/default.json");
    let cap: serde_json::Value = serde_json::from_str(&text).expect("权限文件不是合法 JSON");
    cap["permissions"]
        .as_array()
        .expect("permissions 必须是数组")
        .iter()
        .filter(|p| p["identifier"] == "opener:allow-open-url")
        .flat_map(|p| p["allow"].as_array().cloned().unwrap_or_default())
        .map(|entry| {
            let url = entry["url"].as_str().expect("范围条目要写 url");
            glob::Pattern::new(url).expect("url 不是合法的 glob")
        })
        .collect()
}

fn allowed(scope: &[glob::Pattern], url: &str) -> bool {
    scope.iter().any(|p| p.matches(url))
}

#[test]
fn every_url_the_panel_opens_is_inside_the_opener_scope() {
    let scope = open_url_scope();

    // 酒馆：`sillytavern::start` 交回前端的那个地址。端口使用者可以改，
    // 默认端口之外再核一个 —— 范围写成 `http://127.0.0.1:8000*` 的话默认端口照样过。
    for port in [8000, 18000] {
        let url = qb_extensions::plugins::sillytavern::page_url(port);
        assert!(
            allowed(&scope, &url),
            "{url} 不在 opener 范围里 —— 酒馆起来之后会报 Not allowed to open url"
        );
    }

    // 外链：纯净度判定的来源站点与总览的选购入口，全是 https。
    let c = qb_probe::probe::verdict::criteria();
    let links = [c.ipqs.url, c.ippure.url, c.iproyal]
        .into_iter()
        .chain(c.optional.iter().map(|o| o.url));
    for url in links {
        assert!(allowed(&scope, url), "{url} 不在 opener 范围里");
    }
}

/// `open_url` 最后交给 ShellExecute。范围图省事放成 `*` 的话，
/// 页面里一句 `openUrl` 就能直接运行本机上的程序。
#[test]
fn the_opener_scope_does_not_reach_local_programs() {
    let scope = open_url_scope();
    for target in [
        r"C:\Windows\System32\cmd.exe",
        "file:///C:/Windows/System32/cmd.exe",
    ] {
        assert!(!allowed(&scope, target), "{target} 不该在 opener 范围里");
    }
}
