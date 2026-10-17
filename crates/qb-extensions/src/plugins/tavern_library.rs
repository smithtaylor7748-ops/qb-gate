//! 酒馆的角色卡、世界书、扩展与回收站（2026-09-29，使用者选的「角色卡 / 世界书 / 扩展管理」）。
//!
//! 对照过 al01cn/sillyTavern-launcher（MIT）与 SillyTavern 1.19.0 自己的扩展接口（只核事实）之后定的形状，
//! 代码是自己写的。面板管「有哪些、看一眼、导进来、挪出去、扩展的更新」；**卡与世界书的内容编辑仍在酒馆那边**
//! （`tavern_assets` 那条「不重写酒馆的编辑界面」没变）。
//!
//! # ⛔ 边界
//!
//! 1. **删除 = 挪进面板的回收站**（`state_dir\tavern-trash\<时间>-<类>-<序号>`，[`Trash`]），能原样放回去；
//!    只有「清空回收站」才真删。al01cn 那边是直接删文件，这里不照搬。
//! 2. **只认酒馆自己的格式**：角色卡 = 带 `ccv3` / `chara` 文本块的 PNG；世界书 = 顶层有 `entries` 的 JSON。
//!    JSON 角色卡、`.charx`、NovelAI / Agnai 的世界书请在酒馆里导入 —— 它会转换，面板不替它转。
//! 3. **导入不覆盖**：同名就在名字后面加 ` (2)`、` (3)`。源文件一个字节不动。
//! 4. 读的是 `data\default-user`（酒馆开了多用户时，别的用户不在这里；跟 `tavern_assets` 同一处）。
//! 5. **扩展的 git 操作只在点了之后跑**、只在那个扩展的目录里跑、不经 shell；`GIT_TERMINAL_PROMPT=0` ——
//!    要密码的仓库直接失败，不弹窗、不卡住。远端地址里带的账号密码一律抹掉再显示。
//! 6. **不改酒馆的 `settings.json`**：扩展「停用」只读出来显示（在酒馆里切），面板不写。

use crate::error::{GateError, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 一张卡的 PNG 最多多大（再大多半不是角色卡）。
const CARD_MAX: u64 = 64 << 20;
/// 卡里那段文本块最多多大。
const TEXT_MAX: u32 = 32 << 20;
/// 详情里带的整张图最多多大（base64 之后放进 data URL）。
const IMAGE_MAX: u64 = 8 << 20;
/// 缩略图最多多大。
const THUMB_MAX: u64 = 1 << 20;
/// 世界书 JSON 最多多大。
const WORLD_MAX: u64 = 64 << 20;
/// 一次最多导入多少个文件。
const IMPORT_MAX: usize = 200;

// ================================================================== 位置

/// 酒馆里「使用者数据」的那一层：`<酒馆目录>\data\default-user`。
pub fn user_dir_of(root: &Path) -> PathBuf {
    root.join("data").join("default-user")
}

fn configured_root() -> Result<PathBuf> {
    let root = super::sillytavern::load_config().sillytavern_root;
    if root.as_os_str().is_empty() {
        return Err(GateError::Other(
            "还没配酒馆目录：到软件页 SillyTavern 卡装一份，或在「位置」里指到你自己的那份。"
                .into(),
        ));
    }
    let user = user_dir_of(&root);
    if !user.is_dir() {
        return Err(GateError::Other(format!(
            "{} 不存在 —— 这份酒馆还没起过一次（它第一次启动时才建数据目录），先起一次再来。",
            user.display()
        )));
    }
    Ok(root)
}

fn stamp(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

fn modified(meta: &std::fs::Metadata) -> Option<String> {
    meta.modified().ok().map(stamp)
}

/// 一个文件名能不能原样拿来当 `<目录>\<名字>`：不许带路径、不许是 `.` / `..`、不许有 Windows 不认的字符。
pub fn safe_file_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n == name
        && n.len() <= 255
        && n != "."
        && n != ".."
        && !n.ends_with('.')
        && !n.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
}

/// 纯函数：`dir` 里 `name` 被占了就在扩展名前加 ` (2)`、` (3)`……
pub fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..10_000)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !dir.join(candidate).exists())
        .unwrap_or_else(|| format!("{stem} ({}){ext}", crate::config_io::id()))
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// 按前几个字节认图片类型（酒馆的缩略图文件名是 `.png`，里面装的常常是 JPEG）。
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

fn data_url(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mime = sniff_image(&bytes)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

// ================================================================== 角色卡：PNG 里的那段 JSON

/// 纯函数：从 PNG 里取角色卡那段文本。`ccv3`（V3）优先，其次 `chara`（V1 / V2）。
///
/// 只走块头、按长度跳过（`seek`），不读图像数据 —— 酒馆把卡塞在 `IEND` 前面，也就是文件末尾。
/// 不是 PNG、没有这两个块 → `Ok(None)`。
pub fn card_text<R: Read + Seek>(r: &mut R) -> Result<Option<String>> {
    let mut sig = [0u8; 8];
    if r.read_exact(&mut sig).is_err() || &sig != b"\x89PNG\r\n\x1a\n" {
        return Ok(None);
    }
    let (mut v3, mut v2) = (None, None);
    for _ in 0..100_000 {
        let mut head = [0u8; 8];
        if r.read_exact(&mut head).is_err() {
            break;
        }
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
        let kind = &head[4..8];
        if kind == b"IEND" {
            break;
        }
        if kind == b"tEXt" && len <= TEXT_MAX {
            let mut data = vec![0u8; len as usize];
            r.read_exact(&mut data)?;
            r.seek(SeekFrom::Current(4))?;
            if let Some(nul) = data.iter().position(|&b| b == 0) {
                let keyword = &data[..nul];
                // tEXt 是 Latin-1；卡里放的是 base64，纯 ASCII。
                let text: String = data[nul + 1..].iter().map(|&b| b as char).collect();
                if keyword.eq_ignore_ascii_case(b"ccv3") {
                    v3 = Some(text);
                } else if keyword.eq_ignore_ascii_case(b"chara") {
                    v2 = Some(text);
                }
            }
        } else {
            r.seek(SeekFrom::Current(len as i64 + 4))?;
        }
    }
    let Some(b64) = v3.or(v2) else {
        return Ok(None);
    };
    let clean: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&clean)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&clean))
        .map_err(|e| GateError::Other(format!("卡里那段 base64 解不开：{e}")))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

