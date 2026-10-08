use super::{
    inputs::is_supported_video,
    timeline::{Timeline, TimelineReport},
};
use crate::{
    contracts::*,
    native::{
        process::{MediaRunner, ProcessEvent, RunExit, RunSpec, StdoutPolicy},
        tools::Tool,
    },
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::{io::AsyncReadExt, sync::mpsc};
use tokio_util::sync::CancellationToken;

pub async fn file_identity(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<FileIdentity, AppError> {
    let path = tokio::fs::canonicalize(path).await.map_err(|e| {
        AppError::new(ErrorCode::DamagedMedia, "无法读取输入文件").detail(e.to_string())
    })?;
    let canonical_path = path
        .to_str()
        .ok_or_else(|| AppError::new(ErrorCode::UnsupportedInput, "文件路径无法表示为 UTF-8"))?
        .to_owned();
    let mut f = tokio::fs::File::open(&path)
        .await
        .map_err(|e| AppError::new(ErrorCode::DamagedMedia, e.to_string()))?;
    let before = f
        .metadata()
        .await
        .map_err(|e| AppError::new(ErrorCode::DamagedMedia, e.to_string()))?;
    if !before.is_file() {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "输入必须为普通文件",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "检查已取消"));
        }
        let n = f
            .read(&mut buffer)
            .await
            .map_err(|e| AppError::new(ErrorCode::DamagedMedia, e.to_string()))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    let after = f
        .metadata()
        .await
        .map_err(|e| AppError::new(ErrorCode::DamagedMedia, e.to_string()))?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(AppError::new(
            ErrorCode::InputChanged,
            "读取期间输入发生变化",
        ));
    }
    let modified_ns = before
        .modified()
        .map_err(|e| AppError::new(ErrorCode::DamagedMedia, e.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(ErrorCode::UnsupportedInput, e.to_string()))?
        .as_nanos()
        .to_string();
    Ok(FileIdentity {
        canonical_path,
        size_bytes: before.len(),
        modified_ns,
        sha256: format!("{:x}", hash.finalize()),
    })
}
pub async fn probe_input(
    runner: &dyn MediaRunner,
    path: &Path,
    cancel: CancellationToken,
) -> Result<MediaInfo, AppError> {
    if !is_supported_video(path) {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "支持 MP4、MOV、M4V、MKV 和 WebM 视频",
        ));
    }
    let identity = file_identity(path, &cancel).await?;
    let args = vec![
        "-v",
        "error",
        "-show_streams",
        "-show_format",
        "-of",
        "json",
    ]
    .into_iter()
    .map(String::from)
    .chain([identity.canonical_path.clone()])
    .collect();
    let result = run_capture(
        runner,
        RunSpec {
            tool: Tool::Ffprobe,
            args,
            stdout_policy: StdoutPolicy::CaptureJson,
        },
        cancel.clone(),
    )
    .await?;
    check_exit(&result)?;
    let json: Value = serde_json::from_str(&result.stdout).map_err(|e| {
        AppError::new(ErrorCode::DamagedMedia, "无法解析媒体信息").detail(e.to_string())
    })?;
    let mut info = parse_metadata(&json, identity)?;
    validate_tracks(&info)?;
    let video = read_timeline(
        runner,
        &info.identity.canonical_path,
        info.video.stream_index,
        Timeline::new(
            info.video.time_base,
            info.video
                .start_pts
                .checked_add(info.video.duration_ticks)
                .filter(|_| info.video.duration_ticks > 0),
        ),
        false,
        cancel.clone(),
    )
    .await?;
    info.video.frame_count = video.count;
    info.video.start_pts = video.start;
    info.video.duration_ticks = video.duration;
    info.video.frame_rate = video.rate;
    info.video.timeline = Some(video.timeline);
    if let Some(audio) = &mut info.audio {
        let start_known = json["streams"]
            .as_array()
            .and_then(|streams| {
                streams
                    .iter()
                    .find(|s| number(&s["index"]) == Some(audio.stream_index as i64))
            })
            .is_some_and(|metadata| {
                number(&metadata["start_pts"])
                    .or_else(|| numeric_seconds(&metadata["start_time"], audio.time_base))
                    .is_some()
            });
        if audio.duration_ticks <= 0 || !start_known {
            let timing = read_timeline(
                runner,
                &info.identity.canonical_path,
                audio.stream_index,
                Timeline::new(audio.time_base, None),
                true,
                cancel.clone(),
            )
            .await?;
            if !start_known {
                audio.start_pts = timing.start;
            }
            if audio.duration_ticks <= 0 {
                audio.duration_ticks = timing.duration;
            }
        }
    }
    validate_input(&info)?;
    if file_identity(Path::new(&info.identity.canonical_path), &cancel).await? != info.identity {
        return Err(AppError::new(
            ErrorCode::InputChanged,
            "探测期间输入发生变化",
        ));
    }
    Ok(info)
}

