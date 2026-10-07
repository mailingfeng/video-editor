use super::{JobSnapshot, MediaInfo, MetadataRequest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchSettings {
    pub output_directory: String,
    pub preset_id: String,
    pub metadata: MetadataRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueItemState {
    Waiting,
    Started,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub item_id: String,
    pub input_path: String,
    pub state: QueueItemState,
    pub job_id: Option<String>,
    pub snapshot: Option<JobSnapshot>,
    pub media: Option<MediaInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub version: u64,
    pub running: bool,
    pub items: Vec<QueueItem>,
}
