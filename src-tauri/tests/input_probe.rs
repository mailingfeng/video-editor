#[path = "support/fake_runner.rs"]
mod fake_runner;
use fake_runner::{FakeRunner, Response};
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::{AppError, ErrorCode, MediaInfo, Rational},
    media::probe::{probe_input, validate_input},
};
async fn probe_fixture(name: &str) -> Result<MediaInfo, AppError> {
    let json = std::fs::read_to_string(format!("../tests/fixtures/probe/{name}.json")).unwrap();
    let frames = (0..2713)
        .map(|i| {
            format!(
                "best_effort_timestamp={}|duration=512",
                if name == "equal-rate-vfr" && i >= 2 {
                    (i + 1) * 512
                } else {
                    i * 512
                }
            )
        })
        .collect();
    probe_data(json, frames).await
}
async fn probe_data(json: String, frames: Vec<String>) -> Result<MediaInfo, AppError> {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.mp4");
    std::fs::write(&input, b"test input").unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&json).unwrap();
    let needs_audio_timing = metadata["streams"].as_array().unwrap().iter().any(|s| {
        s["codec_type"] == "audio"
            && (s["duration_ts"].is_null()
                || (s["start_pts"].is_null() && s["start_time"].is_null()))
    });
    let mut responses = vec![
        Response {
            tool: video_editor::native::tools::Tool::Ffprobe,
            required_arg: "-show_streams".into(),
            stdout: json,
            lines: vec![],
            stderr: String::new(),
            exit_code: 0,
        },
        Response {
            tool: video_editor::native::tools::Tool::Ffprobe,
            required_arg: "-show_frames".into(),
            stdout: String::new(),
            lines: frames,
            stderr: String::new(),
            exit_code: 0,
        },
    ];
    if needs_audio_timing {
        responses.push(Response {
            tool: video_editor::native::tools::Tool::Ffprobe,
            required_arg: "-show_packets".into(),
            stdout: String::new(),
            lines: (0..6)
                .map(|i| {
                    format!(
                        "pts={}|duration=1024|side_data_type=Skip Samples",
                        100 + i * 1024
                    )
                })
                .collect(),
            stderr: String::new(),
            exit_code: 0,
        });
    }
    let runner = FakeRunner::new(responses);
    let result = probe_input(&runner, &input, CancellationToken::new()).await;
    assert_eq!(runner.active_child_count(), 0);
    result
}
#[tokio::test]
async fn missing_frame_count_reads_timestamps() {
    let info = probe_fixture("missing-frames").await.unwrap();
    assert_eq!(info.video.frame_count, 2713);
    assert_eq!(info.video.frame_rate, Rational { num: 30, den: 1 });
}
#[tokio::test]
async fn equal_metadata_rates_do_not_hide_variable_timestamps() {
    let info = probe_fixture("equal-rate-vfr").await.unwrap();
    assert_eq!(info.video.frame_count, 2713);
    assert_eq!(info.video.duration_ticks, 1389568);
    assert_eq!(
        serde_json::to_value(info).unwrap()["video"]["timeline"]["variableFrameRate"],
        true
    );
}
#[tokio::test]
async fn missing_frame_durations_are_recovered_from_pts_and_stream_end() {
    let json = std::fs::read_to_string("../tests/fixtures/probe/valid.json").unwrap();
    let frames = (0..2713)
        .map(|i| format!("best_effort_timestamp={i}", i = i * 512))
        .collect();
    let info = probe_data(json, frames).await.unwrap();
    assert_eq!(info.video.frame_count, 2713);
    assert_eq!(info.video.duration_ticks, 1389056);
}
#[tokio::test]
async fn missing_rate_is_derived_from_decoded_timeline() {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/probe/no-audio.json")).unwrap();
    json["streams"][0]["avg_frame_rate"] = "0/0".into();
    json["streams"][0]["r_frame_rate"] = "0/0".into();
    let frames = (0..60)
        .map(|i| format!("best_effort_timestamp={}|duration=512", i * 512))
        .collect();
    let info = probe_data(json.to_string(), frames).await.unwrap();
    assert_eq!(info.video.frame_rate, Rational { num: 30, den: 1 });
    assert_eq!(info.video.duration_ticks, 30720);
}
#[tokio::test]
async fn missing_last_vfr_duration_uses_track_end_but_never_guesses() {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/probe/no-audio.json")).unwrap();
    json["streams"][0]
        .as_object_mut()
        .unwrap()
        .remove("duration_ts");
    let frames = vec![
        "best_effort_timestamp=0",
        "best_effort_timestamp=512",
        "best_effort_timestamp=1536",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    assert_eq!(
        probe_data(json.to_string(), frames.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedInput
    );
    json["streams"][0]["tags"]["DURATION"] = "00:00:00.133333333".into();
    assert_eq!(
        probe_data(json.to_string(), frames)
            .await
            .unwrap()
            .video
            .duration_ticks,
        2048
    );
}
#[tokio::test]
async fn duplicate_or_missing_timestamps_are_still_rejected() {
    for frames in [
        vec![
            "best_effort_timestamp=0|duration=512",
            "best_effort_timestamp=0|duration=512",
        ],
        vec!["best_effort_timestamp=N/A|duration=512"],
    ] {
        let error = probe_data(
            include_str!("../../tests/fixtures/probe/no-audio.json").into(),
            frames.into_iter().map(String::from).collect(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedInput);
        assert!(error.details.is_some());
    }
}
#[tokio::test]
async fn matroska_audio_without_stream_duration_is_measured_from_packets() {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/probe/valid.json")).unwrap();
    json["format"]["format_name"] = "matroska,webm".into();
    let audio = &mut json["streams"][1];
    audio["sample_rate"] = "48000".into();
    audio["time_base"] = "1/48000".into();
    audio.as_object_mut().unwrap().remove("duration_ts");
    audio.as_object_mut().unwrap().remove("start_pts");
    let frames = (0..60)
        .map(|i| format!("best_effort_timestamp={}|duration=512", i * 512))
        .collect();
    let info = probe_data(json.to_string(), frames).await.unwrap();
    assert_eq!(info.audio.as_ref().unwrap().duration_ticks, 6144);
    assert_eq!(info.audio.unwrap().start_pts, 100);
}
#[tokio::test]
async fn missing_audio_start_is_measured_even_when_duration_is_known() {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/probe/valid.json")).unwrap();
    let duration = json["streams"][1]["duration_ts"].as_i64().unwrap();
    json["streams"][1]
        .as_object_mut()
        .unwrap()
        .remove("start_pts");
    json["streams"][1]
        .as_object_mut()
        .unwrap()
        .remove("start_time");
    let frames = (0..60)
        .map(|i| format!("best_effort_timestamp={}|duration=512", i * 512))
        .collect();
    let audio = probe_data(json.to_string(), frames)
        .await
        .unwrap()
        .audio
        .unwrap();
    assert_eq!(audio.start_pts, 100);
    assert_eq!(audio.duration_ticks, duration);
}
#[tokio::test]
async fn audio_start_time_is_used_when_start_pts_is_missing() {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/probe/valid.json")).unwrap();
    json["streams"][1]
        .as_object_mut()
        .unwrap()
        .remove("start_pts");
    json["streams"][1]["start_time"] = "0.12".into();
    let frames = (0..60)
        .map(|i| format!("best_effort_timestamp={}|duration=512", i * 512))
        .collect();
    assert_eq!(
        probe_data(json.to_string(), frames)
            .await
            .unwrap()
            .audio
            .unwrap()
            .start_pts,
        5292
    );
}
#[test]
fn timestamp_digest_matches_equivalent_time_bases_and_detects_retiming() {
    use video_editor::media::timeline::Timeline;
    let mut milliseconds = Timeline::new(Rational { num: 1, den: 1000 }, None);
    let mut ticks = Timeline::new(Rational { num: 1, den: 90000 }, None);
    let mut retimed = Timeline::new(Rational { num: 1, den: 1000 }, None);
    for pts in [100, 133, 167] {
        milliseconds
            .push(&format!("best_effort_timestamp={pts}|duration=33"))
            .unwrap();
    }
    for pts in [9000, 11970, 15030] {
        ticks
            .push(&format!("best_effort_timestamp={pts}|duration=2970"))
            .unwrap();
    }
    for pts in [100, 134, 167] {
        retimed
            .push(&format!("best_effort_timestamp={pts}|duration=33"))
            .unwrap();
    }
    let original = milliseconds.finish().unwrap();
    let changed = retimed.finish().unwrap();
    assert_eq!(original.count, changed.count);
    assert_eq!(original.duration, changed.duration);
    assert_eq!(
        original.timeline.timestamp_sha256,
        ticks.finish().unwrap().timeline.timestamp_sha256
    );
    assert_ne!(
        original.timeline.timestamp_sha256,
        changed.timeline.timestamp_sha256
    );
}
#[tokio::test]
async fn unknown_color_stays_unknown() {
    let v = probe_fixture("unknown-color").await.unwrap().video;
    assert_eq!(v.color_space, None);
    assert_eq!(v.color_transfer, None);
}
#[tokio::test]
async fn rgb_matrix_requires_a_separate_color_conversion_policy() {
    let mut info = probe_fixture("unknown-color").await.unwrap();
    info.video.pixel_format = "gbrp".into();
    info.video.color_space = Some("gbr".into());
    assert_eq!(
        validate_input(&info).unwrap_err().code,
        ErrorCode::UnsupportedInput
    );
}
#[tokio::test]
async fn audio_absence_is_preserved() {
    assert!(probe_fixture("no-audio").await.unwrap().audio.is_none());
}
#[tokio::test]
#[ignore = "requires the supplied readonly sample"]
async fn native_sample_probe() {
    use video_editor::native::{
        process::NativeRunner,
        tools::{current_target, resolve_tools},
    };
    let dir = std::env::var("VIDEO_EDITOR_SAMPLE_DIR").expect("VIDEO_EDITOR_SAMPLE_DIR required");
    let runner = NativeRunner::new(
        resolve_tools(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
            current_target(),
            None,
        )
        .unwrap(),
    );
    let info = probe_input(
        &runner,
        &std::path::Path::new(&dir).join("原视频.mp4"),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        (info.video.width, info.video.height, info.video.frame_count),
        (720, 1280, 2713)
    );
    assert_eq!(info.audio.unwrap().sample_rate, 44100);
    assert_eq!(
        info.identity.sha256,
        "c4fd9bc7b1c8fc928848808aee0ffd7b0c961fffb449f92587f3c756211d8beb"
    );
}
#[tokio::test]
async fn unsupported_tracks_and_hdr_are_rejected() {
    for name in ["hdr", "multi-track"] {
        assert_eq!(
            probe_fixture(name).await.unwrap_err().code,
            ErrorCode::UnsupportedInput
        );
    }
    let mut info = probe_fixture("valid").await.unwrap();
    info.video.width = 719;
    assert_eq!(
        validate_input(&info).unwrap_err().code,
        ErrorCode::UnsupportedInput
    );
    info.video.width = 720;
    info.audio.as_mut().unwrap().channels = 0;
    assert_eq!(
        validate_input(&info).unwrap_err().code,
        ErrorCode::UnsupportedInput
    );
}
#[tokio::test]
async fn surround_audio_is_accepted() {
    let mut info = probe_fixture("valid").await.unwrap();
    for channels in [4, 6, 8] {
        info.audio.as_mut().unwrap().channels = channels;
        validate_input(&info).unwrap();
    }
}
