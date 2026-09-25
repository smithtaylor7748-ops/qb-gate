//! Curated catalog + explicit imports. Importing metadata never executes a repository script.
use crate::{
    config_io::{self, Edit},
    domain::*,
    error::{GateError, Result},
    repository::Repository,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn catalog() -> Result<Vec<ExtensionManifest>> {
    catalog_in(&Repository::open()?)
}
fn catalog_in(db: &Repository) -> Result<Vec<ExtensionManifest>> {
    let mut list: Vec<ExtensionManifest> =
        serde_json::from_str(include_str!("../extension-catalog.json"))?;
    for custom in db.list::<ExtensionManifest>("catalog")? {
        if let Some(existing) = list.iter_mut().find(|m| m.id == custom.id) {
            // 精选条目的**来源与版本以内置目录为准**，数据库行盖不掉。
            //
            // 那两个字段就是 ATTRIBUTION.md 里按哈希写死、核对过许可证的那一版。
            // 只靠 `check_updates` 自觉不去写是不够的 —— 旧版本的「检查来源更新」
            // 真的写过这样的行，而 `catalog_in` 一合并，使用者装到的就不再是
            // 被核过的那一版，许可证声明也跟着不准。放在这里是结构性的：
            // 不管数据库里有什么，固定版本都还原得回来，也不需要单独的迁移。
            let (source, version) = (existing.source.clone(), existing.version.clone());
            *existing = ExtensionManifest {
                source,
                version,
                ..custom
            };
        } else {
            list.push(custom);
        }
    }
    Ok(list)
}
fn manifest(id: &str) -> Result<ExtensionManifest> {
    manifest_in(&Repository::open()?, id)
}
fn manifest_in(db: &Repository, id: &str) -> Result<ExtensionManifest> {
    catalog_in(db)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| GateError::Other("扩展不存在".into()))
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.contains("--")
        && !name.starts_with('-')
        && !name.ends_with('-')
}

#[derive(Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
    license: Option<String>,
}
fn skill_metadata(text: &str) -> Result<SkillMetadata> {
    let normalized = text.replace("\r\n", "\n");
    let front = normalized
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .ok_or_else(|| GateError::Other("SKILL.md 缺少 YAML 元数据".into()))?
        .0;
    let m: SkillMetadata = serde_yaml_ng::from_str(front)
        .map_err(|e| GateError::Other(format!("Skill 元数据无效：{e}")))?;
    if !valid_name(&m.name) || m.description.is_empty() || m.description.len() > 4096 {
        return Err(GateError::Other("Skill 名称或描述不符合格式".into()));
    }
    Ok(m)
}
fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("QB-Gate/0.12")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(Into::into)
}
async fn github_json(path: &str) -> Result<serde_json::Value> {
    let response = http()?
        .get(format!("https://api.github.com{path}"))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(GateError::Other(format!(
            "GitHub 返回 HTTP {}，请检查仓库、版本或稍后重试",
            response.status()
        )));
    }
    response.json().await.map_err(Into::into)
}
struct GitSource {
    repo: String,
    revision: String,
    subdir: String,
    tree: Vec<serde_json::Value>,
}
async fn github_source(source: &str, revision: Option<&str>) -> Result<GitSource> {
    let url =
        reqwest::Url::parse(source).map_err(|_| GateError::Other("GitHub 地址无效".into()))?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") || !url.username().is_empty()
    {
        return Err(GateError::Other(
            "请输入 HTTPS GitHub 仓库或目录地址".into(),
        ));
    }
    let parts: Vec<_> = url
        .path_segments()
        .unwrap()
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err(GateError::Other("地址需要包含仓库所有者和仓库名".into()));
    }
    let repo = format!("{}/{}", parts[0], parts[1].trim_end_matches(".git"));
    if !repo
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-._/".contains(&b))
    {
        return Err(GateError::Other("仓库名称无效".into()));
    }
    let branch = if parts.get(2) == Some(&"tree") {
        parts.get(3).copied().unwrap_or("HEAD")
    } else {
        "HEAD"
    };
    let requested = revision.filter(|s| !s.is_empty()).unwrap_or(branch);
    if !requested
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
    {
        return Err(GateError::Other("请使用分支名或提交 SHA".into()));
    }
    let sha = github_json(&format!("/repos/{repo}/commits/{requested}")).await?["sha"]
        .as_str()
        .ok_or_else(|| GateError::Other("GitHub 未返回提交版本".into()))?
        .to_string();
    let tree = github_json(&format!("/repos/{repo}/git/trees/{sha}?recursive=1")).await?;
    if tree["truncated"] == true {
        return Err(GateError::Other(
            "仓库目录过大，请先下载所需 Skill 并从本地导入".into(),
        ));
    }
    Ok(GitSource {
        repo,
        revision: sha,
        subdir: if parts.get(2) == Some(&"tree") {
            parts.iter().skip(4).copied().collect::<Vec<_>>().join("/")
        } else {
            String::new()
        },
        tree: tree["tree"]
            .as_array()
            .ok_or_else(|| GateError::Other("GitHub 目录响应无效".into()))?
            .clone(),
    })
}
async fn blob(repo: &str, revision: &str, path: &str) -> Result<Vec<u8>> {
    if path.split('/').any(|p| p == ".." || p.is_empty()) || path.contains('\\') {
        return Err(GateError::Other("仓库文件路径无效".into()));
    }
    let url = format!("https://raw.githubusercontent.com/{repo}/{revision}/{path}");
    let mut response = http()?.get(url).send().await?;
    if !response.status().is_success() {
        return Err(GateError::Other(format!(
            "下载扩展文件失败：HTTP {}",
            response.status()
        )));
    }
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        out.extend_from_slice(&chunk);
        if out.len() > 8 * 1024 * 1024 {
            return Err(GateError::Other("单个扩展文件超过 8 MB 导入上限".into()));
        }
    }
    Ok(out)
}
fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = match std::fs::symlink_metadata(ancestor) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(GateError::Other(
                    "所选路径经过联结点或符号链接，请选择实际目录".into(),
                ));
            }
        }
        if metadata.file_type().is_symlink() {
            return Err(GateError::Other(
                "所选路径经过符号链接，请选择实际目录".into(),
            ));
        }
    }
    Ok(())
}
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.contains(['\\', ':', '\0'])
                && !["CON", "PRN", "AUX", "NUL", "COM1", "LPT1"]
                    .contains(&part.split('.').next().unwrap_or("").to_uppercase().as_str())
        })
}
fn local_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
        depth: usize,
        total: &mut usize,
    ) -> Result<()> {
        if depth > 12 {
            return Err(GateError::Other("扩展目录层级过深".into()));
        }
        for e in std::fs::read_dir(path)? {
            let e = e?;
            let meta = std::fs::symlink_metadata(e.path())?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(GateError::Other("扩展包含联结点或符号链接，未跟随".into()));
                }
            }
            if meta.file_type().is_symlink() {
                return Err(GateError::Other("扩展包含符号链接，未跟随".into()));
            }
            let name = e.file_name();
            if [".git", "node_modules", ".venv"].iter().any(|s| name == *s) {
                continue;
            }
            if meta.is_dir() {
                walk(root, &e.path(), out, depth + 1, total)?;
            } else if meta.is_file() {
                if meta.len() > 8 * 1024 * 1024 {
                    return Err(GateError::Other("扩展文件超过 8 MB".into()));
                }
                *total += meta.len() as usize;
                if *total > 32 * 1024 * 1024 || out.len() >= 512 {
                    return Err(GateError::Other("扩展超过 32 MB 或 512 文件上限".into()));
                }
                let key = e
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(key, std::fs::read(e.path())?);
            }
        }
        Ok(())
    }
    reject_links(root)?;
    let mut out = BTreeMap::new();
    walk(root, root, &mut out, 0, &mut 0)?;
    Ok(out)
}

