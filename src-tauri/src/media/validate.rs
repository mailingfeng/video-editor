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
    verify_specifications(plan, &output)?;
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
        || (plan.source.video.timeline.is_none() && v.frame_rate != p.frame_rate)
        || v.frame_count != p.frame_count
    {
        return Err(failed("视频规格或帧数与处理计划不一致"));
    }
    if let Some(source) = &plan.source.video.timeline {
        if v.timeline
            .as_ref()
            .is_none_or(|current| current.timestamp_sha256 != source.timestamp_sha256)
        {
            return Err(failed("逐帧时间戳与输入不一致"));
        }
    }
    if exceeds(
        seconds(v.duration_ticks, v.time_base) - p.video_duration_seconds,
        p.video_tolerance_seconds,
    ) {
        return Err(failed("视频时长超过允许偏差"));
    }
    if exceeds(
        seconds(v.start_pts, v.time_base)
            - seconds(plan.source.video.start_pts, plan.source.video.time_base),
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
        (Some(a), Some(channels), Some(duration), Some(source)) => {
            if a.codec != "aac"
                || a.sample_rate != 48000
                || a.channels != channels
                || exceeds(
                    seconds(a.duration_ticks, a.time_base) - duration,
                    p.audio_tolerance_seconds,
                )
            {
                return Err(failed("音频规格或时长与处理计划不一致"));
            }
            if source.channel_layout.is_some() && source.channel_layout != a.channel_layout {
                return Err(failed("音频声道布局发生变化"));
            }
            let expected = seconds(source.start_pts, source.time_base)
                - seconds(plan.source.video.start_pts, plan.source.video.time_base);
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