async fn read_timeline(
    runner: &dyn MediaRunner,
    path: &str,
    stream_index: u32,
    mut timeline: Timeline,
    packets: bool,
    cancel: CancellationToken,
) -> Result<TimelineReport, AppError> {
    let (tx, mut rx) = mpsc::channel(256);
    let args = vec![
        "-v".into(),
        "error".into(),
        "-select_streams".into(),
        stream_index.to_string(),
        if packets {
            "-show_packets"
        } else {
            "-show_frames"
        }
        .into(),
        "-show_entries".into(),
        if packets {
            "packet=pts,duration"
        } else {
            "frame=best_effort_timestamp,duration,pkt_duration"
        }
        .into(),
        "-of".into(),
        "compact=p=0:nk=0".into(),
        path.into(),
    ];
    let frame_cancel = cancel.child_token();
    let run = runner.run(
        RunSpec {
            tool: Tool::Ffprobe,
            args,
            stdout_policy: StdoutPolicy::Stream,
        },
        frame_cancel.clone(),
        tx,
    );
    tokio::pin!(run);
    let mut timeline_error = None;
    let result = loop {
        tokio::select! {
         result=&mut run=>break result?,
         event=rx.recv()=>{match event{
          Some(ProcessEvent::StdoutLine(line))=>{if timeline_error.is_none(){if let Err(e)=timeline.push(&line){timeline_error=Some(e);frame_cancel.cancel();}}},
          Some(ProcessEvent::StderrLine(_))=>{},
          None=>break run.await?,
         }}
        }
    };
    while let Some(event) = rx.recv().await {
        if let ProcessEvent::StdoutLine(line) = event {
            if timeline_error.is_none() {
                if let Err(e) = timeline.push(&line) {
                    timeline_error = Some(e);
                }
            }
        }
    }
    if let Some(e) = timeline_error {
        return Err(e);
    }
    check_exit(&result)?;
    timeline.finish()
}

pub async fn run_capture(
    runner: &dyn MediaRunner,
    spec: RunSpec,
    cancel: CancellationToken,
) -> Result<RunExit, AppError> {
    let (tx, mut rx) = mpsc::channel(256);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let result = runner.run(spec, cancel, tx).await;
    let _ = drain.await;
    result
}
pub fn check_exit(result: &RunExit) -> Result<(), AppError> {
    if result.canceled {
        return Err(AppError::new(ErrorCode::Canceled, "媒体检查已取消"));
    }
    if result.exit_code != Some(0) || !result.stderr_tail.trim().is_empty() {
        return Err(AppError::new(ErrorCode::DamagedMedia, "媒体无法完整读取")
            .detail(result.stderr_tail.clone()));
    }
    Ok(())
}
pub fn validate_input(info: &MediaInfo) -> Result<(), AppError> {
    validate_tracks(info)?;
    if info.video.frame_rate.num <= 0 || info.video.frame_rate.den == 0 {
        return Err(AppError::new(ErrorCode::UnsupportedInput, "帧率无法确认"));
    }
    if info.audio.as_ref().is_some_and(|a| a.duration_ticks <= 0) {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "音频时间轴无法确认",
        ));
    }
    Ok(())
}