pub async fn import_skills(source: &str) -> Result<Vec<ExtensionManifest>> {
    let mut found = Vec::new();
    if source.starts_with("https://") {
        let g = github_source(source, None).await?;
        for entry in &g.tree {
            let Some(path) = entry["path"].as_str() else {
                continue;
            };
            if !(path == "SKILL.md" || path.ends_with("/SKILL.md"))
                || (!g.subdir.is_empty() && !path.starts_with(&format!("{}/", g.subdir)))
            {
                continue;
            }
            let bytes = blob(&g.repo, &g.revision, path).await?;
            let metadata = skill_metadata(
                std::str::from_utf8(&bytes).map_err(|e| GateError::Other(e.to_string()))?,
            )?;
            let dir = path.strip_suffix("/SKILL.md").unwrap_or("");
            found.push(ExtensionManifest {
                id: format!(
                    "skill-{}-{}",
                    metadata.name,
                    &hex::encode(Sha256::digest(format!("{}/{}", g.repo, dir)))[..8]
                ),
                name: metadata.name,
                description: metadata.description,
                kind: ExtensionKind::Skill,
                source: format!("https://github.com/{}/tree/HEAD/{}", g.repo, dir),
                version: g.revision.clone(),
                license: metadata.license.unwrap_or_else(|| "参见上游许可证".into()),
                // Codex 不读 skills/ 目录，装进去不生效 —— 见 `supported`。
                clients: vec![Client::ClaudeCode],
                install_method: "github-skill".into(),
                configuration: "{}".into(),
                dependencies: vec![],
            });
            if found.len() >= 50 {
                break;
            }
        }
    } else {
        reject_links(Path::new(source))?;
        let root = Path::new(source).canonicalize()?;
        let files = local_files(&root)?;
        for (name, bytes) in files
            .iter()
            .filter(|(p, _)| p.as_str() == "SKILL.md" || p.ends_with("/SKILL.md"))
        {
            let m = skill_metadata(
                std::str::from_utf8(bytes).map_err(|e| GateError::Other(e.to_string()))?,
            )?;
            let dir = root.join(name).parent().unwrap().to_path_buf();
            found.push(ExtensionManifest {
                id: format!(
                    "skill-{}-{}",
                    m.name,
                    &hex::encode(Sha256::digest(dir.to_string_lossy().as_bytes()))[..8]
                ),
                name: m.name,
                description: m.description,
                kind: ExtensionKind::Skill,
                source: dir.display().to_string(),
                version: hash_files(&local_files(&dir)?),
                license: m.license.unwrap_or_else(|| "参见本地许可证".into()),
                // Codex 不读 skills/ 目录，装进去不生效 —— 见 `supported`。
                clients: vec![Client::ClaudeCode],
                install_method: "local-skill".into(),
                configuration: "{}".into(),
                dependencies: vec![],
            });
        }
    }
    if found.is_empty() {
        return Err(GateError::Other("所选来源没有有效的 SKILL.md".into()));
    }
    let db = Repository::open()?;
    for m in &found {
        db.put("catalog", &m.id, m)?;
    }
    Ok(found)
}

pub fn import_mcp(name: &str, text: &str) -> Result<ExtensionManifest> {
    let mut config = config_io::object(text)?;
    if let Some(servers) = config.get("mcpServers").and_then(|v| v.as_object()) {
        config = servers
            .get(name)
            .or_else(|| {
                if servers.len() == 1 {
                    servers.values().next()
                } else {
                    None
                }
            })
            .cloned()
            .ok_or_else(|| GateError::Other("配置包含多个 MCP，请填写要导入的服务名称".into()))?;
    }
    validate_mcp(&config)?;
    if !valid_name(name) {
        return Err(GateError::Other(
            "MCP 名称请使用小写字母、数字和连字符".into(),
        ));
    }
    let id = format!("mcp-{name}");
    let db = Repository::open()?;
    db.set_meta(
        &format!("extension-config:{id}"),
        &crate::secret::seal(&serde_json::to_string(&config)?)?,
    )?;
    let mut public = config.clone();
    for field in ["env", "headers", "http_headers"] {
        if let Some(object) = public.get_mut(field).and_then(|v| v.as_object_mut()) {
            for value in object.values_mut() {
                *value = serde_json::json!("[已加密保存]");
            }
        }
    }
    let m = ExtensionManifest {
        id: id.clone(),
        name: name.into(),
        description: "手动导入的 MCP 连接配置".into(),
        kind: ExtensionKind::Mcp,
        source: "手动配置".into(),
        version: hex::encode(Sha256::digest(text.as_bytes())),
        license: "参见服务来源".into(),
        clients: vec![Client::ClaudeCode, Client::Codex],
        install_method: "mcp-config".into(),
        configuration: serde_json::to_string_pretty(&public)?,
        dependencies: vec![],
    };
    db.put("catalog", &id, &m)?;
    Ok(m)
}
fn validate_mcp(v: &serde_json::Value) -> Result<()> {
    if let Some(url) = v.get("url").and_then(|v| v.as_str()) {
        crate::endpoint::endpoint_base(url)?;
    } else if v
        .get("command")
        .and_then(|v| v.as_str())
        .is_none_or(|s| s.trim().is_empty())
    {
        return Err(GateError::Other("MCP 需要 command 或 HTTP url".into()));
    }
    if v.get("args").is_some_and(|v| {
        !v.as_array()
            .is_some_and(|a| a.iter().all(|v| v.is_string()))
    }) {
        return Err(GateError::Other("MCP args 必须是字符串数组".into()));
    }
    for field in ["env", "headers", "http_headers"] {
        if v.get(field).is_some_and(|v| {
            !v.as_object()
                .is_some_and(|a| a.values().all(|v| v.is_string()))
        }) {
            return Err(GateError::Other(format!("MCP {field} 必须是字符串对象")));
        }
    }
    Ok(())
}

