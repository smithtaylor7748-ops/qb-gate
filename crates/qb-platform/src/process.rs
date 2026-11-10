//! Windows helper-process policy: background commands never create console windows.
//!
//! # ⛔ PowerShell 一律从这里起
//!
//! 面板拉起的 PowerShell 都带 `CREATE_NO_WINDOW`，**没有控制台**。
//! 没有控制台时 `[Console]::OutputEncoding` 会落到系统代码页上，而那个代码页
//! 编不出来的字符会被换成一个字面的 `?` —— 不是乱码，是**真的丢了**，
//! Rust 这边再怎么解码也回不来。
//!
//! 实测（zh-CN，2026-09-16）：`Get-NetAdapter` 的「以太网」到 Rust 手里是 `???`，
//! 「蓝牙网络连接」是 `??????`。出站锁那一页整列网卡名都成了问号，而那串问号
//! 还会被原样送回去当 `-InterfaceAlias` —— 规则加在一个不存在的接口上，
//! 界面却报「已加 N 条」。**这是一个会静默失效的功能，不是显示问题。**
//!
//! 所以拉 PowerShell 只走 [`powershell_std`] / [`powershell_tokio`]：
//! 它们把 `-NoProfile -NonInteractive` 和 UTF-8 前奏一次性钉死，
//! 谁也不用记得加。`src-tauri/tests/architecture.rs` 里有一条测试守着这件事。
//!
//! 这条约束跟区域设置无关：英文机器上 `é`、`ü`、西里尔字母同样编不进
//! CP437/CP850，一样会变成 `?`。开源出去之后这是**每一台机器**的问题。

#[cfg(windows)]
pub fn hidden_std(mut cmd: std::process::Command) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn hidden_std(cmd: std::process::Command) -> std::process::Command {
    cmd
}

#[cfg(windows)]
pub fn hidden_tokio(mut cmd: tokio::process::Command) -> tokio::process::Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn hidden_tokio(cmd: tokio::process::Command) -> tokio::process::Command {
    cmd
}

// ---------------------------------------------------------------- 控制台输出的解码

/// 子进程写进日志文件的一行字节 → 文字（2026-09-29，酒馆的控制台）。
///
/// 先按 UTF-8（node / npm 写的都是）；**不是合法的 UTF-8** 就按系统的 OEM 代码页解 ——
/// `cmd` 自己的报错走它，中文 Windows 上是 GBK。实例：没装 Node.js 时 `Start.bat` 的第一句
/// 「'npm' 不是内部或外部命令」，按 UTF-8 有损解码是一串替换字符，而这恰恰是最要紧的那一句。
/// OEM 也解不出来才退回有损 UTF-8。
pub fn decode_console_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => oem_decode(bytes).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned()),
    }
}

#[cfg(windows)]
fn oem_decode(bytes: &[u8]) -> Option<String> {
    use windows::Win32::Globalization::{
        MultiByteToWideChar, CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS,
    };
    if bytes.is_empty() || bytes.len() > i32::MAX as usize {
        return None;
    }
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        if n <= 0 {
            return None;
        }
        let mut wide = vec![0u16; n as usize];
        let m = MultiByteToWideChar(
            CP_OEMCP,
            MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
            bytes,
            Some(&mut wide),
        );
        if m <= 0 {
            return None;
        }
        wide.truncate(m as usize);
        Some(String::from_utf16_lossy(&wide))
    }
}

#[cfg(not(windows))]
fn oem_decode(_: &[u8]) -> Option<String> {
    None
}

// ---------------------------------------------------------------- Job

/// 一个 `KILL_ON_JOB_CLOSE` 的 Job 对象：最后一个句柄一关（包括面板进程退出），
/// 挂进去的进程树全部结束。
///
/// 给那些**面板自己拿着 stdio 管道**的短命子进程用（酒馆的 GPT 桥接每个请求起一次
/// `codex exec`）：它们走不了 `sessions::start`（那条路不给管道），但同样不能在面板
/// 退出后变成孤儿 —— 一个还在跑的 `codex.exe` 会继续替使用者烧额度。
/// `tokio::process::Command` 的 `kill_on_drop` 只管 `Child` 被 drop 的那一刻，
/// 面板被杀、崩溃时 drop 根本不会跑；Job 是内核层的，靠得住。
pub struct KillOnCloseJob(
    #[cfg(windows)] isize,
    // 保留终止前的进程句柄，跨 await / 取消仍能确认真正退出。
    #[cfg(windows)] std::sync::Mutex<Vec<std::os::windows::io::OwnedHandle>>,
);

