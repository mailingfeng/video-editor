use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidSettings,
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
        if self.code == ErrorCode::ValidationFailed {
            // Only application-owned reasons may enter reports. Tool stderr and
            // arbitrary details can contain private processing expressions.
            // Accept the public form too: history loading sanitizes errors again.
            let reason = self
                .message
                .strip_prefix("处理结果未通过完整性检查：")
                .and_then(|text| text.strip_suffix("。未保存新视频。"))
                .unwrap_or(&self.message);
            if VALIDATION_REASONS.contains(&reason) {
                return Self::new(
                    self.code,
                    format!("处理结果未通过完整性检查：{reason}。未保存新视频。"),
                );
            }
        }
        let message = match self.code {
            ErrorCode::InvalidSettings => "处理设置无效，请恢复推荐设置后重试。",
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

const VALIDATION_REASONS: &[&str] = &[
    "输出不是普通文件",
    "输出为空",
    "无法读取临时输出",
    "无法读取输出媒体信息",
    "无法确认目标时间轴",
    "输出完整解码失败",
    "无法复核输出文件身份",
    "校验期间输出发生变化",
    "视频编码与处理计划不一致",
    "视频像素格式与处理计划不一致",
    "视频尺寸与处理计划不一致",
    "视频帧率与处理计划不一致",
    "视频帧数与处理计划不一致",
    "视频规格或帧数与处理计划不一致",
    "逐帧时间戳与输入不一致",
    "视频时长超过允许偏差",
    "视频起点发生变化",
    "已知颜色标记与计划不一致",
    "音频编码与处理计划不一致",
    "音频采样率与处理计划不一致",
    "音频声道数与处理计划不一致",
    "音频时长超过允许偏差",
    "音频规格或时长与处理计划不一致",
    "音频声道布局发生变化",
    "音频有效采样数无法确认",
    "音频有效采样数与目标不一致",
    "音视频相对起点发生变化",
    "音轨存在性与计划不一致",
    "目标音频时长缺失",
    "目标音频起点缺失",
    "变速校验缺少目标时间轴",
    "待保存媒体无效",
];
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for AppError {}
