use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*,
    jobs::service::JobService,
    media::probe::{file_identity, probe_input},
    native::{
        process::NativeRunner,
        tools::{current_target, resolve_tools},
    },
};

fn runner() -> Arc<NativeRunner> {
    // CI can exercise the actual installed media tools after an NSIS installation.
    let installed = std::env::var_os("VIDEO_EDITOR_INSTALLED_EXE").map(PathBuf::from);
    Arc::new(NativeRunner::new(
        resolve_tools(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            current_target(),
            installed.as_deref(),
        )
        .unwrap(),
    ))
}
fn request(input: &Path, output: &Path) -> StartJobRequest {
    StartJobRequest {
        input_path: input.display().to_string(),
        output_directory: output.display().to_string(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    }
}
async fn process(
    input: &Path,
    cancel_running: bool,
) -> (JobSnapshot, tempfile::TempDir, Vec<JobState>) {
    let d = tempfile::tempdir().unwrap();
    let output = d.path().join("输出 空格 '目录'");
    std::fs::create_dir(&output).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let service = JobService::new(
        runner(),
        d.path().join("records"),
        Arc::new(move |s| {
            let _ = tx.send(s);
        }),
    );
    let id = service.start_job(request(input, &output)).await.unwrap();
    let mut states = vec![];
    let snapshot = tokio::time::timeout(std::time::Duration::from_secs(300), async {
        while let Some(snapshot) = rx.recv().await {
            assert_eq!(snapshot.job_id, id);
            states.push(snapshot.state);
            if snapshot.state != JobState::Succeeded {
                assert!(snapshot.progress.is_none_or(|p| p < 1.0));
            }
            if cancel_running && snapshot.state == JobState::Running {
                service.cancel_job(&id).await.unwrap();
            }
            if matches!(
                snapshot.state,
                JobState::Succeeded | JobState::Failed | JobState::Canceled
            ) {
                return snapshot;
            }
        }
        panic!("missing terminal snapshot")
    })
    .await
    .unwrap();
    if snapshot.state == JobState::Succeeded {
        let log = service.read_log(&id).await.unwrap();
        assert!(log.text.contains("完整校验通过，结果已保存"));
    }
    service.shutdown().await.unwrap();
    (snapshot, d, states)
}
#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_media_end_to_end() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for name in [
        "短片 空格 '引号'.mp4",
        "no-audio.mp4",
        "av-offset.mp4",
        "full-range.mp4",
    ] {
        let source = fixtures.join(name);
        let before = probe_input(runner().as_ref(), &source, CancellationToken::new())
            .await
            .unwrap();
        let (result, _dir, states) = process(&source, false).await;
        assert_eq!(
            result.state,
            JobState::Succeeded,
            "{name}: {:?}",
            result.error
        );
        assert!(states.contains(&JobState::Validating));
        assert!(states.contains(&JobState::Committing));
        let after = probe_input(
            runner().as_ref(),
            Path::new(result.output_path.as_ref().unwrap()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(before.video.frame_count, after.video.frame_count);
        assert_eq!(before.video.frame_rate, after.video.frame_rate);
        assert_eq!(before.video.color_range, after.video.color_range);
        assert_eq!(
            after.audio.as_ref().map(|a| a.sample_rate),
            before.audio.as_ref().map(|_| 48000)
        );
        assert_eq!(
            file_identity(&source, &CancellationToken::new())
                .await
                .unwrap(),
            before.identity
        );
        assert_ne!(
            before.identity.canonical_path,
            after.identity.canonical_path
        );
    }
    for (name, code) in [
        ("vfr.mp4", ErrorCode::UnsupportedInput),
        ("hdr.mp4", ErrorCode::UnsupportedInput),
        ("rgb.mp4", ErrorCode::UnsupportedInput),
        ("damaged.mp4", ErrorCode::DamagedMedia),
    ] {
        let (result, _, _) = process(&fixtures.join(name), false).await;
        assert_eq!(result.state, JobState::Failed);
        assert_eq!(result.error.unwrap().code, code, "{name}");
        assert!(result.output_path.is_none());
    }
}
#[tokio::test]
#[ignore = "requires explicitly supplied read-only sample directory"]
async fn provided_sample_end_to_end() {
    let sample =
        PathBuf::from(std::env::var_os("VIDEO_EDITOR_SAMPLE_DIR").expect("supply --sample-dir"))
            .join("原视频.mp4");
    let before = probe_input(runner().as_ref(), &sample, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(before.video.frame_count, 2713);
    let (result, _dir, _) = process(&sample, false).await;
    assert_eq!(result.state, JobState::Succeeded, "{:?}", result.error);
    let output = probe_input(
        runner().as_ref(),
        Path::new(result.output_path.as_ref().unwrap()),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(output.video.frame_count, 2713);
    assert_eq!(output.audio.unwrap().sample_rate, 48000);
    assert_eq!(
        file_identity(&sample, &CancellationToken::new())
            .await
            .unwrap(),
        before.identity
    );
    let (canceled, dir, _) = process(&sample, true).await;
    assert_eq!(canceled.state, JobState::Canceled);
    assert!(canceled.output_path.is_none());
    assert!(!canceled.cleanup_pending);
    assert_eq!(
        std::fs::read_dir(dir.path().join("输出 空格 '目录'"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        file_identity(&sample, &CancellationToken::new())
            .await
            .unwrap(),
        before.identity
    );
}
