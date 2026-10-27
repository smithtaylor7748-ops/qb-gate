//! 注册表（2026-10-27，中转站重构阶段 7）：开机自启（X7）与环境变量清理（X5）共用的那几下。
//!
//! # 为什么不走 `reg.exe`
//!
//! 别处（`purge_ops`、`sysenv::checkup`）一直是起一个 `reg query` / `reg delete` 子进程再解析输出。
//! 阶段 7 的两件都不适合那样：
//!
//! - 开机自启要写 `HKCU\…\CurrentVersion\Run`。`reg.exe add …\Run` 这一行命令本身就是杀软行为规则里
//!   最典型的「持久化」特征 —— 面板已经因为「行为像木马」被隔离过四次（`CLAUDE.md`「杀软会把这个面板当木马」）。
//!   正经程序登记开机自启都是直接调注册表 API（Tauri 的 autostart 插件、cc-switch 用的 `auto-launch` 都是）。
//! - 环境变量要原样备份、原样还原：`reg query` 的输出认不出值两头的空格，也不好分 `REG_SZ` / `REG_EXPAND_SZ` ——
//!   带 `%USERPROFILE%` 的那种按 `REG_SZ` 写回去就再也不展开了。
//!
//! # 写只认当前用户
//!
//! 写和删只有 HKCU 一侧（[`write_user`] / [`delete_user`]）。面板不提权（安装是 `currentUser`、
//! 没有 requireAdministrator 清单），HKLM 写下去也是「拒绝访问」—— 与其让调用方去撞，不如没有那条路。
//! 读两边都行（[`Hive`]）。
//!
//! 非 Windows 一律「什么都没有」、写就报错：本项目只发 Windows 版，这条分支只为别的平台上 `cargo test` 跑得起来。

use crate::error::{GateError, Result};

/// 读哪一棵。写只有 HKCU，见模块头。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// 一个值。只把这里用得到的几种读成内容，别的只记类型号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// `REG_SZ`。
    Text(String),
    /// `REG_EXPAND_SZ`：里面的 `%VAR%` 由读它的程序展开。写回去时必须还是这个类型。
    Expand(String),
    /// `REG_BINARY`。
    Binary(Vec<u8>),
    /// 别的类型（DWORD、多字符串……）。只记类型号，不读内容 —— 用不到，也就不替它猜格式。
    Other(u32),
}

impl Value {
    /// 两种字符串类型的内容；别的类型是 `None`。
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) | Value::Expand(s) => Some(s),
            _ => None,
        }
    }
}

