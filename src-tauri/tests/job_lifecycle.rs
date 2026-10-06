#[path = "support/controlled_runner.rs"]
mod controlled_runner;
use controlled_runner::{BlockAt, ControlledRunner};
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_event_allows_starting_the_next_job() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.mp4");
    std::fs::write(&input, b"input").unwrap();
    let request = StartJobRequest {
        input_path: input.to_str().unwrap().into(),
        output_directory: d.path().to_str().unwrap().into(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    };
    let slot: Arc<Mutex<Option<JobService>>> = Arc::new(Mutex::new(None));
    let callback_slot = slot.clone();
    let callback_request = request.clone();
    let once = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sink: SnapshotSink = Arc::new(move |snapshot| {
        if is_terminal(snapshot.state) && !once.swap(true, Ordering::SeqCst) {
            let service = callback_slot.lock().unwrap().as_ref().unwrap().clone();
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(service.start_job(callback_request.clone()))
            });
            let _ = tx.send(result);
        }
    });
    let service = JobService::new(
        ControlledRunner::new(BlockAt::None, false),
        d.path().join("records"),
        sink,
    );
    *slot.lock().unwrap() = Some(service.clone());
    service.start_job(request).await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.is_ok(),
        "terminal event still leaves backend busy: {result:?}"
    );
    service.shutdown().await.unwrap();
}
use std::sync::{atomic::Ordering, Arc};
use tokio::sync::mpsc;
use video_editor::{
    contracts::*,
    jobs::{
        progress::{apply_progress, ProgressParser},
        service::{JobService, SnapshotSink},
        state::is_terminal,
    },
};
fn setup(
    at: BlockAt,
    foreign: bool,
) -> (
    tempfile::TempDir,
    Arc<ControlledRunner>,
    JobService,
    mpsc::UnboundedReceiver<JobSnapshot>,
    StartJobRequest,
) {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.mp4");
    std::fs::write(&input, b"input").unwrap();
    let runner = ControlledRunner::new(at, foreign);
    let (tx, rx) = mpsc::unbounded_channel();
    let sink: SnapshotSink = Arc::new(move |s| {
        let _ = tx.send(s);
    });
    let service = JobService::new(runner.clone(), d.path().join("records"), sink);
    let request = StartJobRequest {
        input_path: input.to_str().unwrap().into(),
        output_directory: d.path().to_str().unwrap().into(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    };
    (d, runner, service, rx, request)
}
async fn terminal(rx: &mut mpsc::UnboundedReceiver<JobSnapshot>) -> JobSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let s = rx.recv().await.unwrap();
            if is_terminal(s.state) {
                return s;
            }
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn double_start_runs_one_child() {
    let (_d, r, s, mut rx, request) = setup(BlockAt::Probe, false);
    let id = s.start_job(request.clone()).await.unwrap();
    r.wait_entered().await;
    assert_eq!(
        s.start_job(request).await.unwrap_err().code,
        ErrorCode::Busy
    );
    s.cancel_job(&id).await.unwrap();
    assert_eq!(terminal(&mut rx).await.state, JobState::Canceled);
    assert_eq!(r.calls.load(Ordering::SeqCst), 1);
    assert_eq!(r.active_child_count(), 0);
}
#[tokio::test]
async fn cancel_during_validation_never_publishes() {
    let (d, r, s, mut rx, request) = setup(BlockAt::Validation, false);
    let id = s.start_job(request).await.unwrap();
    r.wait_entered().await;
    s.cancel_job(&id).await.unwrap();
    let end = terminal(&mut rx).await;
    assert_eq!(end.state, JobState::Canceled);
    assert!(!d.path().join(format!("source_processed_{id}.mp4")).exists());
    assert_eq!(r.active_child_count(), 0);
}
#[tokio::test]
async fn cancel_and_commit_choose_one_terminal_state() {
    let (d, r, s, mut rx, request) = setup(BlockAt::Validation, false);
    let id = s.start_job(request).await.unwrap();
    r.wait_entered().await;
    r.resume();
    let end = terminal(&mut rx).await;
    assert_eq!(end.state, JobState::Succeeded);
    let after = s.cancel_job(&id).await.unwrap();
    assert_eq!(after.state, JobState::Succeeded);
    assert_eq!(after.version, end.version);
    assert!(d.path().join(format!("source_processed_{id}.mp4")).exists());
}
#[test]
fn late_progress_cannot_reopen_terminal() {
    let mut snapshot: JobSnapshot =
        serde_json::from_str(include_str!("../../tests/fixtures/job-snapshot.json")).unwrap();
    snapshot.state = JobState::Succeeded;
    snapshot.version = 8;
    snapshot.progress = Some(1.0);
    assert!(!apply_progress(&mut snapshot, 20, 60));
    assert_eq!(snapshot.version, 8);
    assert_eq!(snapshot.progress, Some(1.0));
    let mut parser = ProgressParser::default();
    let values: Vec<_> = include_str!("../../tests/fixtures/progress/chunked.txt")
        .lines()
        .filter_map(|line| parser.push(line))
        .collect();
    assert_eq!(values, vec![30]);
    assert!(include_str!("../../tests/fixtures/progress/malformed.txt")
        .lines()
        .filter_map(|line| parser.push(line))
        .next()
        .is_none());
    for frame in include_str!("../../tests/fixtures/progress/late.txt")
        .lines()
        .filter_map(|line| parser.push(line))
    {
        assert!(!apply_progress(&mut snapshot, frame, 60));
    }
}
#[test]
fn log_tail_is_bounded_and_valid_utf8() {
    use video_editor::jobs::log::LogTail;
    let mut log = LogTail::default();
    log.append(&"中文日志".repeat(10000));
    let excerpt = log.excerpt();
    assert!(excerpt.text.len() <= 65536);
    assert!(excerpt.truncated);
    assert!(excerpt.text.ends_with("中文日志\n"));
}
#[tokio::test]
async fn cleanup_failure_is_pending() {
    let (d, r, s, mut rx, request) = setup(BlockAt::Validation, true);
    let id = s.start_job(request).await.unwrap();
    r.wait_entered().await;
    s.cancel_job(&id).await.unwrap();
    let end = terminal(&mut rx).await;
    assert_eq!(end.state, JobState::Canceled);
    assert!(end.cleanup_pending);
    assert!(d.path().join("records").join(format!("{id}.json")).exists());
}
#[tokio::test]
async fn hung_probe_is_reaped_on_shutdown() {
    let (_d, r, s, _rx, request) = setup(BlockAt::Probe, false);
    let clone = s.clone();
    let inspection = tokio::spawn(async move { clone.inspect_input(request.input_path).await });
    r.wait_entered().await;
    s.shutdown().await.unwrap();
    assert_eq!(
        inspection.await.unwrap().unwrap_err().code,
        ErrorCode::Canceled
    );
    assert_eq!(r.active_child_count(), 0);
}
