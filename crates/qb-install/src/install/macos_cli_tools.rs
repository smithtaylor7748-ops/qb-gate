//! npm tools stay in QB Gate's user-owned prefix; external package managers retain ownership.
use super::cli_tools::{self, CliTool, Roots, Step};
use crate::{
    error::{GateError, Result},
    sink::ProgressSink,
};
use std::path::{Path, PathBuf};

pub fn directory(tool: CliTool) -> PathBuf {
    super::managed::slot_dir(&format!("cli-{}", tool.command()))
}

pub fn entry(tool: CliTool, dir: &Path) -> PathBuf {
    dir.join("node_modules/.bin").join(tool.command())
}

pub fn owns(tool: CliTool, path: &Path) -> bool {
    path == entry(tool, &directory(tool))
}

pub fn plan(tool: CliTool, dir: &Path, roots: &Roots, uninstall: bool) -> Step {
    let Some(package) = tool.npm_package() else {
        return Step::Manual("Hermes 的 macOS 自动安装暂不支持；请按官方文档安装后重新检测：https://hermes-agent.nousresearch.com/docs/getting-started/installation".into());
    };
    let Some(npm) = cli_tools::npm_on_path(roots) else {
        return Step::Manual("请先安装包含 npm 的 Node.js LTS（https://nodejs.org）。".into());
    };
    let Some(node) = roots
        .path_dirs
        .iter()
        .map(|p| p.join("node"))
        .find(|p| p.is_file())
    else {
        return Step::Manual("没找到 Node.js：请先安装 Node.js LTS（https://nodejs.org）。".into());
    };
    let mut paths = vec![
        node.parent().unwrap().to_path_buf(),
        npm.parent().unwrap().to_path_buf(),
    ];
    paths.extend(roots.path_dirs.clone());
    let path = match std::env::join_paths(paths) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(e) => return Step::Manual(format!("Node.js 的 PATH 无效：{e}")),
    };
    let mut args: Vec<String> = vec![
        if uninstall { "uninstall" } else { "install" }.into(),
        "--prefix".into(),
        dir.display().to_string(),
        "--global=false".into(),
        "--no-audit".into(),
        "--no-fund".into(),
    ];
    if uninstall {
        args.push(package.into());
    } else {
        args.extend([
            "--engine-strict".into(),
            "--include=optional".into(),
            "--ignore-scripts=false".into(),
            format!("{package}@latest"),
        ]);
        args.extend(tool.npm_extra_args().iter().map(|s| s.to_string()));
    }
    Step::Run {
        program: npm,
        args,
        env: vec![("PATH".into(), path)],
    }
}

/// Shared production path used by the application and isolated native acceptance tests.
pub async fn install_into(
    tool: CliTool,
    dir: &Path,
    rep: &dyn ProgressSink,
) -> Result<cli_tools::Install> {
    let plan = plan(tool, dir, &Roots::current(), false);
    let Step::Run { program, args, env } = &plan else {
        return Err(GateError::Other(plan.shown()));
    };
    crate::config_io::ensure_plain_path(dir)?;
    std::fs::create_dir_all(dir)?;
    let env: Vec<_> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    rep.phase(2, &format!("安装 {} 到 QB Gate 托管目录", tool.label()));
    let mut log = Vec::new();
    if !super::winget::run_streaming_env(
        &program.display().to_string(),
        args,
        &env,
        rep,
        2,
        &mut log,
    )
    .await?
    {
        return Err(GateError::Other(format!(
            "{} 安装失败：{}",
            tool.label(),
            log.iter()
                .rev()
                .take(5)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        )));
    }
    rep.phase(3, "回读包名称、版本和启动入口");
    inspect(tool, dir)
}

pub fn inspect(tool: CliTool, dir: &Path) -> Result<cli_tools::Install> {
    let package = tool
        .npm_package()
        .ok_or_else(|| GateError::Other("此工具没有 npm 包".into()))?;
    let package_dir = dir.join("node_modules").join(package);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package_dir.join("package.json"))?)?;
    if manifest.get("name").and_then(|v| v.as_str()) != Some(package) {
        return Err(GateError::Other(format!("{} 包名不匹配", tool.label())));
    }
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GateError::Other("安装包没有版本号".into()))?;
    let path = entry(tool, dir);
    if !path.is_file() {
        return Err(GateError::Other(format!(
            "{} 的启动入口没有生成：{}",
            tool.label(),
            path.display()
        )));
    }
    let resolved = std::fs::canonicalize(&path)?;
    let modules = std::fs::canonicalize(dir.join("node_modules"))?;
    if !resolved.starts_with(modules) {
        return Err(GateError::Other(
            "npm 启动入口指向托管依赖目录以外，未确认安装成功。".into(),
        ));
    }
    Ok(cli_tools::Install {
        path,
        version: Some(version.into()),
        package_dir: Some(package_dir),
        source: cli_tools::Source::Npm,
        default: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_install_and_remove_never_change_the_global_npm_prefix() {
        let root = std::env::temp_dir().join(format!("qb-npm-plan-{}", crate::config_io::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/node"), "fixture").unwrap();
        std::fs::write(root.join("bin/npm"), "fixture").unwrap();
        let roots = Roots {
            path_dirs: vec![root.join("bin")],
            ..Roots::default()
        };
        let destination = root.join("用户 O'Brien app");
        for uninstall in [false, true] {
            let Step::Run { args, env, .. } = plan(CliTool::Pi, &destination, &roots, uninstall)
            else {
                panic!("Expected runnable plan")
            };
            assert!(args
                .windows(2)
                .any(|a| a == ["--prefix", destination.to_str().unwrap()]));
            assert!(args.iter().any(|a| a == "--global=false"));
            assert!(!args.iter().any(|a| a == "-g"));
            assert!(env.iter().any(|(k, _)| k == "PATH"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
