//! 可撤销的 CC Switch 链接兼容：只在使用者主动点击时改当前用户的协议处理者。
//! 启动/安装不登记；开发版不登记；取消或卸载只还原仍指向本程序的登记。
//! 只备份被改的四个值，原处理者的其他值与子键保持原样。

use crate::autostart::{exe_in, same_exe};
use crate::error::{GateError, Result};
use crate::registry::{self, Hive, Value};
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

pub const RESTORE_ARG: &str = "--restore-ccswitch-handler";
const BACKUP_KEY: &str = r"Software\QB Gate\DeepLinkCompatibility";
const BACKUP_VALUE: &str = "ccswitch-backup";
const KEYS: [(&str, &str); 4] = [
    (r"Software\Classes\ccswitch", ""),
    (r"Software\Classes\ccswitch", "URL Protocol"),
    (r"Software\Classes\ccswitch\DefaultIcon", ""),
    (r"Software\Classes\ccswitch\shell\open\command", ""),
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct DeepLinkHandlerStatus {
    pub enabled: bool,
    /// `Classes\\ccswitch` 指向本程序；Windows 默认应用选择可能覆盖这条登记。
    pub registered: bool,
    pub can_restore: bool,
    pub dev_build: bool,
    /// 当前处理者的程序路径；不返回参数，避免第三方登记里夹带敏感参数。
    pub handler: Option<String>,
    /// Windows 用户默认应用覆盖了普通协议登记时，需要到系统设置改回。
    pub user_choice: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Backup {
    owner: String,
    values: [Option<Value>; 4],
}

trait Registry {
    fn read(&self, hive: Hive, key: &str, name: &str) -> Result<Option<Value>>;
    fn write(&self, key: &str, name: &str, value: &Value) -> Result<()>;
    fn delete(&self, key: &str, name: &str) -> Result<()>;
}
struct WindowsRegistry;
impl Registry for WindowsRegistry {
    fn read(&self, hive: Hive, key: &str, name: &str) -> Result<Option<Value>> {
        registry::read(hive, key, name)
    }
    fn write(&self, key: &str, name: &str, value: &Value) -> Result<()> {
        registry::write_user(key, name, value)
    }
    fn delete(&self, key: &str, name: &str) -> Result<()> {
        registry::delete_user(key, name).map(|_| ())
    }
}

fn read_text(reg: &impl Registry, hive: Hive, key: &str, name: &str) -> Result<Option<String>> {
    Ok(reg
        .read(hive, key, name)?
        .and_then(|v| v.as_text().map(str::to_owned)))
}
fn class_command(reg: &impl Registry, key: &str) -> Result<Option<String>> {
    let user = read_text(reg, Hive::CurrentUser, key, "")?;
    match user {
        Some(_) => Ok(user),
        None => read_text(reg, Hive::LocalMachine, key, ""),
    }
}
fn backup(reg: &impl Registry) -> Result<Option<Backup>> {
    let Some(value) = reg.read(Hive::CurrentUser, BACKUP_KEY, BACKUP_VALUE)? else {
        return Ok(None);
    };
    let invalid = || GateError::Other("CC Switch 接收方式的备份无法读取，未改动登记".into());
    let text = value.as_text().ok_or_else(invalid)?;
    serde_json::from_str(text).map(Some).map_err(|_| invalid())
}
fn points_to(command: Option<&str>, exe: &Path) -> bool {
    command.and_then(exe_in).is_some_and(|p| same_exe(&p, exe))
}
fn inspect(reg: &impl Registry, exe: &Path, dev_build: bool) -> Result<DeepLinkHandlerStatus> {
    let ordinary = class_command(reg, KEYS[3].0)?;
    let chosen = read_text(
        reg,
        Hive::CurrentUser,
        r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\ccswitch\UserChoice",
        "ProgId",
    )?;
    let command = match chosen.as_deref() {
        Some(id) if !id.is_empty() => {
            class_command(reg, &format!(r"Software\Classes\{id}\shell\open\command"))?
        }
        _ => ordinary.clone(),
    };
    let saved = backup(reg)?;
    let registered = points_to(ordinary.as_deref(), exe);
    Ok(DeepLinkHandlerStatus {
        enabled: points_to(command.as_deref(), exe),
        registered,
        can_restore: saved.is_some() && registered,
        dev_build,
        handler: command
            .as_deref()
            .and_then(exe_in)
            .map(|p| p.display().to_string()),
        user_choice: chosen.is_some() && !points_to(command.as_deref(), exe),
    })
}
fn restore_values(reg: &impl Registry, saved: &Backup) -> Result<()> {
    for ((key, name), value) in KEYS.iter().zip(&saved.values) {
        match value {
            Some(value) => reg.write(key, name, value)?,
            None => reg.delete(key, name)?,
        }
    }
    reg.delete(BACKUP_KEY, BACKUP_VALUE)
}
fn write_backup(reg: &impl Registry, saved: &Backup) -> Result<()> {
    let encoded = serde_json::to_string(saved).map_err(|e| GateError::Other(e.to_string()))?;
    reg.write(BACKUP_KEY, BACKUP_VALUE, &Value::Text(encoded))
}
fn set_in(
    reg: &impl Registry,
    exe: &Path,
    dev_build: bool,
    enabled: bool,
) -> Result<DeepLinkHandlerStatus> {
    if dev_build {
        return Err(GateError::Other(
            "开发版不接管网页导入；请在安装版里开启，开发时可直接粘贴链接".into(),
        ));
    }
    let current = read_text(reg, Hive::CurrentUser, KEYS[3].0, "")?;
    if !enabled {
        if let Some(saved) = backup(reg)? {
            // 别的程序后来接管了协议：不把它覆盖回更旧的处理者。
            if points_to(current.as_deref(), exe) && same_exe(Path::new(&saved.owner), exe) {
                restore_values(reg, &saved)?;
            }
        }
        return inspect(reg, exe, dev_build);
    }
    if points_to(current.as_deref(), exe) {
        return inspect(reg, exe, dev_build);
    }
    let previous_backup = backup(reg)?;
    let mut previous_values: [Option<Value>; 4] = Default::default();
    for (slot, (key, name)) in previous_values.iter_mut().zip(KEYS) {
        *slot = reg.read(Hive::CurrentUser, key, name)?;
        if matches!(slot, Some(Value::Other(_))) {
            return Err(GateError::Other(
                "现有 CC Switch 协议登记包含无法原样还原的值，未改动".into(),
            ));
        }
    }
    let values = if let Some(saved) = previous_backup
        .as_ref()
        .filter(|saved| points_to(current.as_deref(), Path::new(&saved.owner)))
    {
        // 安装位置变了但处理者仍是旧 QB：沿用真正的原处理者，不能把旧 QB
        // 当成还原目标盖掉备份。第三方后来接管时则以下面的现状作为新备份。
        saved.values.clone()
    } else {
        previous_values.clone()
    };
    let saved = Backup {
        owner: exe.display().to_string(),
        values,
    };
    write_backup(reg, &saved)?;
    let values = [
        "URL:CC Switch import (QB Gate)".into(),
        String::new(),
        format!("\"{}\",0", exe.display()),
        format!("\"{}\" \"%1\"", exe.display()),
    ];
    for ((key, name), value) in KEYS.iter().zip(values) {
        if let Err(error) = reg.write(key, name, &Value::Text(value)) {
            // 登记失败要退回本次操作前的状态，包括旧安装位置持有的备份。
            let rollback = restore_values(
                reg,
                &Backup {
                    owner: exe.display().to_string(),
                    values: previous_values,
                },
            )
            .and_then(|()| match &previous_backup {
                Some(previous) => write_backup(reg, previous),
                None => Ok(()),
            });
            return match rollback {
                Ok(()) => Err(error),
                Err(e) => Err(GateError::Other(format!(
                    "登记失败：{error}；还原未完成：{e}"
                ))),
            };
        }
    }
    inspect(reg, exe, dev_build)
}

pub fn status() -> Result<DeepLinkHandlerStatus> {
    inspect(
        &WindowsRegistry,
        &std::env::current_exe()?,
        cfg!(debug_assertions),
    )
}
pub fn set(enabled: bool) -> Result<DeepLinkHandlerStatus> {
    set_in(
        &WindowsRegistry,
        &std::env::current_exe()?,
        cfg!(debug_assertions),
        enabled,
    )
}
/// 安装器卸载钩子调用；不打开数据库、窗口或单实例 IPC。
pub fn restore_for_uninstall() -> Result<()> {
    set(false).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Fake(RefCell<BTreeMap<(String, String), Value>>);
    impl Registry for Fake {
        fn read(&self, hive: Hive, key: &str, name: &str) -> Result<Option<Value>> {
            if hive == Hive::LocalMachine {
                return Ok(None);
            }
            Ok(self.0.borrow().get(&(key.into(), name.into())).cloned())
        }
        fn write(&self, key: &str, name: &str, value: &Value) -> Result<()> {
            self.0
                .borrow_mut()
                .insert((key.into(), name.into()), value.clone());
            Ok(())
        }
        fn delete(&self, key: &str, name: &str) -> Result<()> {
            self.0.borrow_mut().remove(&(key.into(), name.into()));
            Ok(())
        }
    }
    #[test]
    fn explicit_takeover_can_restore_the_previous_handler_exactly() {
        let reg = Fake::default();
        reg.write(
            KEYS[3].0,
            "",
            &Value::Text(r#""C:\Apps\CC Switch\cc-switch.exe" "%1""#.into()),
        )
        .unwrap();
        reg.write(KEYS[0].0, "", &Value::Text("CC Switch".into()))
            .unwrap();
        let before = reg.0.borrow().clone();
        let exe = Path::new(r"C:\Apps\QB Gate\qb-gate.exe");
        assert!(!inspect(&reg, exe, false).unwrap().enabled);
        assert_eq!(*reg.0.borrow(), before, "read must never register");
        assert!(set_in(&reg, exe, false, true).unwrap().enabled);
        assert!(set_in(&reg, exe, false, true).unwrap().enabled);
        assert!(!set_in(&reg, exe, false, false).unwrap().enabled);
        assert_eq!(*reg.0.borrow(), before);
    }
    #[test]
    fn an_empty_registration_restores_to_empty_and_dev_builds_cannot_take_over() {
        let reg = Fake::default();
        let exe = Path::new(r"C:\QB Gate\qb-gate.exe");
        assert!(set_in(&reg, exe, true, true).is_err());
        assert!(reg.0.borrow().is_empty());
        set_in(&reg, exe, false, true).unwrap();
        set_in(&reg, exe, false, false).unwrap();
        assert!(reg.0.borrow().is_empty());
    }

    #[test]
    fn a_windows_user_choice_is_reported_without_being_overwritten() {
        let reg = Fake::default();
        let exe = Path::new(r"C:\QB Gate\qb-gate.exe");
        set_in(&reg, exe, false, true).unwrap();
        reg.write(
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\ccswitch\UserChoice",
            "ProgId",
            &Value::Text("CCSwitch.Url".into()),
        )
        .unwrap();
        reg.write(
            r"Software\Classes\CCSwitch.Url\shell\open\command",
            "",
            &Value::Text(r#""C:\CC Switch\cc-switch.exe" "%1""#.into()),
        )
        .unwrap();
        let status = inspect(&reg, exe, false).unwrap();
        assert!(status.registered);
        assert!(status.can_restore);
        assert!(!status.enabled);
        assert!(status.user_choice);
    }
    #[test]
    fn restoring_never_overwrites_a_newer_handler() {
        let reg = Fake::default();
        let exe = Path::new(r"C:\QB Gate\qb-gate.exe");
        set_in(&reg, exe, false, true).unwrap();
        reg.write(
            KEYS[3].0,
            "",
            &Value::Text(r#""C:\New\cc-switch.exe" "%1""#.into()),
        )
        .unwrap();
        let before = reg.0.borrow().clone();
        set_in(&reg, exe, false, false).unwrap();
        assert_eq!(*reg.0.borrow(), before);
    }

    #[test]
    fn moving_the_install_preserves_the_original_handler_backup() {
        let reg = Fake::default();
        reg.write(
            KEYS[3].0,
            "",
            &Value::Text(r#""C:\Apps\CC Switch\cc-switch.exe" "%1""#.into()),
        )
        .unwrap();
        let original = reg.0.borrow().clone();
        let old_exe = Path::new(r"C:\Old QB Gate\qb-gate.exe");
        let new_exe = Path::new(r"D:\New QB Gate\qb-gate.exe");
        set_in(&reg, old_exe, false, true).unwrap();
        assert!(set_in(&reg, new_exe, false, true).unwrap().enabled);
        assert_eq!(
            backup(&reg).unwrap().unwrap().owner,
            new_exe.display().to_string()
        );
        // 旧路径卸载不得还原新位置刚接管的登记。
        set_in(&reg, old_exe, false, false).unwrap();
        assert!(inspect(&reg, new_exe, false).unwrap().enabled);
        set_in(&reg, new_exe, false, false).unwrap();
        assert_eq!(*reg.0.borrow(), original);
    }

    #[test]
    fn re_enabling_after_another_app_takes_over_backs_up_that_new_handler() {
        let reg = Fake::default();
        let old_exe = Path::new(r"C:\Old QB Gate\qb-gate.exe");
        let new_exe = Path::new(r"D:\New QB Gate\qb-gate.exe");
        set_in(&reg, old_exe, false, true).unwrap();
        reg.write(
            KEYS[3].0,
            "",
            &Value::Text(r#""C:\New Switch\cc-switch.exe" "%1""#.into()),
        )
        .unwrap();
        let mut third_party = reg.0.borrow().clone();
        third_party.remove(&(BACKUP_KEY.into(), BACKUP_VALUE.into()));
        set_in(&reg, new_exe, false, true).unwrap();
        set_in(&reg, new_exe, false, false).unwrap();
        assert_eq!(*reg.0.borrow(), third_party);
    }

    #[test]
    fn a_failed_move_restores_the_previous_registration_and_its_backup() {
        struct FailsOnce<'a> {
            inner: &'a Fake,
            failed: Cell<bool>,
        }
        impl Registry for FailsOnce<'_> {
            fn read(&self, hive: Hive, key: &str, name: &str) -> Result<Option<Value>> {
                self.inner.read(hive, key, name)
            }
            fn write(&self, key: &str, name: &str, value: &Value) -> Result<()> {
                if key == KEYS[2].0 && !self.failed.replace(true) {
                    return Err(GateError::Other("fixture write failure".into()));
                }
                self.inner.write(key, name, value)
            }
            fn delete(&self, key: &str, name: &str) -> Result<()> {
                self.inner.delete(key, name)
            }
        }
        let reg = Fake::default();
        reg.write(
            KEYS[3].0,
            "",
            &Value::Text(r#""C:\CC Switch\cc-switch.exe" "%1""#.into()),
        )
        .unwrap();
        set_in(&reg, Path::new(r"C:\Old QB\qb-gate.exe"), false, true).unwrap();
        let before = reg.0.borrow().clone();
        let failing = FailsOnce {
            inner: &reg,
            failed: Cell::new(false),
        };
        assert!(set_in(&failing, Path::new(r"D:\New QB\qb-gate.exe"), false, true).is_err());
        assert_eq!(*reg.0.borrow(), before);
    }

    #[test]
    fn an_invalid_backup_type_is_not_treated_as_a_missing_backup() {
        let reg = Fake::default();
        let exe = Path::new(r"C:\QB\qb-gate.exe");
        set_in(&reg, exe, false, true).unwrap();
        reg.write(BACKUP_KEY, BACKUP_VALUE, &Value::Binary(vec![1, 2]))
            .unwrap();
        let before = reg.0.borrow().clone();
        assert!(set_in(&reg, exe, false, false).is_err());
        assert_eq!(*reg.0.borrow(), before);
    }
}
