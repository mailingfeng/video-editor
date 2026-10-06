use super::{MediaInfo, Rational};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum MetadataRequest {
    Preserve,
    Override {
        title: Option<String>,
        comment: Option<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartJobRequest {
    pub input_path: String,
    pub output_directory: String,
    pub preset_id: String,
    pub metadata: MetadataRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryIdentity {
    pub volume_id: String,
    pub file_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutputWorkspace {
    pub job_id: String,
    pub directory: String,
    pub temp_path: String,
    pub final_path: String,
    pub record_path: String,
    pub directory_identity: DirectoryIdentity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationPolicy {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub frame_count: u64,
    pub video_duration_seconds: f64,
    pub audio_duration_seconds: Option<f64>,
    pub channels: Option<u8>,
    pub video_tolerance_seconds: f64,
    pub audio_tolerance_seconds: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingPlan {
    pub job_id: String,
    pub source: MediaInfo,
    pub preset_id: String,
    pub version: u32,
    pub args: Vec<String>,
    pub workspace: OutputWorkspace,
    pub policy: ValidationPolicy,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSummary {
    pub preset_id: String,
    pub version: u32,
    pub title: String,
    pub evidence_status: String,
}
