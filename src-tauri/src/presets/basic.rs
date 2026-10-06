use crate::{contracts::*, media::probe::validate_input};
pub fn list_presets() -> Vec<PresetSummary> {
    vec![PresetSummary {
        preset_id: "basic-transcode-v1".into(),
        version: 1,
        title: "基础转换".into(),
        evidence_status: "本地规格校验；样本附加滤镜待校准".into(),
    }]
}
pub fn build_plan(
    info: &MediaInfo,
    request: &StartJobRequest,
    workspace: OutputWorkspace,
    job_id: &str,
) -> Result<ProcessingPlan, AppError> {
    if request.preset_id != "basic-transcode-v1" {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "未知或尚未校准的预设",
        ));
    }
    validate_input(info)?;
    let v = &info.video;
    if v.frame_count == 0 || v.duration_ticks <= 0 {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "完整视频时间轴未确认",
        ));
    }
    let mut args: Vec<String> = ["-hide_banner", "-n", "-copyts", "-i"]
        .into_iter()
        .map(String::from)
        .collect();
    args.push(info.identity.canonical_path.clone());
    let mut pair = |key: &str, value: String| {
        args.push(key.into());
        args.push(value);
    };
    pair("-map", format!("0:{}", v.stream_index));
    if let Some(a) = &info.audio {
        pair("-map", format!("0:{}", a.stream_index));
    }
    pair("-c:v", "libx264".into());
    pair("-preset", "veryfast".into());
    pair("-profile:v", "main".into());
    pair("-pix_fmt", "yuv420p".into());
    pair(
        "-g",
        (10.0 * v.frame_rate.num as f64 / v.frame_rate.den as f64)
            .round()
            .to_string(),
    );
    pair(
        "-keyint_min",
        (10.0 * v.frame_rate.num as f64 / v.frame_rate.den as f64)
            .round()
            .to_string(),
    );
    pair("-sc_threshold", "0".into());
    pair("-bf", "0".into());
    pair("-fps_mode:v", "passthrough".into());
    pair("-video_track_timescale", v.time_base.den.to_string());
    pair("-avoid_negative_ts", "disabled".into());
    match v.bit_rate.filter(|b| *b > 0) {
        Some(b) => pair("-b:v", b.to_string()),
        None => pair("-crf", "18".into()),
    }
    for (flag, value) in [
        ("-color_range", &v.color_range),
        ("-colorspace", &v.color_space),
        ("-color_primaries", &v.color_primaries),
        ("-color_trc", &v.color_transfer),
    ] {
        if let Some(value) = value {
            pair(flag, value.clone());
        }
    }
    if let Some(a) = &info.audio {
        pair("-c:a", "aac".into());
        pair("-ar", "48000".into());
        pair("-ac", a.channels.to_string());
        pair(
            "-b:a",
            a.bit_rate
                .filter(|b| *b > 0)
                .map(|b| b.clamp(64000, 128000))
                .unwrap_or(96000)
                .to_string(),
        );
    }
    pair("-map_metadata", "-1".into());
    let (title, comment) = match &request.metadata {
        MetadataRequest::Preserve => (
            info.title.clone(),
            Some(match info.comment.as_deref() {
                Some(c) if !c.is_empty() => format!("{c}; video-editor/basic-transcode-v1"),
                _ => "video-editor/basic-transcode-v1".into(),
            }),
        ),
        MetadataRequest::Override { title, comment } => (title.clone(), comment.clone()),
    };
    if let Some(value) = title {
        pair("-metadata", format!("title={value}"));
    }
    if let Some(value) = comment {
        pair("-metadata", format!("comment={value}"));
    }
    pair("-movflags", "+faststart".into());
    pair("-progress", "pipe:1".into());
    args.push("-nostats".into());
    if info.audio.is_none() {
        args.push("-an".into());
    }
    args.push(workspace.temp_path.clone());
    let frame_seconds = v.frame_rate.den as f64 / v.frame_rate.num as f64;
    let video_duration = v.duration_ticks as f64 * v.time_base.num as f64 / v.time_base.den as f64;
    let policy = ValidationPolicy {
        width: v.width,
        height: v.height,
        frame_rate: v.frame_rate,
        frame_count: v.frame_count,
        video_duration_seconds: video_duration,
        audio_duration_seconds: info
            .audio
            .as_ref()
            .map(|a| a.duration_ticks as f64 * a.time_base.num as f64 / a.time_base.den as f64),
        channels: info.audio.as_ref().map(|a| a.channels),
        video_tolerance_seconds: frame_seconds,
        audio_tolerance_seconds: frame_seconds.max(0.05),
    };
    Ok(ProcessingPlan {
        job_id: job_id.into(),
        source: info.clone(),
        preset_id: request.preset_id.clone(),
        version: 1,
        args,
        workspace,
        policy,
    })
}
