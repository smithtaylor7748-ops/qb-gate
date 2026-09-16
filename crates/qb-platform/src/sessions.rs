//! Session ownership and Windows process handles. A PID alone never authorizes termination.
use crate::{
    domain::*,
    error::{GateError, Result},
    repository::Repository,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

struct Managed {
    session: Session,
    process: NativeProcess,
}
static SESSIONS: OnceLock<Mutex<BTreeMap<String, Managed>>> = OnceLock::new();
fn registry() -> &'static Mutex<BTreeMap<String, Managed>> {
    SESSIONS.get_or_init(Default::default)
}

pub fn sanitized_environment(
    inherited: impl IntoIterator<Item = (String, String)>,
    additions: Vec<(String, String)>,
) -> Vec<(String, String)> {
    const CONFLICTS: &[&str] = &[
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_USER_DATA_DIR",
        "ELECTRON_RUN_AS_NODE",
        "CODEX_HOME",
        "CODEX_ELECTRON_USER_DATA_PATH",
        "CODEX_ELECTRON_AGENT_RUN_ID",
        "CODEX_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "OPENAI_BASE_URL",
        "OPENAI_ORG_ID",
        "OPENAI_PROJECT_ID",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
        "ANTHROPIC_CUSTOM_HEADERS",
        "OPENAI_CUSTOM_HEADERS",
        "CODEX_AUTH_JSON",
    ];
    let mut out: BTreeMap<String, (String, String)> = inherited
        .into_iter()
        .filter(|(k, _)| !CONFLICTS.contains(&k.to_ascii_uppercase().as_str()))
        .map(|(k, v)| (k.to_ascii_uppercase(), (k, v)))
        .collect();
    for (k, v) in additions {
        out.insert(k.to_ascii_uppercase(), (k, v));
    }
    out.into_values().collect()
}

/// 起一个面板托管的会话。
///
/// `gated` 只回答一件事：**门禁判不过时要不要收掉它**（见
/// `LaunchTarget::stops_with_gate`）。它跟 Job Object 的
/// `KILL_ON_JOB_CLOSE` 无关 —— 那个对**所有**托管会话都要开。
///
/// 这两件事曾经共用同一个布尔值，代价是：不受门禁关停的会话
/// （中转，以及关掉开关的 Codex）拿到 `false` 就不设 KILL_ON_JOB_CLOSE，
/// 于是面板一退出，命名 Job 对象随最后一个句柄消失 —— 进程还在跑，
/// 面板却把它记成「已退出」，此后既停不掉它，切账户清场时还会把它
/// 当成官方进程一起杀掉。宁可跟着面板一起收，也不要一个面板说不清
/// 状态、又管不了的孤儿。
pub fn start(
    context: LaunchContext,
    exe: &Path,
    env: Vec<(String, String)>,
    gated: bool,
    revision: u32,
) -> Result<Session> {
    let id = crate::config_io::id();
    let args = desktop_arguments(context.client, &env);
    let process = NativeProcess::create(
        exe,
        &args,
        &context.working_dir,
        &sanitized_environment(std::env::vars(), env),
        context.client == Client::ClaudeCode,
        // 托管会话一律绑定生命周期，与「收不收」无关。
        true,
        &id,
    )?;
    let mut session = Session {
        id: id.clone(),
        context,
        pid: process.pid,
        process_created: process.created,
        started_at: chrono::Utc::now().to_rfc3339(),
        state: "running".into(),
        gated,
        detail: "进程已登记".into(),
        config_revision: revision,
    };
    if let Err(e) = Repository::open().and_then(|db| db.put("sessions", &id, &session)) {
        let _ = process.stop();
        return Err(e);
    }
    if let Err(e) = process.resume() {
        let _ = process.stop();
        session.state = "failed".into();
        session.detail = e.to_string();
        let _ = Repository::open().and_then(|db| db.put("sessions", &id, &session));
        return Err(e);
    }
    // An Electron second-instance handoff or a crashing client is not a launch.
    std::thread::sleep(std::time::Duration::from_millis(900));
    if !process.running()? {
        session.state = "failed".into();
        session.detail = "客户端启动后立即退出，请检查客户端配置".into();
        Repository::open()?.put("sessions", &id, &session)?;
        return Err(GateError::Other(session.detail));
    }
    registry().lock().unwrap().insert(
        id,
        Managed {
            session: session.clone(),
            process,
        },
    );
    // 历史会话行不清理的话会一直涨，而 workspace_state 每次都全量回前端。
    // 正在跑和待核验的不会被删，见 Repository::prune_history。
    if let Err(e) = Repository::open().and_then(|db| db.prune_history()) {
        crate::audit::write(&format!("清理历史会话记录失败：{e}"));
    }
    Ok(session)
}

