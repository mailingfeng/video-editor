use crate::contracts::{AppError, ErrorCode, Rational, VideoTimeline};
use sha2::{Digest, Sha256};

pub struct Timeline {
    base: Rational,
    stream_end: Option<i64>,
    first: Option<i64>,
    last: i64,
    last_duration: Option<i64>,
    min_interval: Option<i64>,
    max_interval: i64,
    count: u64,
    timestamps: Sha256,
}

pub struct TimelineReport {
    pub count: u64,
    pub start: i64,
    pub duration: i64,
    pub rate: Rational,
    pub timeline: VideoTimeline,
}

impl Timeline {
    pub fn new(base: Rational, stream_end: Option<i64>) -> Self {
        Self {
            base,
            stream_end,
            first: None,
            last: 0,
            last_duration: None,
            min_interval: None,
            max_interval: 0,
            count: 0,
            timestamps: Sha256::new(),
        }
    }

    pub fn push(&mut self, line: &str) -> Result<(), AppError> {
        if line.is_empty()
            || (!line.contains("best_effort_timestamp=")
                && !line.contains("pts=")
                && line.contains("side_data_type="))
        {
            return Ok(());
        }
        let fields: std::collections::HashMap<_, _> = line
            .split('|')
            .filter_map(|part| part.split_once('='))
            .collect();
        let pts = fields
            .get("best_effort_timestamp")
            .or_else(|| fields.get("pts"))
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| unsupported(format!("第 {} 帧缺少有效时间戳", self.count + 1)))?;
        let duration = fields
            .get("duration")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|d| *d > 0)
            .or_else(|| {
                fields
                    .get("pkt_duration")
                    .and_then(|s| s.parse::<i64>().ok())
                    .filter(|d| *d > 0)
            });
        let first = *self.first.get_or_insert(pts);
        if self.count > 0 {
            let interval = pts
                .checked_sub(self.last)
                .filter(|d| *d > 0)
                .ok_or_else(|| {
                    unsupported(format!(
                        "第 {} 帧时间戳重复、逆序或超出范围",
                        self.count + 1
                    ))
                })?;
            self.min_interval = Some(self.min_interval.map_or(interval, |min| min.min(interval)));
            self.max_interval = self.max_interval.max(interval);
        }
        let relative = (pts as i128 - first as i128) * self.base.num as i128;
        // Hash reduced rational seconds, so equivalent time bases compare identically.
        let divisor = gcd(relative as u128, self.base.den as u128);
        self.timestamps
            .update((relative / divisor as i128).to_le_bytes());
        self.timestamps
            .update((self.base.den as u128 / divisor).to_le_bytes());
        self.last = pts;
        self.last_duration = duration;
        self.count += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<TimelineReport, AppError> {
        let first = self.first.ok_or_else(|| unsupported("未读取到视频帧"))?;
        let constant_intervals = self
            .min_interval
            .is_some_and(|min| self.max_interval - min <= 1);
        let last_duration = self
            .last_duration
            .or_else(|| {
                self.stream_end
                    .and_then(|end| end.checked_sub(self.last))
                    .filter(|d| *d > 0)
            })
            .or_else(|| constant_intervals.then_some(self.max_interval))
            .ok_or_else(|| unsupported("最后一帧时长缺失，且轨道终点无法确认"))?;
        let duration = self
            .last
            .checked_add(last_duration)
            .and_then(|end| end.checked_sub(first))
            .filter(|d| *d > 0)
            .ok_or_else(|| unsupported("视频时长无效或超出范围"))?;
        let numerator = self.count as u128 * self.base.den as u128;
        let denominator = duration as u128 * self.base.num as u128;
        let divisor = gcd(numerator, denominator);
        let rate = Rational {
            num: i64::try_from(numerator / divisor).map_err(|_| unsupported("帧率超出范围"))?,
            den: u64::try_from(denominator / divisor).map_err(|_| unsupported("帧率超出范围"))?,
        };
        Ok(TimelineReport {
            count: self.count,
            start: first,
            duration,
            rate,
            timeline: VideoTimeline {
                variable_frame_rate: self.count > 1 && !constant_intervals,
                timestamp_sha256: format!("{:x}", self.timestamps.finalize()),
            },
        })
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn unsupported(detail: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UnsupportedInput, "无法确认完整视频时间轴").detail(detail)
}