fn strings(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .filter(|x| !x.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// 世界书 / 卡内世界书里的一条。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BookEntry {
    pub uid: String,
    /// 标题（酒馆里叫 comment / memo）。
    pub comment: String,
    pub keys: Vec<String>,
    pub secondary: Vec<String>,
    pub content: String,
    /// 常驻（蓝灯）。
    pub constant: bool,
    pub disabled: bool,
    /// 插入顺序。`#[ts(type)]`：ts-rs 默认把 i64 映射成 bigint，而 JSON 上它就是个小整数。
    #[ts(type = "number | null")]
    pub order: Option<i64>,
}

/// 纯函数：一条条目。酒馆自己的世界书（`key` / `keysecondary` / `disable` / `order`）与
/// 卡内 V2 世界书（`keys` / `secondary_keys` / `enabled` / `insertion_order`）两种写法都认。
fn book_entry(uid: String, e: &serde_json::Value) -> BookEntry {
    let keys = if e.get("keys").is_some() {
        strings(e, "keys")
    } else {
        strings(e, "key")
    };
    let secondary = if e.get("secondary_keys").is_some() {
        strings(e, "secondary_keys")
    } else {
        strings(e, "keysecondary")
    };
    let disabled = e
        .get("disable")
        .and_then(|x| x.as_bool())
        .or_else(|| e.get("enabled").and_then(|x| x.as_bool()).map(|on| !on))
        .unwrap_or(false);
    let order = e
        .get("order")
        .or_else(|| e.get("insertion_order"))
        .and_then(|x| x.as_i64());
    let uid = e
        .get("uid")
        .or_else(|| e.get("id"))
        .map(|x| match x {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or(uid);
    BookEntry {
        uid,
        comment: s(e, "comment"),
        keys,
        secondary,
        content: s(e, "content"),
        constant: e.get("constant").and_then(|x| x.as_bool()).unwrap_or(false),
        disabled,
        order,
    }
}

/// 纯函数：`entries` 是对象（酒馆自己的，键是 uid）或数组（V2 卡内）都认。认不出是 `None`。
pub fn book_entries(book: &serde_json::Value) -> Option<Vec<BookEntry>> {
    match book.get("entries")? {
        serde_json::Value::Object(map) => {
            Some(map.iter().map(|(k, e)| book_entry(k.clone(), e)).collect())
        }
        serde_json::Value::Array(list) => Some(
            list.iter()
                .enumerate()
                .map(|(i, e)| book_entry(i.to_string(), e))
                .collect(),
        ),
        _ => None,
    }
}

/// 卡里读出来的字段（列表与详情共用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardFields {
    pub spec: String,
    pub name: String,
    pub creator: String,
    pub version: String,
    pub tags: Vec<String>,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_mes: String,
    pub mes_example: String,
    pub creator_notes: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub alternate_greetings: Vec<String>,
    pub book_name: String,
    pub book: Option<Vec<BookEntry>>,
}

/// 纯函数：卡的 JSON → 字段。V3（`chara_card_v3`）、V2（`chara_card_v2`）读 `data`；
/// V1 读顶层（酒馆写 V1 时顶层与 `data` 两份都有，`data` 优先）。
pub fn card_fields(json: &str) -> Result<CardFields> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| GateError::Other(format!("卡里那段 JSON 读不懂：{e}")))?;
    if !v.is_object() {
        return Err(GateError::Other("卡里那段不是一个 JSON 对象".into()));
    }
    let spec = match v.get("spec").and_then(|x| x.as_str()) {
        Some("chara_card_v3") => "v3",
        Some("chara_card_v2") => "v2",
        _ => "v1",
    };
    let data = match v.get("data") {
        Some(d) if d.is_object() => d,
        _ => &v,
    };
    let pick = |key: &str| {
        let x = s(data, key);
        if x.is_empty() {
            s(&v, key)
        } else {
            x
        }
    };
    let book = data.get("character_book").filter(|b| b.is_object());
    Ok(CardFields {
        spec: spec.into(),
        name: pick("name"),
        creator: pick("creator"),
        version: pick("character_version"),
        tags: strings(data, "tags"),
        description: pick("description"),
        personality: pick("personality"),
        scenario: pick("scenario"),
        first_mes: pick("first_mes"),
        mes_example: pick("mes_example"),
        creator_notes: pick("creator_notes"),
        system_prompt: pick("system_prompt"),
        post_history_instructions: pick("post_history_instructions"),
        alternate_greetings: strings(data, "alternate_greetings"),
        book_name: book.map(|b| s(b, "name")).unwrap_or_default(),
        book: book.and_then(book_entries),
    })
}

fn read_card(path: &Path) -> Result<CardFields> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > CARD_MAX {
        return Err(GateError::Other("文件太大，不像一张角色卡".into()));
    }
    let mut f = std::io::BufReader::new(std::fs::File::open(path)?);
    let text = card_text(&mut f)?.ok_or_else(|| {
        GateError::Other("这张 PNG 里没有角色卡数据（没有 chara / ccv3 文本块）".into())
    })?;
    card_fields(&text)
}

// ================================================================== 角色卡：列表、详情、导入、删除

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CharacterCard {
    /// 文件名（`Seraphina.png`），也是酒馆认这张卡的 id。
    pub file: String,
    /// 卡里写的名字；读不出来时是文件名去掉扩展名。
    pub name: String,
    pub creator: String,
    pub version: String,
    /// `v1` / `v2` / `v3`；读不出来是空串。
    pub spec: String,
    pub tags: Vec<String>,
    #[ts(type = "number")]
    pub size: u64,
    pub modified: Option<String>,
    /// `chats\<去掉扩展名的文件名>\` 里有几段聊天。
    pub chats: u32,
    /// 卡里自带的世界书有几条（没有是 `null`）。
    pub book_entries: Option<u32>,
    /// 卡数据读不出来时的原因（文件照样列出来）。
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CharacterDetail {
    pub card: CharacterCard,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_mes: String,
    pub mes_example: String,
    pub creator_notes: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub alternate_greetings: Vec<String>,
    pub book_name: String,
    pub book: Vec<BookEntry>,
    /// 整张图（data URL）；超过 8 MB 不带。
    pub image: Option<String>,
}

fn stem_of(file: &str) -> &str {
    file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file)
}

fn chat_count(user: &Path, file: &str) -> u32 {
    std::fs::read_dir(user.join("chats").join(stem_of(file)))
        .map(|rd| rd.flatten().filter(|e| has_ext(&e.path(), "jsonl")).count() as u32)
        .unwrap_or(0)
}

fn card_summary(user: &Path, path: &Path) -> Option<CharacterCard> {
    let meta = std::fs::metadata(path).ok()?;
    let file = path.file_name()?.to_string_lossy().to_string();
    let base = CharacterCard {
        name: stem_of(&file).to_string(),
        creator: String::new(),
        version: String::new(),
        spec: String::new(),
        tags: Vec::new(),
        size: meta.len(),
        modified: modified(&meta),
        chats: chat_count(user, &file),
        book_entries: None,
        problem: None,
        file,
    };
    Some(match read_card(path) {
        Ok(c) => CharacterCard {
            name: if c.name.trim().is_empty() {
                base.name.clone()
            } else {
                c.name
            },
            creator: c.creator,
            version: c.version,
            spec: c.spec,
            tags: c.tags,
            book_entries: c.book.map(|b| b.len() as u32),
            ..base
        },
        Err(e) => CharacterCard {
            problem: Some(e.to_string()),
            ..base
        },
    })
}

/// `characters\` 里全部 PNG，按名字排。
pub fn list_characters_in(root: &Path) -> Vec<CharacterCard> {
    let user = user_dir_of(root);
    let Ok(rd) = std::fs::read_dir(user.join("characters")) else {
        return Vec::new();
    };
    let mut out: Vec<CharacterCard> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| has_ext(p, "png"))
        .filter_map(|p| card_summary(&user, &p))
        .collect();
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

pub fn list_characters() -> Result<Vec<CharacterCard>> {
    Ok(list_characters_in(&configured_root()?))
}

fn character_path(root: &Path, file: &str) -> Result<PathBuf> {
    if !safe_file_name(file) || !has_ext(Path::new(file), "png") {
        return Err(GateError::Other(format!("「{file}」不是一个角色卡文件名")));
    }
    let p = user_dir_of(root).join("characters").join(file);
    if !p.is_file() {
        return Err(GateError::NotFound(p.display().to_string()));
    }
    Ok(p)
}

pub fn character_detail(file: &str) -> Result<CharacterDetail> {
    let root = configured_root()?;
    let path = character_path(&root, file)?;
    let user = user_dir_of(&root);
    let card = card_summary(&user, &path)
        .ok_or_else(|| GateError::NotFound(path.display().to_string()))?;
    let fields = read_card(&path).unwrap_or_default();
    Ok(CharacterDetail {
        card,
        description: fields.description,
        personality: fields.personality,
        scenario: fields.scenario,
        first_mes: fields.first_mes,
        mes_example: fields.mes_example,
        creator_notes: fields.creator_notes,
        system_prompt: fields.system_prompt,
        post_history_instructions: fields.post_history_instructions,
        alternate_greetings: fields.alternate_greetings,
        book_name: fields.book_name,
        book: fields.book.unwrap_or_default(),
        image: data_url(&path, IMAGE_MAX),
    })
}

