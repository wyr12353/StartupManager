use serde::ser::{Serialize, SerializeStruct, Serializer};

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn elevation_cancelled() -> Self {
        Self::new("ELEVATION_CANCELLED", "已取消管理员授权，未做任何修改")
    }

    pub fn stale_item() -> Self {
        Self::new("STALE_ITEM", "启动项已发生变化，请刷新后重试")
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("UNSUPPORTED", message)
    }

    pub fn windows(context: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::new("WINDOWS_ERROR", context.into()).with_detail(err.to_string())
    }

    pub fn io(context: impl Into<String>, err: std::io::Error) -> Self {
        Self::new("IO_ERROR", context.into()).with_detail(err.to_string())
    }

    pub fn registry(context: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::new("REGISTRY_ERROR", context.into()).with_detail(err.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("code", &self.code)?;
        s.serialize_field("message", &self.message)?;
        s.serialize_field("detail", &self.detail)?;
        s.end()
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::io("文件操作失败", value)
    }
}

pub type AppResult<T> = Result<T, AppError>;
