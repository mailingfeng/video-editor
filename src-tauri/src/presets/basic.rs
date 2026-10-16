use super::opaque::hidden;
use super::{combined, content, custom, repeat, sample};
use crate::{contracts::*, media::probe::validate_input};
pub fn list_presets() -> Vec<PresetSummary> {
    vec![
        PresetSummary {
            preset_id: content::ID.into(),
            version: 1,
            title: "内容变化".into(),
            evidence_status: String::new(),
        },
        PresetSummary {
            preset_id: combined::ID.into(),
            version: 2,
            title: "轻微变化".into(),
            evidence_status: String::new(),
        },
        PresetSummary {
            preset_id: custom::ID.into(),
            version: 1,
            title: "自定义变化".into(),
            evidence_status: String::new(),
        },
    ]
}
pub fn build_plan(
    info: &MediaInfo,
    request: &StartJobRequest,
    workspace: OutputWorkspace,
    job_id: &str,
) -> Result<ProcessingPlan, AppError> {
    validate_custom_selection(&request.preset_id, request.custom_settings.as_ref())?;
    let (audio_filter, version) = match request.preset_id.as_str() {
        "basic-transcode-v1" | repeat::ID | content::ID | custom::ID => (None, 1),
        combined::ID => (None, 2),
        sample::ID => (Some(sample::audio_filter()), 1),
        sample::V2_ID => (Some(sample::v2_audio_filter()), 2),
        _ => {
            return Err(AppError::new(
                ErrorCode::UnsupportedInput,
                "未知或尚未校准的预设",
            ))
        }
    };
    let sample_matching = audio_filter.is_some();
    validate_input(info)?;
    let combined =
        (request.preset_id == combined::ID).then(|| combined::Parameters::for_job(job_id));
    let content = (request.preset_id == content::ID).then(|| content::Parameters::for_job(job_id));
    let custom = request
        .custom_settings
        .as_ref()
        .map(|s| custom::FrozenCustomParameters::resolve(s, job_id))
        .transpose()?;
    let timing = custom
        .as_ref()
        .map_or(TimingPolicy::Preserve, |p| p.timing());
    let target = crate::media::timing::target_timing(info, &timing)?;
    let custom_encoding = custom
        .as_ref()
        .map(|p| p.encoding(info))
        .transpose()?
        .flatten();
    let encoding = combined
        .as_ref()
        .map(|p| (p.crf, u32::from(p.gop)))
        .or_else(|| content.as_ref().map(|p| (p.crf, u32::from(p.gop))))
        .or_else(|| custom_encoding.as_ref().map(|p| (p.crf, p.gop)));
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
    pair(
        "-preset",
        if combined.is_some() {
            "ultrafast"
        } else {
            "veryfast"
        }
        .into(),
    );
    pair(
        "-profile:v",
        if combined.is_some() {
            "baseline"
        } else {
            "main"
        }
        .into(),
    );
    pair("-pix_fmt", "yuv420p".into());
    let gop = if let Some((_, gop)) = encoding {
        gop.to_string()
    } else if sample_matching {
        "300".into()
    } else {
        (10.0 * v.frame_rate.num as f64 / v.frame_rate.den as f64)
            .round()
            .to_string()
    };
    pair("-g", gop.clone());
    pair(
        "-keyint_min",
        if encoding.is_some() { "25".into() } else { gop },
    );
    pair("-sc_threshold", "0".into());
    pair("-bf", "0".into());
    pair("-fps_mode:v", "passthrough".into());
    pair(
        "-enc_time_base:v",
        if matches!(timing, TimingPolicy::Preserve) {
            "demux".into()
        } else {
            format!(
                "{}/{}",
                target.video_time_base.num, target.video_time_base.den
            )
        },
    );
    pair(
        "-video_track_timescale",
        target.video_time_base.den.to_string(),
    );
    pair("-avoid_negative_ts", "disabled".into());
    if let Some((crf, _)) = encoding {
        pair("-crf", crf.to_string());
    } else {
        match v.bit_rate.filter(|b| *b > 0) {
            Some(b) => {
                let bitrate = if sample_matching {
                    b.checked_add(b / 4).ok_or_else(|| {
                        AppError::new(ErrorCode::UnsupportedInput, "视频码率超出样本处理支持范围")
                    })?
                } else {
                    b
                };
                pair("-b:v", bitrate.to_string());
            }
            None => pair("-crf", "18".into()),
        }
    }
    if let Some(parameters) = &combined {
        pair("-vf", parameters.video_filter(v.width, v.height));
    } else if let Some(parameters) = &content {
        pair("-vf", parameters.video_filter(v.width, v.height));
    } else if let Some(parameters) = &custom {
        if let Some(filter) = parameters.video_filter(v)? {
            pair("-vf", filter);
        }
    } else if sample_matching {
        pair("-vf", sample::video_filter());
    } else if request.preset_id == repeat::ID {
        pair("-vf", repeat::video_filter(job_id));
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
    if let Some(full_range) = match v.color_range.as_deref() {
        Some("tv") => Some(0),
        Some("pc") => Some(1),
        _ => None,
    } {
        pair(
            "-bsf:v",
            format!("h264_metadata=video_full_range_flag={full_range}"),
        );
    }
    if let Some(a) = &info.audio {
        pair("-c:a", "aac".into());
        pair("-ar", "48000".into());
        pair("-ac", a.channels.to_string());
        if let Some(layout) = &a.channel_layout {
            pair("-channel_layout:a", layout.clone());
        }
        let bitrate_channels = if a.channels <= 2 {
            1
        } else {
            a.channels as u64
        };
        let bitrate = if encoding.is_some() {
            192000 * bitrate_channels
        } else if sample_matching && a.channels <= 2 {
            74000
        } else {
            a.bit_rate
                .filter(|b| *b > 0)
                .map(|b| b.clamp(64000 * bitrate_channels, 128000 * bitrate_channels))
                .unwrap_or(96000 * bitrate_channels)
        };
        pair("-b:a", bitrate.to_string());
        if let Some(parameters) = &combined {
            pair("-af", parameters.audio_filter());
        } else if let Some(parameters) = &content {
            pair("-af", parameters.audio_filter(a));
        } else if let Some(parameters) = &custom {
            if let Some(filter) = parameters.audio_filter(a, v)? {
                pair("-af", filter);
            }
        } else if let Some(filter) = audio_filter {
            // Without an explicit resample, low-rate AAC can silently bypass
            // the 5500 Hz lowpass; automatic conversion depends on sample format.
            let filter = if request.preset_id == sample::V2_ID && a.sample_rate <= 11000 {
                hidden!("aresample=48000,") + &filter
            } else {
                filter
            };
            pair("-af", filter);
        }
    }
    pair("-map_metadata", "-1".into());
    let (title, comment) = match &request.metadata {
        MetadataRequest::Preserve => (
            info.title.clone(),
            Some(match info.comment.as_deref() {
                Some(c) if !c.is_empty() => format!("{c}; video-editor/{}", request.preset_id),
                _ => format!("video-editor/{}", request.preset_id),
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
    let frame_seconds = target.frame_rate.den as f64 / target.frame_rate.num as f64;
    let video_duration = target.video_duration_ticks as f64 * target.video_time_base.num as f64
        / target.video_time_base.den as f64;
    let policy = ValidationPolicy {
        audio_target_samples: custom
            .as_ref()
            .filter(|p| p.has_audio_effects() || !matches!(timing, TimingPolicy::Preserve))
            .and(target.audio_samples_48k),
        width: v.width,
        height: v.height,
        frame_rate: target.frame_rate,
        frame_count: v.frame_count,
        video_duration_seconds: video_duration,
        audio_duration_seconds: target.audio_duration_seconds,
        channels: info.audio.as_ref().map(|a| a.channels),
        video_tolerance_seconds: frame_seconds,
        audio_tolerance_seconds: frame_seconds.max(0.05),
    };
    Ok(ProcessingPlan {
        timing,
        job_id: job_id.into(),
        source: info.clone(),
        preset_id: request.preset_id.clone(),
        version,
        args,
        workspace,
        policy,
    })
}
