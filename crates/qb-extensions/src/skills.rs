// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! Skills 的来源（中转站重构阶段 5，cc-switch 的 M2）：从 GitHub 仓库 / 本地目录找出里面的 Skill、取文件、看有没有更新，
//! 以及 skills.sh 的搜索。装到哪、怎么同步在 `qb-app::usecase::skill_ops`。
//!
//! ⛔ **只读元数据、只下文件，绝不执行仓库里的任何脚本**（扩展中心原来那条规矩，照旧）。GitHub 走 API 拿目录树、
//! raw 拿文件，限 512 个文件 / 32 MB / 单个 8 MB；符号链接、子模块、在 Windows 上撞名的文件一律不收。
//!
//! 「有没有更新」只比 Skill 自己那个目录（目录下各文件的 git blob SHA 合成一个指纹），不比整个仓库的提交 ——
//! 仓库别处天天在动，Skill 本身没变就不该报更新。

use crate::error::{GateError, Result};
use crate::extensions::{
    blob, github_source, hash_files, http, local_files, reject_links, safe_relative,
    skill_metadata, GitSource,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use ts_rs::TS;

/// 一个 Skill 的文件：相对 Skill 目录的路径 → 内容。
pub type Files = BTreeMap<String, Vec<u8>>;

/// 来源里找到的一个 Skill。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillCandidate {
    /// 名字（`SKILL.md` 里的 `name`），也是装到各处的目录名。
    pub name: String,
    pub description: String,
    pub license: String,
    /// GitHub：`https://github.com/<仓库>/tree/<分支>/<目录>`（看更新时按这个分支）；本地：目录的绝对路径。
    pub source: String,
    /// 那一版：GitHub 是提交 SHA；本地是文件哈希。
    pub revision: String,
    /// Skill 目录内容的指纹：GitHub 是目录下各文件的 blob SHA 合成的；本地同 `revision`。
    pub fingerprint: String,
}

/// skills.sh 搜到的一个。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillsShHit {
    pub name: String,
    pub owner: String,
    pub repo: String,
    /// skills.sh 给的 Skill id（多半是目录名）。
    pub skill: String,
    #[ts(type = "number")]
    pub installs: u64,
    /// 仓库地址：拿它去 [`discover`]。
    pub url: String,
}

fn branch_of(source: &str) -> String {
    let parts: Vec<&str> = source
        .trim_start_matches("https://github.com/")
        .split('/')
        .collect();
    if parts.get(2) == Some(&"tree") {
        parts.get(3).copied().unwrap_or("HEAD").to_string()
    } else {
        "HEAD".to_string()
    }
}

fn fingerprint(g: &GitSource, dir: &str) -> String {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut h = Sha256::new();
    for e in &g.tree {
        let (Some(path), Some(sha)) = (e["path"].as_str(), e["sha"].as_str()) else {
            continue;
        };
        if e["type"] == "blob" && path.starts_with(&prefix) {
            h.update(path.as_bytes());
            h.update(b":");
            h.update(sha.as_bytes());
            h.update(b"\n");
        }
    }
    hex::encode(h.finalize())
}

pub(crate) fn tree_url(repo: &str, branch: &str, dir: &str) -> String {
    if dir.is_empty() {
        format!("https://github.com/{repo}/tree/{branch}")
    } else {
        format!("https://github.com/{repo}/tree/{branch}/{dir}")
    }
}