#[cfg(windows)]
impl KillOnCloseJob {
    pub fn new() -> crate::error::Result<Self> {
        use windows::Win32::System::JobObjects::*;
        unsafe {
            let job = CreateJobObjectW(None, None)
                .map_err(|e| crate::error::GateError::Other(format!("建不出 Job 对象：{e}")))?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if let Err(e) = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) {
                let _ = windows::Win32::Foundation::CloseHandle(job);
                return Err(crate::error::GateError::Other(format!(
                    "Job 对象设不上 KILL_ON_JOB_CLOSE：{e}"
                )));
            }
            Ok(Self(job.0 as isize, std::sync::Mutex::new(Vec::new())))
        }
    }

    /// 把一个已经起来的子进程挂进来。
    ///
    /// 起来之后才挂有一个极短的窗口（子进程在这之前派生的孙进程不在 Job 里）；
    /// 对 `codex exec` 这种先要读 stdin、解析配置再干活的程序可以接受。
    pub fn assign(&self, child: &tokio::process::Child) -> crate::error::Result<()> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;
        let raw = child.raw_handle().ok_or_else(|| {
            crate::error::GateError::Other("子进程句柄已经没了，挂不进 Job".into())
        })?;
        unsafe {
            AssignProcessToJobObject(HANDLE(self.0 as *mut core::ffi::c_void), HANDLE(raw))
                .map_err(|e| crate::error::GateError::Other(format!("子进程挂不进 Job：{e}")))
        }
    }

    /// 结束 Job 里的全部进程（不关句柄，之后还能继续挂新的）。
    pub fn terminate_all(&self) {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::TerminateJobObject;
        unsafe {
            let _ = TerminateJobObject(HANDLE(self.0 as *mut core::ffi::c_void), 1);
        }
    }

    /// 安装器退出不代表它的后代已退出。确认整棵树释放后才能删除下载文件、
    /// 恢复执行锁；否则刚重锁的文件还可能被一个迟到的子进程替换。
    pub async fn terminate_and_wait(
        &self,
        timeout: std::time::Duration,
    ) -> crate::error::Result<()> {
        self.request_termination()?;
        let deadline = tokio::time::Instant::now() + timeout;
        while self.has_active_processes()? {
            if tokio::time::Instant::now() >= deadline {
                return Err(crate::error::GateError::Other(
                    "安装进程树尚未完全退出，请稍后重试更新。".into(),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Ok(())
    }

    /// 只给安装守卫的 Drop 使用：Future 取消时不能 await，但也不能先恢复
    /// 执行锁、后结束仍可能写文件的安装进程。正常路径使用异步版本。
    pub fn terminate_and_wait_blocking(
        &self,
        timeout: std::time::Duration,
    ) -> crate::error::Result<()> {
        self.request_termination()?;
        let deadline = std::time::Instant::now() + timeout;
        while self.has_active_processes()? {
            if std::time::Instant::now() >= deadline {
                return Err(crate::error::GateError::Other(
                    "安装进程树尚未完全退出，请稍后重试更新。".into(),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Ok(())
    }

    fn request_termination(&self) -> crate::error::Result<()> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::TerminateJobObject;
        // ActiveProcesses 会早于进程对象变为 signaled，不能只等待 Job 计数。
        // 终止前持有成员句柄；即使本次异步等待被取消，同步 Drop 仍能继续等它们。
        let tracked = self.track_processes();
        unsafe { TerminateJobObject(HANDLE(self.0 as *mut core::ffi::c_void), 1) }
            .map_err(|e| crate::error::GateError::Other(format!("无法结束安装进程树：{e}")))?;
        tracked
    }

    fn track_processes(&self) -> crate::error::Result<()> {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows::Win32::Foundation::{BOOL, ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, HANDLE};
        use windows::Win32::System::JobObjects::{
            IsProcessInJob, JobObjectBasicProcessIdList, QueryInformationJobObject,
            JOBOBJECT_BASIC_PROCESS_ID_LIST,
        };
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };
        let failure = |e| crate::error::GateError::Other(format!("无法跟踪安装进程退出：{e}"));
        let mut capacity = 16;
        while capacity <= 65536 {
            // 用 usize 数组提供正确对齐；结构末尾是变长的进程 ID 列表。
            let mut data = vec![0usize; 2 + capacity];
            let list = data.as_mut_ptr() as *mut JOBOBJECT_BASIC_PROCESS_ID_LIST;
            let result = unsafe {
                QueryInformationJobObject(
                    HANDLE(self.0 as *mut core::ffi::c_void),
                    JobObjectBasicProcessIdList,
                    list as *mut _,
                    (data.len() * std::mem::size_of::<usize>()) as u32,
                    None,
                )
            };
            if let Err(e) = result {
                if e.code() == windows::core::HRESULT::from_win32(ERROR_MORE_DATA.0) {
                    capacity *= 2;
                    continue;
                }
                return Err(failure(e));
            }
            let count = unsafe { (*list).NumberOfProcessIdsInList as usize };
            if count > capacity {
                capacity *= 2;
                continue;
            }
            let pids = unsafe { std::slice::from_raw_parts((*list).ProcessIdList.as_ptr(), count) };
            let mut handles = self.1.lock().unwrap_or_else(|e| e.into_inner());
            for &pid in pids {
                let raw = match unsafe {
                    OpenProcess(
                        PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                        false,
                        pid as u32,
                    )
                } {
                    Ok(handle) => handle,
                    Err(e)
                        if e.code()
                            == windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) =>
                    {
                        continue
                    }
                    Err(e) => return Err(failure(e)),
                };
                let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
                let mut member = BOOL::default();
                unsafe {
                    IsProcessInJob(raw, HANDLE(self.0 as *mut core::ffi::c_void), &mut member)
                }
                .map_err(failure)?;
                // PID 可能在查询后被回收复用；只等仍属于本 Job 的进程。
                if member.as_bool() {
                    handles.push(handle);
                }
            }
            return Ok(());
        }
        Err(crate::error::GateError::Other(
            "安装进程树持续变化，无法取得完整进程列表".into(),
        ))
    }

    fn has_active_processes(&self) -> crate::error::Result<bool> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0};
        use windows::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        use windows::Win32::System::Threading::WaitForSingleObject;
        let pending = {
            let mut handles = self.1.lock().unwrap_or_else(|e| e.into_inner());
            let mut failure = None;
            handles.retain(|handle| {
                let status = unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 0) };
                if status == WAIT_FAILED {
                    failure = Some(std::io::Error::last_os_error());
                }
                status != WAIT_OBJECT_0
            });
            if let Some(e) = failure {
                return Err(crate::error::GateError::Other(format!(
                    "等待安装进程退出失败：{e}"
                )));
            }
            !handles.is_empty()
        };
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                HANDLE(self.0 as *mut core::ffi::c_void),
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut _,
                std::mem::size_of_val(&info) as u32,
                None,
            )
        }
        .map_err(|e| crate::error::GateError::Other(format!("无法确认安装进程树已经退出：{e}")))?;
        Ok(pending || info.ActiveProcesses != 0)
    }
}

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(
                self.0 as *mut core::ffi::c_void,
            ));
        }
    }
}

