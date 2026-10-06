use crate::contracts::{JobSnapshot, JobState};
#[derive(Default)]
pub struct ProgressParser {
    frame: Option<u64>,
}
impl ProgressParser {
    pub fn push(&mut self, line: &str) -> Option<u64> {
        let (key, value) = line.split_once('=')?;
        match key {
            "frame" => {
                self.frame = value.trim().parse().ok();
                None
            }
            "progress" if matches!(value, "continue" | "end") => self.frame.take(),
            _ => None,
        }
    }
}
pub fn apply_progress(snapshot: &mut JobSnapshot, frames: u64, total: u64) -> bool {
    if snapshot.state != JobState::Running || total == 0 {
        return false;
    }
    let value = (frames as f64 / total as f64).clamp(0.0, 0.99);
    if value <= snapshot.progress.unwrap_or(0.0) {
        return false;
    }
    snapshot.progress = Some(value);
    snapshot.version += 1;
    true
}