/// 来源里有哪些 Skill（最多 50 个）。GitHub 地址可以是仓库、分支或子目录；本地是目录。只读，不写任何东西。
pub async fn discover(source: &str) -> Result<Vec<SkillCandidate>> {
    let source = source.trim();
    let mut found = Vec::new();
    if source.starts_with("https://") {
        let branch = branch_of(source);
        let g = github_source(source, None).await?;
        for entry in &g.tree {
            let Some(path) = entry["path"].as_str() else {
                continue;
            };
            if !(path == "SKILL.md" || path.ends_with("/SKILL.md"))
                || (!g.subdir.is_empty()
                    && !path.starts_with(&format!("{}/", g.subdir))
                    && path != format!("{}/SKILL.md", g.subdir))
            {
                continue;
            }
            let bytes = blob(&g.repo, &g.revision, path).await?;
            let m = skill_metadata(
                std::str::from_utf8(&bytes).map_err(|e| GateError::Other(e.to_string()))?,
            )?;
            let dir = path.strip_suffix("/SKILL.md").unwrap_or("");
            found.push(SkillCandidate {
                name: m.name,
                description: m.description,
                license: m.license.unwrap_or_else(|| "参见上游许可证".into()),
                source: tree_url(&g.repo, &branch, dir),
                revision: g.revision.clone(),
                fingerprint: fingerprint(&g, dir),
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
            let dir = root
                .join(name)
                .parent()
                .expect("SKILL.md 有上级")
                .to_path_buf();
            let hash = hash_files(&local_files(&dir)?);
            found.push(SkillCandidate {
                name: m.name,
                description: m.description,
                license: m.license.unwrap_or_else(|| "参见本地许可证".into()),
                source: dir.display().to_string(),
                revision: hash.clone(),
                fingerprint: hash,
            });
        }
    }
    if found.is_empty() {
        return Err(GateError::Other("这个来源里没有有效的 SKILL.md".into()));
    }
    Ok(found)
}

/// GitHub 某一版、某个目录下的文件（相对那个目录）。
async fn github_files(g: &GitSource) -> Result<BTreeMap<String, Vec<u8>>> {
    let prefix = if g.subdir.is_empty() {
        String::new()
    } else {
        format!("{}/", g.subdir)
    };
    let mut out = BTreeMap::new();
    let mut total = 0;
    for entry in &g.tree {
        let Some(path) = entry["path"].as_str().filter(|p| p.starts_with(&prefix)) else {
            continue;
        };
        if entry["type"] == "tree" {
            continue;
        }
        if entry["type"] != "blob" || entry["mode"] == "120000" {
            return Err(GateError::Other("Skill 包含符号链接或子模块，没装".into()));
        }
        let bytes = blob(&g.repo, &g.revision, path).await?;
        total += bytes.len();
        if out.len() >= 512 || total > 32 * 1024 * 1024 {
            return Err(GateError::Other(
                "Skill 超过文件数或大小限制（512 个 / 32 MB）".into(),
            ));
        }
        let relative = path.strip_prefix(&prefix).expect("上面筛过");
        if !safe_relative(relative) {
            return Err(GateError::Other("仓库里有不安全的文件路径".into()));
        }
        if out
            .keys()
            .any(|k: &String| k.eq_ignore_ascii_case(relative))
        {
            return Err(GateError::Other("仓库里的文件名在 Windows 上撞名".into()));
        }
        out.insert(relative.to_string(), bytes);
    }
    Ok(out)
}

/// 取这个 Skill 那一版的全部文件（相对 Skill 目录）。本地来源要跟扫描时一样，改过了就让使用者重新扫。
pub async fn fetch(c: &SkillCandidate) -> Result<BTreeMap<String, Vec<u8>>> {
    let files = if c.source.starts_with("https://") {
        github_files(&github_source(&c.source, Some(&c.revision)).await?).await?
    } else {
        let files = local_files(Path::new(&c.source))?;
        if hash_files(&files) != c.revision {
            return Err(GateError::Other(
                "本地来源在扫描之后改过了，请重新扫描".into(),
            ));
        }
        files
    };
    let text = files
        .get("SKILL.md")
        .ok_or_else(|| GateError::Other("这个目录缺少 SKILL.md".into()))?;
    let m =
        skill_metadata(std::str::from_utf8(text).map_err(|e| GateError::Other(e.to_string()))?)?;
    if m.name != c.name {
        return Err(GateError::Other(
            "SKILL.md 的名字跟扫描时不一样，请重新扫描".into(),
        ));
    }
    Ok(files)
}

/// 看来源里这个 Skill 有没有新的一版：有就回新的那一个（指纹变了才算）。
pub async fn check(c: &SkillCandidate) -> Result<Option<SkillCandidate>> {
    let fresh = discover(&c.source).await?;
    let same_dir = |x: &SkillCandidate| x.source == c.source || x.name == c.name;
    Ok(fresh
        .into_iter()
        .find(same_dir)
        .filter(|x| x.fingerprint != c.fingerprint))
}

#[derive(Deserialize)]
struct SkillsShResponse {
    skills: Vec<SkillsShItem>,
}

#[derive(Deserialize)]
struct SkillsShItem {
    name: String,
    source: String,
    #[serde(rename = "skillId", default)]
    skill_id: String,
    #[serde(default)]
    installs: u64,
}

/// GitHub 的用户名 / 仓库名（cc-switch 的 `validate_repo_ref` 那一半）：skills.sh 的结果里不是 GitHub 坐标的丢掉。
fn github_name(s: &str, allow_dot: bool) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || (allow_dot && c == '.'))
}

