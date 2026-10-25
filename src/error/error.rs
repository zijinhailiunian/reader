use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    /// 数据库错误。**保留 sqlx 的原始描述**：早先这里只写 `db error`，于是
    /// `{"isSuccess":false,"errorMsg":"db error"}` 把"哪个表/哪条语句/是锁还是
    /// 约束"全部吞掉了，用户侧与 issue 里都只能看到一句无从下手的提示。
    /// 对外仍是 500（`router::envelope` 按变体判状态码），只是多了原因。
    #[error("db error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Tauri requires command error types to be `Serialize`. We serialize to the
/// message string so the JS side rejects with a readable error (the old axum
/// layer returned an `{isSuccess:false, errorMsg}` JSON body; the IPC client
/// turns this into `new Error(message)`).
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    #[serde(rename = "isSuccess")]
    pub is_success: bool,
    #[serde(rename = "errorMsg")]
    pub error_msg: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            is_success: true,
            error_msg: "".to_string(),
            data: Some(data),
        }
    }
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            is_success: false,
            error_msg: message.into(),
            data: None,
        }
    }
    pub fn err_with_data(message: impl Into<String>, data: T) -> Self {
        Self {
            is_success: false,
            error_msg: message.into(),
            data: Some(data),
        }
    }
}