/// The Windows desktop's Chromium singleton is selected before JS reads env vars.
pub fn desktop_arguments(client: Client, env: &[(String, String)]) -> Vec<String> {
    if client != Client::Codex {
        return Vec::new();
    }
    env.iter()
        .find(|(key, _)| key == "CODEX_ELECTRON_USER_DATA_PATH")
        .map(|(_, path)| vec![format!("--user-data-dir={path}")])
        .unwrap_or_default()
}

pub fn list() -> Vec<Session> {
    registry()
        .lock()
        .unwrap()
        .values()
        .map(|m| m.session.clone())
        .collect()
}

pub fn refresh() -> Result<bool> {
    let db = Repository::open()?;
    let mut changed = false;
    for m in registry().lock().unwrap().values_mut() {
        if m.session.state == "running" && !m.process.running()? {
            changed = true;
            m.session.state = "exited".into();
            m.session.detail = "进程已退出".into();
            db.put("sessions", &m.session.id, &m.session)?;
        }
    }
    Ok(changed)
}

pub fn stop(id: &str) -> Result<()> {
    let mut all = registry().lock().unwrap();
    let m = all
        .get_mut(id)
        .ok_or_else(|| GateError::Other("此会话未登记，不能依据 PID 直接终止".into()))?;
    if m.session.state != "running" {
        return Ok(());
    }
    m.process.stop()?;
    m.session.state = "stopped".into();
    m.session.detail = "已核验进程身份并关闭会话进程树".into();
    Repository::open()?.put("sessions", id, &m.session)
}

/// 放弃一条核验不了的会话记录。**只删记录，不动进程。**
///
/// `unverified`（以及 running 但不在注册表里）的会话会挡住切账户、迁移配置、
/// 恢复快照、回滚软件版本 —— 全靠 [`ensure_verified`]。而界面上又停不掉它：
/// [`stop`] 要求它在注册表里。更糟的是 `gate::stop_managed` 收不掉它就把
/// `pending_stop` 置位，看门狗此后每轮只重试关停、不再复判门禁，
/// 门就再也自动开不回来。
///
/// 多数情况下 [`recover`] 下一轮能把它降级成 `exited` 自愈。这个命令是
/// 自愈不了时（PID 被高权限进程占了，`OpenProcess` 一直 ACCESS_DENIED 之类）
/// 唯一能让使用者靠自己走出来的路，否则只能重装或者手改数据库。
///
/// 不杀进程是故意的：面板既然核验不了这个 PID 的身份，就更没有资格动它 ——
/// 「仅凭 PID 不终止任何进程」这条在这里同样成立。
pub fn forget(id: &str) -> Result<()> {
    if registry().lock().unwrap().contains_key(id) {
        return Err(GateError::Other(
            "此会话仍由面板托管，请用「停止会话」正常结束它".into(),
        ));
    }
    let db = Repository::open()?;
    let session: Session = db.get("sessions", id)?;
    if !["unverified", "running"].contains(&session.state.as_str()) {
        return Err(GateError::Other("此会话已经结束，不需要放弃".into()));
    }
    db.remove("sessions", id)?;
    crate::audit::write(&format!(
        "使用者放弃了核验不了的会话记录 {id}（PID {}，进程未被终止）",
        session.pid
    ));
    Ok(())
}