// Job 句柄只是一个内核对象的引用，跨线程传递是安全的。
#[cfg(windows)]
unsafe impl Send for KillOnCloseJob {}
#[cfg(windows)]
unsafe impl Sync for KillOnCloseJob {}

#[cfg(not(windows))]
impl KillOnCloseJob {
    pub fn new() -> crate::error::Result<Self> {
        Ok(Self())
    }
    pub fn assign(&self, _child: &tokio::process::Child) -> crate::error::Result<()> {
        Ok(())
    }
    pub fn terminate_all(&self) {}
    pub async fn terminate_and_wait(
        &self,
        _timeout: std::time::Duration,
    ) -> crate::error::Result<()> {
        Ok(())
    }
    pub fn terminate_and_wait_blocking(
        &self,
        _timeout: std::time::Duration,
    ) -> crate::error::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------- PowerShell

/// 把输出编码钉死成 UTF-8 的前奏。
///
/// `New-Object System.Text.UTF8Encoding $false` 是「不带 BOM 的 UTF-8」——
/// 用 `[Text.Encoding]::UTF8` 会在每次输出前面插一个 BOM，`serde_json` 当场
/// 解析失败。整句包在 `try`/`catch` 里：真有控制台时这一行本来就没必要，
/// 而任何一种失败都不该让整段脚本停下来。
const UTF8_PRELUDE: &str =
    "try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch { }\n";

/// 给一段脚本加上 UTF-8 前奏。**拼脚本的唯一入口**，单测钉着它的形状。
///
/// 用换行而不是 `;` 分隔：脚本本身可能以注释（`#`）开头，用分号接的话
/// 那一行会把前奏和注释连成一句，后面整段都被注释掉。
pub fn ps_utf8(script: &str) -> String {
    format!("{UTF8_PRELUDE}{script}")
}

/// 起一条读得回非 ASCII 字符的 PowerShell（同步版）。
///
/// 已经带上 `-NoProfile -NonInteractive` 与 UTF-8 前奏，调用方只要 `.output()`。
/// **别再自己 `Command::new("powershell")`** —— 见本文件头。
pub fn powershell_std(script: &str) -> std::process::Command {
    let mut cmd = hidden_std(std::process::Command::new("powershell"));
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &ps_utf8(script),
    ]);
    cmd
}