/// 这个扩展装到这个客户端上**真的会起作用吗**。
///
/// Skills 是 Claude 的机制，Codex 不读 `skills/` 目录。原来精选目录里两个
/// Skill 的 `clients` 写着 `["claude-code","codex"]`，界面照着出选项，
/// 装完显示「已安装」、状态检查也一直报「已安装」（它只比对文件哈希，
/// 文件确实躺在那儿）—— 而 Codex 一眼都不会看。
///
/// 这跟 `launch.rs` 给 `TELEMETRY_OFF` 立的规矩是同一条：设一个不存在的
/// 变量等于什么都没关，而界面上写着「已关闭」，那是在说谎。
fn supported(m: &ExtensionManifest, client: Client) -> Result<()> {
    if m.kind == ExtensionKind::Skill && client == Client::Codex {
        return Err(GateError::Other(
            "Codex 不读取 Skills 目录，装进去不会生效；请选择 Claude Code".into(),
        ));
    }
    if !m.clients.contains(&client) {
        return Err(GateError::Other(format!("{} 不支持此客户端", m.name)));
    }
    Ok(())
}

fn target_dir(db: &Repository, kind: IdentityKind, id: &str, client: Client) -> Result<PathBuf> {
    if client == Client::ClaudeDesktop {
        return Err(GateError::Other(
            "此扩展当前支持 Claude Code 和 Codex".into(),
        ));
    }
    if kind == IdentityKind::Relay {
        let e: Environment = db.get("environments", id)?;
        if e.client != client {
            return Err(GateError::Other("扩展目标客户端不匹配".into()));
        }
        crate::config_io::environment_dir(&db.root, id)
    } else if client == Client::Codex {
        Ok(dirs::home_dir()
            .ok_or_else(|| GateError::Other("用户目录不存在".into()))?
            .join(".codex"))
    } else if id.is_empty() {
        Ok(dirs::home_dir()
            .ok_or_else(|| GateError::Other("用户目录不存在".into()))?
            .join(".claude"))
    } else {
        crate::accounts::validate_label(id)?;
        let path = crate::accounts::AccountRoots::current().slot_dir(id);
        if !path.is_dir() {
            return Err(GateError::Other("官方账户槽位不存在".into()));
        }
        Ok(path)
    }
}
fn hash_files(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut hash = Sha256::new();
    for (name, body) in files {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(body);
    }
    hex::encode(hash.finalize())
}
async fn skill_files(m: &ExtensionManifest) -> Result<BTreeMap<String, Vec<u8>>> {
    if m.install_method == "local-skill" {
        let files = local_files(Path::new(&m.source))?;
        if hash_files(&files) != m.version {
            return Err(GateError::Other(
                "本地来源已修改，请先检查更新，再预览安装".into(),
            ));
        }
        return Ok(files);
    }
    let g = github_source(&m.source, Some(&m.version)).await?;
    let mut out = BTreeMap::new();
    let mut total = 0;
    let prefix = if g.subdir.is_empty() {
        String::new()
    } else {
        format!("{}/", g.subdir)
    };
    for entry in &g.tree {
        let Some(path) = entry["path"].as_str().filter(|p| p.starts_with(&prefix)) else {
            continue;
        };
        if entry["type"] == "tree" {
            continue;
        }
        if entry["type"] != "blob" || entry["mode"] == "120000" {
            return Err(GateError::Other(
                "Skill 包含符号链接或子模块，未安装".into(),
            ));
        }
        let bytes = blob(&g.repo, &g.revision, path).await?;
        total += bytes.len();
        if out.len() >= 512 || total > 32 * 1024 * 1024 {
            return Err(GateError::Other("Skill 超过文件数或大小限制".into()));
        }
        let relative = path.strip_prefix(&prefix).unwrap();
        if !safe_relative(relative) {
            return Err(GateError::Other("仓库包含不安全的文件路径".into()));
        }
        if out
            .keys()
            .any(|key: &String| key.eq_ignore_ascii_case(relative))
        {
            return Err(GateError::Other("仓库文件名在 Windows 上冲突".into()));
        }
        out.insert(relative.to_string(), bytes);
    }
    let text = out
        .get("SKILL.md")
        .ok_or_else(|| GateError::Other("所选目录缺少 SKILL.md".into()))?;
    skill_metadata(std::str::from_utf8(text).map_err(|e| GateError::Other(e.to_string()))?)?;
    Ok(out)
}

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
pub struct InstallRequest {
    pub extension_id: String,
    pub identity_kind: IdentityKind,
    pub environment_id: String,
    pub client: Client,
    pub directory: Option<String>,
    pub configuration: Option<String>,
    #[serde(default)]
    pub preview_fingerprint: Option<String>,
}
#[derive(Serialize, ts_rs::TS)]
pub struct InstallPreview {
    pub destination: String,
    pub files: Vec<String>,
    pub changes: Vec<String>,
    pub conflict: bool,
    pub diff: String,
    pub fingerprint: String,
}

fn mcp_configuration(
    db: &Repository,
    m: &ExtensionManifest,
    r: &InstallRequest,
) -> Result<serde_json::Value> {
    let text = if let Some(text) = r.configuration.as_ref().filter(|s| !s.trim().is_empty()) {
        text.clone()
    } else if let Some(sealed) = db.meta(&format!("extension-config:{}", m.id))? {
        crate::secret::open(&sealed)
            .ok_or_else(|| GateError::Other("MCP 配置无法解密，请重新导入".into()))?
    } else {
        m.configuration.clone()
    };
    let mut v = config_io::object(&text)?;
    if let Some(args) = v.get_mut("args").and_then(|v| v.as_array_mut()) {
        for arg in args {
            if arg.as_str() == Some("${DIRECTORY}") {
                let dir = r
                    .directory
                    .as_deref()
                    .filter(|s| Path::new(s).is_dir())
                    .ok_or_else(|| GateError::Other("请先选择可访问的本地目录".into()))?;
                *arg = serde_json::json!(dir);
            }
        }
    }
    validate_mcp(&v)?;
    if r.client == Client::Codex {
        let map = v.as_object_mut().unwrap();
        map.remove("type");
        if let Some(headers) = map.remove("headers") {
            map.insert("http_headers".into(), headers);
        }
    }
    Ok(v)
}
fn mcp_edit(
    dir: &Path,
    client: Client,
    id: &str,
    config: Option<serde_json::Value>,
) -> Result<Edit> {
    if client == Client::ClaudeCode {
        let path = dir.join(".claude.json");
        let before = config_io::read_optional(&path)?;
        let mut v = config_io::object(
            &String::from_utf8(before.clone().unwrap_or_default())
                .map_err(|e| GateError::Other(e.to_string()))?,
        )?;
        if v.get("mcpServers").is_some_and(|v| !v.is_object()) {
            return Err(GateError::Other("mcpServers 不是对象".into()));
        }
        if v.get("mcpServers").is_none() {
            v["mcpServers"] = serde_json::json!({});
        }
        if let Some(config) = config {
            v["mcpServers"][id] = config;
        } else {
            v["mcpServers"].as_object_mut().unwrap().remove(id);
        }
        Ok(Edit {
            path,
            expected: before,
            body: Some(serde_json::to_vec_pretty(&v)?),
        })
    } else {
        let path = dir.join("config.toml");
        let before = config_io::read_optional(&path)?;
        let text = String::from_utf8(before.clone().unwrap_or_default())
            .map_err(|e| GateError::Other(e.to_string()))?;
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| GateError::Other(e.to_string()))?;
        if let Some(config) = config {
            let value: toml_edit::DocumentMut =
                toml_edit::ser::to_string(&serde_json::json!({"mcp_servers":{id:config}}))
                    .map_err(|e| GateError::Other(e.to_string()))?
                    .parse()
                    .map_err(|e: toml_edit::TomlError| GateError::Other(e.to_string()))?;
            if !doc.contains_key("mcp_servers") {
                doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            doc["mcp_servers"][id] = value["mcp_servers"][id].clone();
        } else if let Some(table) = doc.get_mut("mcp_servers").and_then(|t| t.as_table_mut()) {
            table.remove(id);
        }
        Ok(Edit {
            path,
            expected: before,
            body: Some(doc.to_string().into_bytes()),
        })
    }
}