pub fn ensure_verified(predicate: impl Fn(&Session) -> bool) -> Result<()> {
    let registry = registry().lock().unwrap();
    for s in Repository::open()?.list::<Session>("sessions")? {
        if predicate(&s)
            && (s.state == "unverified" || (s.state == "running" && !registry.contains_key(&s.id)))
        {
            return Err(GateError::Other(format!(
                "会话 {} 的进程身份尚未核验；请先关闭原客户端并重新检查会话",
                s.id
            )));
        }
    }
    Ok(())
}
pub fn stop_matching(predicate: impl Fn(&Session) -> bool) -> Result<Vec<String>> {
    let ids: Vec<String> = list()
        .into_iter()
        .filter(|s| s.state == "running" && predicate(s))
        .map(|s| s.id)
        .collect();
    let mut failures = Vec::new();
    for id in &ids {
        if let Err(e) = stop(id) {
            failures.push(format!("{id}: {e}"));
        }
    }
    if let Err(e) = ensure_verified(predicate) {
        failures.push(e.to_string());
    }
    if failures.is_empty() {
        Ok(ids)
    } else {
        Err(GateError::Other(failures.join("；")))
    }
}

pub fn owns_pid(pid: u32, kind: IdentityKind) -> bool {
    registry().lock().unwrap().values().any(|m| {
        m.session.context.identity_kind == kind
            && m.session.state == "running"
            && m.process.contains(pid)
    })
}
#[cfg(windows)]
pub fn creation_time(pid: u32) -> Result<u64> {
    native::creation_time(pid)
}
#[cfg(windows)]
pub fn terminate_verified(pid: u32, time: u64) -> Result<()> {
    native::terminate_verified(pid, time)
}
#[cfg(not(windows))]
pub fn creation_time(_: u32) -> Result<u64> {
    Err(GateError::Other("需要 Windows".into()))
}
#[cfg(not(windows))]
pub fn terminate_verified(_: u32, _: u64) -> Result<()> {
    Err(GateError::Other("需要 Windows".into()))
}

