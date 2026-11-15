//! 配对的落盘：`<目录>\browser-link.json`。
//!
//! 口令用仓库现成的 `secret::seal`（Windows DPAPI）封住 —— 文件被拷走、换一个 Windows 用户
//! 打开，里面的口令都解不开。目录由调用方给：正式面板是 `paths::state_dir()`，
//! 单测与独立测试桥给临时目录（单测不许碰真实的运行期状态）。
//!
//! 一条配对 = 一个浏览器资料里的那份插件。**不按扩展 ID 去重**：插件的 ID 固定之后，
//! 同一台机器上每个 Chrome 资料的扩展 ID 都一样，按 ID 存就只能连一个资料。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::link_auth::{is_hex, valid_extension_id};
use super::MAX_PAIRINGS;
use crate::error::{GateError, Result};

const FILE: &str = "browser-link.json";
const VERSION: u32 = 1;

/// 内存里的一条配对。`token` 是明文，只在进程里。
/// 故意不派生 `Debug`：口令不该有任何机会被打印进日志或报错。
#[derive(Clone)]
pub(super) struct Record {
    pub pairing_id: String,
    pub extension_id: String,
    pub token: String,
    pub label: String,
    pub extension_version: String,
    pub paired_at: u64,
    /// 这份插件最后一次被告知（或自己连上）的端口。跟桥现在的端口不一样 = 它还照着旧端口在找。
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
struct SavedRecord {
    pairing_id: String,
    extension_id: String,
    /// `secret::seal` 的产物。
    token: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    extension_version: String,
    #[serde(default)]
    paired_at: u64,
    #[serde(default)]
    port: u16,
}

#[derive(Serialize, Deserialize)]
struct SavedFile {
    version: u32,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    pairings: Vec<SavedRecord>,
}

#[derive(Default)]
pub(super) struct Loaded {
    /// 插件们最近一次被告知的端口（下次启动先试它）；0 = 没有。
    pub port: u16,
    pub records: Vec<Record>,
    /// 文件里有、但这次用不了的条数（口令解不开、形状不对）。
    pub dropped: usize,
}

pub(super) fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// 没有文件 = 从没配对过（空）。文件在、却读不懂 = 报错，不当成空的。
pub(super) fn load(dir: &Path) -> Result<Loaded> {
    let Some(bytes) = crate::config_io::read_optional(&path(dir))? else {
        return Ok(Loaded::default());
    };
    let file: SavedFile = serde_json::from_slice(&bytes)
        .map_err(|_| GateError::Other("浏览器助手的配对记录读不懂".into()))?;
    if file.version != VERSION {
        return Err(GateError::Other(
            "浏览器助手的配对记录来自不认识的版本".into(),
        ));
    }
    let mut loaded = Loaded {
        port: file.port,
        ..Default::default()
    };
    for saved in file.pairings {
        let token = crate::secret::open(&saved.token).filter(|t| is_hex(t, 64));
        match token {
            Some(token)
                if is_hex(&saved.pairing_id, 16)
                    && valid_extension_id(&saved.extension_id)
                    && loaded.records.len() < MAX_PAIRINGS
                    && !loaded
                        .records
                        .iter()
                        .any(|r| r.pairing_id == saved.pairing_id) =>
            {
                loaded.records.push(Record {
                    pairing_id: saved.pairing_id,
                    extension_id: saved.extension_id,
                    token,
                    label: saved.label,
                    extension_version: saved.extension_version,
                    paired_at: saved.paired_at,
                    port: saved.port,
                })
            }
            _ => loaded.dropped += 1,
        }
    }
    Ok(loaded)
}

/// 一条配对都没有就删掉文件：没配对过的机器上不留任何东西。
pub(super) fn save(dir: &Path, port: u16, records: &[Record]) -> Result<()> {
    if records.is_empty() {
        return crate::config_io::replace(&path(dir), None);
    }
    let file = SavedFile {
        version: VERSION,
        port,
        pairings: records
            .iter()
            .map(|r| {
                Ok(SavedRecord {
                    pairing_id: r.pairing_id.clone(),
                    extension_id: r.extension_id.clone(),
                    token: crate::secret::seal(&r.token)?,
                    label: r.label.clone(),
                    extension_version: r.extension_version.clone(),
                    paired_at: r.paired_at,
                    port: r.port,
                })
            })
            .collect::<Result<_>>()?,
    };
    let body = serde_json::to_vec_pretty(&file)
        .map_err(|_| GateError::Other("浏览器助手的配对记录无法编码".into()))?;
    crate::config_io::replace(&path(dir), Some(&body))
}

/// 读不懂的那份挪到一边（使用者点「连接浏览器」时才做）：不覆盖、不删除。
pub(super) fn quarantine(dir: &Path) -> Result<PathBuf> {
    let aside = dir.join(format!(
        "browser-link.unreadable-{}.json",
        chrono::Utc::now().timestamp()
    ));
    std::fs::rename(path(dir), &aside)?;
    Ok(aside)
}