pub async fn preview(r: &InstallRequest) -> Result<InstallPreview> {
    preview_in(&mut Repository::open()?, r).await
}
async fn preview_in(db: &mut Repository, r: &InstallRequest) -> Result<InstallPreview> {
    let m = manifest_in(db, &r.extension_id)?;
    supported(&m, r.client)?;
    let dir = target_dir(db, r.identity_kind, &r.environment_id, r.client)?;
    let installed = db
        .list::<ExtensionInstallation>("installations")?
        .into_iter()
        .find(|i| {
            i.extension_id == m.id
                && i.environment_id == r.environment_id
                && i.identity_kind == r.identity_kind
                && i.client == r.client
        });
    match m.kind {
        ExtensionKind::Skill => {
            let files = skill_files(&m).await?;
            let metadata = skill_metadata(
                std::str::from_utf8(&files["SKILL.md"])
                    .map_err(|e| GateError::Other(e.to_string()))?,
            )?;
            let dst = dir.join("skills").join(metadata.name);
            reject_links(&dst)?;
            let old = if dst.exists() {
                local_files(&dst)?
            } else {
                BTreeMap::new()
            };
            let conflict = !old.is_empty()
                && installed
                    .as_ref()
                    .is_none_or(|i| i.content_hash != hash_files(&old));
            let changes = files
                .iter()
                .filter(|(p, b)| old.get(*p) != Some(*b))
                .map(|(p, _)| format!("更新 {p}"))
                .chain(
                    old.keys()
                        .filter(|p| !files.contains_key(*p))
                        .map(|p| format!("移除旧扩展文件 {p}")),
                )
                .collect();
            Ok(InstallPreview {
                destination: dst.display().to_string(),
                files: files.keys().cloned().collect(),
                changes,
                conflict,
                diff: skill_diff(&old, &files),
                fingerprint: format!("{}:{}", hash_files(&old), hash_files(&files)),
            })
        }
        ExtensionKind::Mcp => {
            reject_links(&dir)?;
            let config = mcp_configuration(db, &m, r)?;
            let edit = mcp_edit(&dir, r.client, &m.id, Some(config))?;
            // Detect a same-name unmanaged entry or a changed managed config on install.
            let existing = mcp_from_bytes(r.client, edit.expected.as_deref(), &m.id)?;
            let conflict = existing.as_ref().is_some_and(|v| {
                installed
                    .as_ref()
                    .is_none_or(|i| i.content_hash != hash_json(v))
            });
            Ok(InstallPreview {
                destination: edit.path.display().to_string(),
                files: vec![m.id.clone()],
                diff: format!(
                    "当前：{}\n应用后：{}",
                    redact_mcp(existing.as_ref()),
                    redact_mcp(Some(&mcp_configuration(db, &m, r)?))
                ),
                fingerprint: config_io::fingerprint(&[edit], 0),
                changes: vec!["只更新此 MCP 条目，保留其他服务和客户端设置".into()],
                conflict,
            })
        }
        _ => Err(GateError::Other(
            "应用集成请进入详情接入；配置模板请创建使用环境".into(),
        )),
    }
}
fn hash_json(v: &serde_json::Value) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap_or_default()))
}
fn mcp_entry(dir: &Path, client: Client, id: &str) -> Result<Option<serde_json::Value>> {
    let file = if client == Client::ClaudeCode {
        ".claude.json"
    } else {
        "config.toml"
    };
    mcp_from_bytes(
        client,
        config_io::read_optional(&dir.join(file))?.as_deref(),
        id,
    )
}
fn mcp_from_bytes(
    client: Client,
    bytes: Option<&[u8]>,
    id: &str,
) -> Result<Option<serde_json::Value>> {
    if client == Client::ClaudeCode {
        Ok(config_io::parse_object(bytes)?["mcpServers"]
            .get(id)
            .cloned())
    } else {
        let text = std::str::from_utf8(bytes.unwrap_or_default())
            .map_err(|e| GateError::Other(e.to_string()))?;
        let value: serde_json::Value =
            toml_edit::de::from_str(text).map_err(|e| GateError::Other(e.to_string()))?;
        Ok(value["mcp_servers"].get(id).cloned())
    }
}