pub fn recover() -> Result<()> {
    let db = Repository::open()?;
    for mut s in db.list::<Session>("sessions")? {
        if !["running", "unverified"].contains(&s.state.as_str())
            || registry().lock().unwrap().contains_key(&s.id)
        {
            continue;
        }
        match NativeProcess::reopen(s.pid, s.process_created, &s.id) {
            Ok(Some(process)) => {
                s.state = "running".into();
                s.detail = "已核验并恢复会话作业".into();
                db.put("sessions", &s.id, &s)?;
                registry().lock().unwrap().insert(
                    s.id.clone(),
                    Managed {
                        session: s,
                        process,
                    },
                );
            }
            Ok(None) => {
                s.state = "exited".into();
                s.detail = "面板恢复时原进程已退出".into();
                db.put("sessions", &s.id, &s)?;
            }
            Err(e) => {
                s.state = "unverified".into();
                s.detail = e.to_string();
                db.put("sessions", &s.id, &s)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ProgramRecord {
    pub id: String,
    pub pid: u32,
    pub created: u64,
}
pub struct OwnedProgram {
    pub record: ProgramRecord,
    process: NativeProcess,
}
impl OwnedProgram {
    pub fn launch(
        exe: &Path,
        args: &[String],
        cwd: &Path,
        env: Vec<(String, String)>,
    ) -> Result<Self> {
        let id = crate::config_io::id();
        let process = NativeProcess::create(
            exe,
            args,
            &cwd.display().to_string(),
            &sanitized_environment(std::env::vars(), env),
            false,
            true,
            &id,
        )?;
        if let Err(e) = process.resume() {
            let _ = process.stop();
            return Err(e);
        }
        Ok(Self {
            record: ProgramRecord {
                id,
                pid: process.pid,
                created: process.created,
            },
            process,
        })
    }
    pub fn reopen(record: ProgramRecord) -> Result<Option<Self>> {
        Ok(
            NativeProcess::reopen(record.pid, record.created, &record.id)?
                .map(|process| Self { record, process }),
        )
    }
    pub fn running(&self) -> Result<bool> {
        self.process.running()
    }
    pub fn stop(&self) -> Result<()> {
        self.process.stop()
    }
}
fn quote_argument(arg: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            out.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        out.push(c);
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0},
            System::{JobObjects::*, Threading::*},
        },
    };
    pub struct NativeProcess {
        process: isize,
        thread: isize,
        job: isize,
        pub pid: u32,
        pub created: u64,
    }
    fn handle(raw: isize) -> HANDLE {
        HANDLE(raw as *mut core::ffi::c_void)
    }
    fn err(e: windows::core::Error) -> GateError {
        GateError::Other(format!("会话进程操作失败：{e}"))
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn created(h: HANDLE) -> Result<u64> {
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u).map_err(err)?;
        }
        Ok(((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64)
    }
    impl NativeProcess {
        pub fn create(
            exe: &Path,
            args: &[String],
            cwd: &str,
            env: &[(String, String)],
            console: bool,
            // kill_on_close：面板退出时把这棵进程树一起收掉
            // （`KILL_ON_JOB_CLOSE`）。**跟 IP 门禁无关**，见 `start` 的说明。
            kill_on_close: bool,
            id: &str,
        ) -> Result<Self> {
            let exe = exe.to_string_lossy();
            // Batch entrypoints need cmd, but the executable path is a quoted argument and delayed expansion is off.
            let batch = exe.to_ascii_lowercase().ends_with(".cmd")
                || exe.to_ascii_lowercase().ends_with(".bat");
            if exe.contains(['"', '\r', '\n', '%', '!']) {
                return Err(GateError::Other(
                    "启动路径含不支持的命令字符，请使用托管安装".into(),
                ));
            }
            let application = if batch {
                std::env::var("COMSPEC").unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".into())
            } else {
                exe.to_string()
            };
            if batch
                && args
                    .iter()
                    .any(|a| a.contains(['%', '!', '&', '|', '<', '>', '^', '"', '\r', '\n']))
            {
                return Err(GateError::Other("批处理参数含不支持的命令字符".into()));
            }
            let args = args
                .iter()
                .map(|a| quote_argument(a))
                .collect::<Vec<_>>()
                .join(" ");
            let command = if batch {
                format!("\"{application}\" /D /V:OFF /S /C \"\"{exe}\" {args}\"")
            } else {
                format!("\"{exe}\" {args}")
            };
            let application = wide(&application);
            let mut command = wide(&command);
            let cwd = wide(cwd);
            let mut block: Vec<u16> = Vec::new();
            for (k, v) in env {
                if k.contains(['=', '\0']) || v.contains('\0') {
                    continue;
                }
                block.extend(wide(&format!("{k}={v}")));
            }
            block.push(0);
            let job_name = wide(&format!("Local\\QB-Gate-{id}"));
            unsafe {
                let job = CreateJobObjectW(None, PCWSTR(job_name.as_ptr())).map_err(err)?;
                if kill_on_close {
                    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    if let Err(e) = SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        &info as *const _ as *const _,
                        std::mem::size_of_val(&info) as u32,
                    ) {
                        let _ = CloseHandle(job);
                        return Err(err(e));
                    }
                }
                let si = STARTUPINFOW {
                    cb: std::mem::size_of::<STARTUPINFOW>() as u32,
                    ..Default::default()
                };
                let mut pi = PROCESS_INFORMATION::default();
                let flags = CREATE_SUSPENDED
                    | CREATE_UNICODE_ENVIRONMENT
                    | if console {
                        CREATE_NEW_CONSOLE
                    } else {
                        CREATE_NO_WINDOW
                    };
                if let Err(e) = CreateProcessW(
                    PCWSTR(application.as_ptr()),
                    PWSTR(command.as_mut_ptr()),
                    None,
                    None,
                    false,
                    flags,
                    Some(block.as_ptr() as *const _),
                    PCWSTR(cwd.as_ptr()),
                    &si,
                    &mut pi,
                ) {
                    let _ = CloseHandle(job);
                    return Err(err(e));
                }
                let result = (|| -> Result<Self> {
                    AssignProcessToJobObject(job, pi.hProcess).map_err(err)?;
                    Ok(Self {
                        process: pi.hProcess.0 as isize,
                        thread: pi.hThread.0 as isize,
                        job: job.0 as isize,
                        pid: pi.dwProcessId,
                        created: created(pi.hProcess)?,
                    })
                })();
                if result.is_err() {
                    let _ = TerminateProcess(pi.hProcess, 1);
                    let _ = CloseHandle(pi.hThread);
                    let _ = CloseHandle(pi.hProcess);
                    let _ = CloseHandle(job);
                }
                result
            }
        }
        pub fn contains(&self, pid: u32) -> bool {
            unsafe {
                let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                    return false;
                };
                let mut member = windows::Win32::Foundation::BOOL(0);
                let ok =
                    IsProcessInJob(p, handle(self.job), &mut member).is_ok() && member.as_bool();
                let _ = CloseHandle(p);
                ok
            }
        }
        pub fn resume(&self) -> Result<()> {
            unsafe {
                if self.thread != 0 && ResumeThread(handle(self.thread)) == u32::MAX {
                    return Err(err(windows::core::Error::from_win32()));
                }
            }
            Ok(())
        }
        pub fn running(&self) -> Result<bool> {
            unsafe {
                let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                QueryInformationJobObject(
                    handle(self.job),
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of_val(&info) as u32,
                    None,
                )
                .map_err(err)?;
                Ok(info.ActiveProcesses > 0)
            }
        }
        pub fn stop(&self) -> Result<()> {
            unsafe {
                // Retained process/job handles cannot be redirected by PID reuse.
                if self.process != 0 && created(handle(self.process))? != self.created {
                    return Err(GateError::Other("进程身份已改变，停止操作被阻断".into()));
                }
                TerminateJobObject(handle(self.job), 1).map_err(err)?;
                if self.process != 0 {
                    let _ = WaitForSingleObject(handle(self.process), 3000);
                }
            }
            for _ in 0..30 {
                if !self.running()? {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(GateError::Other("会话进程树退出超时".into()))
        }
        pub fn reopen(pid: u32, time: u64, id: &str) -> Result<Option<Self>> {
            unsafe {
                let name = wide(&format!("Local\\QB-Gate-{id}"));
                let job = match OpenJobObjectW(0x0004 | 0x0008, false, PCWSTR(name.as_ptr())) {
                    Ok(j) => Some(j),
                    Err(e) if e.code().0 as u32 == 0x80070002 => None,
                    Err(e) => return Err(err(e)),
                };
                let process = match OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    pid,
                ) {
                    Ok(p) => Some(p),
                    Err(e) if e.code().0 as u32 == 0x80070057 => None,
                    Err(e) => {
                        if let Some(j) = job {
                            let _ = CloseHandle(j);
                        }
                        return Err(err(e));
                    }
                };
                let mut root = 0;
                if let Some(p) = process {
                    let actual = match created(p) {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = CloseHandle(p);
                            if let Some(j) = job {
                                let _ = CloseHandle(j);
                            }
                            return Err(e);
                        }
                    };
                    if actual == time && WaitForSingleObject(p, 0) != WAIT_OBJECT_0 {
                        let mut member = windows::Win32::Foundation::BOOL(0);
                        if !job.is_some_and(|j| {
                            IsProcessInJob(p, j, &mut member).is_ok() && member.as_bool()
                        }) {
                            let _ = CloseHandle(p);
                            if let Some(j) = job {
                                let _ = CloseHandle(j);
                            }
                            return Err(GateError::Other(
                                "原进程仍在运行，但无法核验会话作业".into(),
                            ));
                        }
                        root = p.0 as isize;
                    } else {
                        let _ = CloseHandle(p);
                    }
                }
                let Some(job) = job else { return Ok(None) };
                let owned = Self {
                    process: root,
                    thread: 0,
                    job: job.0 as isize,
                    pid,
                    created: time,
                };
                // A launcher can exit while its children remain. The retained named job is
                // still the ownership evidence; a reused root PID is never terminated.
                if owned.running()? {
                    Ok(Some(owned))
                } else {
                    Ok(None)
                }
            }
        }
    }
    pub fn creation_time(pid: u32) -> Result<u64> {
        unsafe {
            let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(err)?;
            let time = created(p);
            let _ = CloseHandle(p);
            time
        }
    }
    pub fn terminate_verified(pid: u32, time: u64) -> Result<()> {
        if time == 0 {
            return Err(GateError::Other(
                "缺少进程创建时间，拒绝仅凭 PID 终止".into(),
            ));
        }
        unsafe {
            let p = match OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                false,
                pid,
            ) {
                Ok(p) => p,
                Err(e) if e.code().0 as u32 == 0x80070057 => return Ok(()),
                Err(e) => return Err(err(e)),
            };
            let result = (|| -> Result<()> {
                if created(p)? != time {
                    return Err(GateError::Other("PID 已被其他进程复用，未终止".into()));
                }
                if WaitForSingleObject(p, 0) == WAIT_OBJECT_0 {
                    return Ok(());
                }
                TerminateProcess(p, 1).map_err(err)?;
                if WaitForSingleObject(p, 3000) != WAIT_OBJECT_0 {
                    return Err(GateError::Other("等待进程退出超时".into()));
                }
                Ok(())
            })();
            let _ = CloseHandle(p);
            result
        }
    }
    impl Drop for NativeProcess {
        fn drop(&mut self) {
            unsafe {
                for h in [self.thread, self.process, self.job] {
                    if h != 0 {
                        let _ = CloseHandle(handle(h));
                    }
                }
            }
        }
    }
}
#[cfg(windows)]
use native::NativeProcess;

