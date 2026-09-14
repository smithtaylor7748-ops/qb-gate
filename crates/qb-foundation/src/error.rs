pub use qb_contract::error_kind::ErrorKind;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("操作已由用户取消")]
    Cancelled,
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

    /// 门禁判定没过，原因已经是一句可以直接显示的人话（见 `gate::judge::Judgement::reason`）。
    ///
    /// 单独一个变体是因为国家层有四种不同的拒绝理由，硬塞进 `IpNotAllowed`
    /// 会拼出「当前出口 IP 出口 IP 1.2.3.4 落在 HK…… 不在白名单内」这种句子。
    #[error("{0}")]
    GateRejected(String),

    /// 数据库操作失败。
    ///
    /// 单独一个变体，是因为 SQLite 的错误里藏着**调用方必须分得清**的几种：
    /// `UNIQUE` 冲突（这条记录已经有了）、外键 `RESTRICT`（还有别的东西在引用它）、
    /// `SQLITE_BUSY`（另一个写者占着）。原来它们全被压成 `Other`，
    /// 于是「重复了」和「磁盘满了」在前端长得一模一样。
    #[error("数据库操作失败：{0}")]
    Database(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GateError>;

impl GateError {
    /// 这个错误属于哪一类。**前端做条件处理只许看它。**
    ///
    /// 加新变体时这里的 `match` 会编译失败 —— 那是故意的：
    /// 一个没有分类的错误在前端就是 `other`，跟没分类之前一样没用。
    pub fn kind(&self) -> ErrorKind {
        match self {
            GateError::Cancelled => ErrorKind::Cancelled,
            GateError::Io(_) => ErrorKind::Io,
            GateError::Http(_) => ErrorKind::Http,
            GateError::Json(_) => ErrorKind::Json,
            GateError::WinApi { .. } => ErrorKind::WinApi,
            GateError::NotFound(_) => ErrorKind::NotFound,
            GateError::IpNotAllowed { .. } => ErrorKind::IpNotAllowed,
            GateError::IpUnknown => ErrorKind::IpUnknown,
            GateError::GateRejected(_) => ErrorKind::GateRejected,
            GateError::Database(_) => ErrorKind::Database,
            GateError::Other(_) => ErrorKind::Other,
        }
    }
}

/// 过 IPC 时的形状：`{ kind, message }`。
///
/// # 为什么不再是一个裸字符串
///
/// 原来是 `s.serialize_str(&self.to_string())`。于是前端拿到的所有错误都是
/// 一句中文，想区分「这条重复了」和「磁盘满了」只能 `message.includes("重复")`。
/// 那条路一定会烂：**中文文案是会改的**，而改文案的人不知道某个页面正靠
/// 那几个字做分支 —— 症状是错误处理静默失效，没有任何报错。
///
/// `message` 仍然是那句可以直接显示的人话，一个字没变；多出来的只有 `kind`。
impl Serialize for GateError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut m = s.serialize_struct("GateError", 2)?;
        m.serialize_field("kind", &self.kind())?;
        m.serialize_field("message", &self.to_string())?;
        m.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_cross_ipc_as_kind_plus_message() {
        let v = serde_json::to_value(GateError::IpUnknown).unwrap();
        assert_eq!(v["kind"], "ip-unknown");
        assert_eq!(v["message"], "查不到当前出口 IP");
    }

    #[test]
    fn the_message_is_still_the_sentence_a_human_reads() {
        // 分类是加上去的，不是替换掉的 —— 界面上显示的那句话必须原样还在。
        let e = GateError::IpNotAllowed {
            ip: "1.2.3.4".into(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["message"], e.to_string());
        assert_eq!(v["message"], "当前出口 IP 1.2.3.4 不在白名单内");
    }

    #[test]
    fn a_database_error_is_not_lumped_into_other() {
        // 这一条是 E1 的理由本身：`repository` 原来把整个 SQLite 错误层
        // （UNIQUE 冲突、外键 RESTRICT、SQLITE_BUSY）压成一个 Other，
        // 调用方分不出「重复了」和「磁盘满了」。
        let e = GateError::Database("UNIQUE constraint failed".into());
        assert_eq!(e.kind(), ErrorKind::Database);
        assert_ne!(e.kind(), ErrorKind::Other);
    }

    #[test]
    fn a_user_cancellation_serializes_as_cancelled_not_other() {
        // 界面据此决定「弹红色报错」还是「静静收场」。
        let v = serde_json::to_value(GateError::Cancelled).unwrap();
        assert_eq!(v["kind"], "cancelled");
    }
}
