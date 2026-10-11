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
    pub fn for_user(&self) -> Self {
        let message = match self.code {
            ErrorCode::UnsupportedInput => "该视频暂不支持处理，请尝试更换视频文件。",
            ErrorCode::DamagedMedia => {
                "无法读取视频文件，请确认文件可以正常播放，或重新获取文件后重试。"
            }
            ErrorCode::ToolMissing => "处理组件不可用，请重新安装软件后重试。",
            ErrorCode::OutputPermission => "无法写入保存位置，请检查目录权限或选择其他位置。",
            ErrorCode::DiskFull => "保存位置的空间不足，请释放空间后重试。",
            ErrorCode::ProcessExit => "视频处理未完成，请重试；若仍失败，请更换视频文件。",
            ErrorCode::ValidationFailed => {
                "处理结果未通过完整性检查，未保存新视频。请重试或更换视频文件。"
            }
            ErrorCode::OutputConflict => "保存位置已存在同名文件，请更换保存位置后重试。",
            ErrorCode::InputChanged => "原视频在处理期间发生变化，请重新添加视频后重试。",
            ErrorCode::Busy => "已有任务正在进行，请等待完成后再试。",
            ErrorCode::Canceled => "处理已取消。",
            ErrorCode::CleanupPending => "临时文件尚未完全清理，应用下次启动时会重试。",
            ErrorCode::LicenseExpired => {
                "使用许可已于 2026-12-31 到期，暂时无法开始处理。请联系软件提供方更新许可。"
            }
        };
        Self::new(self.code, message)
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for AppError {}
