use serde::{Deserialize, Serialize};
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
        self.text.push_str(line);
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
}
