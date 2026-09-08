use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("IO 失败: {0}")]
    Io(#[from] std::io::Error),

    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Windows API {api} 失败，错误码 {code}")]
    WinApi { api: &'static str, code: u32 },

    #[error("路径不存在: {0}")]
    NotFound(String),

    #[error("当前出口 IP {ip} 不在白名单内")]
    IpNotAllowed { ip: String },

    #[error("查不到当前出口 IP")]
    IpUnknown,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GateError>;

/// Tauri command 的返回错误必须可序列化，统一降成字符串。
impl Serialize for GateError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
