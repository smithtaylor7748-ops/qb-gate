//! Windows FILETIME 过 IPC 时的编码：**字符串，不是数字。**
//!
//! # 为什么
//!
//! 这个值现在的量级是 1.3e17，远超 JavaScript 的安全整数上限 2^53（约 9e15）。
//! 当成 JSON 数字传过去，`JSON.parse` 会**静默把末几位抹掉** —— 不报错、
//! 不警告，就是悄悄变成另一个数。
//!
//! 而这个值的用途恰恰是身份核验：`sessions::terminate_verified(pid, created)`
//! 靠它判断「这个 PID 是不是被复用了」。抹掉几位之后那个判断会错，
//! 而且错得完全没有症状 —— 直到某天杀错一个无关进程。
//!
//! # 为什么反序列化也收数字
//!
//! v0.12 之前落库的会话行里这个字段是 JSON 数字。只认字符串的话，
//! 升级上来的那些行会直接反序列化失败，而那一刻使用者正在升级途中。

use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Text(String),
        Number(u64),
    }
    Ok(match Either::deserialize(d)? {
        Either::Text(s) => s.parse().map_err(serde::de::Error::custom)?,
        Either::Number(n) => n,
    })
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Row {
        #[serde(with = "super")]
        created: u64,
    }

    #[test]
    fn a_filetime_goes_out_as_a_string() {
        // 不是字符串的话，JS 那边这个数会被静默截断。
        let json = serde_json::to_string(&Row {
            created: 133_800_000_000_000_000,
        })
        .unwrap();
        assert_eq!(json, r#"{"created":"133800000000000000"}"#);
    }

    #[test]
    fn an_old_row_that_stored_a_number_still_loads() {
        // v0.12 之前落库的行。只认字符串的话，升级上来的人会在升级途中炸。
        let row: Row = serde_json::from_str(r#"{"created":133800000000000000}"#).unwrap();
        assert_eq!(row.created, 133_800_000_000_000_000);
    }

    #[test]
    fn the_round_trip_keeps_every_digit() {
        // 这条是这个模块存在的理由：这个量级下 f64 存不住最后两位。
        let original = Row {
            created: 133_812_345_678_912_345,
        };
        let back: Row = serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        assert_eq!(back, original);
    }
}