/// 酒馆自己生成的缩略图（`thumbnails\avatar\<文件名>`，没点开过的角色可能还没有）。没有就是 `None`。
pub fn character_thumbnail(file: &str) -> Result<Option<String>> {
    let root = configured_root()?;
    if !safe_file_name(file) {
        return Err(GateError::Other(format!("「{file}」不是一个角色卡文件名")));
    }
    let user = user_dir_of(&root);
    Ok(data_url(
        &user.join("thumbnails").join("avatar").join(file),
        THUMB_MAX,
    ))
}

/// 导入的结果：进来了哪些、没进来的为什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ImportReport {
    /// 进来的（落盘用的文件名）。
    pub imported: Vec<String>,
    /// 没进来的：「文件 —— 原因」。
    pub skipped: Vec<String>,
}

fn import_into(
    dir: &Path,
    sources: &[PathBuf],
    check: impl Fn(&Path) -> Result<()>,
) -> Result<ImportReport> {
    if sources.len() > IMPORT_MAX {
        return Err(GateError::Other(format!(
            "一次最多导入 {IMPORT_MAX} 个文件"
        )));
    }
    std::fs::create_dir_all(dir)?;
    let mut report = ImportReport::default();
    for src in sources {
        let shown = src.display().to_string();
        let name = match src.file_name().map(|n| n.to_string_lossy().to_string()) {
            Some(n) if safe_file_name(&n) => n,
            _ => {
                report.skipped.push(format!("{shown} —— 文件名认不出"));
                continue;
            }
        };
        if !src.is_file() {
            report.skipped.push(format!("{shown} —— 不是文件"));
            continue;
        }
        if let Err(e) = check(src) {
            report.skipped.push(format!("{name} —— {e}"));
            continue;
        }
        let target = unique_name(dir, &name);
        match std::fs::copy(src, dir.join(&target)) {
            Ok(_) => report.imported.push(target),
            Err(e) => report.skipped.push(format!("{name} —— 复制失败：{e}")),
        }
    }
    Ok(report)
}

/// 纯校验：这是不是一张酒馆读得懂的 PNG 角色卡。
pub fn check_card_file(p: &Path) -> Result<()> {
    if !has_ext(p, "png") {
        return Err(GateError::Other(
            "只收 PNG 角色卡；JSON 卡、.charx 请在酒馆里导入（它会配一张图）".into(),
        ));
    }
    let fields = read_card(p)?;
    if fields.name.trim().is_empty() {
        return Err(GateError::Other("卡里没有名字".into()));
    }
    Ok(())
}

pub fn import_characters_in(root: &Path, sources: &[PathBuf]) -> Result<ImportReport> {
    import_into(
        &user_dir_of(root).join("characters"),
        sources,
        check_card_file,
    )
}

pub fn import_characters(sources: &[PathBuf]) -> Result<ImportReport> {
    let root = configured_root()?;
    let r = import_characters_in(&root, sources)?;
    if !r.imported.is_empty() {
        crate::audit::write(&format!("酒馆：导入了 {} 张角色卡", r.imported.len()));
    }
    Ok(r)
}

/// 把一张卡（可选连同它的聊天记录）挪进回收站。
pub fn delete_character(file: &str, with_chats: bool) -> Result<TrashEntry> {
    let root = configured_root()?;
    let path = character_path(&root, file)?;
    let mut items = vec![path];
    let chats = user_dir_of(&root).join("chats").join(stem_of(file));
    if with_chats && chats.is_dir() {
        items.push(chats);
    }
    let name = card_summary(&user_dir_of(&root), &items[0])
        .map(|c| c.name)
        .unwrap_or_else(|| file.to_string());
    let entry = Trash::default_root().put("character", &name, &items)?;
    crate::audit::write(&format!("酒馆：角色卡挪进了回收站（{}）", entry.id));
    Ok(entry)
}

// ================================================================== 世界书

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct WorldInfo {
    pub file: String,
    /// 酒馆里显示的名字 = 文件名去掉 `.json`。
    pub name: String,
    pub entries: u32,
    /// 没被停用的条目数。
    pub enabled: u32,
    #[ts(type = "number")]
    pub size: u64,
    pub modified: Option<String>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct WorldDetail {
    pub info: WorldInfo,
    pub entries: Vec<BookEntry>,
}

fn read_world(path: &Path) -> Result<Vec<BookEntry>> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > WORLD_MAX {
        return Err(GateError::Other("文件太大，不像一本世界书".into()));
    }
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|e| GateError::Other(format!("JSON 读不懂：{e}")))?;
    let mut entries = book_entries(&v).ok_or_else(|| {
        GateError::Other(
            "不是酒馆自己的世界书格式（顶层没有 entries）；别家的世界书请在酒馆里导入，它会转换"
                .into(),
        )
    })?;
    entries.sort_by_key(|e| {
        (
            e.order.unwrap_or(i64::MAX),
            e.uid.parse::<i64>().unwrap_or(i64::MAX),
        )
    });
    Ok(entries)
}

fn world_info(path: &Path) -> Option<WorldInfo> {
    let meta = std::fs::metadata(path).ok()?;
    let file = path.file_name()?.to_string_lossy().to_string();
    let base = WorldInfo {
        name: stem_of(&file).to_string(),
        entries: 0,
        enabled: 0,
        size: meta.len(),
        modified: modified(&meta),
        problem: None,
        file,
    };
    Some(match read_world(path) {
        Ok(e) => WorldInfo {
            entries: e.len() as u32,
            enabled: e.iter().filter(|x| !x.disabled).count() as u32,
            ..base
        },
        Err(e) => WorldInfo {
            problem: Some(e.to_string()),
            ..base
        },
    })
}

pub fn list_worlds_in(root: &Path) -> Vec<WorldInfo> {
    let Ok(rd) = std::fs::read_dir(user_dir_of(root).join("worlds")) else {
        return Vec::new();
    };
    let mut out: Vec<WorldInfo> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| has_ext(p, "json"))
        .filter_map(|p| world_info(&p))
        .collect();
    out.sort_by_key(|w| w.name.to_lowercase());
    out
}

pub fn list_worlds() -> Result<Vec<WorldInfo>> {
    Ok(list_worlds_in(&configured_root()?))
}

fn world_path(root: &Path, file: &str) -> Result<PathBuf> {
    if !safe_file_name(file) || !has_ext(Path::new(file), "json") {
        return Err(GateError::Other(format!("「{file}」不是一个世界书文件名")));
    }
    let p = user_dir_of(root).join("worlds").join(file);
    if !p.is_file() {
        return Err(GateError::NotFound(p.display().to_string()));
    }
    Ok(p)
}

pub fn world_detail(file: &str) -> Result<WorldDetail> {
    let root = configured_root()?;
    let path = world_path(&root, file)?;
    let info = world_info(&path).ok_or_else(|| GateError::NotFound(path.display().to_string()))?;
    let entries = read_world(&path)?;
    Ok(WorldDetail { info, entries })
}

/// 纯校验：这是不是一本酒馆自己格式的世界书。
pub fn check_world_file(p: &Path) -> Result<()> {
    if !has_ext(p, "json") {
        return Err(GateError::Other("只收 .json 世界书".into()));
    }
    read_world(p).map(|_| ())
}

