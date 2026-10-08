use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rational {
    pub num: i64,
    pub den: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub canonical_path: String,
    pub size_bytes: u64,
    pub modified_ns: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub stream_index: u32,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub pixel_format: String,
    pub frame_rate: Rational,
    pub time_base: Rational,
    pub frame_count: u64,
    pub start_pts: i64,
    pub duration_ticks: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<VideoTimeline>,
    pub bit_rate: Option<u64>,
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoTimeline {
    pub variable_frame_rate: bool,
    pub timestamp_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub stream_index: u32,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    pub time_base: Rational,
    pub start_pts: i64,
    pub duration_ticks: i64,
    pub bit_rate: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub identity: FileIdentity,
    pub container: String,
    pub video: VideoInfo,
    pub audio: Option<AudioInfo>,
    pub title: Option<String>,
    pub comment: Option<String>,
}