/// 在 skills.sh 上搜（cc-switch 的 `search_skills_sh`）。只在使用者点「搜索」时联网；回来的只是仓库坐标，装之前还要 [`discover`]。
pub async fn search_skills_sh(query: &str) -> Result<Vec<SkillsShHit>> {
    let url = reqwest::Url::parse_with_params(
        "https://skills.sh/api/search",
        &[("q", query.trim()), ("limit", "30"), ("offset", "0")],
    )
    .map_err(|e| GateError::Other(e.to_string()))?;
    let response = http()?.get(url).send().await?;
    if !response.status().is_success() {
        return Err(GateError::Other(format!(
            "skills.sh 返回 HTTP {}",
            response.status()
        )));
    }
    let body: SkillsShResponse = response.json().await?;
    Ok(body
        .skills
        .into_iter()
        .filter_map(|s| {
            let (owner, repo) = s.source.split_once('/')?;
            if !github_name(owner, false) || !github_name(repo, true) {
                return None;
            }
            Some(SkillsShHit {
                url: format!("https://github.com/{owner}/{repo}"),
                name: s.name,
                owner: owner.into(),
                repo: repo.into(),
                skill: s.skill_id,
                installs: s.installs,
            })
        })
        .collect())
}

/// 一个 ZIP 里的 Skill（cc-switch 的 `install_from_zip`）：回候选和各自的文件。逐个读条目、不用 `extract`：
/// 路径走 `enclosed_name`（不许 `..`、绝对路径），符号链接、超限（512 个 / 32 MB / 单个 8 MB）一律拒绝。
pub fn discover_zip(path: &Path) -> Result<Vec<(SkillCandidate, Files)>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| GateError::Other(format!("不是有效的 ZIP：{e}")))?;
    let mut all: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| GateError::Other(format!("读不出 ZIP 里的第 {i} 项：{e}")))?;
        if entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(GateError::Other("ZIP 里有符号链接，没装".into()));
        }
        let name = entry
            .enclosed_name()
            .ok_or_else(|| GateError::Other("ZIP 里有不安全的路径，没装".into()))?
            .to_string_lossy()
            .replace('\\', "/");
        if name.split('/').any(|p| p == "__MACOSX" || p == ".git") {
            continue;
        }
        if !safe_relative(&name) {
            return Err(GateError::Other("ZIP 里有不安全的文件名，没装".into()));
        }
        if entry.size() > 8 * 1024 * 1024 {
            return Err(GateError::Other("ZIP 里有超过 8 MB 的文件".into()));
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        total += bytes.len();
        if bytes.len() > 8 * 1024 * 1024 || total > 32 * 1024 * 1024 || all.len() >= 512 {
            return Err(GateError::Other(
                "ZIP 超过文件数或大小限制（512 个 / 32 MB）".into(),
            ));
        }
        all.insert(name, bytes);
    }
    let label = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = Vec::new();
    for skill_md in all
        .keys()
        .filter(|k| k.as_str() == "SKILL.md" || k.ends_with("/SKILL.md"))
    {
        let dir = skill_md.strip_suffix("SKILL.md").unwrap_or("");
        let files: BTreeMap<String, Vec<u8>> = all
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(dir)?.to_string(), v.clone())))
            .collect();
        let (name, description, license) = metadata(&files)?;
        let hash = hash_files(&files);
        out.push((
            SkillCandidate {
                name,
                description,
                license: license.unwrap_or_else(|| "参见压缩包里的许可证".into()),
                source: format!("zip:{label}"),
                revision: hash.clone(),
                fingerprint: hash,
            },
            files,
        ));
    }
    if out.is_empty() {
        return Err(GateError::Other("这个 ZIP 里没有 SKILL.md".into()));
    }
    Ok(out)
}

