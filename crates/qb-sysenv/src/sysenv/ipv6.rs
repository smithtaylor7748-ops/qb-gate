//! 用户要求的整机网卡 IPv6 开关。启动时应用，关闭开关恢复逐张网卡的原值。
//! 原值先落盘，再请求管理员权限；失败、重启和重复应用均不能覆盖原值。
use crate::error::{GateError, Result};
use qb_platform::config_io;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Ipv6Binding {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Ipv6Status {
    pub disable: bool,
    pub busy: bool,
    pub bindings: Vec<Ipv6Binding>,
    pub restore_pending: usize,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    disable: bool,
    originals: Vec<Ipv6Binding>,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            disable: true,
            originals: vec![],
        }
    }
}

static BUSY: AtomicBool = AtomicBool::new(false);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

trait Backend {
    fn load(&mut self) -> Result<Saved>;
    fn save(&mut self, saved: &Saved) -> Result<()>;
    fn bindings(&mut self) -> Result<Vec<Ipv6Binding>>;
    fn set(&mut self, targets: &[Ipv6Binding]) -> Result<()>;
}

fn reconcile(backend: &mut impl Backend, preference: Option<bool>) -> Result<()> {
    let mut saved = backend.load()?;
    if let Some(disable) = preference {
        saved.disable = disable;
        // 读网卡失败也能关闭启动意图，避免不支持 NetAdapter 的机器每次启动重试。
        backend.save(&saved)?;
    }
    let before = backend.bindings()?;
    if saved.disable {
        for binding in &before {
            if !saved.originals.iter().any(|b| b.id == binding.id) {
                saved.originals.push(binding.clone());
            }
        }
    }
    // 先提交意图和原值。即使之后提权取消、进程中断，也有恢复依据。
    backend.save(&saved)?;
    let targets: Vec<_> = if saved.disable {
        before
            .iter()
            .filter(|b| b.enabled)
            .map(|b| Ipv6Binding {
                enabled: false,
                ..b.clone()
            })
            .collect()
    } else {
        saved
            .originals
            .iter()
            .filter(|original| {
                before
                    .iter()
                    .any(|b| b.id == original.id && b.enabled != original.enabled)
            })
            .cloned()
            .collect()
    };
    let changed = if targets.is_empty() {
        Ok(())
    } else {
        backend.set(&targets)
    };
    // 无论命令是否成功都重新读取，不拿开关意图或进程退出码冒充实际状态。
    let after = backend.bindings()?;
    if !saved.disable {
        saved.originals.retain(|original| {
            !after
                .iter()
                .any(|b| b.id == original.id && b.enabled == original.enabled)
        });
        backend.save(&saved)?;
    }
    changed?;
    if saved.disable && (after.is_empty() || after.iter().any(|b| b.enabled)) {
        return Err(GateError::Other(
            "部分网卡仍启用 IPv6，或没有读到网卡；请查看清单并重试".into(),
        ));
    }
    if !saved.disable && !saved.originals.is_empty() {
        return Err(GateError::Other(format!(
            "还有 {} 张网卡未恢复（可能已断开或被移除），原值已保留，可连接后重试",
            saved.originals.len()
        )));
    }
    Ok(())
}

struct System;
impl Backend for System {
    fn load(&mut self) -> Result<Saved> {
        let path = crate::paths::state_dir().join("ipv6-state.json");
        match config_io::read_optional(&path)? {
            None => Ok(Saved::default()),
            Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        }
    }
    fn save(&mut self, saved: &Saved) -> Result<()> {
        let path = crate::paths::state_dir().join("ipv6-state.json");
        config_io::ensure_plain_path(&path)?;
        config_io::replace(&path, Some(&serde_json::to_vec_pretty(saved)?))
    }
    fn bindings(&mut self) -> Result<Vec<Ipv6Binding>> {
        bindings()
    }
    fn set(&mut self, targets: &[Ipv6Binding]) -> Result<()> {
        set_bindings(targets)
    }
}

