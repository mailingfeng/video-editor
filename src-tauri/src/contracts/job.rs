use super::AppError;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Probing,
    Preparing,
    Running,
    Validating,
    Committing,
    Succeeded,
    Failed,
    Canceling,
    Canceled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub job_id: String,
    pub version: u64,
    pub state: JobState,
    pub progress: Option<f64>,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub output_path: Option<String>,
    pub error: Option<AppError>,
    pub cleanup_pending: bool,
}