/// `REG_SZ` / `REG_EXPAND_SZ` 的字节 → 字符串。
///
/// 字节是 UTF-16LE，本该以 NUL 结尾，但注册表不保证（别的程序写进来的什么样都有）：
/// 读到第一个 NUL 为止，没有就读完；单出来的最后一个字节丢掉。坏的代理项替换成 U+FFFD，不报错。
pub fn decode_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// 字符串 → `REG_SZ` / `REG_EXPAND_SZ` 的字节（UTF-16LE，带结尾的 NUL）。
pub fn encode_text(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// 读一个值。键或值不在都是 `Ok(None)` —— 「没有」是答案，不是错误。
pub fn read(hive: Hive, key: &str, name: &str) -> Result<Option<Value>> {
    imp::read(hive, key, name)
}

/// 一个键下面的全部值（名字 + 内容），按注册表给的顺序。键不在就是空的。
pub fn values(hive: Hive, key: &str) -> Result<Vec<(String, Value)>> {
    imp::values(hive, key)
}

/// 在 HKCU 下写一个值（键不在就建）。只收 [`Value::Text`] / [`Value::Expand`] / [`Value::Binary`]。
pub fn write_user(key: &str, name: &str, value: &Value) -> Result<()> {
    if matches!(value, Value::Other(_)) {
        return Err(GateError::Other(
            "只写得了字符串和二进制这两类注册表值".into(),
        ));
    }
    imp::write_user(key, name, value)
}

/// 删 HKCU 下的一个值。删掉了是 `true`，本来就没有是 `false`。
pub fn delete_user(key: &str, name: &str) -> Result<bool> {
    imp::delete_user(key, name)
}

/// 告诉正在跑的程序「用户环境变量改了」（`WM_SETTINGCHANGE`，参数 `"Environment"`）。
///
/// 不发的话，资源管理器手里那份环境还是旧的：从开始菜单、桌面新开的终端照样带着刚删掉的变量，
/// 要注销一次才干净（`setx`、系统「环境变量」对话框改完都会发这一条）。
/// 已经在跑的程序不受影响 —— 进程的环境是起来那一刻抄下的，包括面板自己。
/// 有窗口卡死时最多等 5 秒（`SMTO_ABORTIFHUNG`），不会把调用方挂住。
pub fn broadcast_environment_changed() {
    imp::broadcast_environment_changed()
}

#[cfg(windows)]
mod imp {
    use super::{decode_text, encode_text, Hive, Value};
    use crate::error::{GateError, Result};
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, LPARAM,
        WIN32_ERROR, WPARAM,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW,
        RegQueryInfoKeyW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_EXPAND_SZ,
        REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
    };

    /// 打开的键，离开作用域就关。
    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: 句柄由 RegOpenKeyExW / RegCreateKeyExW 成功时给出，只在这里关一次。
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    fn root(hive: Hive) -> HKEY {
        match hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn fail(api: &'static str, rc: WIN32_ERROR) -> GateError {
        GateError::WinApi { api, code: rc.0 }
    }

    fn open(hive: Hive, key: &str, access: REG_SAM_FLAGS) -> Result<Option<Key>> {
        let k = wide(key);
        let mut h = HKEY::default();
        // SAFETY: `k` 以 NUL 结尾；`h` 只在成功时被写入，交给 `Key` 负责关闭。
        let rc = unsafe { RegOpenKeyExW(root(hive), PCWSTR(k.as_ptr()), 0, access, &mut h) };
        if rc == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if rc != ERROR_SUCCESS {
            return Err(fail("RegOpenKeyExW", rc));
        }
        Ok(Some(Key(h)))
    }

    fn value_of(ty: REG_VALUE_TYPE, data: &[u8]) -> Value {
        match ty {
            REG_SZ => Value::Text(decode_text(data)),
            REG_EXPAND_SZ => Value::Expand(decode_text(data)),
            REG_BINARY => Value::Binary(data.to_vec()),
            other => Value::Other(other.0),
        }
    }

    pub fn read(hive: Hive, key: &str, name: &str) -> Result<Option<Value>> {
        let Some(k) = open(hive, key, KEY_QUERY_VALUE)? else {
            return Ok(None);
        };
        let n = wide(name);
        // 先问大小再读；两次之间值变长了（别的程序正在写）就按新的大小再来，最多几次。
        for _ in 0..4 {
            let mut ty = REG_VALUE_TYPE::default();
            let mut size = 0u32;
            // SAFETY: 只问类型与大小，不给缓冲区。
            let rc = unsafe {
                RegQueryValueExW(
                    k.0,
                    PCWSTR(n.as_ptr()),
                    None,
                    Some(&mut ty),
                    None,
                    Some(&mut size),
                )
            };
            if rc == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if rc != ERROR_SUCCESS {
                return Err(fail("RegQueryValueExW", rc));
            }
            let mut buf = vec![0u8; size as usize];
            let mut got = size;
            // SAFETY: 缓冲区长度就是 `got` 字节。
            let rc = unsafe {
                RegQueryValueExW(
                    k.0,
                    PCWSTR(n.as_ptr()),
                    None,
                    Some(&mut ty),
                    Some(buf.as_mut_ptr()),
                    Some(&mut got),
                )
            };
            if rc == ERROR_MORE_DATA {
                continue;
            }
            if rc == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if rc != ERROR_SUCCESS {
                return Err(fail("RegQueryValueExW", rc));
            }
            buf.truncate(got as usize);
            return Ok(Some(value_of(ty, &buf)));
        }
        Err(GateError::Other(format!(
            "注册表值 {name} 一直在变，读不稳"
        )))
    }

    pub fn values(hive: Hive, key: &str) -> Result<Vec<(String, Value)>> {
        let Some(k) = open(hive, key, KEY_READ)? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let mut index = 0u32;
        let mut retries = 0;
        loop {
            let mut max_name = 0u32;
            let mut max_data = 0u32;
            // SAFETY: 只取最长的值名（字符数，不含 NUL）与最长的数据（字节）。
            let rc = unsafe {
                RegQueryInfoKeyW(
                    k.0,
                    PWSTR::null(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&mut max_name),
                    Some(&mut max_data),
                    None,
                    None,
                )
            };
            if rc != ERROR_SUCCESS {
                return Err(fail("RegQueryInfoKeyW", rc));
            }
            let mut name = vec![0u16; max_name as usize + 1];
            let mut data = vec![0u8; max_data as usize];
            let mut name_len = name.len() as u32;
            let mut data_len = data.len() as u32;
            let mut ty = 0u32;
            // SAFETY: 两个缓冲区的长度分别就是 `name_len`（字符）与 `data_len`（字节）。
            let rc = unsafe {
                RegEnumValueW(
                    k.0,
                    index,
                    PWSTR(name.as_mut_ptr()),
                    &mut name_len,
                    None,
                    Some(&mut ty),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                )
            };
            if rc == ERROR_NO_MORE_ITEMS {
                return Ok(out);
            }
            if rc == ERROR_MORE_DATA && retries < 4 {
                // 枚举期间有值变长了：按新的最大长度重读同一项。
                retries += 1;
                continue;
            }
            if rc != ERROR_SUCCESS {
                return Err(fail("RegEnumValueW", rc));
            }
            retries = 0;
            name.truncate(name_len as usize);
            data.truncate(data_len as usize);
            out.push((
                String::from_utf16_lossy(&name),
                value_of(REG_VALUE_TYPE(ty), &data),
            ));
            index += 1;
        }
    }

    pub fn write_user(key: &str, name: &str, value: &Value) -> Result<()> {
        let k = wide(key);
        let mut h = HKEY::default();
        // SAFETY: `k` 以 NUL 结尾；`h` 只在成功时被写入，交给 `Key` 负责关闭。
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(k.as_ptr()),
                0,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut h,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(fail("RegCreateKeyExW", rc));
        }
        let key = Key(h);
        let (ty, bytes) = match value {
            Value::Text(s) => (REG_SZ, encode_text(s)),
            Value::Expand(s) => (REG_EXPAND_SZ, encode_text(s)),
            Value::Binary(b) => (REG_BINARY, b.clone()),
            Value::Other(_) => unreachable!("write_user 在外面就拦掉了"),
        };
        let n = wide(name);
        // SAFETY: `n` 以 NUL 结尾；数据按类型编好。
        let rc = unsafe { RegSetValueExW(key.0, PCWSTR(n.as_ptr()), 0, ty, Some(&bytes)) };
        if rc != ERROR_SUCCESS {
            return Err(fail("RegSetValueExW", rc));
        }
        Ok(())
    }

    pub fn delete_user(key: &str, name: &str) -> Result<bool> {
        let Some(k) = open(Hive::CurrentUser, key, KEY_SET_VALUE)? else {
            return Ok(false);
        };
        let n = wide(name);
        // SAFETY: `n` 以 NUL 结尾。
        let rc = unsafe { RegDeleteValueW(k.0, PCWSTR(n.as_ptr())) };
        if rc == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        if rc != ERROR_SUCCESS {
            return Err(fail("RegDeleteValueW", rc));
        }
        Ok(true)
    }

    pub fn broadcast_environment_changed() {
        use windows::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let what = wide("Environment");
        // SAFETY: lParam 指向以 NUL 结尾的 "Environment"，在调用返回之前一直有效。
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(what.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Hive, Value};
    use crate::error::{GateError, Result};

    pub fn read(_hive: Hive, _key: &str, _name: &str) -> Result<Option<Value>> {
        Ok(None)
    }
    pub fn values(_hive: Hive, _key: &str) -> Result<Vec<(String, Value)>> {
        Ok(Vec::new())
    }
    pub fn write_user(_key: &str, _name: &str, _value: &Value) -> Result<()> {
        Err(GateError::Other("仅 Windows 支持".into()))
    }
    pub fn delete_user(_key: &str, _name: &str) -> Result<bool> {
        Err(GateError::Other("仅 Windows 支持".into()))
    }
    pub fn broadcast_environment_changed() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips_through_the_wire_format_with_a_trailing_nul() {
        for s in [
            "",
            "D:\\工具\\张三\\bin",
            "%USERPROFILE%\\.local\\bin",
            " 两头有空格 ",
        ] {
            let bytes = encode_text(s);
            assert_eq!(bytes.len() % 2, 0);
            assert_eq!(&bytes[bytes.len() - 2..], &[0, 0], "要带结尾的 NUL");
            assert_eq!(decode_text(&bytes), s, "两头的空格也要原样回来");
        }
    }

    #[test]
    fn text_without_a_nul_or_with_an_odd_tail_still_decodes() {
        // 别的程序写进来的值不保证以 NUL 结尾，也可能长度是奇数。
        let mut bytes: Vec<u8> = "abc".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_text(&bytes), "abc");
        bytes.push(0x41);
        assert_eq!(decode_text(&bytes), "abc", "单出来的那个字节丢掉");
        // 第一个 NUL 之后的不算。
        let with_tail: Vec<u8> = "ab\0zz".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_text(&with_tail), "ab");
    }

    #[test]
    fn only_strings_and_binary_can_be_written() {
        // 拦在调用注册表之前 —— 这条测试不碰真实的注册表。
        assert!(write_user(r"Software\qb-gate-test-never", "x", &Value::Other(4)).is_err());
    }

    #[test]
    fn a_value_that_cannot_exist_reads_as_none_not_as_an_error() {
        // 单测不许碰真实的运行期状态：只读，而且问的是不可能存在的键。
        let key = r"Software\qb-gate-test-key-that-never-exists-7c1d";
        assert_eq!(read(Hive::CurrentUser, key, "x").unwrap(), None);
        assert!(values(Hive::CurrentUser, key).unwrap().is_empty());
    }

    #[test]
    fn as_text_covers_both_string_kinds_only() {
        assert_eq!(Value::Text("a".into()).as_text(), Some("a"));
        assert_eq!(Value::Expand("%X%".into()).as_text(), Some("%X%"));
        assert_eq!(Value::Binary(vec![1]).as_text(), None);
        assert_eq!(Value::Other(4).as_text(), None);
    }
}
