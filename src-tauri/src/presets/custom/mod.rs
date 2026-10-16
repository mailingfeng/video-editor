mod audio;
mod video;
use crate::contracts::*;
use sha2::{Digest, Sha256};

pub const ID: &str = "custom-variation-v1";
const KEYS: [&str; 14] = [
    "foregroundScale",
    "backgroundBlur",
    "dynamicOffset",
    "cropScale",
    "hue",
    "saturation",
    "tone",
    "noise",
    "softness",
    "pitch",
    "timbre",
    "volume",
    "speed",
    "encoding",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenCustomParameters {
    settings: CustomVariationSettings,
    random: [[u8; 32]; 14],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomEncoding {
    pub crf: u8,
    pub gop: u32,
    pub audio_bit_rate: Option<u64>,
}

impl FrozenCustomParameters {
    pub fn resolve(settings: &CustomVariationSettings, job_id: &str) -> Result<Self, AppError> {
        settings.validate()?;
        let random =
            std::array::from_fn(|i| Sha256::digest(format!("{ID}:{job_id}:{}", KEYS[i])).into());
        Ok(Self {
            settings: settings.clone(),
            random,
        })
    }
    fn fraction(&self, effect: usize, index: usize) -> f64 {
        f64::from(self.random[effect][index]) / 255.0
    }
    fn weight(level: u8) -> f64 {
        f64::from(level) / 5.0
    }
    fn speed(&self) -> Rational {
        let level = self.settings.speed;
        if level == 0 {
            return Rational { num: 1, den: 1 };
        }
        let delta = i64::from(5 * level) + i64::from(self.random[12][0] % (5 * level + 1));
        let mut num = 1000
            + if self.random[12][1] & 1 == 0 {
                delta
            } else {
                -delta
            };
        let mut den = 1000;
        let (mut a, mut b) = (num, den);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        num /= a;
        den /= a;
        Rational {
            num,
            den: den as u64,
        }
    }
    pub fn timing(&self) -> TimingPolicy {
        if self.settings.speed == 0 {
            TimingPolicy::Preserve
        } else {
            TimingPolicy::UniformSpeed {
                speed: self.speed(),
            }
        }
    }
    pub fn encoding(&self, source: &MediaInfo) -> Result<Option<CustomEncoding>, AppError> {
        if self.settings.encoding == 0 {
            return Ok(None);
        }
        let weight = Self::weight(self.settings.encoding);
        let crf = (18.0 + (weight * f64::from(2 + self.random[13][0] % 3)).round())
            .clamp(18.0, 26.0) as u8;
        let frame_rate = source.video.frame_rate;
        if frame_rate.num <= 0 || frame_rate.den == 0 {
            return Err(crate::media::timing::timing_error());
        }
        let basic = (10.0 * frame_rate.num as f64 / frame_rate.den as f64).round();
        let target = 180.0 + f64::from(self.random[13][1] % 141);
        let gop = (basic + (target - basic) * weight.min(1.0)).round();
        if !gop.is_finite() || gop < 1.0 || gop > f64::from(u32::MAX) {
            return Err(crate::media::timing::timing_error());
        }
        Ok(Some(CustomEncoding {
            crf,
            gop: gop as u32,
            audio_bit_rate: source.audio.as_ref().map(|a| {
                192000
                    * if a.channels <= 2 {
                        1
                    } else {
                        u64::from(a.channels)
                    }
            }),
        }))
    }
    pub fn has_audio_effects(&self) -> bool {
        self.settings.pitch > 0 || self.settings.timbre > 0 || self.settings.volume > 0
    }
}