pub fn import_worlds_in(root: &Path, sources: &[PathBuf]) -> Result<ImportReport> {
    import_into(&user_dir_of(root).join("worlds"), sources, check_world_file)
}

pub fn import_worlds(sources: &[PathBuf]) -> Result<ImportReport> {
    let root = configured_root()?;
    let r = import_worlds_in(&root, sources)?;
    if !r.imported.is_empty() {
        crate::audit::write(&format!("酒馆：导入了 {} 本世界书", r.imported.len()));
    }
    Ok(r)
}

pub fn delete_world(file: &str) -> Result<TrashEntry> {
    let root = configured_root()?;
    let path = world_path(&root, file)?;
    let entry = Trash::default_root().put("world", stem_of(file), &[path])?;
    crate::audit::write(&format!("酒馆：世界书挪进了回收站（{}）", entry.id));
    Ok(entry)
}

// ================================================================== 扩展

/// 扩展装在哪一层：`user` = `data\default-user\extensions\`（酒馆默认装到这里），
/// `global` = `public\scripts\extensions\third-party\`（给全部用户）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "TavernExtensionScope")]
#[serde(rename_all = "lowercase")]
pub enum ExtensionScope {
    User,
    Global,
}

impl ExtensionScope {
    fn dir(self, root: &Path) -> PathBuf {
        match self {
            Self::User => user_dir_of(root).join("extensions"),
            Self::Global => root
                .join("public")
                .join("scripts")
                .join("extensions")
                .join("third-party"),
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::User => "",
            Self::Global => "（全局）",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TavernExtension {
    /// 目录名。
    pub name: String,
    pub scope: ExtensionScope,
    pub path: String,
    pub display_name: String,
    pub version: String,
    pub author: String,
    /// manifest 里的 `homePage`（只收 https）。
    pub home_page: String,
    /// 没有 manifest.json 的目录酒馆不会加载（多半是删到一半留下的）。
    pub has_manifest: bool,
    /// 是 git 克隆来的（能检查更新、能更新）。
    pub git: bool,
    /// 远端地址（账号密码已抹掉）。
    pub remote: String,
    pub branch: String,
    /// 当前提交的前 7 位。
    pub commit: String,
    /// 酒馆 `settings.json` 里把它停用了（只读显示，在酒馆里切）。
    pub disabled: bool,
    pub modified: Option<String>,
}

/// 纯函数：远端地址里的 `user:token@` 抹掉（`.git/config` 里可能真存着令牌）。
pub fn redact_url(url: &str) -> String {
    let url = url.trim();
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let (auth, tail) = match rest.find('/') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, ""),
            };
            match auth.rfind('@') {
                Some(at) => format!("{scheme}://{}{tail}", &auth[at + 1..]),
                None => url.to_string(),
            }
        }
        None => url.to_string(),
    }
}

/// 纯函数：`.git/config` 里 `[remote "origin"]` 的 `url`。
pub fn git_remote_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_origin = t.replace(' ', "") == "[remote\"origin\"]";
            continue;
        }
        if in_origin {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case("url") {
                    return Some(redact_url(v.trim()));
                }
            }
        }
    }
    None
}

/// 纯函数：`.git/HEAD` → (分支, 游离时的提交)。
pub fn git_head(head: &str) -> (Option<String>, Option<String>) {
    let t = head.trim();
    match t.strip_prefix("ref:") {
        Some(r) => (
            r.trim().strip_prefix("refs/heads/").map(str::to_string),
            None,
        ),
        None if t.len() >= 7 && t.chars().all(|c| c.is_ascii_hexdigit()) => {
            (None, Some(t.to_string()))
        }
        None => (None, None),
    }
}

/// 纯函数：`packed-refs` 里某个分支的提交。
pub fn packed_ref(packed: &str, branch: &str) -> Option<String> {
    let want = format!("refs/heads/{branch}");
    packed.lines().find_map(|l| {
        let (hash, name) = l.trim().split_once(' ')?;
        (name == want && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_string())
    })
}

fn git_facts(dir: &Path) -> (bool, String, String, String) {
    let git = dir.join(".git");
    if git.is_file() {
        // 子模块 / 工作树：`gitdir: …` —— 是 git，细节不追过去。
        return (true, String::new(), String::new(), String::new());
    }
    if !git.is_dir() {
        return (false, String::new(), String::new(), String::new());
    }
    let remote = std::fs::read_to_string(git.join("config"))
        .ok()
        .and_then(|c| git_remote_url(&c))
        .unwrap_or_default();
    let (branch, detached) = std::fs::read_to_string(git.join("HEAD"))
        .map(|h| git_head(&h))
        .unwrap_or((None, None));
    let commit = detached.or_else(|| {
        let b = branch.as_deref()?;
        std::fs::read_to_string(git.join("refs").join("heads").join(b))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::fs::read_to_string(git.join("packed-refs"))
                    .ok()
                    .and_then(|p| packed_ref(&p, b))
            })
    });
    (
        true,
        remote,
        branch.unwrap_or_default(),
        commit
            .map(|c| c.chars().take(7).collect())
            .unwrap_or_default(),
    )
}

