use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnsupportedInput,
    DamagedMedia,
    ToolMissing,
    OutputPermission,
    DiskFull,
    ProcessExit,
    ValidationFailed,
    OutputConflict,
    InputChanged,
    Busy,
    Canceled,
    CleanupPending,
    LicenseExpired,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<String>,
}
impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details = Some(detail.into());
        self
    }
    pub fn io(error: std::io::Error) -> Self {
        let code = if error.raw_os_error() == Some(28) || error.raw_os_error() == Some(112) {
            ErrorCode::DiskFull
        } else {
            ErrorCode::OutputPermission
        };
        Self::new(code, error.to_string())
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for AppError {}