fn validate_tracks(info: &MediaInfo) -> Result<(), AppError> {
    let v = &info.video;
    let supported_pixel = [
        "yuv420p", "yuv422p", "yuv444p", "yuvj420p", "yuvj422p", "yuvj444p", "nv12", "nv21",
        "rgb24", "bgr24", "gbrp", "gray",
    ];
    if matches!(v.pixel_format.as_str(), "rgb24" | "bgr24" | "gbrp")
        || v.color_space.as_deref() == Some("gbr")
    {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "当前预设不支持 RGB 色彩矩阵转换",
        ));
    }
    if v.width == 0
        || v.height == 0
        || !v.width.is_multiple_of(2)
        || !v.height.is_multiple_of(2)
        || v.bit_depth != 8
        || !supported_pixel.contains(&v.pixel_format.as_str())
    {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "需要偶数尺寸的 8 bit SDR 视频",
        ));
    }
    if matches!(
        v.color_transfer.as_deref(),
        Some("smpte2084" | "arib-std-b67")
    ) {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "当前预设不支持 HDR",
        ));
    }
    if v.time_base.num <= 0 || v.time_base.den == 0 {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "帧率或时间基准无法确认",
        ));
    }
    if let Some(a) = &info.audio {
        if !(1..=8).contains(&a.channels) || a.sample_rate == 0 {
            return Err(AppError::new(
                ErrorCode::UnsupportedInput,
                "当前预设支持 1 至 8 声道且采样率有效的音轨",
            ));
        }
    }
    Ok(())
}
fn rational(value: &Value) -> Result<Rational, AppError> {
    let text = value
        .as_str()
        .ok_or_else(|| AppError::new(ErrorCode::UnsupportedInput, "时间基准缺失"))?;
    let (n, d) = text
        .split_once('/')
        .ok_or_else(|| AppError::new(ErrorCode::UnsupportedInput, "时间基准无效"))?;
    let n: i64 = n
        .parse()
        .map_err(|_| AppError::new(ErrorCode::UnsupportedInput, "时间基准无效"))?;
    let d: u64 = d
        .parse()
        .map_err(|_| AppError::new(ErrorCode::UnsupportedInput, "时间基准无效"))?;
    if n <= 0 || d == 0 {
        return Err(AppError::new(ErrorCode::UnsupportedInput, "时间基准无效"));
    }
    let mut a = n as u64;
    let mut b = d;
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    Ok(Rational {
        num: n / a as i64,
        den: d / a,
    })
}
fn number(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
fn required(v: &Value, name: &str) -> Result<i64, AppError> {
    number(v).ok_or_else(|| AppError::new(ErrorCode::UnsupportedInput, format!("{name} 未提供")))
}
fn text(v: &Value) -> Option<String> {
    v.as_str()
        .filter(|s| !s.is_empty() && *s != "unknown" && *s != "unspecified")
        .map(String::from)
}
fn bitrate(v: &Value) -> Option<u64> {
    number(v).filter(|x| *x > 0).map(|x| x as u64)
}
fn seconds_ticks(seconds: f64, base: Rational) -> Option<i64> {
    let ticks = (seconds * base.den as f64 / base.num as f64).round();
    (ticks.is_finite() && ticks >= i64::MIN as f64 && ticks < i64::MAX as f64)
        .then_some(ticks as i64)
}
fn numeric_seconds(value: &Value, base: Rational) -> Option<i64> {
    seconds_ticks(
        value.as_f64().or_else(|| value.as_str()?.parse().ok())?,
        base,
    )
}
fn tagged_end(stream: &Value, base: Rational) -> Option<i64> {
    let parts: Vec<_> = stream["tags"]["DURATION"].as_str()?.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;
    seconds_ticks(hours * 3600.0 + minutes * 60.0 + seconds, base)
}
fn parse_metadata(j: &Value, identity: FileIdentity) -> Result<MediaInfo, AppError> {
    let container = text(&j["format"]["format_name"])
        .ok_or_else(|| AppError::new(ErrorCode::DamagedMedia, "容器信息缺失"))?;
    if !container
        .split(',')
        .any(|x| matches!(x, "mov" | "mp4" | "matroska" | "webm"))
    {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "当前视频容器不受支持",
        ));
    }
    let streams = j["streams"]
        .as_array()
        .ok_or_else(|| AppError::new(ErrorCode::DamagedMedia, "轨道信息缺失"))?;
    let videos: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .collect();
    let audios: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .collect();
    if videos.len() != 1 || audios.len() > 1 {
        return Err(AppError::new(
            ErrorCode::UnsupportedInput,
            "需要一个视频轨和零或一个音轨",
        ));
    }
    let v = videos[0];
    let time_base = rational(&v["time_base"])?;
    let start_pts = number(&v["start_pts"])
        .or_else(|| numeric_seconds(&v["start_time"], time_base))
        .unwrap_or(0);
    let duration_ticks = number(&v["duration_ts"])
        .filter(|d| *d > 0)
        .or_else(|| numeric_seconds(&v["duration"], time_base).filter(|d| *d > 0))
        .or_else(|| {
            tagged_end(v, time_base)
                .and_then(|end| end.checked_sub(start_pts))
                .filter(|d| *d > 0)
        })
        .unwrap_or(0);
    let video = VideoInfo {
        stream_index: required(&v["index"], "轨道序号")? as u32,
        codec: text(&v["codec_name"]).unwrap_or_default(),
        width: required(&v["width"], "宽度")? as u32,
        height: required(&v["height"], "高度")? as u32,
        bit_depth: number(&v["bits_per_raw_sample"])
            .filter(|x| *x > 0)
            .unwrap_or(
                if text(&v["pix_fmt"])
                    .is_some_and(|s| s.contains("10") || s.contains("12") || s.contains("16"))
                {
                    10
                } else {
                    8
                },
            ) as u8,
        pixel_format: text(&v["pix_fmt"]).unwrap_or_default(),
        frame_rate: Rational { num: 0, den: 1 },
        time_base,
        frame_count: 0,
        start_pts,
        duration_ticks,
        timeline: None,
        bit_rate: bitrate(&v["bit_rate"]),
        color_range: text(&v["color_range"]),
        color_space: text(&v["color_space"]),
        color_primaries: text(&v["color_primaries"]),
        color_transfer: text(&v["color_transfer"]),
    };
    let audio = audios
        .first()
        .map(|a| -> Result<AudioInfo, AppError> {
            let time_base = rational(&a["time_base"])?;
            Ok(AudioInfo {
                stream_index: required(&a["index"], "音轨序号")? as u32,
                codec: text(&a["codec_name"]).unwrap_or_default(),
                sample_rate: required(&a["sample_rate"], "采样率")? as u32,
                channels: required(&a["channels"], "声道数")? as u8,
                channel_layout: text(&a["channel_layout"]),
                time_base,
                start_pts: number(&a["start_pts"])
                    .or_else(|| numeric_seconds(&a["start_time"], time_base))
                    .unwrap_or(0),
                duration_ticks: number(&a["duration_ts"]).unwrap_or(0),
                bit_rate: bitrate(&a["bit_rate"]),
            })
        })
        .transpose()?;
    Ok(MediaInfo {
        identity,
        container,
        video,
        audio,
        title: text(&j["format"]["tags"]["title"]),
        comment: text(&j["format"]["tags"]["comment"]),
    })
}