/// 纯函数：酒馆 `settings.json` 里停用的扩展（`extension_settings.disabledExtensions`，写法是 `third-party/<目录名>`）。
pub fn disabled_extensions(settings: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(settings)
        .ok()
        .and_then(|v| {
            v.get("extension_settings")?
                .get("disabledExtensions")?
                .as_array()
                .cloned()
        })
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .filter_map(|x| x.strip_prefix("third-party/"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn https_only(url: String) -> String {
    if url.starts_with("https://") {
        url
    } else {
        String::new()
    }
}

pub fn list_extensions_in(root: &Path) -> Vec<TavernExtension> {
    let disabled = std::fs::read_to_string(user_dir_of(root).join("settings.json"))
        .map(|s| disabled_extensions(&s))
        .unwrap_or_default();
    let mut out = Vec::new();
    for scope in [ExtensionScope::User, ExtensionScope::Global] {
        let Ok(rd) = std::fs::read_dir(scope.dir(root)) else {
            continue;
        };
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let dir = e.path();
            let manifest: Option<serde_json::Value> = std::fs::read(dir.join("manifest.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            let m = manifest.clone().unwrap_or_default();
            let (git, remote, branch, commit) = git_facts(&dir);
            let display = s(&m, "display_name");
            out.push(TavernExtension {
                display_name: if display.trim().is_empty() {
                    name.clone()
                } else {
                    display
                },
                version: s(&m, "version"),
                author: s(&m, "author"),
                home_page: https_only(s(&m, "homePage")),
                has_manifest: manifest.is_some(),
                git,
                remote,
                branch,
                commit,
                disabled: disabled.contains(&name),
                modified: std::fs::metadata(&dir).ok().as_ref().and_then(modified),
                path: dir.display().to_string(),
                scope,
                name,
            });
        }
    }
    out.sort_by(|a, b| {
        (
            a.scope == ExtensionScope::Global,
            a.display_name.to_lowercase(),
        )
            .cmp(&(
                b.scope == ExtensionScope::Global,
                b.display_name.to_lowercase(),
            ))
    });
    out
}

/// 扩展清单 + 本机的 git 在哪（没有就是 `null`：检查更新 / 更新 / 从地址安装都用不了，界面照实说）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TavernExtensionList {
    pub git: Option<String>,
    pub items: Vec<TavernExtension>,
}

pub fn list_extensions() -> Result<TavernExtensionList> {
    let root = configured_root()?;
    Ok(TavernExtensionList {
        git: find_git().map(|p| p.display().to_string()),
        items: list_extensions_in(&root),
    })
}

/// git 操作一次只跑一个（同一个扩展两次点击不会叠着跑两个 git）。
///
/// ⛔ **不拿 `operations::exclusive()`**：克隆可能要几分钟，而看门狗每 5 秒要拿那把锁巡检 ——
/// 拿着它跑 git，巡检就停了几分钟。
fn git_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(Default::default)
}

fn extension_dir(root: &Path, name: &str, scope: ExtensionScope) -> Result<PathBuf> {
    if !safe_file_name(name) || name.starts_with('.') {
        return Err(GateError::Other(format!("「{name}」不是一个扩展目录名")));
    }
    let dir = scope.dir(root).join(name);
    if !dir.is_dir() {
        return Err(GateError::NotFound(dir.display().to_string()));
    }
    Ok(dir)
}

/// 本机的 git：先 `where git.exe`，再看 Git for Windows 的几个默认位置。
pub fn find_git() -> Option<PathBuf> {
    if let Ok(out) = crate::process::hidden_std(std::process::Command::new("where"))
        .arg("git.exe")
        .output()
    {
        if out.status.success() {
            if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
                let p = PathBuf::from(first.trim());
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let candidates = [
        std::env::var("ProgramFiles")
            .ok()
            .map(|p| PathBuf::from(p).join("Git")),
        std::env::var("ProgramW6432")
            .ok()
            .map(|p| PathBuf::from(p).join("Git")),
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("Programs").join("Git")),
    ];
    candidates
        .into_iter()
        .flatten()
        .map(|g| g.join("cmd").join("git.exe"))
        .find(|p| p.is_file())
}

const NO_GIT: &str = "本机没找到 git。检查与更新扩展要用 git（装 Git for Windows：winget install Git.Git，或到 git-scm.com）；\
                      或者在酒馆自己的扩展管理里装 —— 1.19 起它没有 git 也能装。";

/// 跑一次 git，拿回（成功与否，stdout，stderr）。**不经 shell**，隐藏窗口，要密码的仓库直接失败。
async fn run_git(
    git: &Path,
    cwd: &Path,
    args: &[&str],
    secs: u64,
) -> Result<(bool, String, String)> {
    let mut cmd = crate::process::hidden_tokio(tokio::process::Command::new(git));
    cmd.current_dir(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let out = tokio::time::timeout(std::time::Duration::from_secs(secs), cmd.output())
        .await
        .map_err(|_| GateError::Other(format!("git 超过 {secs} 秒没有结束，已经停下")))?
        .map_err(|e| GateError::Other(format!("git 起不来：{e}")))?;
    Ok((
        out.status.success(),
        crate::process::decode_console_bytes(&out.stdout)
            .trim()
            .to_string(),
        crate::process::decode_console_bytes(&out.stderr)
            .trim()
            .to_string(),
    ))
}

fn last_lines(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(3)..];
    redact_url(&tail.join(" / "))
}

/// 一个扩展和远端比落后几个提交。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ExtensionCheck {
    pub name: String,
    pub scope: ExtensionScope,
    /// 落后几个提交；判断不了是 `null`（看 `detail`）。
    pub behind: Option<u32>,
    pub detail: String,
}

fn branch_of(dir: &Path) -> Result<String> {
    let head = std::fs::read_to_string(dir.join(".git").join("HEAD")).unwrap_or_default();
    match git_head(&head) {
        (Some(b), _) => Ok(b),
        _ => Err(GateError::Other(
            "它现在不在任何分支上（游离的提交），没法比较；在酒馆的扩展管理里切回一个分支".into(),
        )),
    }
}

/// **联网**（`git fetch`）：只在点了「检查更新」时跑。
pub async fn check_extension(name: &str, scope: ExtensionScope) -> Result<ExtensionCheck> {
    let _git = git_lock().lock().await;
    let root = configured_root()?;
    let dir = extension_dir(&root, name, scope)?;
    if !dir.join(".git").exists() {
        return Ok(ExtensionCheck {
            name: name.into(),
            scope,
            behind: None,
            detail: "不是 git 克隆来的，面板没法检查更新（它多半是解压进来的）".into(),
        });
    }
    let git = find_git().ok_or_else(|| GateError::Other(NO_GIT.into()))?;
    let branch = branch_of(&dir)?;
    let (ok, _, err) = run_git(&git, &dir, &["fetch", "--quiet", "origin"], 120).await?;
    if !ok {
        return Err(GateError::Other(format!(
            "git fetch 没成：{}",
            last_lines(&err)
        )));
    }
    let range = format!("HEAD..origin/{branch}");
    let (ok, out, err) = run_git(&git, &dir, &["rev-list", "--count", &range], 30).await?;
    if !ok {
        return Err(GateError::Other(format!(
            "比较不了（{}）",
            last_lines(&err)
        )));
    }
    let behind = out.trim().parse::<u32>().ok();
    Ok(ExtensionCheck {
        name: name.into(),
        scope,
        detail: match behind {
            Some(0) => "已经是最新的".into(),
            Some(n) => format!("远端多了 {n} 个提交"),
            None => format!(
                "git 回了认不出的东西：{}",
                out.chars().take(80).collect::<String>()
            ),
        },
        behind,
    })
}

/// **联网**：`git pull --ff-only`。本地改过（没法快进）就不动，照实说。
pub async fn update_extension(name: &str, scope: ExtensionScope) -> Result<String> {
    let _git = git_lock().lock().await;
    let root = configured_root()?;
    let dir = extension_dir(&root, name, scope)?;
    if !dir.join(".git").exists() {
        return Err(GateError::Other(
            "不是 git 克隆来的，面板没法更新它；删掉重装，或在酒馆里处理".into(),
        ));
    }
    let git = find_git().ok_or_else(|| GateError::Other(NO_GIT.into()))?;
    let branch = branch_of(&dir)?;
    let (ok, out, err) = run_git(
        &git,
        &dir,
        &["pull", "--ff-only", "--quiet", "origin", &branch],
        300,
    )
    .await?;
    if !ok {
        let why = last_lines(&format!("{out}\n{err}"));
        let hint = if why.to_lowercase().contains("fast-forward") {
            "本地改过这个扩展的文件，没法直接快进 —— 面板不替你丢改动；在酒馆的扩展管理里更新，或者自己处理"
        } else {
            "多半是网络或权限问题"
        };
        return Err(GateError::Other(format!("更新没成（{why}）：{hint}")));
    }
    let (_, _, _, commit) = git_facts(&dir);
    crate::audit::write(&format!("酒馆：扩展 {name} 已更新到 {commit}"));
    Ok(format!("已更新到 {commit}（{branch}）"))
}

/// 纯函数：从仓库地址得一个目录名。只收 `https://`；地址里不许有空白、引号。
pub fn repo_dir_name(url: &str) -> Result<String> {
    let u = url.trim();
    if !u.starts_with("https://") || u.len() > 500 {
        return Err(GateError::Other("只收 https:// 开头的仓库地址".into()));
    }
    if u.chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '"' || c == '\'')
    {
        return Err(GateError::Other("地址里有空白或引号".into()));
    }
    let path = u["https://".len()..]
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let (host, rest) = path.split_once('/').unwrap_or((path, ""));
    if host.is_empty() || host.contains('@') {
        return Err(GateError::Other(
            "地址里不要带账号密码（面板不替私有仓库存凭据）".into(),
        ));
    }
    let last = rest
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let name = last.strip_suffix(".git").unwrap_or(last).to_string();
    if !safe_file_name(&name) || name.starts_with('.') {
        return Err(GateError::Other("从地址里认不出仓库名".into()));
    }
    Ok(name)
}

/// **联网**：`git clone --depth 1 <地址>` 到 `data\default-user\extensions\<仓库名>`（酒馆默认装的那一层）。
/// 克隆下来没有 manifest.json 就删掉这份刚克隆的、照实说它不是酒馆扩展。
pub async fn install_extension(url: &str) -> Result<String> {
    let _git = git_lock().lock().await;
    let root = configured_root()?;
    let name = repo_dir_name(url)?;
    let parent = ExtensionScope::User.dir(&root);
    let target = parent.join(&name);
    if target.exists() {
        return Err(GateError::Other(format!(
            "已经有一个叫 {name} 的扩展了（{}）",
            target.display()
        )));
    }
    let git = find_git().ok_or_else(|| GateError::Other(NO_GIT.into()))?;
    std::fs::create_dir_all(&parent)?;
    let target_s = target.display().to_string();
    let (ok, out, err) = run_git(
        &git,
        &parent,
        &["clone", "--depth", "1", "--quiet", url.trim(), &target_s],
        300,
    )
    .await?;
    if !ok {
        if target.exists() {
            let _ = remove_tree(&target);
        }
        return Err(GateError::Other(format!(
            "克隆没成：{}",
            last_lines(&format!("{out}\n{err}"))
        )));
    }
    if !target.join("manifest.json").is_file() {
        let _ = remove_tree(&target);
        return Err(GateError::Other(
            "这个仓库里没有 manifest.json，不是酒馆扩展 —— 刚克隆的那份已经删掉了".into(),
        ));
    }
    crate::audit::write(&format!("酒馆：从 {} 装了扩展 {name}", redact_url(url)));
    Ok(format!("装好了 {name}；刷新一下酒馆的页面就能看到"))
}

pub fn delete_extension(name: &str, scope: ExtensionScope) -> Result<TrashEntry> {
    let root = configured_root()?;
    let dir = extension_dir(&root, name, scope)?;
    let entry =
        Trash::default_root().put("extension", &format!("{}{name}", scope.prefix()), &[dir])?;
    crate::audit::write(&format!("酒馆：扩展挪进了回收站（{}）", entry.id));
    Ok(entry)
}

// ================================================================== 回收站

/// 回收站里的一项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrashEntry {
    pub id: String,
    /// `character` / `world` / `extension`。
    pub kind: String,
    pub name: String,
    /// 原来在哪（恢复就放回这些地方）。
    pub originals: Vec<String>,
    pub deleted_at: String,
    #[ts(type = "number")]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrashMeta {
    kind: String,
    name: String,
    deleted_at: String,
    /// (原来的绝对路径, 在 `items\` 下的名字)
    items: Vec<(String, String)>,
}