fn redact_mcp(value: Option<&serde_json::Value>) -> String {
    let mut value = value.cloned().unwrap_or_default();
    for key in ["env", "headers", "http_headers"] {
        if let Some(map) = value.get_mut(key).and_then(|v| v.as_object_mut()) {
            for v in map.values_mut() {
                *v = serde_json::json!("[已脱敏]");
            }
        }
    }
    serde_json::to_string_pretty(&value).unwrap_or_default()
}
fn skill_diff(old: &BTreeMap<String, Vec<u8>>, new: &BTreeMap<String, Vec<u8>>) -> String {
    let mut out = String::new();
    for (name, body) in new {
        if old.get(name) == Some(body) {
            continue;
        }
        out.push_str(&format!("--- {name}（当前）\n+++ {name}（来源）\n"));
        if let Ok(text) = std::str::from_utf8(old.get(name).map(Vec::as_slice).unwrap_or_default())
        {
            for line in text.lines().take(60) {
                out.push_str(&format!("- {line}\n"));
            }
        }
        if let Ok(text) = std::str::from_utf8(body) {
            for line in text.lines().take(60) {
                out.push_str(&format!("+ {line}\n"));
            }
        } else {
            out.push_str("二进制文件变化\n");
        }
        if out.len() > 24000 {
            out.push_str("差异预览已截断，请在本地查看其余文件。\n");
            break;
        }
    }
    out
}
pub async fn install(r: InstallRequest) -> Result<ExtensionInstallation> {
    install_in(&mut Repository::open()?, r).await
}
async fn install_in(db: &mut Repository, r: InstallRequest) -> Result<ExtensionInstallation> {
    let m = manifest_in(db, &r.extension_id)?;
    let preview = preview_in(db, &r).await?;
    if preview.conflict {
        return Err(GateError::Other(
            "安装位置存在未托管或已修改的内容；请先查看差异并另存用户修改".into(),
        ));
    }
    if r.preview_fingerprint.as_deref() != Some(&preview.fingerprint) {
        return Err(GateError::Other(
            "来源或目标已变化，请重新预览安装变更".into(),
        ));
    }
    let dir = target_dir(db, r.identity_kind, &r.environment_id, r.client)?;
    reject_links(&dir)?;
    let previous = db
        .list::<ExtensionInstallation>("installations")?
        .into_iter()
        .find(|i| {
            i.extension_id == m.id
                && i.environment_id == r.environment_id
                && i.identity_kind == r.identity_kind
                && i.client == r.client
        });
    let id = previous
        .as_ref()
        .map(|i| i.id.clone())
        .unwrap_or_else(config_io::id);
    let (path, hash, state, edits) = if m.kind == ExtensionKind::Skill {
        let files = skill_files(&m).await?;
        let dst = PathBuf::from(&preview.destination);
        reject_links(&dst)?;
        let old = if dst.exists() {
            local_files(&dst)?
        } else {
            BTreeMap::new()
        };
        if format!("{}:{}", hash_files(&old), hash_files(&files)) != preview.fingerprint {
            return Err(GateError::Other("安装前文件已变化，请重新预览".into()));
        }
        let mut edits = Vec::new();
        for (name, bytes) in &files {
            edits.push(Edit {
                path: dst.join(name),
                expected: old.get(name).cloned(),
                body: Some(bytes.clone()),
            });
        }
        for (name, bytes) in &old {
            if !files.contains_key(name) {
                edits.push(Edit {
                    path: dst.join(name),
                    expected: Some(bytes.clone()),
                    body: None,
                });
            }
        }
        (dst, hash_files(&files), "installed", edits)
    } else {
        let config = mcp_configuration(db, &m, &r)?;
        let edit = mcp_edit(&dir, r.client, &m.id, Some(config))?;
        if config_io::fingerprint(std::slice::from_ref(&edit), 0) != preview.fingerprint {
            return Err(GateError::Other("MCP 配置在预览后改变，请重新预览".into()));
        }
        let hash = hash_json(
            &mcp_from_bytes(r.client, edit.body.as_deref(), &m.id)?
                .ok_or_else(|| GateError::Other("生成的 MCP 条目不存在".into()))?,
        );
        (edit.path.clone(), hash, "enabled", vec![edit])
    };
    let installation = ExtensionInstallation {
        id: id.clone(),
        extension_id: m.id,
        environment_id: r.environment_id,
        identity_kind: r.identity_kind,
        client: r.client,
        version: m.version,
        state: state.into(),
        path: path.display().to_string(),
        content_hash: hash,
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    let hash_edits = edits.clone();
    crate::repository::commit_database(db, edits, |_| {
        db.put("installations", &id, &installation)?;
        sync_config_hash(db, &installation, &hash_edits)
    })?;
    Ok(installation)
}
fn sync_config_hash(db: &Repository, i: &ExtensionInstallation, edits: &[Edit]) -> Result<()> {
    if i.identity_kind != IdentityKind::Relay || i.client != Client::Codex {
        return Ok(());
    }
    let path = crate::config_io::environment_dir(&db.root, &i.environment_id)?.join("config.toml");
    let Some(edit) = edits.iter().find(|edit| edit.path == path) else {
        return Ok(());
    };
    let recorded: String = db
        .conn
        .query_row(
            "SELECT hashes FROM environments WHERE id=?1",
            [&i.environment_id],
            |r| r.get(0),
        )
        .map_err(|e| GateError::Database(e.to_string()))?;
    let mut hashes: BTreeMap<String, String> = serde_json::from_str(&recorded)?;
    let before = edit
        .expected
        .as_ref()
        .map(|body| hex::encode(Sha256::digest(body)));
    // Installing one MCP entry must not endorse unrelated external configuration changes.
    if before.is_none() || hashes.get("config.toml") != before.as_ref() {
        return Ok(());
    }
    if let Some(body) = &edit.body {
        hashes.insert("config.toml".into(), hex::encode(Sha256::digest(body)));
    } else {
        hashes.remove("config.toml");
    }
    db.conn
        .execute(
            "UPDATE environments SET hashes=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&hashes)?, i.environment_id],
        )
        .map_err(|e| GateError::Database(e.to_string()))?;
    Ok(())
}
pub fn installation_state(db: &Repository, i: &ExtensionInstallation) -> Result<String> {
    let m = manifest_in(db, &i.extension_id)?;
    if m.kind == ExtensionKind::Application {
        return Ok(if Path::new(&i.path).join("server.js").is_file() {
            "connected"
        } else {
            "missing"
        }
        .into());
    }
    let dir = target_dir(db, i.identity_kind, &i.environment_id, i.client)?;
    if m.kind == ExtensionKind::Mcp {
        return Ok(match mcp_entry(&dir, i.client, &m.id)? {
            Some(v) if hash_json(&v) == i.content_hash => "enabled",
            Some(_) => "external",
            None => "missing",
        }
        .into());
    }
    let path = Path::new(&i.path);
    if !path.starts_with(dir.join("skills")) {
        return Ok("unverified".into());
    }
    if !path.join("SKILL.md").is_file() {
        return Ok("missing".into());
    }
    let files = local_files(path)?;
    Ok(if hash_files(&files) == i.content_hash {
        "installed"
    } else {
        "external"
    }
    .into())
}
pub fn connect_application() -> Result<ExtensionInstallation> {
    let db = Repository::open()?;
    let cfg = crate::plugins::sillytavern::load_config_checked()?;
    let root = cfg.sillytavern_root.canonicalize()?;
    if !root.join("server.js").is_file() {
        return Err(GateError::Other(
            "请先在应用详情填写已有 SillyTavern 的目录".into(),
        ));
    }
    let package = config_io::read_object(&root.join("package.json"))?;
    if package["version"].as_str().is_none() {
        return Err(GateError::Other("应用 package.json 缺少版本信息".into()));
    }
    let installation = ExtensionInstallation {
        id: "application-sillytavern".into(),
        extension_id: "sillytavern".into(),
        environment_id: String::new(),
        identity_kind: IdentityKind::Official,
        client: Client::ClaudeCode,
        version: "integration-1".into(),
        state: "connected".into(),
        path: root.display().to_string(),
        content_hash: hash_json(&package),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    db.put("installations", &installation.id, &installation)?;
    Ok(installation)
}
pub fn uninstall(id: &str) -> Result<()> {
    uninstall_in(&Repository::open()?, id)
}
fn uninstall_in(db: &Repository, id: &str) -> Result<()> {
    let i: ExtensionInstallation = db.get("installations", id)?;
    let m = manifest_in(db, &i.extension_id)?;
    if m.kind == ExtensionKind::Application {
        if crate::plugins::sillytavern::status().state == crate::plugins::PluginState::Running {
            return Err(GateError::Other(
                "请先停止应用，再解除接入；应用文件与数据会保留".into(),
            ));
        }
        return db.remove("installations", id);
    }
    let dir = target_dir(db, i.identity_kind, &i.environment_id, i.client)?;
    reject_links(&dir)?;
    let mut edits = Vec::new();
    if m.kind == ExtensionKind::Mcp {
        if let Some(existing) = mcp_entry(&dir, i.client, &m.id)? {
            if hash_json(&existing) != i.content_hash {
                return Err(GateError::Other(
                    "MCP 条目已经修改，请先保留修改并处理冲突".into(),
                ));
            }
        }
        edits.push(mcp_edit(&dir, i.client, &m.id, None)?);
    } else if m.kind == ExtensionKind::Skill {
        let path = PathBuf::from(&i.path);
        reject_links(&path)?;
        if !path.starts_with(dir.join("skills")) {
            return Err(GateError::Other("扩展路径不属于所选环境".into()));
        }
        if path.exists() {
            let files = local_files(&path)?;
            let retained = db
                .root
                .join("retained-extension-data")
                .join(config_io::id());
            for (name, body) in files {
                edits.push(Edit {
                    path: retained.join(&name),
                    expected: None,
                    body: Some(body.clone()),
                });
                edits.push(Edit {
                    path: path.join(name),
                    expected: Some(body),
                    body: None,
                });
            }
        }
    }
    let hash_edits = edits.clone();
    crate::repository::commit_database(db, edits, |_| {
        db.remove("installations", id)?;
        sync_config_hash(db, &i, &hash_edits)
    })?;
    if m.kind == ExtensionKind::Skill {
        remove_empty_dirs(Path::new(&i.path));
    }
    Ok(())
}
fn remove_empty_dirs(path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                remove_empty_dirs(&e.path());
            }
        }
        let _ = std::fs::remove_dir(path);
    }
}

