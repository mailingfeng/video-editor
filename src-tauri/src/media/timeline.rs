use crate::contracts::{AppError, ErrorCode, Rational};
pub struct Timeline {
    rate: Rational,
    base: Rational,
    first: Option<i64>,
    last: i64,
    last_duration: i64,
    count: u64,
}
impl Timeline {
    pub fn new(rate: Rational, base: Rational) -> Self {
        Self {
            rate,
            base,
            first: None,
            last: 0,
            last_duration: 0,
            count: 0,
        }
    }
    pub fn push(&mut self, line: &str) -> Result<(), AppError> {
        if line.is_empty()
            || (!line.contains("best_effort_timestamp=") && line.contains("side_data_type="))
        {
            return Ok(());
        }
        let fields: std::collections::HashMap<_, _> = line
            .split('|')
            .filter_map(|part| part.split_once('='))
            .collect();
        let pts = fields
            .get("best_effort_timestamp")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(unsupported)?;
        let duration = fields
            .get("duration")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|d| *d > 0)
            .ok_or_else(unsupported)?;
        let first = *self.first.get_or_insert(pts);
        if self.count > 0 && pts <= self.last {
            return Err(unsupported());
        }
        let scale = self.base.num as i128 * self.rate.num as i128;
        let ideal = self.count as i128 * self.base.den as i128 * self.rate.den as i128;
        let actual = (pts as i128 - first as i128) * scale;
        if (actual - ideal).abs() > scale.abs() {
            return Err(unsupported());
        }
        let expected_duration = self.base.den as i128 * self.rate.den as i128;
        if (duration as i128 * scale - expected_duration).abs() > scale.abs() {
            return Err(unsupported());
        }
        self.last = pts;
        self.last_duration = duration;
        self.count += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<(u64, i64, i64), AppError> {
        let first = self.first.ok_or_else(unsupported)?;
        let duration = self
            .last
            .checked_add(self.last_duration)
            .and_then(|x| x.checked_sub(first))
            .ok_or_else(unsupported)?;
        Ok((self.count, first, duration))
    }
}
fn unsupported() -> AppError {
    AppError::new(
        ErrorCode::UnsupportedInput,
        "无法确认恒定帧率或完整视频时间轴",
    )
}