/// 回收站：`<根>\<id>\meta.json` + `<根>\<id>\items\…`。根默认是 `state_dir\tavern-trash`，单测给临时目录。
pub struct Trash {
    root: PathBuf,
}

fn valid_trash_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn dir_size(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p)
            .map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    }
}

/// 删一棵树之前先把只读属性去掉（git 的对象文件是只读的，`remove_dir_all` 直接失败）。
/// 不跟符号链接 / 目录联结走。
pub fn remove_tree(p: &Path) -> std::io::Result<()> {
    fn clear(p: &Path) {
        let Ok(meta) = std::fs::symlink_metadata(p) else {
            return;
        };
        if meta.file_type().is_symlink() {
            return;
        }
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    clear(&e.path());
                }
            }
        }
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(p, perm);
        }
    }
    let meta = std::fs::symlink_metadata(p)?;
    clear(p);
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// 挪一个文件或目录：同一个盘上改名；跨盘就先复制、复制全了再删原处。
fn move_path(src: &Path, dst: &Path) -> Result<()> {
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if let Err(e) = copy_tree(src, dst) {
        let _ = remove_tree(dst);
        return Err(GateError::Other(format!(
            "复制 {} 没成（{e}），原处没动",
            src.display()
        )));
    }
    remove_tree(src).map_err(|e| {
        GateError::Other(format!(
            "已经复制到 {}，但原处 {} 没删干净（{e}；多半有文件正被占用）",
            dst.display(),
            src.display()
        ))
    })
}

impl Trash {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> Self {
        Self::at(crate::paths::state_dir().join("tavern-trash"))
    }

    /// 把 `items` 挪进一个新的回收站条目。任何一项挪不动就把已经挪进来的放回去，整件事不做。
    pub fn put(&self, kind: &str, name: &str, items: &[PathBuf]) -> Result<TrashEntry> {
        let base = format!("{}-{kind}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let id = (1..1000)
            .map(|n| format!("{base}-{n}"))
            .find(|c| !self.root.join(c).exists())
            .ok_or_else(|| GateError::Other("回收站里同一秒的条目太多了".into()))?;
        let dir = self.root.join(&id);
        std::fs::create_dir_all(dir.join("items"))?;
        let mut moved: Vec<(String, String)> = Vec::new();
        for (i, src) in items.iter().enumerate() {
            let label = format!(
                "{i}-{}",
                src.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "item".into())
            );
            if let Err(e) = move_path(src, &dir.join("items").join(&label)) {
                for (orig, l) in moved.iter().rev() {
                    let _ = move_path(&dir.join("items").join(l), Path::new(orig));
                }
                let _ = remove_tree(&dir);
                return Err(e);
            }
            moved.push((src.display().to_string(), label));
        }
        let meta = TrashMeta {
            kind: kind.into(),
            name: name.into(),
            deleted_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            items: moved,
        };
        crate::config_io::replace(
            &dir.join("meta.json"),
            Some(&serde_json::to_vec_pretty(&meta)?),
        )?;
        Ok(self.entry(&id, meta))
    }

    fn entry(&self, id: &str, meta: TrashMeta) -> TrashEntry {
        TrashEntry {
            id: id.into(),
            size: dir_size(&self.root.join(id).join("items")),
            kind: meta.kind,
            name: meta.name,
            originals: meta.items.into_iter().map(|(o, _)| o).collect(),
            deleted_at: meta.deleted_at,
        }
    }

