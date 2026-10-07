use async_trait::async_trait;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*,
    jobs::{service::JobService, state::is_terminal},
    media::probe::{file_identity, probe_input, run_capture},
    native::{
        process::{MediaRunner, NativeRunner, ProcessEvent, RunExit, RunSpec, StdoutPolicy},
        tools::{current_target, resolve_tools, Tool},
    },
};

struct ObservedRunner {
    native: NativeRunner,
    paced: bool,
    active: AtomicUsize,
    max_active: AtomicUsize,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait]
impl MediaRunner for ObservedRunner {
    async fn run(
        &self,
        mut spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError> {
        if self.paced && spec.args.iter().any(|a| a == "-progress") {
            let input = spec.args.iter().position(|a| a == "-i").unwrap();
            spec.args.insert(input, "-re".into());
        }
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        let _guard = Active(&self.active);
        self.native.run(spec, cancel, events).await
    }
}
fn runner(paced: bool) -> Arc<ObservedRunner> {
    let installed = std::env::var_os("VIDEO_EDITOR_INSTALLED_EXE").map(PathBuf::from);
    let native = NativeRunner::new(
        resolve_tools(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            current_target(),
            installed.as_deref(),
        )
        .unwrap(),
    );
    Arc::new(ObservedRunner {
        native,
        paced,
        active: AtomicUsize::new(0),
        max_active: AtomicUsize::new(0),
    })
}
fn fixtures() -> PathBuf {
    PathBuf::from(std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"))
}
fn settings(output: &Path, metadata: MetadataRequest) -> BatchSettings {
    BatchSettings {
        output_directory: output.display().to_string(),
        preset_id: "basic-transcode-v1".into(),
        metadata,
    }
}
fn service(
    r: Arc<ObservedRunner>,
    root: &Path,
) -> (JobService, mpsc::UnboundedReceiver<JobSnapshot>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        JobService::new(
            r,
            root.join("records"),
            Arc::new(move |s| {
                let _ = tx.send(s);
            }),
        ),
        rx,
    )
}
async fn next(rx: &mut mpsc::UnboundedReceiver<JobSnapshot>) -> JobSnapshot {
    tokio::time::timeout(Duration::from_secs(300), rx.recv())
        .await
        .expect("batch timeout")
        .unwrap()
}
async fn completed(
    rx: &mut mpsc::UnboundedReceiver<JobSnapshot>,
    count: usize,
) -> HashMap<String, JobSnapshot> {
    let mut ends = HashMap::new();
    while ends.len() < count {
        let s = next(rx).await;
        if s.state != JobState::Succeeded {
            assert!(s.progress.is_none_or(|p| p < 1.0));
        }
        if is_terminal(s.state) {
            assert!(ends.insert(s.job_id.clone(), s).is_none());
        }
    }
    ends
}
fn copies(root: &Path, name: &str, count: usize) -> Vec<PathBuf> {
    let folder = root.join("输入 中文 空格");
    std::fs::create_dir_all(&folder).unwrap();
    (0..count)
        .map(|i| {
            let path = folder.join(format!("视频 {i}.mp4"));
            std::fs::copy(fixtures().join(name), &path).unwrap();
            path
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_batch_validates_metadata_preserves_inputs_and_continues_after_failure() {
    let root = tempfile::tempdir().unwrap();
    let paths = copies(root.path(), "短片 空格 '引号'.mp4", 3);
    let native = runner(false);
    // Distinct metadata makes accidental reuse of one selected input observable.
    for (i, path) in paths.iter().enumerate() {
        let temp = path.with_file_name(format!("tagged-{i}.mp4"));
        let args = vec![
            "-v".into(),
            "error".into(),
            "-i".into(),
            path.display().to_string(),
            "-c".into(),
            "copy".into(),
            "-metadata".into(),
            format!("title=input {i}"),
            "-metadata".into(),
            format!("comment=comment {i}"),
            temp.display().to_string(),
        ];
        let exit = run_capture(
            native.as_ref(),
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
        std::fs::rename(temp, path).unwrap();
    }
    let damaged = paths[0].parent().unwrap().join("broken.MP4");
    std::fs::write(&damaged, b"broken MP4").unwrap();
    std::fs::create_dir(paths[0].parent().unwrap().join("nested")).unwrap();
    std::fs::copy(
        &paths[0],
        paths[0].parent().unwrap().join("nested/hidden.mp4"),
    )
    .unwrap();
    let mut before = Vec::new();
    for p in &paths {
        before.push(
            probe_input(native.as_ref(), p, CancellationToken::new())
                .await
                .unwrap(),
        );
    }
    let output = root.path().join("输出 中文");
    std::fs::create_dir(&output).unwrap();
    let (service, mut rx) = service(native.clone(), root.path());
    for metadata in [
        MetadataRequest::Preserve,
        MetadataRequest::Override {
            title: Some("shared title".into()),
            comment: Some("shared comment".into()),
        },
    ] {
        let imported = service
            .import_folder(paths[0].parent().unwrap().display().to_string())
            .await
            .unwrap();
        assert_eq!(imported.items.len(), 4);
        let start = service
            .start_batch(settings(&output, metadata.clone()))
            .await
            .unwrap();
        assert_eq!(start.items.iter().filter(|i| i.job_id.is_some()).count(), 2);
        let ends = completed(&mut rx, 4).await;
        let q = service.queue_snapshot().await;
        assert!(!q.running);
        for item in &q.items {
            let s = &ends[item.job_id.as_ref().unwrap()];
            if item.input_path.ends_with("broken.MP4") {
                assert_eq!(s.state, JobState::Failed);
                assert!(s.output_path.is_none());
                continue;
            }
            assert_eq!(s.state, JobState::Succeeded, "{:?}", s.error);
            assert!(!s.cleanup_pending);
            let source = before
                .iter()
                .find(|m| m.identity.canonical_path == item.input_path)
                .unwrap();
            let result = probe_input(
                native.as_ref(),
                Path::new(s.output_path.as_ref().unwrap()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
            assert_eq!(result.video.frame_count, source.video.frame_count);
            match &metadata {
                MetadataRequest::Preserve => {
                    assert_eq!(result.title, source.title);
                    assert_eq!(
                        result.comment,
                        Some(format!(
                            "{}; video-editor/basic-transcode-v1",
                            source.comment.as_deref().unwrap()
                        ))
                    );
                }
                MetadataRequest::Override { title, comment } => {
                    assert_eq!(&result.title, title);
                    assert_eq!(&result.comment, comment);
                }
            }
            assert_eq!(
                file_identity(Path::new(&item.input_path), &CancellationToken::new())
                    .await
                    .unwrap(),
                source.identity
            );
            service.remove_item(&item.item_id).await.unwrap();
        }
        for item in &q.items {
            if item.input_path.ends_with("broken.MP4") {
                service.remove_item(&item.item_id).await.unwrap();
            }
        }
    }
    service.shutdown().await.unwrap();
    assert_eq!(native.active.load(Ordering::SeqCst), 0);
    assert_eq!(native.max_active.load(Ordering::SeqCst), 2);
}

#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_batch_cancels_waiting_and_active_individually() {
    let root = tempfile::tempdir().unwrap();
    let paths = copies(root.path(), "lifecycle 空格.mp4", 4);
    let native = runner(true);
    let mut identities = Vec::new();
    for p in &paths {
        identities.push(file_identity(p, &CancellationToken::new()).await.unwrap());
    }
    let (service, mut rx) = service(native.clone(), root.path());
    let items = service
        .import_paths(paths.iter().map(|p| p.display().to_string()).collect())
        .await
        .unwrap()
        .items;
    service
        .start_batch(settings(root.path(), MetadataRequest::Preserve))
        .await
        .unwrap();
    service.cancel_item(&items[2].item_id).await.unwrap();
    let q = service.queue_snapshot().await;
    let id = q.items[0].job_id.clone().unwrap();
    loop {
        let s = next(&mut rx).await;
        if s.job_id == id && s.state == JobState::Running && s.progress.is_some_and(|p| p > 0.0) {
            break;
        }
        assert!(!is_terminal(s.state), "task finished before cancellation");
    }
    service.cancel_item(&items[0].item_id).await.unwrap();
    let ends = completed(&mut rx, 3).await;
    assert_eq!(ends[&id].state, JobState::Canceled);
    assert!(ends[&id].output_path.is_none());
    assert!(!ends[&id].cleanup_pending);
    assert_eq!(
        ends.values()
            .filter(|s| s.state == JobState::Succeeded)
            .count(),
        2
    );
    assert!(service.queue_snapshot().await.items[2].job_id.is_none());
    for (p, before) in paths.iter().zip(identities) {
        assert_eq!(
            file_identity(p, &CancellationToken::new()).await.unwrap(),
            before
        );
    }
    service.shutdown().await.unwrap();
    assert_eq!(native.active.load(Ordering::SeqCst), 0);
    assert_eq!(native.max_active.load(Ordering::SeqCst), 2);
}

#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_batch_shutdown_reaps_two_media_activities() {
    let root = tempfile::tempdir().unwrap();
    let paths = copies(root.path(), "lifecycle 空格.mp4", 3);
    let native = runner(true);
    let (service, mut rx) = service(native.clone(), root.path());
    service
        .import_paths(paths.iter().map(|p| p.display().to_string()).collect())
        .await
        .unwrap();
    let initial = service
        .start_batch(settings(root.path(), MetadataRequest::Preserve))
        .await
        .unwrap();
    let mut progressing = std::collections::HashSet::new();
    while progressing.len() < 2 {
        let s = next(&mut rx).await;
        assert!(!is_terminal(s.state));
        if s.state == JobState::Running && s.progress.is_some_and(|p| p > 0.0) {
            progressing.insert(s.job_id);
        }
    }
    tokio::time::timeout(Duration::from_secs(10), service.shutdown())
        .await
        .unwrap()
        .unwrap();
    let q = service.queue_snapshot().await;
    assert!(q.items[2].job_id.is_none());
    assert!(!q.running);
    for item in &q.items[..2] {
        let s = item.snapshot.as_ref().unwrap();
        assert_eq!(s.state, JobState::Canceled);
        assert!(s.output_path.is_none());
        assert!(!s.cleanup_pending);
    }
    assert_eq!(native.active.load(Ordering::SeqCst), 0);
    assert_eq!(initial.items[0].job_id, q.items[0].job_id);
    assert_eq!(initial.items[1].job_id, q.items[1].job_id);
    for p in paths {
        assert!(p.is_file());
    }
    assert_eq!(
        std::fs::read_dir(root.path().join("records"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|s| s == "json"))
            .count(),
        0
    );
}

#[tokio::test]
#[ignore = "requires explicitly supplied original sample"]
async fn provided_sample_batch_preserves_original_and_validates_all_outputs() {
    let sample =
        PathBuf::from(std::env::var_os("VIDEO_EDITOR_SAMPLE_DIR").expect("supply --sample-dir"))
            .join("原视频.mp4");
    let before = file_identity(&sample, &CancellationToken::new())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..3 {
        let path = root.path().join(format!("原视频 副本 {i}.mp4"));
        std::fs::copy(&sample, &path).unwrap();
        paths.push(path);
    }
    let native = runner(false);
    let (service, mut rx) = service(native.clone(), root.path());
    service
        .import_paths(paths.iter().map(|p| p.display().to_string()).collect())
        .await
        .unwrap();
    service
        .start_batch(settings(root.path(), MetadataRequest::Preserve))
        .await
        .unwrap();
    let ends = completed(&mut rx, 3).await;
    for s in ends.values() {
        assert_eq!(s.state, JobState::Succeeded, "{:?}", s.error);
        let output = probe_input(
            native.as_ref(),
            Path::new(s.output_path.as_ref().unwrap()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(output.video.frame_count, 2713);
    }
    for p in paths {
        assert_eq!(
            file_identity(&p, &CancellationToken::new())
                .await
                .unwrap()
                .sha256,
            before.sha256
        );
    }
    assert_eq!(
        file_identity(&sample, &CancellationToken::new())
            .await
            .unwrap(),
        before
    );
    service.shutdown().await.unwrap();
    assert_eq!(native.max_active.load(Ordering::SeqCst), 2);
}
