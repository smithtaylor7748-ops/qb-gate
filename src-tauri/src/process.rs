//! Windows helper-process policy: background commands never create console windows.

#[cfg(windows)]
pub fn hidden_std(mut cmd: std::process::Command) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn hidden_std(cmd: std::process::Command) -> std::process::Command { cmd }

#[cfg(windows)]
pub fn hidden_tokio(mut cmd: tokio::process::Command) -> tokio::process::Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn hidden_tokio(cmd: tokio::process::Command) -> tokio::process::Command { cmd }