/// Skill 的名字能不能当目录名（跟扩展中心原来的规矩一样：小写字母、数字、连字符）。
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.contains("--")
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// 一个目录里的 Skill 文件（不跟随链接、有大小上限）和它的哈希。
pub fn read_dir(dir: &Path) -> Result<(BTreeMap<String, Vec<u8>>, String)> {
    let files = local_files(dir)?;
    let hash = hash_files(&files);
    Ok((files, hash))
}

/// 文件集合的哈希（跟 [`read_dir`] 同一种算法）。
pub fn hash(files: &BTreeMap<String, Vec<u8>>) -> String {
    hash_files(files)
}

/// `SKILL.md` 里的名字和描述。
pub fn metadata(files: &BTreeMap<String, Vec<u8>>) -> Result<(String, String, Option<String>)> {
    let text = files
        .get("SKILL.md")
        .ok_or_else(|| GateError::Other("这个目录缺少 SKILL.md".into()))?;
    let m =
        skill_metadata(std::str::from_utf8(text).map_err(|e| GateError::Other(e.to_string()))?)?;
    Ok((m.name, m.description, m.license))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_sources_are_discovered_and_fetched_only_while_unchanged() {
        let root = std::env::temp_dir().join(format!("qb-skill-{}", uuid::Uuid::new_v4()));
        let dir = root.join("pack").join("hello-skill");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: hello-skill\ndescription: says hi\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let found = rt
            .block_on(discover(&root.join("pack").display().to_string()))
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "hello-skill");
        let files = rt.block_on(fetch(&found[0])).unwrap();
        assert!(files.contains_key("SKILL.md") && files.contains_key("notes.txt"));
        assert_eq!(
            rt.block_on(check(&found[0])).unwrap(),
            None,
            "没改就没有更新"
        );
        std::fs::write(dir.join("notes.txt"), "changed").unwrap();
        assert!(
            rt.block_on(fetch(&found[0])).is_err(),
            "扫描之后改过了要重新扫"
        );
        assert!(
            rt.block_on(check(&found[0])).unwrap().is_some(),
            "改过了就有新的一版"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zip_skills_are_read_entry_by_entry() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("qb-zip-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("pack.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("pack/zip-skill/SKILL.md", opts).unwrap();
        w.write_all(b"---\nname: zip-skill\ndescription: from a zip\n---\n")
            .unwrap();
        w.start_file("pack/zip-skill/a.txt", opts).unwrap();
        w.write_all(b"a").unwrap();
        w.start_file("__MACOSX/junk", opts).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        let found = discover_zip(&path).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "zip-skill");
        assert!(found[0].1.contains_key("SKILL.md") && found[0].1.contains_key("a.txt"));
        assert!(!found[0].1.keys().any(|k| k.contains("MACOSX")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_and_branches() {
        assert!(valid_name("skill-creator"));
        assert!(!valid_name("Skill") && !valid_name("a--b") && !valid_name("../x"));
        assert_eq!(branch_of("https://github.com/o/r/tree/dev/skills/x"), "dev");
        assert_eq!(branch_of("https://github.com/o/r"), "HEAD");
        assert!(github_name("anthropics", false) && !github_name("skills.volces.com", false));
    }
}
