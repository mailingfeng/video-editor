#[path = "support/fake_runner.rs"]
mod fake_runner;
use fake_runner::{FakeRunner, Response};
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::{AppError, ErrorCode, MediaInfo, Rational},
    media::probe::{probe_input, validate_input},
};
async fn probe_fixture(name: &str) -> Result<MediaInfo, AppError> {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.mp4");
    std::fs::write(&input, b"test input").unwrap();
    let json = std::fs::read_to_string(format!("../tests/fixtures/probe/{name}.json")).unwrap();
    let frames = (0..2713)
        .map(|i| {
            format!(
                "best_effort_timestamp={}|duration=512",
                if name == "equal-rate-vfr" && i == 2 {
                    1536
                } else {
                    i * 512
                }
            )
        })
        .collect();
    let runner = FakeRunner::new(vec![
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
    ]);
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
async fn equal_rate_vfr_is_rejected() {
    assert_eq!(
        probe_fixture("equal-rate-vfr").await.unwrap_err().code,
        ErrorCode::UnsupportedInput
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
    info.audio.as_mut().unwrap().channels = 6;
    assert_eq!(
        validate_input(&info).unwrap_err().code,
        ErrorCode::UnsupportedInput
    );
}