pub fn status() -> Result<Ipv6Status> {
    let saved = System.load()?;
    let (bindings, read_error) = match bindings() {
        Ok(rows) => (rows, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    Ok(Ipv6Status {
        disable: saved.disable,
        busy: BUSY.load(Ordering::SeqCst),
        bindings,
        restore_pending: saved.originals.len(),
        error: read_error.or_else(|| LAST_ERROR.lock().unwrap().clone()),
    })
}

/// None = 启动时应用保存的意图；Some = 用户操作开关。
pub fn apply(preference: Option<bool>) -> Result<()> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err(GateError::Other("IPv6 设置正在执行，请稍候".into()));
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            BUSY.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset;
    let result = reconcile(&mut System, preference);
    *LAST_ERROR.lock().unwrap() = result.as_ref().err().map(ToString::to_string);
    match &result {
        Ok(()) => crate::audit::write("IPv6 网卡设置已应用并核验"),
        Err(e) => crate::audit::write(&format!("IPv6 网卡设置未完成：{e}")),
    }
    result
}

#[cfg(windows)]
pub fn bindings() -> Result<Vec<Ipv6Binding>> {
    let script = r#"$ErrorActionPreference = 'Stop'
try {
  $rows = @(Get-NetAdapterBinding -Name '*' -ComponentID ms_tcpip6 -IncludeHidden -ErrorAction Stop | ForEach-Object {
    @{ id = [string]$_.InstanceID; name = [string]$_.Name; enabled = [bool]$_.Enabled }
  })
  ConvertTo-Json -InputObject $rows -Compress
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }"#;
    let out = crate::process::powershell_std(script).output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "读取 IPv6 网卡失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

#[cfg(not(windows))]
pub fn bindings() -> Result<Vec<Ipv6Binding>> {
    Err(GateError::Other("IPv6 网卡开关仅支持 Windows".into()))
}

fn mutation_script(targets: &[Ipv6Binding]) -> Result<String> {
    // 管理员进程只接受受限的网卡实例标识和布尔值；不执行磁盘上的脚本，
    // 不传网卡名称（含通配符的名称会误伤其他网卡），也不接受任意命令。
    let mut plan = Vec::new();
    for b in targets {
        let guid = b.id.strip_suffix("::ms_tcpip6").unwrap_or("");
        if guid.len() != 38
            || !guid.starts_with('{')
            || !guid.ends_with('}')
            || !guid[1..37]
                .bytes()
                .all(|ch| ch.is_ascii_hexdigit() || ch == b'-')
        {
            return Err(GateError::Other("网卡实例标识无效，未修改 IPv6".into()));
        }
        // ⛔ **一个双引号都不许出现在这段脚本里**，所以计划用 PowerShell 的
        // 数组字面量写，不用 JSON。理由在 `set_bindings` 的说明里：整段脚本要作为
        // **一个参数**穿过 `Start-Process -ArgumentList`，而 PowerShell 5.1 在那里
        // 会把内嵌的 `"` 吃掉 —— 实测的下场是子进程 **exit 0 但一件事都没做**。
        //
        // id 的字符集上面刚校验过（只有 `{`、`}`、十六进制、`-`、后缀 `::ms_tcpip6`），
        // 拼不出引号来，所以单引号包起来是安全的。
        plan.push(format!(
            "  [pscustomobject]@{{ id = '{}'; enabled = ${} }}",
            b.id, b.enabled
        ));
    }
    let plan = plan.join("\n");
    Ok(format!(
        r#"$ErrorActionPreference = 'Stop'
try {{
  $plan = @(
{plan}
  )
  $failed = $false
  foreach ($item in $plan) {{
    try {{
      $binding = @(Get-NetAdapterBinding -Name '*' -ComponentID ms_tcpip6 -IncludeHidden -ErrorAction Stop | Where-Object {{ $_.InstanceID -eq $item.id }})
      if ($binding.Count -ne 1) {{ throw 'Adapter missing or ambiguous' }}
      if ($item.enabled) {{ Enable-NetAdapterBinding -InputObject $binding[0] -Confirm:$false -ErrorAction Stop | Out-Null }}
      else {{ Disable-NetAdapterBinding -InputObject $binding[0] -Confirm:$false -ErrorAction Stop | Out-Null }}
    }} catch {{ $failed = $true }}
  }}
  if ($failed) {{ exit 1 }}
}} catch {{ exit 1 }}
exit 0"#
    ))
}

/// 提权跑一遍 [`mutation_script`]。
///
/// # ⛔ 这里为什么**不再**用 `-EncodedCommand`（0.29.0）
///
/// 老写法是 `Start-Process -Verb RunAs -WindowStyle Hidden -ArgumentList @(…, '-EncodedCommand', $encoded)`。
/// `-Verb RunAs` + `-WindowStyle Hidden` + `-EncodedCommand` **三件凑在一条命令行上**，
/// 是 Defender 那套启发式里权重最高的 PowerShell 组合之一 —— 而这台机器 2026-09
/// 连着三次 `Trojan:Win32/Bearfoos.A!ml`（见 `docs/ANTIVIRUS.zh-CN.md`）。
/// 提权和隐藏窗口是功能需要，留着；base64 只是个传输技巧，可以不要。
///
/// # ⚠ 但 base64 当初是为了绕一个真坑，不能直接删了事
///
/// 整段脚本要作为**一个参数**穿过 `Start-Process -ArgumentList`。PowerShell 5.1 在
/// 这一步会把参数里的双引号吃掉 —— 2026-09-20 实测三种形状：
///
/// | 内层脚本 | 结果 |
/// |---|---|
/// | 单行、无引号 | 正常 |
/// | 单行、**含双引号**（老脚本的 JSON） | **exit 0，但什么都没做** |
/// | 多行、只有单引号 | 正常 |
///
/// 中间那一行才是关键：它**不报错**。所以真正的修法不是「换个传法」，而是让
/// [`mutation_script`] **一个双引号都不产生**（计划改用 PowerShell 数组字面量，
/// 不用 `ConvertFrom-Json`）。多行是安全的，实测过。
///
/// ⛔ **以后往那段脚本里加任何带 `"` 的东西之前，先回来读这一段。**
/// 加了之后的症状是「点了没反应、也没有报错」，而不是编译失败。
#[cfg(windows)]
fn set_bindings(targets: &[Ipv6Binding]) -> Result<()> {
    let inner = crate::process::ps_utf8(&mutation_script(targets)?).replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$inner = '{inner}'
try {{
  $child = Start-Process -FilePath "$PSHOME\powershell.exe" -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile', '-NonInteractive', '-Command', $inner) -PassThru -Wait
  exit $child.ExitCode
}} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"#
    );
    let out = crate::process::powershell_std(&script).output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "IPv6 修改未完成：管理员授权被取消，或有网卡不允许修改。原设置已保留，可重试。{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_bindings(_: &[Ipv6Binding]) -> Result<()> {
    Err(GateError::Other("IPv6 网卡开关仅支持 Windows".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        saved: Saved,
        live: Vec<Ipv6Binding>,
        fail_save: bool,
        fail_read: bool,
        partial: bool,
        calls: usize,
    }
    fn binding(n: u8, enabled: bool) -> Ipv6Binding {
        Ipv6Binding {
            id: format!("{{00000000-0000-0000-0000-{n:012}}}::ms_tcpip6"),
            name: format!("本地连接* {n}"),
            enabled,
        }
    }
    fn fake() -> Fake {
        Fake {
            saved: Saved::default(),
            live: vec![binding(1, true), binding(2, false)],
            fail_save: false,
            fail_read: false,
            partial: false,
            calls: 0,
        }
    }
    impl Backend for Fake {
        fn load(&mut self) -> Result<Saved> {
            Ok(self.saved.clone())
        }
        fn save(&mut self, s: &Saved) -> Result<()> {
            if self.fail_save {
                return Err(GateError::Other("disk full".into()));
            }
            self.saved = s.clone();
            Ok(())
        }
        fn bindings(&mut self) -> Result<Vec<Ipv6Binding>> {
            if self.fail_read {
                return Err(GateError::Other("NetAdapter unavailable".into()));
            }
            Ok(self.live.clone())
        }
        fn set(&mut self, targets: &[Ipv6Binding]) -> Result<()> {
            self.calls += 1;
            for target in targets
                .iter()
                .take(if self.partial { 1 } else { targets.len() })
            {
                assert!(
                    self.saved.originals.iter().any(|b| b.id == target.id),
                    "must save before mutation"
                );
                self.live
                    .iter_mut()
                    .find(|b| b.id == target.id)
                    .unwrap()
                    .enabled = target.enabled;
            }
            if self.partial {
                Err(GateError::Other("partial failure".into()))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn defaults_on_and_repeated_start_preserves_originals() {
        let mut f = fake();
        assert!(f.saved.disable);
        reconcile(&mut f, None).unwrap();
        reconcile(&mut f, None).unwrap();
        assert_eq!(f.calls, 1);
        reconcile(&mut f, Some(false)).unwrap();
        assert_eq!(f.live, vec![binding(1, true), binding(2, false)]);
        assert!(f.saved.originals.is_empty());
        assert!(!f.saved.disable);
        reconcile(&mut f, None).unwrap();
        assert_eq!(f.calls, 2);
    }
    #[test]
    fn new_and_missing_adapters_keep_individual_restore_states() {
        let mut f = fake();
        reconcile(&mut f, None).unwrap();
        f.live.push(binding(3, true));
        reconcile(&mut f, None).unwrap();
        f.live.retain(|b| b.id != binding(1, true).id);
        assert!(reconcile(&mut f, Some(false)).is_err());
        assert_eq!(f.saved.originals, vec![binding(1, true)]);
        f.live.push(binding(1, false));
        reconcile(&mut f, None).unwrap();
        assert!(f.saved.originals.is_empty());
        assert!(
            f.live
                .iter()
                .find(|b| b.id == binding(3, true).id)
                .unwrap()
                .enabled
        );
    }
    #[test]
    fn disk_failure_prevents_any_mutation() {
        let mut f = fake();
        f.fail_save = true;
        assert!(reconcile(&mut f, None).is_err());
        assert_eq!(f.calls, 0);
    }
    #[test]
    fn partial_failure_retains_recovery_and_can_be_restored() {
        let mut f = fake();
        f.live.push(binding(3, true));
        f.partial = true;
        assert!(reconcile(&mut f, None).is_err());
        assert_eq!(f.saved.originals.len(), 3);
        assert!(f.live[2].enabled);
        f.partial = false;
        reconcile(&mut f, Some(false)).unwrap();
        assert_eq!(
            f.live,
            vec![binding(1, true), binding(2, false), binding(3, true)]
        );
    }
    #[test]
    fn script_uses_exact_ids_and_rejects_injection() {
        let mut b = binding(1, false);
        b.name = "*'; Stop-Process -Name example; '".into();
        let s = mutation_script(&[b.clone()]).unwrap();
        assert!(!s.contains(&b.name));
        assert!(s.contains("-InputObject $binding[0]"));
        b.id = "'; Write-Host bad; '".into();
        assert!(mutation_script(&[b]).is_err());
    }

    /// ⛔ 这段脚本里**一个双引号都不许有**。
    ///
    /// 它要作为一个参数穿过 `Start-Process -ArgumentList`，而 PowerShell 5.1 在那里
    /// 会把内嵌的 `"` 吃掉。2026-09-20 实测：含双引号的那一版子进程 **exit 0
    /// 而一张网卡都没改** —— 不报错、界面上看着像成功。老版本用 `-EncodedCommand`
    /// 绕开这件事，0.29.0 为了不触发杀软的启发式改成了 `-Command`，
    /// 于是这条不变量成了硬要求。
    ///
    /// 删这条测试，等于把「点了没反应也没有报错」放回来。
    #[test]
    fn the_elevated_script_never_contains_a_double_quote() {
        let s = mutation_script(&[binding(1, true), binding(2, false)]).unwrap();
        assert!(
            !s.contains('"'),
            "提权脚本里出现了双引号，它会被 Start-Process 吃掉而且不报错：\n{s}"
        );
        // 计划真的带上了两张网卡，而且布尔值是 PowerShell 认的写法。
        assert!(s.contains("$true"));
        assert!(s.contains("$false"));
        assert_eq!(s.matches("[pscustomobject]").count(), 2);
    }
    #[test]
    fn no_adapters_is_not_reported_as_disabled() {
        let mut f = fake();
        f.live.clear();
        assert!(reconcile(&mut f, None).is_err());
    }

    #[test]
    fn unsupported_machine_can_still_turn_off_startup_preference() {
        let mut f = fake();
        f.fail_read = true;
        assert!(reconcile(&mut f, Some(false)).is_err());
        assert!(!f.saved.disable);
        assert_eq!(f.calls, 0);
    }
}