/// 起一条读得回非 ASCII 字符的 PowerShell（异步版）。见 [`powershell_std`]。
pub fn powershell_tokio(script: &str) -> tokio::process::Command {
    let mut cmd = hidden_tokio(tokio::process::Command::new("powershell"));
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &ps_utf8(script),
    ]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前奏必须在最前面 —— 编码要在**第一句输出之前**就定下来。
    #[test]
    fn the_prelude_comes_before_the_script() {
        let s = ps_utf8("Get-NetAdapter");
        assert!(s.starts_with("try {"), "{s}");
        assert!(s.ends_with("Get-NetAdapter"), "{s}");
    }

    /// ⛔ 必须是**不带 BOM** 的 UTF8Encoding。
    ///
    /// 写成 `[Text.Encoding]::UTF8` 会在输出最前面插 `\u{feff}`，
    /// 而这些脚本的产物大多直接喂给 `serde_json` —— BOM 进去当场解析失败，
    /// 而且失败信息只会说「第 1 行有意外字符」，查半天查不到这里。
    #[test]
    fn the_encoding_carries_no_bom() {
        let s = ps_utf8("x");
        assert!(
            s.contains("New-Object System.Text.UTF8Encoding $false"),
            "{s}"
        );
    }

    /// 前奏与脚本之间是换行，不是分号。
    ///
    /// 用分号的话，以 `#` 注释开头的脚本会被整段接到同一行上注释掉。
    #[test]
    fn a_script_that_starts_with_a_comment_survives() {
        let s = ps_utf8("# 说明\nGet-NetAdapter");
        let last = s.lines().last().unwrap_or_default();
        assert_eq!(last, "Get-NetAdapter", "前奏把脚本吞进注释里了：{s}");
    }

    /// node / npm 写的 UTF-8 原样通过，一个字节都不换。
    #[test]
    fn utf8_console_output_passes_through_untouched() {
        let line = "SillyTavern is listening on IPv4: 127.0.0.1:8000 · 酒馆";
        assert_eq!(decode_console_bytes(line.as_bytes()), line);
        assert_eq!(decode_console_bytes(b""), "");
    }

    /// cmd 自己的报错走 OEM 代码页（中文 Windows 上是 GBK）：不是合法 UTF-8 的那一行不许变成一串替换字符。
    /// 具体解成什么取决于这台机器的代码页，这里只钉「ASCII 部分原样、不出现 U+FFFD」。
    #[cfg(windows)]
    #[test]
    fn a_line_that_is_not_utf8_is_read_in_the_oem_code_page() {
        // 系统设了「使用 Unicode UTF-8 提供全球语言支持」的机器上 OEM 代码页就是 UTF-8，
        // 那时 cmd 写出来的本来就是 UTF-8，这条没什么可测的。
        if unsafe { windows::Win32::Globalization::GetOEMCP() } == 65001 {
            return;
        }
        // GBK 的「不是」。
        let bytes = b"'npm' \xb2\xbb\xca\xc7";
        let text = decode_console_bytes(bytes);
        assert!(text.starts_with("'npm' "), "{text}");
        assert!(!text.contains('\u{fffd}'), "{text}");
        assert!(text.chars().count() > "'npm' ".len(), "{text}");
    }

    /// 前奏永远包在 try/catch 里：有控制台的场合它本来就多余，
    /// 而任何一种失败都不该把整段脚本带停。
    #[test]
    fn the_prelude_can_never_abort_the_script() {
        let s = ps_utf8("x");
        assert!(s.contains("try {"), "{s}");
        assert!(s.contains("catch { }"), "{s}");
    }
}
