//! Install Gemini into a user-owned managed directory without a shell or global npm writes.
use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use std::path::{Path, PathBuf};

pub struct Installed {
    pub entry: PathBuf,
    pub version: String,
}

fn child_path(node: &Path, npm: &Path, inherited: Vec<PathBuf>) -> Result<String> {
    let mut paths = Vec::new();
    for path in node
        .parent()
        .into_iter()
        .chain(npm.parent())
        .chain(inherited.iter().map(PathBuf::as_path))
    {
        if !paths.iter().any(|p| p == path) {
            paths.push(path.to_path_buf());
        }
    }
    std::env::join_paths(paths)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| GateError::Other(format!("Node.js 的 PATH 无效：{e}")))
}

/// The explicit destination also lets native CI exercise the production installer in isolation.
pub async fn install_into(dir: &Path, rep: &dyn ProgressSink) -> Result<Installed> {
    rep.phase(1, "检查 Node.js 与 npm");
    let node = super::detect::node_exe().ok_or_else(|| {
        GateError::Other(
            "没找到 Node.js：请先从 https://nodejs.org 安装 macOS 版 Node.js LTS。".into(),
        )
    })?;
    let npm = super::macos_install::command("npm").ok_or_else(|| {
        GateError::Other(
            "没找到 npm：请安装包含 npm 的 Node.js LTS，再回来安装 Gemini CLI。".into(),
        )
    })?;
    crate::config_io::ensure_plain_path(dir)?;
    std::fs::create_dir_all(dir)?;
    let path = child_path(&node, &npm, super::macos_install::path_dirs())?;
    // npm and package lifecycle scripts need node on PATH when QB Gate was opened from Finder.
    let env = [("PATH", path.as_str())];
    let args: Vec<String> = vec![
        "install".into(),
        "--prefix".into(),
        dir.display().to_string(),
        "--no-audit".into(),
        "--no-fund".into(),
        "--global=false".into(),
        "--engine-strict".into(),
        "@google/gemini-cli@latest".into(),
    ];
    rep.phase(2, "安装 Gemini CLI 到 QB Gate 托管目录");
    let mut log = Vec::new();
    let ok =
        super::winget::run_streaming_env(&npm.display().to_string(), &args, &env, rep, 2, &mut log)
            .await?;
    if !ok {
        return Err(GateError::Other(format!(
            "npm 安装 Gemini CLI 失败：{}",
            log.iter()
                .rev()
                .take(3)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        )));
    }
    rep.phase(3, "回读托管目录核对 Gemini CLI");
    let package = dir.join("node_modules/@google/gemini-cli");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("package.json"))?)?;
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| GateError::Other("Gemini CLI 包没有版本号，未确认安装成功。".into()))?;
    if manifest.get("name").and_then(|v| v.as_str()) != Some("@google/gemini-cli") {
        return Err(GateError::Other(
            "安装目录中的包名不匹配 Gemini CLI。".into(),
        ));
    }
    let entry = super::detect::gemini_cli_entries(&package)
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            GateError::Other("npm 报告成功，但托管目录中没有 Gemini CLI 入口。".into())
        })?;
    Ok(Installed {
        entry,
        version: version.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finder_path_can_execute_npm_shebang_and_preserves_existing_tools() {
        let path = child_path(
            Path::new("/Users/测试 用户/.nvm/versions/node/v24/bin/node"),
            Path::new("/opt/homebrew/bin/npm"),
            vec!["/usr/bin".into(), "/bin".into(), "/opt/homebrew/bin".into()],
        )
        .unwrap();
        let parts: Vec<_> = std::env::split_paths(&path).collect();
        assert_eq!(
            parts,
            [
                PathBuf::from("/Users/测试 用户/.nvm/versions/node/v24/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        );
    }
}