fn builtin_ids() -> Result<BTreeMap<String, String>> {
    Ok(
        serde_json::from_str::<Vec<ExtensionManifest>>(include_str!("../extension-catalog.json"))?
            .into_iter()
            .map(|m| (m.id, m.version))
            .collect(),
    )
}
fn short(version: &str) -> String {
    version.chars().take(12).collect()
}

/// 查上游有没有新版本。返回给界面原样显示的几句话。
///
/// # 精选目录里的版本**不会**被自动改掉
///
/// 内置目录里 `skill-creator` / `webapp-testing` 的 `version` 就是
/// ATTRIBUTION.md 里按哈希写死、核对过许可证的那个提交。原来这里对所有
/// Skill 一律取 `main` 最新 SHA 并写进 `catalog` 表 —— 而 `catalog_in`
/// 里 DB 行会盖住内置行。于是使用者点一次「检查来源更新」，
/// 签字画押的那句话就不准了：别人装到的根本不是被核过的那一版。
///
/// 现在内置条目只**报告**有新提交，换版要自己从来源手动导入（导入进来的是
/// 一条独立记录，跟内置的那条互不影响）。使用者自己导入的 Skill 照旧跟随更新。
///
/// 单个来源查不动（本地目录被删、仓库没了）只登记一句，不再中断整轮 ——
/// 原来一个 `?` 会让后面的扩展一个都查不到。
pub async fn check_updates() -> Result<Vec<String>> {
    let db = Repository::open()?;
    let builtin = builtin_ids()?;
    let mut notes = Vec::new();
    for mut m in catalog_in(&db)? {
        if m.kind != ExtensionKind::Skill {
            continue;
        }
        let latest = if m.install_method == "local-skill" {
            local_files(Path::new(&m.source)).map(|f| hash_files(&f))
        } else {
            github_source(&m.source, None).await.map(|g| g.revision)
        };
        let latest = match latest {
            Ok(v) => v,
            Err(e) => {
                notes.push(format!("{}：查不到来源版本（{e}）", m.name));
                continue;
            }
        };
        if m.version == latest {
            continue;
        }
        if let Some(pinned) = builtin.get(&m.id) {
            notes.push(format!(
                "{}：上游有新提交 {}；精选目录仍固定在核对过许可证的 {}。需要新版请从来源手动导入。",
                m.name,
                short(&latest),
                short(pinned)
            ));
            continue;
        }
        notes.push(format!(
            "{}：{} → {}，已更新来源版本；重新预览安装才会写入文件。",
            m.name,
            short(&m.version),
            short(&latest)
        ));
        m.version = latest;
        db.put("catalog", &m.id, &m)?;
    }
    if notes.is_empty() {
        notes.push("所有来源都已是记录的版本。".into());
    }
    Ok(notes)
}

