use crate::contracts::{JobSnapshot, JobState};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogExcerpt {
    pub text: String,
    pub truncated: bool,
}
#[derive(Default)]
pub struct LogTail {
    text: String,
    truncated: bool,
}
impl LogTail {
    pub fn append(&mut self, line: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.append_at(now, line);
    }
    fn append_at(&mut self, time_ms: u64, line: &str) {
        // Reports use Beijing time, matching the fixed license deadline.
        let offset = UtcOffset::from_hms(8, 0, 0).expect("valid Beijing offset");
        let timestamp = OffsetDateTime::from_unix_timestamp((time_ms / 1000) as i64)
            .ok()
            .and_then(|date| date.checked_to_offset(offset))
            .map(|date| {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    date.year(),
                    date.month() as u8,
                    date.day(),
                    date.hour(),
                    date.minute(),
                    date.second(),
                )
            })
            .unwrap_or_else(|| "时间未知".into());
        self.text.push_str(&format!("[{timestamp}] {line}"));
        self.text.push('\n');
        if self.text.len() > 65536 {
            let mut cut = self.text.len() - 65536;
            while !self.text.is_char_boundary(cut) {
                cut += 1;
            }
            self.text.drain(..cut);
            self.truncated = true;
        }
    }
    pub fn excerpt(&self) -> LogExcerpt {
        LogExcerpt {
            text: self.text.clone(),
            truncated: self.truncated,
        }
    }
    pub fn from_excerpt(log: LogExcerpt) -> Self {
        Self {
            text: log.text,
            truncated: log.truncated,
        }
    }
    pub(crate) fn from_legacy(snapshot: &JobSnapshot) -> Self {
        // Old reports mixed commands and stderr with user messages. Rebuild
        // from trusted result fields instead of filtering arbitrary tool text.
        let mut log = Self::default();
        log.append_at(snapshot.started_at_ms, "开始检查视频");
        let ended = snapshot.ended_at_ms.unwrap_or(snapshot.started_at_ms);
        log.append_at(ended, phase_message(snapshot.state));
        if let Some(error) = &snapshot.error {
            log.append_at(ended, &error.for_user().message);
        }
        if snapshot.cleanup_pending {
            log.append_at(ended, "临时文件尚未完全清理，应用下次启动时会重试。");
        }
        log
    }
}

pub(crate) fn phase_message(state: JobState) -> &'static str {
    match state {
        JobState::Probing => "开始检查视频",
        JobState::Preparing => "准备任务",
        JobState::Running => "开始处理视频",
        JobState::Validating => "校验结果",
        JobState::Committing => "保存结果",
        JobState::Succeeded => "处理完成，结果已保存",
        JobState::Failed => "处理失败",
        JobState::Canceling => "正在取消处理",
        JobState::Canceled => "处理已取消",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_dates_follow_beijing_midnight_leap_day_and_new_year() {
        for (time_ms, timestamp) in [
            (0, "1970-01-01 08:00:00"),
            (1_709_135_999_000, "2024-02-28 23:59:59"),
            (1_709_136_000_000, "2024-02-29 00:00:00"),
            (1_735_660_799_000, "2024-12-31 23:59:59"),
            (1_735_660_800_000, "2025-01-01 00:00:00"),
        ] {
            let mut log = LogTail::default();
            log.append_at(time_ms, "检查视频");
            assert_eq!(log.excerpt().text, format!("[{timestamp}] 检查视频\n"));
        }
    }
}
