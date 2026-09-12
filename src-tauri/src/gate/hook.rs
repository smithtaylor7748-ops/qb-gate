//! 会话内门禁：装进 Claude Code 的 hook，每次请求前拦一道。
//!
//! # 它补的是哪个空档
//!
//! 执行锁只管**启动**：`claude.exe` 上挂 Deny ExecuteFile，IP 不对就起不来。
//! 可是会话一旦跑起来，锁就管不着它了 —— 进程已经在内存里，照样能发请求。
//! 看门狗手里只有一招：`taskkill` 硬杀，未保存的对话跟着没。
//!
//! 中间缺的就是这一档：**在下一次请求发出前温和拦下**，进程留着，上下文不丢。
//!
//! 形态抄自 claude-ip-guard（MIT）：往 `settings.json` 挂 `SessionStart` 与
//! `UserPromptSubmit`，每条带一个自标记，卸载时只删自己那两条。
//!
//! # 脚本里**没有**判定逻辑，这是刻意的
//!
//! 脚本只干一件事：读 Rust 落下的 `gate-verdict.json`，不新鲜或者没通过就拦。
//! 判定仍然只有 `gate::judge` 一处 —— 在 PowerShell 里再写一遍白名单和国家比对，
//! 就是硬约束 10 明令禁止的「在别处再拼一份」，两份迟早对不上，
//! 而对不上的那一边就是绕过入口。
//!
//! 代价说清楚：**面板没在跑 = 裁决过期 = 一律拦**。这是 fail-closed 的直接后果，
//! 也是使用者明确选的那一档。所以拦下时的 stderr 必须把「怎么自救」原样印出来。

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::error::{GateError, Result};

/// 我们自己写的那几条 hook 的标记。卸载时**只删带这个标记的**，
/// 绝不整段覆盖 —— 使用者自己配的 hook 不能被这个面板吃掉。
const MARK: &str = "_qb_gate";

/// 挂在哪两个事件上。
///
/// `SessionStart` 管开场，`UserPromptSubmit` 管之后的每一次提交。
/// 只挂前者的话，会话开着不关、中途换网，后面所有请求都是没人看的。
const EVENTS: &[(&str, Option<&str>)] = &[
    ("SessionStart", Some("startup|resume")),
    ("UserPromptSubmit", None),
];

pub fn hooks_dir() -> PathBuf {
    super::state_dir().join("hooks")
}

pub fn script_path() -> PathBuf {
    hooks_dir().join("gate-check.ps1")
}

pub fn blocked_log() -> PathBuf {
    hooks_dir().join("blocked.log")
}

/// hook 要装进哪个 `settings.json`。
///
/// 跟着**当前槽位的具体目录**走，与 `launch.rs::claude_code_env` 设的
/// `CLAUDE_CONFIG_DIR` 是同一个目录 —— 装到联结点上等于装错地方。
/// 没有激活槽位时落到 Claude Code 自己的默认目录 `~\.claude`。
pub fn settings_target() -> Option<PathBuf> {
    let roots = crate::accounts::AccountRoots::current();
    if let Some(dir) = crate::accounts::active_slot_dir(&roots) {
        return Some(dir.join("settings.json"));
    }
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HookStatus {
    pub installed: bool,
    /// 装在哪个槽位。`None` = 没有激活槽位，装在 `~\.claude`。
    pub slot: Option<String>,
    pub settings_path: Option<PathBuf>,
    pub script_path: PathBuf,
    /// 最近几条拦截记录，最新的在前。
    pub recent_blocks: Vec<String>,
}

fn hook_command() -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script_path().display()
    )
}

/// 我们那一条 hook 长什么样。
fn our_entry(matcher: Option<&str>) -> Value {
    let mut e = json!({
        MARK: true,
        "hooks": [{
            "type": "command",
            "command": hook_command(),
            "timeout": 15
        }]
    });
    if let Some(m) = matcher {
        e["matcher"] = json!(m);
    }
    e
}