    fn meta(&self, id: &str) -> Result<TrashMeta> {
        if !valid_trash_id(id) {
            return Err(GateError::Other("回收站条目的 id 不合法".into()));
        }
        let bytes = std::fs::read(self.root.join(id).join("meta.json"))
            .map_err(|_| GateError::NotFound(format!("回收站里没有 {id}")))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn list(&self) -> Vec<TrashEntry> {
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out: Vec<TrashEntry> = rd
            .flatten()
            .filter_map(|e| {
                let id = e.file_name().to_string_lossy().to_string();
                let meta = self.meta(&id).ok()?;
                Some(self.entry(&id, meta))
            })
            .collect();
        out.sort_by(|a, b| b.id.cmp(&a.id));
        out
    }

    /// 原样放回去。原处已经有同名的就一项都不动（不覆盖）。
    pub fn restore(&self, id: &str) -> Result<TrashEntry> {
        let meta = self.meta(id)?;
        if let Some((orig, _)) = meta.items.iter().find(|(o, _)| Path::new(o).exists()) {
            return Err(GateError::Other(format!(
                "原处已经有同名的了（{orig}），没有恢复 —— 先把那一个改名或挪走"
            )));
        }
        let dir = self.root.join(id);
        for (orig, label) in &meta.items {
            move_path(&dir.join("items").join(label), Path::new(orig))?;
        }
        let entry = TrashEntry {
            id: id.into(),
            kind: meta.kind.clone(),
            name: meta.name.clone(),
            originals: meta.items.iter().map(|(o, _)| o.clone()).collect(),
            deleted_at: meta.deleted_at.clone(),
            size: 0,
        };
        let _ = remove_tree(&dir);
        Ok(entry)
    }

    /// 真删。`id` 为 `None` 就是清空整个回收站。
    pub fn purge(&self, id: Option<&str>) -> Result<u32> {
        let ids: Vec<String> = match id {
            Some(i) => {
                self.meta(i)?;
                vec![i.to_string()]
            }
            None => self.list().into_iter().map(|e| e.id).collect(),
        };
        let mut n = 0;
        for i in &ids {
            remove_tree(&self.root.join(i)).map_err(|e| {
                GateError::Other(format!("{i} 没删干净（{e}；多半有文件正被占用）"))
            })?;
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "qb-lib-{name}-{}-{}",
            std::process::id(),
            crate::config_io::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut c = (data.len() as u32).to_be_bytes().to_vec();
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        c.extend_from_slice(&[0, 0, 0, 0]); // CRC 不核
        c
    }

    /// 一张最小的「PNG」：签名 + IHDR + 若干 tEXt + IDAT + IEND。酒馆把卡放在 IEND 前面。
    fn png(texts: &[(&str, &str)]) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend(chunk(b"IHDR", &[0; 13]));
        out.extend(chunk(b"IDAT", &[1, 2, 3, 4, 5]));
        for (k, json) in texts {
            let mut data = k.as_bytes().to_vec();
            data.push(0);
            data.extend(
                base64::engine::general_purpose::STANDARD
                    .encode(json)
                    .bytes(),
            );
            out.extend(chunk(b"tEXt", &data));
        }
        out.extend(chunk(b"IEND", &[]));
        out
    }

    const V2: &str = r#"{"spec":"chara_card_v2","spec_version":"2.0","data":{"name":"林澈","description":"图书管理员","first_mes":"你好","creator":"demo","character_version":"1.2","tags":["原创","  ","图书馆"],"alternate_greetings":["早","晚"],"character_book":{"name":"馆藏","entries":[{"keys":["书"],"content":"藏书十万","enabled":true,"insertion_order":3},{"keys":["门"],"content":"朝北","enabled":false}]}}}"#;

    #[test]
    fn the_card_text_is_found_at_the_end_of_the_png() {
        let bytes = png(&[("chara", V2)]);
        let text = card_text(&mut Cursor::new(bytes)).unwrap().unwrap();
        let f = card_fields(&text).unwrap();
        assert_eq!(f.spec, "v2");
        assert_eq!(f.name, "林澈");
        assert_eq!(f.creator, "demo");
        assert_eq!(f.version, "1.2");
        assert_eq!(f.tags, ["原创", "图书馆"], "空白标签不算");
        assert_eq!(f.alternate_greetings.len(), 2);
        let book = f.book.unwrap();
        assert_eq!(book.len(), 2);
        assert_eq!(book[0].order, Some(3));
        assert!(book[1].disabled, "V2 的 enabled:false 就是停用");
    }

    /// V3 与 V2 同时在的时候读 V3（酒馆自己也是这个顺序）。
    #[test]
    fn ccv3_wins_over_chara() {
        let v3 = r#"{"spec":"chara_card_v3","data":{"name":"新名字"}}"#;
        let bytes = png(&[("chara", V2), ("ccv3", v3)]);
        let f = card_fields(&card_text(&mut Cursor::new(bytes)).unwrap().unwrap()).unwrap();
        assert_eq!((f.spec.as_str(), f.name.as_str()), ("v3", "新名字"));
    }

    /// V1：字段在顶层。
    #[test]
    fn a_v1_card_reads_its_top_level_fields() {
        let f = card_fields(r#"{"name":"Old","description":"d","first_mes":"hi"}"#).unwrap();
        assert_eq!(
            (f.spec.as_str(), f.name.as_str(), f.first_mes.as_str()),
            ("v1", "Old", "hi")
        );
    }

    #[test]
    fn a_png_without_a_card_or_not_a_png_is_none() {
        assert!(card_text(&mut Cursor::new(png(&[]))).unwrap().is_none());
        assert!(card_text(&mut Cursor::new(b"GIF89a....".to_vec()))
            .unwrap()
            .is_none());
        assert!(card_text(&mut Cursor::new(Vec::<u8>::new()))
            .unwrap()
            .is_none());
    }

    /// 截断的 PNG（下载到一半）不 panic、不死循环。
    #[test]
    fn a_truncated_png_is_handled() {
        let mut bytes = png(&[("chara", V2)]);
        bytes.truncate(40);
        assert!(card_text(&mut Cursor::new(bytes)).unwrap().is_none());
    }

    #[test]
    fn st_world_entries_are_read_from_an_object() {
        let w: serde_json::Value = serde_json::from_str(
            r#"{"entries":{"0":{"uid":0,"key":["a","b"],"keysecondary":["c"],"comment":"标题","content":"内容","constant":true,"disable":false,"order":100},"1":{"uid":1,"key":[],"content":"x","disable":true}}}"#,
        )
        .unwrap();
        let e = book_entries(&w).unwrap();
        assert_eq!(e.len(), 2);
        let first = e.iter().find(|x| x.uid == "0").unwrap();
        assert_eq!(first.keys, ["a", "b"]);
        assert_eq!(first.secondary, ["c"]);
        assert!(first.constant && !first.disabled);
        assert_eq!(first.order, Some(100));
        assert!(e.iter().find(|x| x.uid == "1").unwrap().disabled);
        assert!(book_entries(&serde_json::json!({"name": "x"})).is_none());
    }

    #[test]
    fn file_names_that_could_escape_the_folder_are_refused() {
        for bad in [
            "",
            " x.png",
            "..",
            ".",
            "a/b.png",
            "a\\b.png",
            "c:x.png",
            "x.png.",
            "a?.png",
            "a\u{0}.png",
        ] {
            assert!(!safe_file_name(bad), "{bad:?}");
        }
        for good in ["Seraphina.png", "林澈 (2).png", "a.b.c.json"] {
            assert!(safe_file_name(good), "{good:?}");
        }
    }

    #[test]
    fn import_never_overwrites() {
        let d = temp("unique");
        std::fs::write(d.join("a.png"), b"x").unwrap();
        std::fs::write(d.join("a (2).png"), b"x").unwrap();
        assert_eq!(unique_name(&d, "a.png"), "a (3).png");
        assert_eq!(unique_name(&d, "b.png"), "b.png");
        std::fs::write(d.join("noext"), b"x").unwrap();
        assert_eq!(unique_name(&d, "noext"), "noext (2)");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn importing_cards_checks_every_file_and_keeps_the_rest() {
        let root = temp("import");
        std::fs::create_dir_all(user_dir_of(&root).join("characters")).unwrap();
        let src = temp("import-src");
        std::fs::write(src.join("good.png"), png(&[("chara", V2)])).unwrap();
        std::fs::write(src.join("plain.png"), png(&[])).unwrap();
        std::fs::write(src.join("card.json"), V2).unwrap();
        let r = import_characters_in(
            &root,
            &[
                src.join("good.png"),
                src.join("plain.png"),
                src.join("card.json"),
                src.join("missing.png"),
            ],
        )
        .unwrap();
        assert_eq!(r.imported, ["good.png"]);
        assert_eq!(r.skipped.len(), 3, "{:?}", r.skipped);
        // 再导一次同一张：不覆盖，换名字。
        let again = import_characters_in(&root, &[src.join("good.png")]).unwrap();
        assert_eq!(again.imported, ["good (2).png"]);
        let list = list_characters_in(&root);
        assert_eq!(list.len(), 2);
        assert!(list
            .iter()
            .all(|c| c.name == "林澈" && c.book_entries == Some(2)));
        assert!(src.join("good.png").is_file(), "源文件不许动");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn a_broken_card_is_listed_with_its_problem_not_hidden() {
        let root = temp("broken");
        let chars = user_dir_of(&root).join("characters");
        std::fs::create_dir_all(&chars).unwrap();
        std::fs::write(chars.join("broken.png"), png(&[])).unwrap();
        std::fs::create_dir_all(user_dir_of(&root).join("chats").join("broken")).unwrap();
        std::fs::write(
            user_dir_of(&root)
                .join("chats")
                .join("broken")
                .join("1.jsonl"),
            b"{}",
        )
        .unwrap();
        let list = list_characters_in(&root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "broken");
        assert_eq!(list[0].chats, 1);
        assert!(list[0].problem.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn worlds_in_other_formats_are_refused_at_import() {
        let root = temp("worlds");
        let src = temp("worlds-src");
        std::fs::write(
            src.join("st.json"),
            r#"{"entries":{"0":{"key":["k"],"content":"c"}}}"#,
        )
        .unwrap();
        std::fs::write(
            src.join("novel.json"),
            r#"{"lorebookVersion":5,"entries":"nope"}"#,
        )
        .unwrap();
        std::fs::write(src.join("bad.json"), "{").unwrap();
        let r = import_worlds_in(
            &root,
            &[
                src.join("st.json"),
                src.join("novel.json"),
                src.join("bad.json"),
            ],
        )
        .unwrap();
        assert_eq!(r.imported, ["st.json"]);
        assert_eq!(r.skipped.len(), 2);
        let list = list_worlds_in(&root);
        assert_eq!(
            (list[0].name.as_str(), list[0].entries, list[0].enabled),
            ("st", 1, 1)
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn credentials_in_a_remote_url_are_never_shown() {
        assert_eq!(
            redact_url("https://user:ghp_secret@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(
            redact_url("https://github.com/a/b"),
            "https://github.com/a/b"
        );
        let cfg = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://x/y\n[remote \"origin\"]\n\turl = https://tok@github.com/o/ext.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n";
        assert_eq!(
            git_remote_url(cfg).as_deref(),
            Some("https://github.com/o/ext.git")
        );
    }

    #[test]
    fn git_head_and_packed_refs_are_read_without_git() {
        assert_eq!(
            git_head("ref: refs/heads/main\n"),
            (Some("main".into()), None)
        );
        assert_eq!(
            git_head("0123456789abcdef0123456789abcdef01234567"),
            (
                None,
                Some("0123456789abcdef0123456789abcdef01234567".into())
            )
        );
        assert_eq!(git_head("garbage"), (None, None));
        let packed = "# pack-refs with: peeled fully-peeled sorted\nabcdef0123456789abcdef0123456789abcdef01 refs/heads/main\n^1111111111111111111111111111111111111111\n";
        assert_eq!(
            packed_ref(packed, "main").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
        assert!(packed_ref(packed, "dev").is_none());
    }

    #[test]
    fn disabled_extensions_come_from_sillytavern_own_settings() {
        let s = r#"{"extension_settings":{"disabledExtensions":["third-party/Foo","memory","third-party/Bar"]}}"#;
        assert_eq!(disabled_extensions(s), ["Foo", "Bar"]);
        assert!(disabled_extensions("{").is_empty());
    }

    #[test]
    fn extensions_are_listed_from_both_places_with_their_git_facts() {
        let root = temp("ext");
        let user = ExtensionScope::User.dir(&root).join("Mine");
        std::fs::create_dir_all(user.join(".git").join("refs").join("heads")).unwrap();
        std::fs::write(user.join("manifest.json"), r#"{"display_name":"我的扩展","version":"1.0","author":"me","homePage":"http://insecure"}"#).unwrap();
        std::fs::write(user.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            user.join(".git").join("refs").join("heads").join("main"),
            "abcdef0123456789\n",
        )
        .unwrap();
        std::fs::write(
            user.join(".git").join("config"),
            "[remote \"origin\"]\n\turl = https://a:b@github.com/o/Mine\n",
        )
        .unwrap();
        let global = ExtensionScope::Global.dir(&root).join("Ghost");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            user_dir_of(&root).join("settings.json"),
            r#"{"extension_settings":{"disabledExtensions":["third-party/Mine"]}}"#,
        )
        .unwrap();
        let list = list_extensions_in(&root);
        assert_eq!(list.len(), 2);
        let mine = &list[0];
        assert_eq!(
            (mine.display_name.as_str(), mine.scope),
            ("我的扩展", ExtensionScope::User)
        );
        assert!(mine.git && mine.disabled && mine.has_manifest);
        assert_eq!(
            (mine.branch.as_str(), mine.commit.as_str()),
            ("main", "abcdef0")
        );
        assert_eq!(mine.remote, "https://github.com/o/Mine");
        assert_eq!(mine.home_page, "", "非 https 的主页不给");
        let ghost = &list[1];
        assert_eq!(
            (ghost.scope, ghost.has_manifest, ghost.git),
            (ExtensionScope::Global, false, false)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn repo_names_come_from_https_urls_only() {
        assert_eq!(
            repo_dir_name("https://github.com/o/Ext-Name.git").unwrap(),
            "Ext-Name"
        );
        assert_eq!(repo_dir_name("https://gitee.com/o/ext/").unwrap(), "ext");
        for bad in [
            "http://github.com/o/x",
            "git@github.com:o/x.git",
            "https://user:tok@github.com/o/x",
            "https://github.com/o/x y",
            "https://github.com/",
            "https://github.com/o/..",
        ] {
            assert!(repo_dir_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sniffing_tells_png_jpeg_and_webp_apart() {
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\nxx"), Some("image/png"));
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            sniff_image(b"<svg"),
            None,
            "不认的就不给 —— 不把任意文件塞进 data URL"
        );
    }

    /// 回收站：挪进去、列得出、原样放回；原处有同名的就不放。
    #[test]
    fn the_trash_moves_things_out_and_back() {
        let root = temp("trash-src");
        let trash = Trash::at(temp("trash"));
        let card = root.join("a.png");
        let chats = root.join("chats-a");
        std::fs::write(&card, b"card").unwrap();
        std::fs::create_dir_all(&chats).unwrap();
        std::fs::write(chats.join("1.jsonl"), b"{}").unwrap();

        let e = trash
            .put("character", "A", &[card.clone(), chats.clone()])
            .unwrap();
        assert!(!card.exists() && !chats.exists());
        assert_eq!(e.size, 6);
        let list = trash.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "A");

        std::fs::write(&card, b"someone else").unwrap();
        assert!(trash.restore(&e.id).is_err(), "原处有同名的不许覆盖");
        assert_eq!(std::fs::read(&card).unwrap(), b"someone else");
        std::fs::remove_file(&card).unwrap();

        trash.restore(&e.id).unwrap();
        assert_eq!(std::fs::read(&card).unwrap(), b"card");
        assert!(chats.join("1.jsonl").is_file());
        assert!(trash.list().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 只读文件（git 的对象）也删得掉；清空回收站按条数说。
    #[test]
    fn purging_removes_read_only_files_too() {
        let root = temp("purge-src");
        let trash = Trash::at(temp("purge"));
        let ext = root.join("Ext");
        std::fs::create_dir_all(ext.join(".git").join("objects")).unwrap();
        let obj = ext.join(".git").join("objects").join("pack");
        std::fs::write(&obj, b"x").unwrap();
        let mut p = std::fs::metadata(&obj).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&obj, p).unwrap();
        trash.put("extension", "Ext", &[ext.clone()]).unwrap();
        trash.put("world", "W", &[]).unwrap();
        assert_eq!(trash.purge(None).unwrap(), 2);
        assert!(trash.list().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn trash_ids_cannot_walk_out_of_the_trash() {
        let trash = Trash::at(temp("ids"));
        for bad in ["..", "../x", "a\\b", "", "a/b"] {
            assert!(trash.restore(bad).is_err(), "{bad}");
            assert!(trash.purge(Some(bad)).is_err(), "{bad}");
        }
    }
}
