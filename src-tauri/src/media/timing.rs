use super::{
    probe::{file_identity, read_timeline},
    timeline::Timeline,
};
use crate::{contracts::*, native::process::MediaRunner};
use std::path::Path;
use tokio_util::sync::CancellationToken;

const EXACT_INTEGER: i128 = (1_i128 << 53) - 1;

#[derive(Debug, Clone)]
pub struct TargetTiming {
    pub video_time_base: Rational,
    pub video_start_pts: i64,
    pub video_duration_ticks: i64,
    pub frame_rate: Rational,
    pub audio_start_seconds: Option<f64>,
    pub audio_duration_seconds: Option<f64>,
    pub audio_samples_48k: Option<u64>,
}

pub(crate) fn timing_error() -> AppError {
    AppError::new(
        ErrorCode::UnsupportedInput,
        "视频时间轴无法安全表示本次变化",
    )
}

pub(crate) fn speed_ratio(policy: &TimingPolicy) -> Result<(i128, i128), AppError> {
    match policy {
        TimingPolicy::Preserve => Ok((1, 1)),
        TimingPolicy::UniformSpeed { speed } if speed.num > 0 && speed.den > 0 => {
            let divisor = gcd(speed.num as u128, speed.den as u128) as i128;
            Ok((speed.num as i128 / divisor, speed.den as i128 / divisor))
        }
        _ => Err(timing_error()),
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn exact(value: i128) -> Result<i64, AppError> {
    if value.abs() > EXACT_INTEGER {
        return Err(timing_error());
    }
    i64::try_from(value).map_err(|_| timing_error())
}

fn multiply(values: &[i128]) -> Result<i128, AppError> {
    values.iter().try_fold(1_i128, |total, value| {
        total.checked_mul(*value).ok_or_else(timing_error)
    })
}

pub(crate) fn video_target(
    source: &VideoInfo,
    policy: &TimingPolicy,
) -> Result<(Rational, i64, i64, Rational), AppError> {
    let (p, q) = speed_ratio(policy)?;
    if source.time_base.num <= 0
        || source.time_base.den == 0
        || source.duration_ticks <= 0
        || source.frame_rate.num <= 0
        || source.frame_rate.den == 0
    {
        return Err(timing_error());
    }
    if matches!(policy, TimingPolicy::Preserve) {
        return Ok((
            source.time_base,
            source.start_pts,
            source.duration_ticks,
            source.frame_rate,
        ));
    }
    let timescale = multiply(&[source.time_base.den as i128, p])?;
    if timescale > i32::MAX as i128 {
        return Err(timing_error());
    }
    let source_end = source
        .start_pts
        .checked_add(source.duration_ticks)
        .ok_or_else(timing_error)?;
    // settb rescales input PTS before setpts; its intermediate integers must
    // also remain exactly representable by FFmpeg's expression evaluator.
    exact(multiply(&[
        source_end as i128,
        source.time_base.num as i128,
        p,
    ])?)?;
    let start = project_video_pts(source, policy, source.start_pts)?;
    let duration = project_video_duration(source, policy, source.duration_ticks)?;
    exact(start as i128 + duration as i128)?;
    let numerator = multiply(&[source.frame_rate.num as i128, p])?;
    let denominator = multiply(&[source.frame_rate.den as i128, q])?;
    let divisor = gcd(numerator as u128, denominator as u128) as i128;
    let rate = Rational {
        num: i64::try_from(numerator / divisor).map_err(|_| timing_error())?,
        den: u64::try_from(denominator / divisor).map_err(|_| timing_error())?,
    };
    Ok((
        Rational {
            num: 1,
            den: timescale as u64,
        },
        start,
        duration,
        rate,
    ))
}

pub fn project_video_pts(
    source: &VideoInfo,
    policy: &TimingPolicy,
    pts: i64,
) -> Result<i64, AppError> {
    if matches!(policy, TimingPolicy::Preserve) {
        return Ok(pts);
    }
    let (p, q) = speed_ratio(policy)?;
    if source.time_base.num <= 0 || source.time_base.den == 0 {
        return Err(timing_error());
    }
    let anchor = exact(multiply(&[
        source.start_pts as i128,
        source.time_base.num as i128,
        p,
    ])?)?;
    let relative = exact(multiply(&[
        pts as i128 - source.start_pts as i128,
        source.time_base.num as i128,
        q,
    ])?)?;
    exact(anchor as i128 + relative as i128)
}

pub(crate) fn project_video_duration(
    source: &VideoInfo,
    policy: &TimingPolicy,
    duration: i64,
) -> Result<i64, AppError> {
    if matches!(policy, TimingPolicy::Preserve) {
        return Ok(duration);
    }
    let (_, q) = speed_ratio(policy)?;
    exact(multiply(&[
        duration as i128,
        source.time_base.num as i128,
        q,
    ])?)
}

pub fn target_timing(source: &MediaInfo, policy: &TimingPolicy) -> Result<TargetTiming, AppError> {
    let (video_time_base, video_start_pts, video_duration_ticks, frame_rate) =
        video_target(&source.video, policy)?;
    let mut target = TargetTiming {
        video_time_base,
        video_start_pts,
        video_duration_ticks,
        frame_rate,
        audio_start_seconds: None,
        audio_duration_seconds: None,
        audio_samples_48k: None,
    };
    if let Some(audio) = &source.audio {
        let (start, duration, samples) = audio_timing(audio, &source.video, policy)?;
        target.audio_start_seconds = Some(start);
        target.audio_duration_seconds = Some(duration);
        target.audio_samples_48k = Some(samples);
    }
    Ok(target)
}

pub async fn expected_timeline(
    runner: &dyn MediaRunner,
    source: &MediaInfo,
    policy: &TimingPolicy,
    cancel: CancellationToken,
) -> Result<Option<VideoTimeline>, AppError> {
    if matches!(policy, TimingPolicy::Preserve) {
        return Ok(source.video.timeline.clone());
    }
    let path = Path::new(&source.identity.canonical_path);
    let ensure_identity = |identity: FileIdentity| {
        if identity == source.identity {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::InputChanged,
                "输入文件在校验期间发生变化",
            ))
        }
    };
    ensure_identity(file_identity(path, &cancel).await?)?;
    let report = read_timeline(
        runner,
        &source.identity.canonical_path,
        source.video.stream_index,
        Timeline::with_timing(&source.video, policy)?,
        false,
        cancel.clone(),
    )
    .await?;
    ensure_identity(file_identity(path, &cancel).await?)?;
    if report.count != source.video.frame_count {
        return Err(timing_error());
    }
    Ok(Some(report.timeline))
}