fn is_ours(v: &Value) -> bool {
    v.get(MARK).and_then(Value::as_bool).unwrap_or(false)
}

/// 把我们的 hook 合并进一份 settings。**先删自己的旧条目再加**，
/// 重复安装不会越堆越多。
pub fn merge_into(root: &mut Value) {
    strip_from(root);
    if !root.is_object() {
        *root = json!({});
    }
    let hooks = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    for (event, matcher) in EVENTS {
        let arr = hooks
            .as_object_mut()
            .unwrap()
            .entry(*event)
            .or_insert_with(|| json!([]));
        if !arr.is_array() {
            *arr = json!([]);
        }
        arr.as_array_mut().unwrap().push(our_entry(*matcher));
    }
}

/// 只摘掉带标记的那几条，其它一律原样留着。
///
/// 空下来的数组 / 对象顺手清掉，免得在使用者的配置里留一堆
/// `"UserPromptSubmit": []` 这种看不懂的残渣。
pub fn strip_from(root: &mut Value) {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    for (_, arr) in hooks.iter_mut() {
        if let Some(a) = arr.as_array_mut() {
            a.retain(|e| !is_ours(e));
        }
    }
    hooks.retain(|_, arr| !arr.as_array().map(|a| a.is_empty()).unwrap_or(false));
    let empty = hooks.is_empty();
    if empty {
        if let Some(o) = root.as_object_mut() {
            o.remove("hooks");
        }
    }
}

pub fn is_installed(root: &Value) -> bool {
    root.get("hooks")
        .and_then(Value::as_object)
        .map(|h| {
            h.values()
                .filter_map(Value::as_array)
                .any(|a| a.iter().any(is_ours))
        })
        .unwrap_or(false)
}

