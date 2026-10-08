#[path = "support/fake_runner.rs"]
mod fake_runner;
#[path = "support/media_fixture.rs"]
mod media_fixture;
use fake_runner::{FakeRunner, Response};
use media_fixture::{media, request, workspace};
use tokio_util::sync::CancellationToken;
#[tokio::test]
#[ignore = "requires prepared native tools"]
async fn native_conversion_is_completely_validated() {
    use video_editor::media::probe::{probe_input, run_capture};
    use video_editor::native::{
        process::{NativeRunner, RunSpec, StdoutPolicy},
        tools::{current_target, resolve_tools},
    };
    use video_editor::output::workspace::{cleanup_workspace, prepare_workspace};
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source.mp4");
    let runner = NativeRunner::new(
        resolve_tools(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
            current_target(),
            None,
        )
        .unwrap(),
    );
    let args = [
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=128x96:rate=30",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=44100",
        "-t",
        "1.2",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
    ]
    .into_iter()
    .map(String::from)
    .chain([source.to_str().unwrap().into()])
    .collect();
    let exit = run_capture(
        &runner,
        RunSpec {
            tool: Tool::Ffmpeg,
            args,
            stdout_policy: StdoutPolicy::Discard,
        },
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(exit.exit_code, Some(0));
    let input = probe_input(&runner, &source, CancellationToken::new())
        .await
        .unwrap();
    let workspace =
        prepare_workspace(d.path(), &source, "native", &d.path().join("records")).unwrap();
    let plan = build_plan(
        &input,
        &request(source.to_str().unwrap(), d.path().to_str().unwrap()),
        workspace,
        "native",
    )
    .unwrap();
    let exit = run_capture(
        &runner,
        RunSpec {
            tool: Tool::Ffmpeg,
            args: plan.args.clone(),
            stdout_policy: StdoutPolicy::CaptureJson,
        },
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(exit.exit_code, Some(0), "{}", exit.stderr_tail);
    assert_eq!(
        validate_output(&runner, &plan, CancellationToken::new())
            .await
            .unwrap()
            .decoded_frames,
        36
    );
    std::fs::write(&plan.workspace.temp_path, b"only a broken container header").unwrap();
    assert_eq!(
        validate_output(&runner, &plan, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        ErrorCode::ValidationFailed
    );
    cleanup_workspace(&plan.workspace).unwrap();
}
use video_editor::native::tools::Tool;
use video_editor::{
    contracts::*,
    media::{
        probe::file_identity,
        validate::{validate_output, verify_specifications, ValidationResult},
    },
    presets::basic::build_plan,
};
#[test]
fn changed_frame_timestamps_fail_even_when_count_and_duration_match() {
    let mut source_json = serde_json::to_value(media()).unwrap();
    source_json["video"]["timeline"] =
        serde_json::json!({"variableFrameRate": true, "timestampSha256": "original"});
    let source: MediaInfo = serde_json::from_value(source_json.clone()).unwrap();
    let plan = build_plan(
        &source,
        &request("/input/original.mp4", "/output"),
        workspace(std::path::Path::new("/output")),
        "test",
    )
    .unwrap();
    source_json["audio"]["sampleRate"] = 48000.into();
    source_json["video"]["timeline"]["timestampSha256"] = "retimed".into();
    let output = serde_json::from_value(source_json).unwrap();
    assert_eq!(
        verify_specifications(&plan, &output).unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
async fn validate_fixture(name: &str) -> Result<ValidationResult, AppError> {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.mp4");
    std::fs::write(&input, b"source").unwrap();
    let token = CancellationToken::new();
    let mut m = media();
    m.identity = file_identity(&input, &token).await.unwrap();
    let mut p = build_plan(
        &m,
        &request(&m.identity.canonical_path, d.path().to_str().unwrap()),
        workspace(d.path()),
        "test",
    )
    .unwrap();
    std::fs::write(&p.workspace.temp_path, b"output").unwrap();
    if name == "changed-input" {
        std::fs::write(&input, b"changed").unwrap();
    }
    let fixture = if name == "changed-input" {
        "valid"
    } else {
        name
    };
    let json =
        std::fs::read_to_string(format!("../tests/fixtures/validation/{fixture}.json")).unwrap();
    let count = if name == "wrong-frames" { 59 } else { 60 };
    let frames = (0..count)
        .map(|i| format!("best_effort_timestamp={}|duration=512", i * 512))
        .collect();
    let runner = FakeRunner::new(vec![
        Response {
            tool: Tool::Ffprobe,
            required_arg: "-show_streams".into(),
            stdout: json,
            lines: vec![],
            stderr: String::new(),
            exit_code: 0,
        },
        Response {
            tool: Tool::Ffprobe,
            required_arg: "-show_frames".into(),
            stdout: String::new(),
            lines: frames,
            stderr: String::new(),
            exit_code: 0,
        },
        Response {
            tool: Tool::Ffmpeg,
            required_arg: "-xerror".into(),
            stdout: String::new(),
            lines: vec![],
            stderr: if name == "decode-error" {
                "corrupted frame".into()
            } else {
                String::new()
            },
            exit_code: 0,
        },
    ]);
    if name == "wrong-pixel" {
        p.source.video.pixel_format = "nv12".into();
    }
    let result = validate_output(&runner, &p, token).await;
    assert_eq!(runner.active_child_count(), 0);
    result
}
#[tokio::test]
async fn zero_exit_with_decode_error_fails() {
    assert_eq!(
        validate_fixture("decode-error").await.unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
#[tokio::test]
async fn video_frame_count_must_match() {
    assert_eq!(
        validate_fixture("wrong-frames").await.unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
#[tokio::test]
async fn audio_presence_and_48k_must_match() {
    assert_eq!(
        validate_fixture("wrong-audio").await.unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
#[tokio::test]
async fn relative_av_start_is_preserved() {
    assert_eq!(
        validate_fixture("shifted-start").await.unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
#[tokio::test]
async fn changed_input_fails() {
    assert_eq!(
        validate_fixture("changed-input").await.unwrap_err().code,
        ErrorCode::InputChanged
    );
}
#[tokio::test]
async fn complete_valid_output_passes() {
    assert_eq!(validate_fixture("valid").await.unwrap().decoded_frames, 60);
}
#[test]
fn duration_tolerances_follow_frame_rate() {
    let m = media();
    let p = build_plan(
        &m,
        &request(&m.identity.canonical_path, "/output"),
        workspace(std::path::Path::new("/output")),
        "test",
    )
    .unwrap();
    let mut o = m.clone();
    o.audio.as_mut().unwrap().sample_rate = 48000;
    o.video.duration_ticks = 31232;
    assert!(verify_specifications(&p, &o).is_ok()); // 1/30 sec
    o.video.duration_ticks = 31233;
    assert_eq!(
        verify_specifications(&p, &o).unwrap_err().code,
        ErrorCode::ValidationFailed
    );
    o.video.duration_ticks = 30720;
    o.audio.as_mut().unwrap().duration_ticks = 90405;
    assert!(verify_specifications(&p, &o).is_ok()); // 50 ms at 44100 timebase
    o.audio.as_mut().unwrap().duration_ticks = 90406;
    assert_eq!(
        verify_specifications(&p, &o).unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
#[test]
fn full_range_decoder_alias_is_validated_as_8_bit_420() {
    let mut m = media();
    m.video.pixel_format = "yuvj420p".into();
    m.video.color_range = Some("pc".into());
    let p = build_plan(
        &m,
        &request(&m.identity.canonical_path, "/output"),
        workspace(std::path::Path::new("/output")),
        "range",
    )
    .unwrap();
    let mut output = m.clone();
    output.audio.as_mut().unwrap().sample_rate = 48000;
    assert!(verify_specifications(&p, &output).is_ok());
    output.video.color_range = Some("tv".into());
    assert_eq!(
        verify_specifications(&p, &output).unwrap_err().code,
        ErrorCode::ValidationFailed
    );
}
