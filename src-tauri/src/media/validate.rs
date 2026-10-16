use super::probe::{check_exit, file_identity, probe_input, run_capture};
use crate::{
    contracts::*,
    native::{
        process::{MediaRunner, RunSpec, StdoutPolicy},
        tools::Tool,
    },
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio_util::sync::CancellationToken;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub output_path: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub decoded_frames: u64,
    pub video_duration_seconds: f64,
    pub audio_duration_seconds: Option<f64>,
    pub checks: Vec<String>,
    pub color_information_complete: bool,
}
pub async fn validate_output(
    runner: &dyn MediaRunner,
    plan: &ProcessingPlan,
    cancel: CancellationToken,
) -> Result<ValidationResult, AppError> {
    verify_input_identity(plan, &cancel).await?;
    let path = Path::new(&plan.workspace.temp_path);
    if !tokio::fs::metadata(path)
        .await
        .map_err(validation_io)?
        .is_file()
    {
        return Err(failed("输出不是普通文件"));
    }
    let output = probe_input(runner, path, cancel.clone())
        .await
        .map_err(as_validation)?;
    if output.identity.size_bytes == 0 {
        return Err(failed("输出为空"));
    }
    let expected =
        super::timing::expected_timeline(runner, &plan.source, &plan.timing, cancel.clone())
            .await
            .map_err(as_validation)?;
    verify_specifications_against(plan, &output, expected.as_ref())?;
    let args = ["-v", "error", "-xerror", "-i"]
        .into_iter()
        .map(String::from)
        .chain([plan.workspace.temp_path.clone()])
        .chain(
            [
                "-map",
                "0:v:0",
                "-map",
                "0:a:0?",
                "-fps_mode:v",
                "passthrough",
                "-enc_time_base:v",
                "demux",
                "-f",
                "null",
                "-",
            ]
            .into_iter()
            .map(String::from),
        )
        .collect();
    let decoded = run_capture(
        runner,
        RunSpec {
            tool: Tool::Ffmpeg,
            args,
            stdout_policy: StdoutPolicy::Discard,
        },
        cancel.clone(),
    )
    .await
    .map_err(as_validation)?;
    check_exit(&decoded).map_err(as_validation)?;
    verify_input_identity(plan, &cancel).await?;
    if file_identity(path, &cancel).await.map_err(as_validation)? != output.identity {
        return Err(failed("校验期间输出发生变化"));
    }
    Ok(ValidationResult {
        output_path: plan.workspace.temp_path.clone(),
        input_sha256: plan.source.identity.sha256.clone(),
        output_sha256: output.identity.sha256,
        decoded_frames: output.video.frame_count,
        video_duration_seconds: seconds(output.video.duration_ticks, output.video.time_base),
        audio_duration_seconds: output
            .audio
            .as_ref()
            .map(|a| seconds(a.duration_ticks, a.time_base)),
        checks: vec![
            "完整解码".into(),
            "输出规格".into(),
            "帧数与时间轴".into(),
            "输入身份".into(),
        ],
        color_information_complete: [
            &plan.source.video.color_range,
            &plan.source.video.color_space,
            &plan.source.video.color_primaries,
            &plan.source.video.color_transfer,
        ]
        .iter()
        .all(|c| c.is_some()),
    })
}
async fn verify_input_identity(
    plan: &ProcessingPlan,
    cancel: &CancellationToken,
) -> Result<(), AppError> {
    let current = file_identity(Path::new(&plan.source.identity.canonical_path), cancel)
        .await
        .map_err(|e| {
            if e.code == ErrorCode::Canceled {
                e
            } else {
                AppError::new(ErrorCode::InputChanged, "输入已改变或无法读取").detail(e.to_string())
            }
        })?;
    if current != plan.source.identity {
        return Err(AppError::new(
            ErrorCode::InputChanged,
            "输入文件在处理期间发生变化",
        ));
    }
    Ok(())
}
pub fn verify_specifications(plan: &ProcessingPlan, output: &MediaInfo) -> Result<(), AppError> {
    if !matches!(plan.timing, TimingPolicy::Preserve) {
        return Err(failed("变速校验缺少目标时间轴"));
    }
    verify_specifications_against(plan, output, plan.source.video.timeline.as_ref())
}

