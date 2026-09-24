//! 开发工具：看一眼反重力 Hub 那条凭据长什么形状（0.32.0）。
//!
//! ```powershell
//! cargo run -p qb-accounts --example antigravity-hub-cred
//! cargo run -p qb-accounts --example antigravity-hub-cred -- gemini:antigravity
//! ```
//!
//! # ⛔ 这个例子只打印**形状**，一个值都不打印
//!
//! 它存在的唯一理由是回答「Hub 的那条凭据里到底有没有一个能用的令牌」——
//! 在写 `qb-app::usecase::antigravity_hub` 之前必须先知道答案，而不是照着猜写一遍
//! （坑 7.42：先复现再改；坑 7.62：位置与形状不许猜）。
//!
//! 所以：JSON 就打键名、类型与长度；不是 JSON 就打前几个字节的**类别**
//! （可打印 / 不可打印）与总长度。**任何情况下都不要往这里加一句打印值的代码。**
//!
//! 背景（2026-09-21 实机核过）：Hub 在本机别的地方一个字的身份都没写 ——
//! `app_storage.json` 16 个键没有账户、`antigravity_state.pbtxt` 只有引导与迁移状态、
//! Local Storage 搜不到邮箱、日志只有一句 `Auth succeeded`。

use qb_platform::credentials;

/// Hub 的凭据目标名。`cmdkey /list` 在本机上看到的就是这一条。
const HUB_TARGET: &str = "gemini:antigravity";

fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| HUB_TARGET.into());
    println!("凭据目标名：{target}");

    if !credentials::exists(&target) {
        println!("没有这条凭据 —— 这台机器上 Hub 没登录过，或者它换了目标名。");
        println!("核对办法（PowerShell）：cmdkey /list | Select-String antigravity");
        return;
    }
    println!("这条凭据在。（「Hub 登没登录」到这一步就答得出来了，不必往下读。）");

    let blob = match credentials::read_generic(&target) {
        Ok(Some(b)) => b,
        Ok(None) => {
            println!("读的时候又说没有 —— 多半是刚好被改写了，重跑一次。");
            return;
        }
        Err(e) => {
            println!("读不出来：{e}");
            return;
        }
    };
    println!("blob 长度：{} 字节", blob.len());
    if blob.is_empty() {
        println!("空的 —— 当成「没登录」处理。");
        return;
    }

    describe(blob.as_bytes());
}

/// 只描述形状。
fn describe(bytes: &[u8]) {
    // 凭据管理器里的 blob 常见是 UTF-16LE 的文本。两种都试一下，只为判断「是不是 JSON」。
    let as_utf8 = std::str::from_utf8(bytes).ok().map(str::to_owned);
    let as_utf16 = if bytes.len() % 2 == 0 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&units).ok()
    } else {
        None
    };
    let (text, enc) = match (as_utf8, as_utf16) {
        (Some(t), _) if t.trim_start().starts_with('{') => (Some(t), "UTF-8"),
        (_, Some(t)) if t.trim_start().starts_with('{') => (Some(t), "UTF-16LE"),
        (Some(t), _) => (Some(t), "UTF-8（不是 JSON）"),
        (_, Some(t)) => (Some(t), "UTF-16LE（不是 JSON）"),
        _ => (None, "二进制"),
    };
    println!("编码看起来是：{enc}");

    let Some(text) = text else {
        let printable = bytes.iter().filter(|b| (0x20..0x7f).contains(*b)).count();
        println!("可打印字节占 {}/{}", printable, bytes.len());
        println!("→ 不是文本。那条「拿 Hub 的令牌去问 Google」的路走不通，改走只报登录态那条。");
        return;
    };

    match serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) {
        Ok(v) => {
            println!("是 JSON。键（只打键名、类型、长度）：");
            walk(&v, 0);
            println!();
            // 访问令牌什么时候过期 —— 这是个时刻，不是秘密，而它决定了
            // 「联网那一格读不出来」到底是令牌过期还是接口改版。
            if let Some(exp) = v
                .get("token")
                .and_then(|t| t.get("expiry"))
                .and_then(serde_json::Value::as_str)
            {
                match chrono::DateTime::parse_from_rfc3339(exp) {
                    Ok(t) => {
                        let left = t.timestamp() - chrono::Utc::now().timestamp();
                        println!(
                            "access_token 过期时刻：{}（{}）",
                            t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M"),
                            if left > 0 {
                                format!("还有 {} 分钟", left / 60)
                            } else {
                                format!("已经过期 {} 分钟了 —— 起一次 Hub 让它换新的", -left / 60)
                            }
                        );
                    }
                    Err(_) => println!("access_token 的 expiry 不是 RFC3339，没法判断新旧"),
                }
            }
            if let Some(jwt) = v.get("id_token").and_then(serde_json::Value::as_str) {
                jwt_claims(jwt);
            }
            println!("→ 下一步：看上面有没有 access_token / id_token / refresh_token / expiry 之类的键。");
            println!("  有 → `antigravity_hub` 可以拿它去问 Google；没有 → 只做登录态那条。");
        }
        Err(e) => {
            println!(
                "不是 JSON（{e}）。文本长度 {} 个字符。",
                text.chars().count()
            );
            println!("→ 形状不认识就别写解析器，先把这段形状贴给下一个人看。");
        }
    }
}

fn walk(v: &serde_json::Value, depth: usize) {
    let pad = "  ".repeat(depth + 1);
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                let kind = match val {
                    serde_json::Value::String(s) => format!("string，{} 个字符", s.chars().count()),
                    serde_json::Value::Number(_) => "number".into(),
                    serde_json::Value::Bool(_) => "bool".into(),
                    serde_json::Value::Null => "null".into(),
                    serde_json::Value::Array(a) => format!("array，{} 项", a.len()),
                    serde_json::Value::Object(o) => format!("object，{} 个键", o.len()),
                };
                println!("{pad}{k}: {kind}");
                if depth < 2 {
                    if let serde_json::Value::Object(_) = val {
                        walk(val, depth + 1);
                    }
                }
            }
        }
        other => println!("{pad}（顶层不是对象，是 {}）", kind_of(other)),
    }
}

/// 把 `id_token`（OIDC 的 JWT）的**声明名字**列出来。
///
/// ⛔ 同样只打名字、类型与长度 —— **一个值都不打**。
/// 这一步要回答的问题只有一个：邮箱在不在这里面。在的话，「Hub 的账户」
/// 就是一次纯本地的 base64 解码，**根本不用联网**。
fn jwt_claims(jwt: &str) {
    use base64::Engine as _;
    let Some(payload) = jwt.split('.').nth(1) else {
        println!("id_token 不是三段式的 JWT，跳过。");
        return;
    };
    let Ok(raw) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        println!("id_token 的载荷 base64 解不开，跳过。");
        return;
    };
    match serde_json::from_slice::<serde_json::Value>(&raw) {
        Ok(v) => {
            println!("id_token 的载荷声明（只打名字、类型、长度）：");
            walk(&v, 0);
            let has_email = v.get("email").and_then(serde_json::Value::as_str).is_some();
            println!(
                "  → 载荷里{}邮箱。{}",
                if has_email { "有" } else { "没有" },
                if has_email {
                    "那么「Hub 登的是谁」是一次纯本地解码，零网络请求。"
                } else {
                    "那就只能靠联网问，或者只报登录态。"
                }
            );
            println!();
        }
        Err(e) => println!("id_token 的载荷不是 JSON（{e}），跳过。"),
    }
}

fn kind_of(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}