#[derive(Serialize, ts_rs::TS)]
pub struct McpCheck {
    pub connected: bool,
    pub tools: Vec<String>,
    pub detail: String,
}
pub async fn check_mcp(r: InstallRequest) -> Result<McpCheck> {
    use rmcp::ServiceExt;
    let db = Repository::open()?;
    let m = manifest(&r.extension_id)?;
    let config = mcp_configuration(&db, &m, &r)?;
    let future = async {
        if let Some(url) = config["url"].as_str() {
            let mut options=rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url);
            if let Some(headers) = config
                .get("headers")
                .or_else(|| config.get("http_headers"))
                .and_then(|v| v.as_object())
            {
                for (k, v) in headers {
                    options.custom_headers.insert(
                        k.parse()
                            .map_err(|_| GateError::Other("HTTP 头名称无效".into()))?,
                        v.as_str()
                            .unwrap()
                            .parse()
                            .map_err(|_| GateError::Other("HTTP 头值无效".into()))?,
                    );
                }
            }
            let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
                reqwest_mcp::Client::builder()
                    .redirect(reqwest_mcp::redirect::Policy::none())
                    .build()
                    .map_err(|e| GateError::Other(e.to_string()))?,
                options,
            );
            let service = ()
                .serve(transport)
                .await
                .map_err(|e| GateError::Other(format!("MCP 初始化失败：{e}")))?;
            let result = service
                .list_all_tools()
                .await
                .map_err(|e| GateError::Other(format!("MCP 能力读取失败：{e}")));
            let _ = service.cancel().await;
            result.map(|tools| {
                tools
                    .into_iter()
                    .map(|t| t.name.to_string())
                    .collect::<Vec<_>>()
            })
        } else {
            let command = config["command"].as_str().unwrap();
            let resolved = resolve_mcp_command(command)?;
            let mut process = crate::process::hidden_tokio(tokio::process::Command::new(&resolved));
            process
                .env_clear()
                .envs(crate::sessions::sanitized_environment(
                    std::env::vars(),
                    vec![],
                ));
            if let Some(args) = config["args"].as_array() {
                process.args(args.iter().filter_map(|a| a.as_str()));
            }
            if let Some(env) = config["env"].as_object() {
                process.envs(env.iter().filter_map(|(k, v)| v.as_str().map(|v| (k, v))));
            }
            process.kill_on_drop(true);
            let transport = rmcp::transport::TokioChildProcess::new(process)
                .map_err(|e| GateError::Other(format!("启动 MCP 失败（请检查依赖）：{e}")))?;
            let service = ()
                .serve(transport)
                .await
                .map_err(|e| GateError::Other(format!("MCP 初始化失败：{e}")))?;
            let result = service
                .list_all_tools()
                .await
                .map_err(|e| GateError::Other(format!("MCP 能力读取失败：{e}")));
            let _ = service.cancel().await;
            result.map(|tools| {
                tools
                    .into_iter()
                    .map(|t| t.name.to_string())
                    .collect::<Vec<_>>()
            })
        }
    };
    let tools = tokio::time::timeout(std::time::Duration::from_secs(30), future)
        .await
        .map_err(|_| GateError::Other("MCP 连接超时（30 秒）".into()))??;
    Ok(McpCheck {
        connected: true,
        tools,
        detail: "已完成 MCP 初始化并读取工具能力；测试连接已关闭".into(),
    })
}

