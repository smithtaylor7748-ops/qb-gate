//! macOS process ownership and a minimal helper, dispatched before GUI initialization.
//! Launch descriptions are authenticated ciphertext, private, single-use and addressed by UUID.
#[cfg(not(target_os = "macos"))]
pub fn handle_helper_args() -> Option<i32> {
    None
}

#[cfg(target_os = "macos")]
pub use native::*;

#[cfg(target_os = "macos")]
mod native {
    use crate::error::{GateError, Result};
    use serde::{Deserialize, Serialize};
    use std::{
        fs,
        io::Write,
        os::unix::{
            fs::{DirBuilderExt, OpenOptionsExt},
            process::CommandExt,
        },
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[derive(Debug, Clone)]
    pub struct ProcessInfo {
        pub pid: u32,
        pub ppid: u32,
        pub path: PathBuf,
        pub cmdline: String,
        pub argv: Vec<String>,
        pub created: u64,
    }

    fn bsd(pid: u32) -> Result<libc::proc_bsdinfo> {
        if pid == 0 || pid > i32::MAX as u32 {
            return Err(GateError::Other("无效进程 ID".into()));
        }
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info);
        let n = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                &mut info as *mut _ as *mut _,
                size as i32,
            )
        };
        if n != size as i32 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(info)
    }

    pub fn creation_time(pid: u32) -> Result<u64> {
        let info = bsd(pid)?;
        Ok(info
            .pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.pbi_start_tvusec))
    }

    pub fn process_path(pid: u32) -> Result<PathBuf> {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = vec![0u8; 4096];
        let n = unsafe {
            libc::proc_pidpath(pid as i32, bytes.as_mut_ptr() as *mut _, bytes.len() as u32)
        };
        if n <= 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(n as usize);
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(&bytes[..end])))
    }

    pub fn processes() -> Result<Vec<ProcessInfo>> {
        let output = Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,args="])
            .output()?;
        if !output.status.success() {
            return Err(GateError::Other("无法枚举 macOS 进程".into()));
        }
        let mut result = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut words = line.split_whitespace();
            let Some(pid) = words.next().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Some(ppid) = words.next().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(info) = bsd(pid) else { continue };
            if info.pbi_uid != unsafe { libc::geteuid() } {
                continue;
            }
            let Ok(path) = process_path(pid) else {
                continue;
            };
            let created = info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec;
            let argv = process_argv(pid).unwrap_or_default();
            result.push(ProcessInfo {
                pid,
                ppid,
                path,
                cmdline: words.collect::<Vec<_>>().join(" "),
                argv,
                created,
            });
        }
        Ok(result)
    }

    fn same_process(pid: u32, created: u64) -> bool {
        created != 0
            && bsd(pid).is_ok_and(|info| {
                info.pbi_status != libc::SZOMB
                    && info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec == created
            })
    }

    pub fn process_argv(pid: u32) -> Result<Vec<String>> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
        let mut bytes = vec![0u8; 4 * 1024 * 1024];
        let mut size = bytes.len();
        let status = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                bytes.as_mut_ptr() as *mut _,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        bytes.truncate(size);
        parse_procargs(&bytes).ok_or_else(|| GateError::Other("无法解析进程参数".into()))
    }
    fn parse_procargs(bytes: &[u8]) -> Option<Vec<String>> {
        let count = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
        if !(1..=65536).contains(&count) {
            return None;
        }
        let mut pos = 4 + bytes.get(4..)?.iter().position(|b| *b == 0)? + 1;
        while bytes.get(pos) == Some(&0) {
            pos += 1;
        }
        let mut args = Vec::new();
        for _ in 0..count {
            let end = pos + bytes.get(pos..)?.iter().position(|b| *b == 0)?;
            args.push(String::from_utf8_lossy(&bytes[pos..end]).into_owned());
            pos = end + 1;
        }
        Some(args)
    }
    fn reject_protected_ancestor(pid: u32) -> Result<()> {
        let mut current = std::process::id();
        for _ in 0..256 {
            if current == pid {
                return Err(GateError::Other(
                    "不能终止 QB Gate 自身或宿主祖先进程".into(),
                ));
            }
            if current <= 1 {
                return Ok(());
            }
            let parent = bsd(current)?.pbi_ppid;
            if parent == current {
                return Err(GateError::Other("无法核验宿主进程链，未终止".into()));
            }
            current = parent;
        }
        Err(GateError::Other("宿主进程链过长，未终止".into()))
    }

    pub fn terminate_verified(pid: u32, created: u64) -> Result<()> {
        reject_protected_ancestor(pid)?;
        if !same_process(pid, created) {
            return Err(GateError::Other("进程身份已改变，未终止".into()));
        }
        if unsafe { libc::kill(pid as i32, libc::SIGTERM) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        for _ in 0..20 {
            if !same_process(pid, created) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if same_process(pid, created) && unsafe { libc::kill(pid as i32, libc::SIGKILL) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        for _ in 0..40 {
            if !same_process(pid, created) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(GateError::Other("等待进程退出超时".into()))
    }

    fn terminate_group(pid: u32, created: u64) -> Result<()> {
        if !same_process(pid, created) {
            return Ok(());
        }
        if unsafe { libc::getpgid(pid as i32) } != pid as i32 {
            return Err(GateError::Other("进程不再属于原受管进程组，未终止".into()));
        }
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        for _ in 0..12 {
            if !same_process(pid, created) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if same_process(pid, created) && unsafe { libc::getpgid(pid as i32) } == pid as i32 {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        Ok(())
    }

    pub(crate) fn stop_owned_group(pid: u32, created: u64) -> Result<()> {
        terminate_group(pid, created)
    }

    fn group_members(pid: u32) -> Vec<(u32, u64)> {
        // Only read process metadata here. A watchdog must not spawn ps or read argv repeatedly.
        const ALL_PIDS: u32 = 1;
        let needed = unsafe { libc::proc_listpids(ALL_PIDS, 0, std::ptr::null_mut(), 0) };
        if needed <= 0 {
            return Vec::new();
        }
        let mut pids = vec![0u32; needed as usize / 4 + 256];
        let size = unsafe {
            libc::proc_listpids(
                ALL_PIDS,
                0,
                pids.as_mut_ptr() as *mut _,
                (pids.len() * 4) as i32,
            )
        };
        if size <= 0 {
            return Vec::new();
        }
        pids.truncate(size as usize / 4);
        pids.into_iter()
            .filter(|p| *p != 0 && *p != pid)
            .filter_map(|p| {
                let info = bsd(p).ok()?;
                (info.pbi_pgid == pid
                    && info.pbi_uid == unsafe { libc::geteuid() }
                    && info.pbi_status != libc::SZOMB)
                    .then_some((p, info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec))
            })
            .collect()
    }

    fn stop_members(group: u32, members: &[(u32, u64)]) {
        for (pid, created) in members {
            if same_process(*pid, *created) && unsafe { libc::getpgid(*pid as i32) } == group as i32
            {
                let _ = terminate_verified(*pid, *created);
            }
        }
    }

    /// A separate minimal process survives an abrupt panel exit and ends this owned group.
    pub(crate) fn watch_child(pid: u32) -> Result<u64> {
        use std::io::Read;
        let info = bsd(pid)?;
        if info.pbi_ppid != std::process::id() || unsafe { libc::getpgid(pid as i32) } != pid as i32
        {
            return Err(GateError::Other(
                "子进程未在独立受管进程组内，拒绝托管".into(),
            ));
        }
        let created = creation_time(pid)?;
        let parent = std::process::id();
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--qb-watch",
                &parent.to_string(),
                &creation_time(parent)?.to_string(),
                &pid.to_string(),
                &created.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        let mut ready = [0u8; 1];
        if child
            .stdout
            .take()
            .ok_or_else(|| GateError::Other("监护管道不可用".into()))?
            .read_exact(&mut ready)
            .is_err()
            || ready != *b"1"
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(GateError::Other("子进程监护未就绪，拒绝继续调用".into()));
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(created)
    }

    fn run_watch(args: &[String]) -> Result<i32> {
        if args.len() != 6 {
            return Ok(2);
        }
        let number = |i: usize| {
            args[i]
                .parse::<u64>()
                .map_err(|_| GateError::Other("无效监护参数".into()))
        };
        let parent =
            u32::try_from(number(2)?).map_err(|_| GateError::Other("无效父进程".into()))?;
        let parent_created = number(3)?;
        let pid = u32::try_from(number(4)?).map_err(|_| GateError::Other("无效子进程".into()))?;
        let created = number(5)?;
        if !same_process(parent, parent_created)
            || !same_process(pid, created)
            || bsd(pid)?.pbi_ppid != parent
            || unsafe { libc::getpgid(pid as i32) } != pid as i32
            || process_path(parent)?.canonicalize()? != std::env::current_exe()?.canonicalize()?
        {
            return Ok(1);
        }
        std::io::stdout().write_all(b"1")?;
        std::io::stdout().flush()?;
        let mut members = group_members(pid);
        while same_process(parent, parent_created) && same_process(pid, created) {
            members = group_members(pid);
            std::thread::sleep(Duration::from_millis(100));
        }
        if same_process(pid, created) {
            let _ = terminate_group(pid, created);
        }
        stop_members(pid, &members);
        Ok(0)
    }

    #[derive(Serialize, Deserialize)]
    struct Plan {
        parent: u32,
        parent_created: u64,
        exe: PathBuf,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        terminal: bool,
        kill_on_close: bool,
    }
    #[derive(Serialize, Deserialize)]
    struct Receipt {
        pid: u32,
        created: u64,
    }

    fn valid_id(id: &str) -> bool {
        uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id)
    }
    fn launch_dir(id: &str) -> Result<PathBuf> {
        if !valid_id(id) {
            return Err(GateError::Other("无效启动标识".into()));
        }
        Ok(crate::paths::state_dir().join("managed-launch").join(id))
    }
    fn private_write(path: &Path, body: &[u8]) -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(body)?;
        f.sync_all()?;
        Ok(())
    }
    fn check_private_dir(dir: &Path) -> Result<()> {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(dir)?;
        if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
            return Err(GateError::Other("启动目录所有者或权限不符".into()));
        }
        Ok(())
    }
    fn read_private(path: &Path) -> Result<Vec<u8>> {
        use std::io::Read;
        use std::os::unix::fs::MetadataExt;
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let m = file.metadata()?;
        if !m.is_file()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
            || m.len() > 4 * 1024 * 1024
        {
            return Err(GateError::Other("启动文件权限或大小不符".into()));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    fn shell_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
    fn apple_quote(s: &str) -> String {
        format!(
            "\"{}\"",
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        )
    }

    fn terminal_command(helper: &Path, id: &str, parent_home: &Path) -> String {
        // Terminal starts a fresh login shell, so it otherwise loses the panel's
        // HOME and looks for both the launch plan and Keychain in another context.
        // Pass only this directory, never the client's environment or credentials.
        format!(
            "exec /usr/bin/env {} {} --qb-managed {}",
            shell_quote(&format!("HOME={}", parent_home.to_string_lossy())),
            shell_quote(&helper.to_string_lossy()),
            shell_quote(id)
        )
    }

    pub(crate) fn environment_with_path(mut env: Vec<(String, String)>) -> Vec<(String, String)> {
        let existing = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let dirs =
            crate::macos_paths::directories(Some(existing.as_ref()), dirs::home_dir().as_deref());
        if let Ok(path) = std::env::join_paths(dirs) {
            env.retain(|(key, _)| key != "PATH");
            env.push(("PATH".into(), path.to_string_lossy().into_owned()));
        }
        env
    }

    pub(crate) struct NativeProcess {
        pub(crate) pid: u32,
        pub(crate) created: u64,
        dir: PathBuf,
        kill_on_close: bool,
    }
    impl NativeProcess {
        #[allow(clippy::too_many_arguments)]
        pub(crate) fn create(
            exe: &Path,
            args: &[String],
            cwd: &str,
            env: &[(String, String)],
            terminal: bool,
            kill_on_close: bool,
            log: Option<&Path>,
            _id: &str,
        ) -> Result<Self> {
            // Session identifiers include a timestamp; the helper uses its own strict UUID.
            let helper_id = uuid::Uuid::new_v4().to_string();
            let id = helper_id.as_str();
            let dir = launch_dir(id)?;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir.parent().unwrap())?;
            fs::DirBuilder::new().mode(0o700).create(&dir)?;
            let result = (|| -> Result<Self> {
                let plan = Plan {
                    parent: std::process::id(),
                    parent_created: creation_time(std::process::id())?,
                    exe: exe.to_path_buf(),
                    args: args.to_vec(),
                    cwd: cwd.into(),
                    env: environment_with_path(env.to_vec()),
                    terminal,
                    kill_on_close,
                };
                let encoded = crate::secret::seal(&serde_json::to_string(&plan)?)?;
                private_write(&dir.join("plan"), encoded.as_bytes())?;
                let helper = std::env::current_exe()?;
                if terminal {
                    let parent_home = dirs::home_dir()
                        .filter(|p| p.is_absolute())
                        .ok_or_else(|| GateError::Other("无法确定受管终端的用户主目录".into()))?;
                    let line = terminal_command(&helper, id, &parent_home);
                    let script = format!(
                        "tell application \"Terminal\"\nactivate\ndo script {}\nend tell",
                        apple_quote(&line)
                    );
                    let mut cmd = Command::new("/usr/bin/osascript");
                    cmd.args(["-e", &script])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                    let mut child = cmd.spawn()?;
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                } else {
                    let mut cmd = Command::new(&helper);
                    cmd.args(["--qb-managed", id])
                        .stdin(Stdio::null())
                        .process_group(0);
                    if let Some(log) = log {
                        let file = fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .mode(0o600)
                            .open(log)?;
                        cmd.stdout(file.try_clone()?).stderr(file);
                    } else {
                        cmd.stdout(Stdio::null()).stderr(Stdio::null());
                    }
                    let mut child = cmd.spawn()?;
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                }
                let deadline = Instant::now() + Duration::from_secs(45);
                loop {
                    if let Ok(bytes) = read_private(&dir.join("ready")) {
                        if let Ok(receipt) = serde_json::from_slice::<Receipt>(&bytes) {
                            if same_process(receipt.pid, receipt.created)
                                && process_path(receipt.pid)
                                    .ok()
                                    .and_then(|p| p.canonicalize().ok())
                                    == helper.canonicalize().ok()
                                && unsafe { libc::getpgid(receipt.pid as i32) }
                                    == receipt.pid as i32
                            {
                                return Ok(Self {
                                    pid: receipt.pid,
                                    created: receipt.created,
                                    dir: dir.clone(),
                                    kill_on_close,
                                });
                            }
                        }
                    }
                    if Instant::now() >= deadline {
                        return Err(GateError::Other(
                            "受管启动超时；请允许 QB Gate 控制终端并访问钥匙串后重试".into(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
            })();
            if result.is_err() {
                let _ = fs::remove_dir_all(&dir);
            }
            result
        }
        pub(crate) fn contains(&self, pid: u32) -> bool {
            same_process(self.pid, self.created)
                && unsafe { libc::getpgid(pid as i32) } == self.pid as i32
        }
        pub(crate) fn resume(&self) -> Result<()> {
            private_write(&self.dir.join("start"), b"start")?;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Ok(bytes) = read_private(&self.dir.join("spawned")) {
                    let status = std::str::from_utf8(&bytes).unwrap_or("failed");
                    let _ = fs::remove_dir_all(&self.dir);
                    return if status == "started" {
                        Ok(())
                    } else {
                        Err(GateError::Other(
                            "客户端启动失败，请检查程序执行权限、安装状态和工作目录".into(),
                        ))
                    };
                }
                if !same_process(self.pid, self.created) || Instant::now() >= deadline {
                    return Err(GateError::Other(
                        "客户端启动回执未到达，未登记为已运行".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        pub(crate) fn running(&self) -> Result<bool> {
            Ok(same_process(self.pid, self.created))
        }
        pub(crate) fn stop(&self) -> Result<()> {
            terminate_group(self.pid, self.created)
        }
        pub(crate) fn reopen(pid: u32, created: u64, _id: &str) -> Result<Option<Self>> {
            if same_process(pid, created) {
                return Err(GateError::Other(
                    "原受管进程仍存在；重启后不能重建所有权，请先关闭原客户端".into(),
                ));
            }
            Ok(None)
        }
    }
    impl Drop for NativeProcess {
        fn drop(&mut self) {
            if self.kill_on_close {
                let _ = self.stop();
                let _ = fs::remove_dir_all(&self.dir);
            }
            // Detached console helpers consume and remove their own start description.
            // Dropping the handle before they observe `start` must not cancel the launch.
        }
    }

    fn run_managed(id: &str) -> Result<i32> {
        let dir = launch_dir(id)?;
        check_private_dir(&dir)?;
        let encrypted = read_private(&dir.join("plan"))?;
        fs::remove_file(dir.join("plan"))?;
        let encoded =
            std::str::from_utf8(&encrypted).map_err(|_| GateError::Other("启动描述无效".into()))?;
        if !encoded.starts_with("keychain-aes-gcm-v1:") {
            return Err(GateError::Other("启动描述必须为已认证的加密内容".into()));
        }
        let plain = crate::secret::open(encoded)
            .ok_or_else(|| GateError::Other("启动描述解密失败".into()))?;
        let plain = crate::credentials::Secret::new(plain.into_bytes());
        let plan: Plan = serde_json::from_slice(plain.as_bytes())?;
        let own_exe = std::env::current_exe()?.canonicalize()?;
        if !same_process(plan.parent, plan.parent_created)
            || process_path(plan.parent)?.canonicalize()? != own_exe
        {
            return Err(GateError::Other("启动父进程身份无法核验".into()));
        }
        let pid = std::process::id();
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
            libc::signal(libc::SIGINT, libc::SIG_IGN);
            libc::signal(libc::SIGTTOU, libc::SIG_IGN);
            if libc::getpgrp() != pid as i32 && libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if plan.terminal && libc::isatty(0) == 1 {
                libc::tcsetpgrp(0, pid as i32);
            }
        }
        let created = creation_time(pid)?;
        private_write(
            &dir.join("ready"),
            &serde_json::to_vec(&Receipt { pid, created })?,
        )?;
        let deadline = Instant::now() + Duration::from_secs(60);
        while !dir.join("start").is_file() {
            if !same_process(plan.parent, plan.parent_created)
                || Instant::now() > deadline
                || !dir.exists()
            {
                return Ok(1);
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        if plan.kill_on_close {
            let parent = plan.parent;
            let parent_created = plan.parent_created;
            std::thread::spawn(move || {
                while same_process(parent, parent_created) {
                    std::thread::sleep(Duration::from_millis(200));
                }
                let _ = terminate_group(pid, created);
            });
        }
        let mut cmd = Command::new(plan.exe);
        cmd.args(plan.args).env_clear().envs(plan.env);
        if !plan.cwd.is_empty() {
            cmd.current_dir(plan.cwd);
        }
        unsafe {
            cmd.pre_exec(|| {
                libc::signal(libc::SIGTERM, libc::SIG_DFL);
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::signal(libc::SIGTTOU, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = private_write(&dir.join("spawned"), b"failed");
                return Err(error.into());
            }
        };
        if let Err(error) = private_write(&dir.join("spawned"), b"started") {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let status = child.wait();
        // The supervisor remains the group leader until descendants have been stopped.
        stop_members(pid, &group_members(pid));
        Ok(status?.code().unwrap_or(1))
    }

    pub fn handle_helper_args() -> Option<i32> {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--qb-watch") {
            return Some(run_watch(&args).unwrap_or(1));
        }
        if args.get(1).map(String::as_str) != Some("--qb-managed") {
            return None;
        }
        Some(if args.len() == 3 {
            run_managed(&args[2]).unwrap_or_else(|e| {
                eprintln!("QB Gate：{e}");
                1
            })
        } else {
            2
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        #[test]
        fn launch_identifiers_cannot_escape_private_root() {
            assert!(valid_id("00000000-0000-4000-8000-000000000000"));
            for id in [
                "../plan",
                "/tmp/plan",
                "",
                "00000000-0000-4000-8000-000000000000/../",
            ] {
                assert!(!valid_id(id));
            }
        }
        #[test]
        fn process_identity_includes_creation_time() {
            let pid = std::process::id();
            let created = creation_time(pid).unwrap();
            assert!(created > 0);
            assert!(same_process(pid, created));
            assert!(!same_process(pid, created + 1));
            assert!(process_path(pid).unwrap().is_absolute());
        }
        #[test]
        fn terminal_quoting_keeps_arguments_literal() {
            assert_eq!(shell_quote("a'b $(x)"), "'a'\\''b $(x)'");
            assert_eq!(apple_quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
        }
        #[test]
        fn terminal_receives_the_parent_home_without_shell_expansion() {
            let parent_home = Path::new(r#"/tmp/QB Gate's "double" $(false) `false` \slash"#);
            let id = "8a0718fc-6b43-4cc3-af19-25c7b7d06724";
            let command = terminal_command(Path::new("/usr/bin/printenv"), id, parent_home);
            // Parse the actual AppleScript literal without asking Terminal for TCC access.
            let decoded = Command::new("/usr/bin/osascript")
                .args(["-e", &format!("return {}", apple_quote(&command))])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            let decoded = String::from_utf8(decoded.stdout).unwrap();
            assert_eq!(decoded, format!("{command}\n"));
            // Use printenv as a harmless target so the shell reports only the literal HOME.
            let command = decoded
                .strip_suffix('\n')
                .unwrap()
                .strip_suffix(&format!(" --qb-managed '{}'", id))
                .unwrap();
            let output = Command::new("/bin/sh")
                .args(["-c", &format!("{command} HOME")])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("{}\n", parent_home.display())
            );
        }
        #[test]
        fn private_launch_files_reject_symlinks_and_public_permissions() {
            let dir = std::env::temp_dir().join(format!("qb-private-{}", uuid::Uuid::new_v4()));
            fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
            let path = dir.join("plan");
            private_write(&path, b"encrypted fixture").unwrap();
            assert!(private_write(&path, b"overwrite").is_err());
            assert_eq!(read_private(&path).unwrap(), b"encrypted fixture");
            std::os::unix::fs::symlink(&path, dir.join("link")).unwrap();
            assert!(read_private(&dir.join("link")).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read_private(&path).is_err());
            fs::remove_dir_all(dir).unwrap();
        }
        #[test]
        fn procargs_preserves_spaces_empty_arguments_and_excludes_environment() {
            let mut bytes = 3i32.to_ne_bytes().to_vec();
            bytes.extend_from_slice(b"/Applications/Test.app/bin\0\0\0program\0--user-data-dir=/Users/me/My Data\0\0TOKEN=not-an-argument\0");
            assert_eq!(
                parse_procargs(&bytes).unwrap(),
                ["program", "--user-data-dir=/Users/me/My Data", ""]
            );
            assert_eq!(parse_procargs(&bytes[..8]), None);
        }
        #[test]
        fn finder_environment_gains_tools_without_reordering_explicit_path() {
            let env = environment_with_path(vec![
                ("PATH".into(), "/custom/bin:/usr/bin".into()),
                ("HOME".into(), "/fixture".into()),
            ]);
            let path = &env.iter().find(|(key, _)| key == "PATH").unwrap().1;
            assert!(path.starts_with("/custom/bin:/usr/bin:"));
            assert!(path.contains("/opt/homebrew/bin"));
            assert!(env.contains(&("HOME".into(), "/fixture".into())));
        }
        #[test]
        fn termination_refuses_own_identity() {
            assert!(terminate_verified(
                std::process::id(),
                creation_time(std::process::id()).unwrap()
            )
            .is_err());
        }
        #[test]
        fn only_an_identified_owned_process_group_is_stopped() {
            let mut child = Command::new("/bin/sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .unwrap();
            let pid = child.id();
            let created = creation_time(pid).unwrap();
            terminate_group(pid, created + 1).unwrap();
            assert!(child.try_wait().unwrap().is_none());
            terminate_group(pid, created).unwrap();
            let _ = child.wait();
            assert!(!same_process(pid, created));
            // An ungrouped fixture shares the runner's group; refusal must leave it alive.
            let mut unrelated = Command::new("/bin/sleep").arg("30").spawn().unwrap();
            assert!(
                terminate_group(unrelated.id(), creation_time(unrelated.id()).unwrap()).is_err()
            );
            assert!(unrelated.try_wait().unwrap().is_none());
            unrelated.kill().unwrap();
            let _ = unrelated.wait();
        }
    }
}