pub fn verify_specifications_against(
    plan: &ProcessingPlan,
    output: &MediaInfo,
    expected: Option<&VideoTimeline>,
) -> Result<(), AppError> {
    let expected = if matches!(plan.timing, TimingPolicy::Preserve) {
        plan.source.video.timeline.as_ref()
    } else {
        expected
    };
    if !matches!(plan.timing, TimingPolicy::Preserve) && expected.is_none() {
        return Err(failed("变速校验缺少目标时间轴"));
    }
    let target = super::timing::target_timing(&plan.source, &plan.timing).map_err(as_validation)?;
    let p = &plan.policy;
    let v = &output.video;
    // Decoders report full-range H.264 using the deprecated yuvj420p alias.
    // It is still 8-bit 4:2:0; range must also match the known source range.
    let pixel_matches = v.pixel_format == "yuv420p"
        || (v.pixel_format == "yuvj420p"
            && v.bit_depth == 8
            && v.color_range.as_deref() == Some("pc")
            && plan.source.video.color_range.as_deref() == Some("pc"));
    if v.codec != "h264"
        || !pixel_matches
        || v.width != p.width
        || v.height != p.height
        || (expected.is_none() && v.frame_rate != target.frame_rate)
        || (!matches!(plan.timing, TimingPolicy::Preserve) && !frame_rate_matches_timeline(v))
        || v.frame_count != p.frame_count
    {
        return Err(failed("视频规格或帧数与处理计划不一致"));
    }
    if let Some(source) = expected {
        if v.timeline
            .as_ref()
            .is_none_or(|current| current.timestamp_sha256 != source.timestamp_sha256)
        {
            return Err(failed("逐帧时间戳与输入不一致"));
        }
    }
    if exceeds(
        seconds(v.duration_ticks, v.time_base)
            - seconds(target.video_duration_ticks, target.video_time_base),
        p.video_tolerance_seconds,
    ) {
        return Err(failed("视频时长超过允许偏差"));
    }
    if exceeds(
        seconds(v.start_pts, v.time_base) - seconds(target.video_start_pts, target.video_time_base),
        p.video_tolerance_seconds,
    ) {
        return Err(failed("视频起点发生变化"));
    }
    for (original, current) in [
        (&plan.source.video.color_range, &v.color_range),
        (&plan.source.video.color_space, &v.color_space),
        (&plan.source.video.color_primaries, &v.color_primaries),
        (&plan.source.video.color_transfer, &v.color_transfer),
    ] {
        if original.is_some() && original != current {
            return Err(failed("已知颜色标记与计划不一致"));
        }
    }
    match (
        &output.audio,
        p.channels,
        p.audio_duration_seconds,
        &plan.source.audio,
    ) {
        (None, None, None, None) => {}
        (Some(a), Some(channels), Some(_duration), Some(source)) => {
            if a.codec != "aac"
                || a.sample_rate != 48000
                || a.channels != channels
                || exceeds(
                    seconds(a.duration_ticks, a.time_base)
                        - target
                            .audio_duration_seconds
                            .ok_or_else(|| failed("目标音频时长缺失"))?,
                    p.audio_tolerance_seconds,
                )
            {
                return Err(failed("音频规格或时长与处理计划不一致"));
            }
            if source.channel_layout.is_some() && source.channel_layout != a.channel_layout {
                return Err(failed("音频声道布局发生变化"));
            }
            if let Some(samples) = p.audio_target_samples {
                if a.duration_ticks <= 0 || a.time_base.num <= 0 || a.time_base.den == 0 {
                    return Err(failed("音频有效采样数无法确认"));
                }
                let mut actual = seconds(a.duration_ticks, a.time_base) * 48000.0;
                // Positive-start AAC can expose the encoder's leading packet
                // instead of an MP4 edit that skips it. Count effective audio,
                // recognizing only the exact native encoder delay, never the
                // broader mux tolerance.
                let expected_start = target
                    .audio_start_seconds
                    .ok_or_else(|| failed("目标音频起点缺失"))?;
                let delay = (expected_start - seconds(a.start_pts, a.time_base)) * 48000.0;
                if a.start_pts >= 0 && (delay - 1024.0).abs() <= 1.0 {
                    actual -= 1024.0;
                }
                if !actual.is_finite() || (actual - samples as f64).abs() > 1.0 + 1e-8 {
                    return Err(failed("音频有效采样数与目标不一致"));
                }
            }
            let expected = target
                .audio_start_seconds
                .ok_or_else(|| failed("目标音频起点缺失"))?
                - seconds(target.video_start_pts, target.video_time_base);
            let actual = seconds(a.start_pts, a.time_base) - seconds(v.start_pts, v.time_base);
            if exceeds(actual - expected, p.audio_tolerance_seconds) {
                return Err(failed("音视频相对起点发生变化"));
            }
        }
        _ => return Err(failed("音轨存在性与计划不一致")),
    }
    Ok(())
}
fn seconds(ticks: i64, base: Rational) -> f64 {
    ticks as f64 * base.num as f64 / base.den as f64
}
fn exceeds(difference: f64, tolerance: f64) -> bool {
    !difference.is_finite() || difference.abs() > tolerance + 1e-8
}
fn failed(message: &str) -> AppError {
    AppError::new(ErrorCode::ValidationFailed, message)
}
fn validation_io(e: std::io::Error) -> AppError {
    failed("无法读取临时输出").detail(e.to_string())
}
fn as_validation(e: AppError) -> AppError {
    if matches!(e.code, ErrorCode::Canceled | ErrorCode::ToolMissing) {
        e
    } else {
        let details = match e.details {
            Some(detail) => format!("{}：{detail}", e.message),
            None => e.message,
        };
        failed("输出校验未通过").detail(details)
    }
}

fn frame_rate_matches_timeline(video: &VideoInfo) -> bool {
    if video.frame_rate.num <= 0
        || video.frame_rate.den == 0
        || video.time_base.num <= 0
        || video.time_base.den == 0
        || video.duration_ticks <= 0
    {
        return false;
    }
    fn reduce(mut a: u128, mut b: u128) -> (u128, u128) {
        let (n, d) = (a, b);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        (n / a, d / a)
    }
    reduce(
        video.frame_count as u128 * video.time_base.den as u128,
        video.duration_ticks as u128 * video.time_base.num as u128,
    ) == reduce(video.frame_rate.num as u128, video.frame_rate.den as u128)
}