fn resolve_mcp_command(command: &str) -> Result<PathBuf> {
    if Path::new(command).is_absolute() {
        return Ok(command.into());
    }
    let extensions = if cfg!(windows) {
        vec![".exe", ".cmd", ".bat", ""]
    } else {
        vec![""]
    };
    for root in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for ext in &extensions {
            let path = root.join(format!("{command}{ext}"));
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(GateError::Other(format!(
        "找不到 MCP 启动命令 {command}，请先安装依赖"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex 不读 `skills/`。装进去不生效却报「已安装」＝面板在说谎，
    /// 跟 `launch.rs` 里「设一个不存在的变量却显示已关闭」是同一条规矩。
    #[test]
    fn skills_are_never_offered_or_installed_for_codex() {
        let skill = ExtensionManifest {
            id: "skill-x".into(),
            name: "x".into(),
            description: "d".into(),
            kind: ExtensionKind::Skill,
            source: "s".into(),
            version: "1".into(),
            license: "MIT".into(),
            clients: vec![Client::ClaudeCode],
            install_method: "local-skill".into(),
            configuration: "{}".into(),
            dependencies: vec![],
        };
        assert!(supported(&skill, Client::ClaudeCode).is_ok());
        assert!(supported(&skill, Client::Codex).is_err());
        assert!(supported(&skill, Client::ClaudeDesktop).is_err());
        // 就算有人把 clients 写回带 codex，也照样挡住。
        let lying = ExtensionManifest {
            clients: vec![Client::ClaudeCode, Client::Codex],
            ..skill
        };
        assert!(supported(&lying, Client::Codex).is_err());
        // 精选目录本身也不许再声明 codex。
        for m in serde_json::from_str::<Vec<ExtensionManifest>>(include_str!(
            "../extension-catalog.json"
        ))
        .unwrap()
        .iter()
        .filter(|m| m.kind == ExtensionKind::Skill)
        {
            assert_eq!(m.clients, vec![Client::ClaudeCode], "{}", m.id);
        }
    }

    /// 精选目录里的版本就是 ATTRIBUTION.md 按哈希写死、核对过许可证的那一版。
    /// 「检查来源更新」不许把它们冲掉 —— 冲掉之后别人装到的就不是被核过的版本。
    #[test]
    fn curated_pins_are_the_ones_attribution_signed_for() {
        let pinned = builtin_ids().unwrap();
        // 用 CARGO_MANIFEST_DIR 而不是裸相对路径：`cargo test` 的工作目录
        // 随「从哪儿跑」而变，而这个 crate 从 `src-tauri/` 搬到
        // `crates/qb-extensions/` 之后，`../ATTRIBUTION.md` 指到了 `crates/` 里。
        let attribution =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ATTRIBUTION.md"))
                .unwrap();
        for id in ["skill-creator", "webapp-testing"] {
            let version = pinned.get(id).expect(id);
            assert!(
                attribution.contains(version.as_str()),
                "{id} 固定在 {version}，但 ATTRIBUTION.md 里没有这个哈希"
            );
        }
    }

    /// 数据库里的行盖不掉精选条目的来源与版本。
    ///
    /// 旧版本的「检查来源更新」真的写过这样的行；只靠新版 `check_updates`
    /// 自觉不写是不够的，得让合并本身就还原得回固定版本。
    #[test]
    fn a_database_row_cannot_repoint_a_curated_entry() {
        let root = std::env::temp_dir().join(format!("qb-catalog-{}", config_io::id()));
        let db = Repository::open_in(&root).unwrap();
        let pinned = builtin_ids().unwrap();
        let id = "skill-creator";
        let mut hijacked = manifest_in(&db, id).unwrap();
        hijacked.version = "0000000000000000000000000000000000000000".into();
        hijacked.source = "https://github.com/someone/else".into();
        hijacked.description = "使用者自己改的描述".into();
        db.put("catalog", id, &hijacked).unwrap();

        let merged = manifest_in(&db, id).unwrap();
        assert_eq!(merged.version.as_str(), pinned[id], "固定版本必须还原");
        assert!(merged
            .source
            .starts_with("https://github.com/anthropics/skills"));
        // 其余字段照旧允许覆盖 —— 收紧的只有「装到的是不是核过的那一版」。
        assert_eq!(merged.description, "使用者自己改的描述");
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn fixture(client: Client) -> (std::path::PathBuf, Repository, InstallRequest) {
        let root = std::env::temp_dir().join(format!("QB-扩展测试-{}", config_io::id()));
        let db = Repository::open_in(&root).unwrap();
        let p = Provider {
            id: "provider".into(),
            name: "fixture".into(),
            base_url: "https://example.invalid".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
            topup_per_usd: None,
        };
        db.put("providers", &p.id, &p).unwrap();
        let e = Environment {
            id: "env".into(),
            provider_id: p.id,
            name: "fixture".into(),
            client,
            credential_id: None,
            model: String::new(),
            small_model: String::new(),
            wire_api: "responses".into(),
            auth_style: "none".into(),
            config_dir: root.join("environments/env").display().to_string(),
            revision: 1,
            applied_revision: None,
            config_state: "saved".into(),
            via_router: false,
        };
        db.set_environment(&e).unwrap();
        let m=ExtensionManifest{id:"test-mcp".into(),name:"test".into(),description:"fixture".into(),kind:ExtensionKind::Mcp,source:"fixture".into(),version:"1".into(),license:"MIT".into(),clients:vec![client],install_method:"mcp-config".into(),configuration:r#"{"url":"https://example.invalid/mcp","headers":{"Authorization":"synthetic-test-value"}}"#.into(),dependencies:vec![]};
        db.put("catalog", &m.id, &m).unwrap();
        let r = InstallRequest {
            extension_id: m.id,
            identity_kind: IdentityKind::Relay,
            environment_id: e.id,
            client,
            directory: None,
            configuration: None,
            preview_fingerprint: None,
        };
        (root, db, r)
    }
    #[tokio::test]
    async fn codex_mcp_roundtrip_uses_native_headers_and_keeps_other_settings() {
        let (root, mut db, mut r) = fixture(Client::Codex);
        let dir = root.join("environments/env");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "model = 'keep-me'\n").unwrap();
        r.preview_fingerprint = Some(preview_in(&mut db, &r).await.unwrap().fingerprint);
        let i = install_in(&mut db, r.clone()).await.unwrap();
        let entry = mcp_entry(&dir, Client::Codex, &i.extension_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            entry["http_headers"]["Authorization"],
            "synthetic-test-value"
        );
        assert!(entry.get("headers").is_none());
        let preview = preview_in(&mut db, &r).await.unwrap();
        assert!(!preview.conflict);
        r.preview_fingerprint = Some(preview.fingerprint);
        let again = install_in(&mut db, r).await.unwrap();
        assert_eq!(i.id, again.id);
        assert_eq!(
            db.list::<ExtensionInstallation>("installations")
                .unwrap()
                .len(),
            1
        );
        uninstall_in(&db, &i.id).unwrap();
        assert!(mcp_entry(&dir, Client::Codex, &i.extension_id)
            .unwrap()
            .is_none());
        assert!(std::fs::read_to_string(dir.join("config.toml"))
            .unwrap()
            .contains("keep-me"));
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn mcp_changes_preserve_external_environment_conflict() {
        let (root, mut db, mut r) = fixture(Client::Codex);
        let path = root.join("environments/env/config.toml");
        config_io::replace(&path, Some(b"model='managed'\n")).unwrap();
        let original = hex::encode(Sha256::digest(std::fs::read(&path).unwrap()));
        let hashes = serde_json::json!({"config.toml": original}).to_string();
        db.conn
            .execute(
                "UPDATE environments SET hashes=?1 WHERE id='env'",
                [&hashes],
            )
            .unwrap();
        config_io::replace(&path, Some(b"model='user-edit'\n")).unwrap();
        r.preview_fingerprint = Some(preview_in(&mut db, &r).await.unwrap().fingerprint);
        let i = install_in(&mut db, r).await.unwrap();
        let recorded: String = db
            .conn
            .query_row("SELECT hashes FROM environments WHERE id='env'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(recorded, hashes);
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("user-edit"));
        uninstall_in(&db, &i.id).unwrap();
        let recorded: String = db
            .conn
            .query_row("SELECT hashes FROM environments WHERE id='env'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(recorded, hashes);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn mcp_preview_rejects_external_changes_and_corrupt_configuration() {
        let (root, mut db, mut r) = fixture(Client::ClaudeCode);
        let dir = root.join("environments/env");
        std::fs::create_dir_all(&dir).unwrap();
        r.preview_fingerprint = Some(preview_in(&mut db, &r).await.unwrap().fingerprint);
        let external = br#"{"userPreference":true}"#;
        std::fs::write(dir.join(".claude.json"), external).unwrap();
        assert!(install_in(&mut db, r.clone()).await.is_err());
        assert_eq!(std::fs::read(dir.join(".claude.json")).unwrap(), external);
        std::fs::write(dir.join(".claude.json"), b"{").unwrap();
        assert!(preview_in(&mut db, &r).await.is_err());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn skill_updates_refuse_conflicts_and_uninstall_retains_user_files() {
        let (root, mut db, mut r) = fixture(Client::ClaudeCode);
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: sample\ndescription: fixture\n---\nSample body",
        )
        .unwrap();
        let mut m: ExtensionManifest = db.get("catalog", &r.extension_id).unwrap();
        m.kind = ExtensionKind::Skill;
        m.source = source.display().to_string();
        m.install_method = "local-skill".into();
        m.version = hash_files(&local_files(&source).unwrap());
        db.put("catalog", &m.id, &m).unwrap();
        r.preview_fingerprint = Some(preview_in(&mut db, &r).await.unwrap().fingerprint);
        let i = install_in(&mut db, r.clone()).await.unwrap();
        std::fs::write(Path::new(&i.path).join("my-notes.txt"), "keep my notes").unwrap();
        assert!(preview_in(&mut db, &r).await.unwrap().conflict);
        uninstall_in(&db, &i.id).unwrap();
        let retained = std::fs::read_dir(root.join("retained-extension-data"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read_to_string(retained.join("my-notes.txt")).unwrap(),
            "keep my notes"
        );
        assert!(!Path::new(&i.path).join("SKILL.md").exists());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn windows_paths_and_skill_names_are_bounded() {
        for p in ["../escape", "C:/escape", "a:stream", "CON.txt", "a/../../b"] {
            assert!(!safe_relative(p), "{p}");
        }
        assert!(!valid_name("repeated--hyphen"));
        assert!(valid_name("my-skill"));
    }
    #[test]
    fn skills_require_valid_frontmatter() {
        assert!(skill_metadata(
            "---\nname: useful-skill\ndescription: >\n  A useful skill\n---\n# Hello"
        )
        .is_ok());
        assert!(skill_metadata("# no metadata").is_err());
        assert!(skill_metadata("---\nname: ../outside\ndescription: bad\n---").is_err());
    }
    #[test]
    fn curated_catalog_has_four_types_and_unique_ids() {
        let list: Vec<ExtensionManifest> =
            serde_json::from_str(include_str!("../extension-catalog.json")).unwrap();
        let ids: std::collections::BTreeSet<_> = list.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), list.len());
        for kind in [
            ExtensionKind::Application,
            ExtensionKind::Mcp,
            ExtensionKind::Skill,
            ExtensionKind::Template,
        ] {
            assert!(list.iter().any(|m| m.kind == kind));
        }
    }
    #[test]
    fn invalid_mcp_arguments_are_rejected() {
        assert!(validate_mcp(&serde_json::json!({"command":"node","args":"--bad"})).is_err());
    }
}
