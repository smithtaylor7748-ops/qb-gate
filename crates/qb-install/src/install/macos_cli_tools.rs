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
    // npm 11.19 can retain orphan lockfile nodes when /var and /private/var
    // identify the same prefix. Use one physical path for every operation,
    // after the shared guard rejects user-created links and reparse points.
    if let Err(e) = crate::config_io::ensure_plain_path(dir) {
        return Step::Manual(format!("托管 npm 目录不安全：{e}"));
    }
    let prefix = if dir.exists() {
        match std::fs::canonicalize(dir) {
            Ok(path) => path,
            Err(e) => return Step::Manual(format!("无法确认托管 npm 目录：{e}")),
        }
    } else {
        dir.to_path_buf()
    };
    let mut args: Vec<String> = vec![
        if uninstall { "uninstall" } else { "install" }.into(),
        "--prefix".into(),
        prefix.display().to_string(),
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
        if tool == CliTool::MiniMaxCode {
            // npm 11.19 rejects CLI --allow-scripts for project-scoped installs.
            // The exact package policy lives in this managed prefix's manifest.
            args.push("--strict-allow-scripts".into());
        }
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
    crate::config_io::ensure_plain_path(dir)?;
    std::fs::create_dir_all(dir)?;
    prepare_manifest(tool, dir)?;
    // Resolve the now-existing prefix before constructing npm's first command.
    let plan = plan(tool, dir, &Roots::current(), false);
    let Step::Run { program, args, env } = &plan else {
        return Err(GateError::Other(plan.shown()));
    };
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

/// Preserve project metadata while enabling only MiniMax's required native install hooks.
pub fn prepare_manifest(tool: CliTool, dir: &Path) -> Result<()> {
    if tool != CliTool::MiniMaxCode {
        return Ok(());
    }
    let path = dir.join("package.json");
    crate::config_io::ensure_plain_path(&path)?;
    let before = crate::config_io::read_optional(&path)?;
    let mut manifest = crate::config_io::parse_object(before.as_deref())?;
    let object = manifest.as_object_mut().unwrap();
    object
        .entry("private")
        .or_insert(serde_json::Value::Bool(true));
    let policy = object
        .entry("allowScripts")
        .or_insert_with(|| serde_json::json!({}));
    let policy = policy.as_object_mut().ok_or_else(|| {
        GateError::Other("托管 package.json 的 allowScripts 格式不正确，未覆盖。".into())
    })?;
    for package in ["@minimax-ai/code", "better-sqlite3"] {
        if policy
            .get(package)
            .is_some_and(|v| v != &serde_json::Value::Bool(true))
        {
            return Err(GateError::Other(format!(
                "托管安装策略不允许 {package} 的构建脚本，原策略未改动。"
            )));
        }
        policy.insert(package.into(), serde_json::Value::Bool(true));
    }
    crate::config_io::replace(&path, Some(&serde_json::to_vec_pretty(&manifest)?))
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

    #[test]
    fn scoped_minimax_policy_preserves_dependencies_and_rejects_explicit_denials() {
        let root =
            std::env::temp_dir().join(format!("qb-minimax-policy-{}", crate::config_io::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("package.json");
        std::fs::write(
            &path,
            br#"{"dependencies":{"@minimax-ai/code":"0.7.6"},"allowScripts":{"unrelated":false}}"#,
        )
        .unwrap();
        prepare_manifest(CliTool::MiniMaxCode, &root).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(manifest["dependencies"]["@minimax-ai/code"], "0.7.6");
        assert_eq!(
            manifest["allowScripts"],
            serde_json::json!({"unrelated":false,"@minimax-ai/code":true,"better-sqlite3":true})
        );
        let denied = br#"{"allowScripts":{"better-sqlite3":false}}"#;
        std::fs::write(&path, denied).unwrap();
        assert!(prepare_manifest(CliTool::MiniMaxCode, &root).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), denied);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn canonical_system_alias_prefix_keeps_user_link_protection() {
        let root = Path::new("/var/tmp").join(format!("qb-npm-prefix-{}", crate::config_io::id()));
        let dir = root.join("managed");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/node"), "fixture").unwrap();
        std::fs::write(root.join("bin/npm"), "fixture").unwrap();
        let roots = Roots {
            path_dirs: vec![root.join("bin")],
            ..Roots::default()
        };
        let canonical = std::fs::canonicalize(&dir).unwrap();
        for uninstall in [false, true] {
            let Step::Run { args, .. } = plan(CliTool::MiniMaxCode, &dir, &roots, uninstall) else {
                panic!("System alias must remain supported")
            };
            assert!(args
                .windows(2)
                .any(|pair| pair == ["--prefix", canonical.to_str().unwrap()]));
        }
        let alias = root.join("user-link");
        std::os::unix::fs::symlink(&dir, &alias).unwrap();
        assert!(matches!(
            plan(CliTool::MiniMaxCode, &alias, &roots, false),
            Step::Manual(_)
        ));
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