fn read_settings(p: &std::path::Path) -> Value {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_settings(p: &std::path::Path, v: &Value) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let body = serde_json::to_string_pretty(v)?;
    // 原子写：Claude Code 可能正好在读自己的 settings.json。
    let tmp = p.with_extension("json.qbtmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, p)?;
    Ok(())
}

pub fn status() -> HookStatus {
    let roots = crate::accounts::AccountRoots::current();
    let target = settings_target();
    let installed = target
        .as_ref()
        .map(|p| is_installed(&read_settings(p)))
        .unwrap_or(false);
    HookStatus {
        installed,
        slot: crate::accounts::active_label(&roots),
        settings_path: target,
        script_path: script_path(),
        recent_blocks: recent_blocks(8),
    }
}

fn recent_blocks(n: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(blocked_log()) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();
    lines.reverse();
    lines.truncate(n);
    lines
}

/// 装。**白名单为空时拒装。**
///
/// 空白名单意味着 `judge` 永远判不过，装上去等于把使用者直接关在门外 ——
/// 与硬约束 4「白名单为空时不许上锁」是同一条不变量，只是换了个地方触发。
pub fn install() -> Result<HookStatus> {
    if super::allowlist::read().unwrap_or_default().is_empty() {
        return Err(GateError::Other(
            "白名单是空的，装上会话内门禁会让每一次请求都被拦。先加一条出口 IP 再装。".into(),
        ));
    }
    let target = settings_target()
        .ok_or_else(|| GateError::Other("找不到 Claude Code 的配置目录".into()))?;

    install_into(&target)?;
    set_enabled(true);
    super::log::write(&format!("已装上会话内门禁：{}", target.display()));
    Ok(status())
}

fn install_into(target: &std::path::Path) -> Result<()> {
    write_script()?;
    let mut v = read_settings(target);
    merge_into(&mut v);
    write_settings(target, &v)
}

pub fn uninstall() -> Result<HookStatus> {
    set_enabled(false);
    if let Some(target) = settings_target() {
        let mut v = read_settings(&target);
        strip_from(&mut v);
        write_settings(&target, &v)?;
        super::log::write(&format!("已停用会话内门禁：{}", target.display()));
    }
    Ok(status())
}

/// 记下使用者的意图。**先记意图再改文件**（停用时），
/// 这样即使写 settings 失败，下次切槽位也不会又把它装回去。
fn set_enabled(on: bool) {
    let mut s = crate::settings::load();
    s.hook_enabled = on;
    let _ = crate::settings::save(&s);
}

/// 槽位一换，`CLAUDE_CONFIG_DIR` 就换了目录，hook 得跟着搬过去。
///
/// 不搬的话切完账户 hook 就静默失效了 —— 界面上还写着「已启用」，
/// 实际一次都不会跑，而这正是最危险的一种失效：**看起来有，其实没有**。
/// 账户切换流程末尾调它。
///
/// 用 `settings.json` 里的 `hook_enabled` 当使用者的**意图**，
/// 每个槽位的 settings 是**现状**，这里把现状对齐到意图。
pub fn follow_active_slot() {
    if !crate::settings::load().hook_enabled {
        return;
    }
    let Some(target) = settings_target() else {
        return;
    };
    if !is_installed(&read_settings(&target)) {
        match install_into(&target) {
            Ok(_) => super::log::write(&format!("会话内门禁已跟到新槽位：{}", target.display())),
            // 补装失败必须说出来。静默失败 = 界面写着「已启用」而实际没有，
            // 比明着报错危险得多。
            Err(e) => super::log::write(&format!(
                "⚠ 会话内门禁没能跟到新槽位（{}）：{e}。这个槽位的请求现在没人拦。",
                target.display()
            )),
        }
    }
}

fn write_script() -> Result<()> {
    std::fs::create_dir_all(hooks_dir())?;
    let settings = settings_target()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<Claude Code 的 settings.json>".into());
    let body = SCRIPT.replace("__SETTINGS_PATH__", &settings);
    // ⚠ 必须 UTF-8 **with BOM**：Windows PowerShell 5.1 没有 BOM 就按 ANSI 读，
    // 中文提示整段变乱码并引发一连串语法错误。这个坑踩过不止一次。
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(body.as_bytes());
    std::fs::write(script_path(), bytes)?;
    Ok(())
}

const SCRIPT: &str = r#"# QB Gate 会话内门禁 —— 由面板生成，别手改（改了下次装会被覆盖）。
#
# 这里**没有判定逻辑**：只读面板落下的裁决文件。判定全在 Rust 侧的
# gate::judge 一处，在这里再写一遍白名单比对，两份迟早对不上。
$ErrorActionPreference = 'Stop'

$state    = Join-Path $env:LOCALAPPDATA 'ClaudeIpGate'
$verdict  = Join-Path $state 'gate-verdict.json'
$log      = Join-Path $state 'hooks\blocked.log'
$settings = '__SETTINGS_PATH__'

# 裁决超过这个秒数就算不新鲜。看门狗 15 秒一轮，90 秒足够容忍几轮抖动。
$MaxAge = 90

function Deny($why) {
    $stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    try { Add-Content -Path $log -Value "$stamp  $why" -Encoding utf8 } catch { }

    [Console]::Error.WriteLine("QB Gate 拦下了这次请求：$why")
    [Console]::Error.WriteLine("")
    [Console]::Error.WriteLine("这是严格档（fail-closed）：裁决不新鲜或者没通过，一律拦。")
    [Console]::Error.WriteLine("怎么办：")
    [Console]::Error.WriteLine("  1. 打开 QB Gate 面板，看总览上的门禁状态；")
    [Console]::Error.WriteLine("  2. 出口 IP 或国家真的变了，就换回来，或把新 IP 加进白名单；")
    [Console]::Error.WriteLine("  3. 临时停用：面板 -> IP 锁 -> 会话内门禁 -> 停用；")
    [Console]::Error.WriteLine("  4. 面板打不开时手动解除：编辑")
    [Console]::Error.WriteLine("     $settings")
    [Console]::Error.WriteLine("     删掉 hooks 里带 _qb_gate 标记的那两段即可。")
    exit 2
}

if (-not (Test-Path $verdict)) {
    Deny '门禁裁决文件不存在（面板没跑过？）'
}

try {
    $v = Get-Content $verdict -Raw -Encoding UTF8 | ConvertFrom-Json
} catch {
    Deny '门禁裁决文件读不出来'
}

$now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$age = $now - [int64]$v.checked_at
if ($age -gt $MaxAge) {
    Deny "门禁裁决已过期 $age 秒（面板没在跑，或看门狗已停）"
}

if (-not $v.allowed) {
    Deny $v.reason
}

exit 0
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_adds_both_events_with_our_mark() {
        let mut v = json!({});
        merge_into(&mut v);
        assert!(is_installed(&v));
        let hooks = v["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks["SessionStart"][0]["matcher"], "startup|resume");
        // UserPromptSubmit 不带 matcher —— 每一次提交都要过。
        assert!(hooks["UserPromptSubmit"][0].get("matcher").is_none());
    }

    /// 核心不变量：**使用者自己的 hook 一条都不能少。**
    ///
    /// 这条挂了，装一次门禁就把别人辛苦配的格式化、提交检查全吃掉了，
    /// 而且他多半要过很久才发现。
    #[test]
    fn user_hooks_survive_install_and_uninstall() {
        let mut v = json!({
            "hooks": {
                "UserPromptSubmit": [
                    { "hooks": [{ "type": "command", "command": "my-own-thing" }] }
                ],
                "PostToolUse": [
                    { "matcher": "Edit", "hooks": [{ "type": "command", "command": "fmt" }] }
                ]
            },
            "model": "opus"
        });

        merge_into(&mut v);
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 2);

        strip_from(&mut v);
        let ups = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(ups.len(), 1);
        assert_eq!(ups[0]["hooks"][0]["command"], "my-own-thing");
        assert_eq!(v["hooks"]["PostToolUse"][0]["matcher"], "Edit");
        assert_eq!(v["model"], "opus", "settings 里别的键不许动");
        assert!(!is_installed(&v));
    }

    #[test]
    fn installing_twice_does_not_stack_up() {
        let mut v = json!({});
        merge_into(&mut v);
        merge_into(&mut v);
        merge_into(&mut v);
        assert_eq!(v["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    /// 卸载要卸干净：不留 `"UserPromptSubmit": []` 这种看不懂的残渣。
    #[test]
    fn uninstall_leaves_no_empty_leftovers() {
        let mut v = json!({});
        merge_into(&mut v);
        strip_from(&mut v);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn stripping_a_settings_without_hooks_is_a_no_op() {
        let mut v = json!({ "model": "opus" });
        strip_from(&mut v);
        assert_eq!(v, json!({ "model": "opus" }));
        assert!(!is_installed(&v));
    }

    /// 别人家的 hook 即使长得像，也不能因为「看着像我们的」就删掉。
    #[test]
    fn only_marked_entries_are_ours() {
        let looks_alike = json!({
            "hooks": { "SessionStart": [
                { "matcher": "startup|resume",
                  "hooks": [{ "type": "command", "command": "gate-check.ps1" }] }
            ]}
        });
        assert!(!is_installed(&looks_alike));
        let mut v = looks_alike.clone();
        strip_from(&mut v);
        assert_eq!(v, looks_alike);
    }

    #[test]
    fn script_carries_a_bom_and_the_self_rescue_path() {
        // 脚本里必须写着怎么自救 —— fail-closed 把人关在门外时，
        // 这段 stderr 可能是他唯一看得到的东西。
        assert!(SCRIPT.contains("__SETTINGS_PATH__"));
        assert!(SCRIPT.contains("临时停用"));
        assert!(SCRIPT.contains("_qb_gate"));
        assert!(SCRIPT.contains("exit 2"));
    }
}