pub(crate) fn audio_timing(
    audio: &AudioInfo,
    video: &VideoInfo,
    policy: &TimingPolicy,
) -> Result<(f64, f64, u64), AppError> {
    let (p, q) = speed_ratio(policy)?;
    if audio.time_base.num <= 0 || audio.time_base.den == 0 || audio.duration_ticks <= 0 {
        return Err(timing_error());
    }
    let v0 = video.start_pts as f64 * video.time_base.num as f64 / video.time_base.den as f64;
    let a0 = audio.start_pts as f64 * audio.time_base.num as f64 / audio.time_base.den as f64;
    let start = v0 + (a0 - v0) * q as f64 / p as f64;
    let numerator = multiply(&[
        audio.duration_ticks as i128,
        audio.time_base.num as i128,
        q,
        48000,
    ])?;
    let denominator = multiply(&[audio.time_base.den as i128, p])?;
    let rounded = numerator
        .checked_add(denominator / 2)
        .ok_or_else(timing_error)?
        / denominator;
    let samples = u64::try_from(rounded).map_err(|_| timing_error())?;
    if !start.is_finite() || samples == 0 || rounded > EXACT_INTEGER {
        return Err(timing_error());
    }
    Ok((
        start,
        numerator as f64 / denominator as f64 / 48000.0,
        samples,
    ))
}
