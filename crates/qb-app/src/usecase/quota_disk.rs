//! 联网额度「上次成功」那一份落盘：`<state_dir>\quota-last.json`。
//!
//! 按账户存：读数原样（各家自己的结构）+ 时刻 + 凭据指纹 + 账户标识。**不含任何令牌** ——
//! 存进来的都是已经解析好的额度结构，那些结构本来就要过 IPC，里面没有令牌。
//! 重启后先显示它并标「上次成功：几点」；账户标识变了（换了人登录）就作废。
//! 不进数据库、不升结构版本：一个普通 JSON 文件，坏了就当没有。
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    entries: HashMap<String, Entry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    at: String,
    #[serde(default)]
    fingerprint: Option<(i64, u64)>,
    #[serde(default)]
    identity: Option<String>,
    value: Value,
}

// 每个测试使用自己的目录，避免并行测试互换全局路径。
#[cfg(test)]
thread_local! {
    static TEST_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}
static LOCK: Mutex<()> = Mutex::new(());

fn path() -> PathBuf {
    #[cfg(test)]
    let root = TEST_ROOT
        .with(|g| g.borrow().clone())
        .unwrap_or_else(|| std::env::temp_dir().join("qb-quota-disk-default"));
    #[cfg(not(test))]
    let root = qb_foundation::paths::state_dir();
    root.join("quota-last.json")
}

#[cfg(test)]
pub fn set_root_for_test(dir: &Path) {
    TEST_ROOT.with(|g| *g.borrow_mut() = Some(dir.to_path_buf()));
}

fn load(p: &Path) -> File {
    std::fs::read(p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// 记下这一次成功的读数。写不进去不报错（只是重启后没有「上次成功」）。
pub fn put<T: Serialize>(
    key: &str,
    identity: Option<&str>,
    fingerprint: Option<(i64, u64)>,
    value: &T,
) {
    let Ok(value) = serde_json::to_value(value) else {
        return;
    };
    let _g = LOCK.lock();
    let p = path();
    let mut f = load(&p);
    f.entries.insert(
        key.to_string(),
        Entry {
            at: chrono::Local::now().to_rfc3339(),
            fingerprint,
            identity: identity.map(|s| s.trim().to_lowercase()),
            value,
        },
    );
    if let Ok(bytes) = serde_json::to_vec_pretty(&f) {
        let _ = qb_platform::config_io::replace(&p, Some(&bytes));
    }
}

/// 读上次成功的那份。`identity` 给了且跟存的对不上 → 作废（删掉）并返回 `None`。
/// 返回 `(读数, 记下的时刻 RFC3339)`。
pub fn get<T: DeserializeOwned>(key: &str, identity: Option<&str>) -> Option<(T, String)> {
    let _g = LOCK.lock();
    let p = path();
    let mut f = load(&p);
    let e = f.entries.get(key)?.clone();
    if let (Some(now), Some(then)) = (identity, e.identity.as_deref()) {
        if now.trim().to_lowercase() != then {
            f.entries.remove(key);
            if let Ok(bytes) = serde_json::to_vec_pretty(&f) {
                let _ = qb_platform::config_io::replace(&p, Some(&bytes));
            }
            return None;
        }
    }
    Some((serde_json::from_value(e.value).ok()?, e.at))
}

/// 槽位移除时一起忘掉。
pub fn forget(key: &str) {
    let _g = LOCK.lock();
    let p = path();
    let mut f = load(&p);
    if f.entries.remove(key).is_some() {
        if let Ok(bytes) = serde_json::to_vec_pretty(&f) {
            let _ = qb_platform::config_io::replace(&p, Some(&bytes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_success_survives_and_identity_change_voids_it() {
        let dir =
            std::env::temp_dir().join(format!("qb-quota-disk-{}", qb_platform::config_io::id()));
        std::fs::create_dir_all(&dir).unwrap();
        set_root_for_test(&dir);
        put(
            "gpt:a",
            Some("Acct-1"),
            Some((1, 2)),
            &serde_json::json!({"used": 3}),
        );
        let (v, at) = get::<Value>("gpt:a", Some("acct-1")).unwrap();
        assert_eq!(v["used"], 3);
        assert!(!at.is_empty());
        let text = std::fs::read_to_string(dir.join("quota-last.json")).unwrap();
        assert!(!text.contains("token"));
        assert!(get::<Value>("gpt:a", Some("acct-2")).is_none());
        assert!(
            get::<Value>("gpt:a", Some("acct-1")).is_none(),
            "作废之后就没了"
        );
        put("gpt:b", None, None, &1);
        forget("gpt:b");
        assert!(get::<Value>("gpt:b", None).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