#[cfg(not(windows))]
struct NativeProcess {
    pid: u32,
    created: u64,
}
#[cfg(not(windows))]
impl NativeProcess {
    fn create(
        _: &Path,
        _: &[String],
        _: &str,
        _: &[(String, String)],
        _: bool,
        _: bool,
        _: &str,
    ) -> Result<Self> {
        Err(GateError::Other("会话管理需要 Windows".into()))
    }
    fn contains(&self, _: u32) -> bool {
        false
    }
    fn resume(&self) -> Result<()> {
        Ok(())
    }
    fn running(&self) -> Result<bool> {
        Ok(false)
    }
    fn stop(&self) -> Result<()> {
        Ok(())
    }
    fn reopen(_: u32, _: u64, _: &str) -> Result<Option<Self>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_arguments_preserve_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote_argument(""), "\"\"");
        assert_eq!(quote_argument("two words"), "\"two words\"");
        assert_eq!(quote_argument("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_argument("C:\\folder\\"), "\"C:\\folder\\\\\"");
    }
    #[test]
    fn mixed_case_inherited_credentials_are_removed_without_mutating_parent() {
        let inherited = vec![
            ("Path".into(), "bin".into()),
            ("OpenAi_Api_Key".into(), "official".into()),
            ("CLAUDE_CONFIG_DIR".into(), "old".into()),
            (
                "CODEX_ELECTRON_USER_DATA_PATH".into(),
                "official-desktop".into(),
            ),
            ("CODEX_ELECTRON_AGENT_RUN_ID".into(), "parent-agent".into()),
            ("ELECTRON_RUN_AS_NODE".into(), "1".into()),
        ];
        let result = sanitized_environment(
            inherited.clone(),
            vec![("OPENAI_API_KEY".into(), "relay".into())],
        );
        assert_eq!(inherited[1].1, "official");
        assert_eq!(result.len(), 2);
        assert!(result
            .iter()
            .any(|(k, v)| k == "OPENAI_API_KEY" && v == "relay"));
        assert!(!result.iter().any(|(k, _)| k == "CLAUDE_CONFIG_DIR"));
    }

    #[test]
    fn codex_selects_its_desktop_profile_before_the_chromium_singleton_check() {
        let env = vec![(
            "CODEX_ELECTRON_USER_DATA_PATH".into(),
            "C:\\profiles\\relay desktop".into(),
        )];
        assert_eq!(
            desktop_arguments(Client::Codex, &env),
            vec!["--user-data-dir=C:\\profiles\\relay desktop"]
        );
        assert!(desktop_arguments(Client::ClaudeCode, &env).is_empty());
        assert!(desktop_arguments(Client::ClaudeDesktop, &env).is_empty());
    }
}
