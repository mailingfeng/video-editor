use async_trait::async_trait;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*,
    jobs::{service::JobService, state::is_terminal},
    media::probe::file_identity,
    native::{
        process::{MediaRunner, NativeRunner, ProcessEvent, RunExit, RunSpec},
        tools::{current_target, resolve_tools},
    },
    output::{recovery::recover_incomplete_jobs, workspace::JobRecord},
};

// Production service/runner and real bundled tools; only input pacing is added in
// cancellation/crash tests so a fast CI machine cannot finish before the action.
struct PacedRunner(NativeRunner);
#[async_trait]
impl MediaRunner for PacedRunner {
    async fn run(
        &self,
        mut spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError> {
        if spec.args.iter().any(|arg| arg == "-progress") {
            let input = spec.args.iter().position(|arg| arg == "-i").unwrap();
            spec.args.insert(input, "-re".into());
        }
        self.0.run(spec, cancel, events).await
    }
}
fn runner(paced: bool) -> Arc<dyn MediaRunner> {
    let installed = std::env::var_os("VIDEO_EDITOR_INSTALLED_EXE").map(PathBuf::from);
    let native = NativeRunner::new(
        resolve_tools(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            current_target(),
            installed.as_deref(),
        )
        .unwrap(),
    );
    if paced {
        Arc::new(PacedRunner(native))
    } else {
        Arc::new(native)
    }
}
fn source() -> PathBuf {
    PathBuf::from(std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").unwrap()).join("lifecycle 空格.mp4")
}
fn request(output: &Path) -> StartJobRequest {
    StartJobRequest {
        input_path: source().display().to_string(),
        output_directory: output.display().to_string(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    }
}
fn record(root: &Path, id: &str) -> JobRecord {
    serde_json::from_slice(&fs::read(root.join("records").join(format!("{id}.json"))).unwrap())
        .unwrap()
}
fn output(root: &Path) -> PathBuf {
    let output = root.join("输出 空格 '目录'");
    fs::create_dir(&output).unwrap();
    output
}
async fn next(rx: &mut mpsc::UnboundedReceiver<JobSnapshot>) -> JobSnapshot {
    tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .expect("media job timed out")
        .expect("missing job snapshot")
}
async fn terminal(rx: &mut mpsc::UnboundedReceiver<JobSnapshot>) -> JobSnapshot {
    loop {
        let snapshot = next(rx).await;
        if is_terminal(snapshot.state) {
            return snapshot;
        }
    }
}

// Break caught: skipping cancel/reaping/cleanup after a real child has written media.
#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_cancel_after_media_progress() {
    let before = file_identity(&source(), &CancellationToken::new())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let output = output(root.path());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let service = JobService::new(
        runner(true),
        root.path().join("records"),
        Arc::new(move |s| {
            let _ = tx.send(s);
        }),
    );
    let id = service.start_job(request(&output)).await.unwrap();
    loop {
        let snapshot = next(&mut rx).await;
        assert!(
            !is_terminal(snapshot.state),
            "finished before cancel: {snapshot:?}"
        );
        if snapshot.state == JobState::Running && snapshot.progress.is_some_and(|p| p > 0.0) {
            break;
        }
    }
    let workspace = record(root.path(), &id).workspace;
    assert!(fs::metadata(&workspace.temp_path).unwrap().len() > 0);
    let canceled_at = Instant::now();
    service.cancel_job(&id).await.unwrap();
    let result = terminal(&mut rx).await;
    assert_eq!(result.state, JobState::Canceled);
    assert!(canceled_at.elapsed() < Duration::from_secs(6));
    assert!(result.output_path.is_none());
    assert!(!result.cleanup_pending);
    assert!(!Path::new(&workspace.directory).exists());
    assert!(!Path::new(&workspace.record_path).exists());
    assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    assert_eq!(
        file_identity(&source(), &CancellationToken::new())
            .await
            .unwrap(),
        before
    );
    // A second conversion also catches a stale busy slot after cancellation.
    service.start_job(request(&output)).await.unwrap();
    assert_eq!(terminal(&mut rx).await.state, JobState::Succeeded);
    service.shutdown().await.unwrap();
}

// Break caught: replacing an existing destination when validated media is published.
#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_output_conflict_preserves_existing_file() {
    let before = file_identity(&source(), &CancellationToken::new())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let output = output(root.path());
    let records = root.path().join("records");
    let callback_records = records.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let service = JobService::new(
        runner(false),
        records,
        Arc::new(move |s| {
            if s.state == JobState::Running && s.progress.is_none() {
                let record: JobRecord = serde_json::from_slice(
                    &fs::read(callback_records.join(format!("{}.json", s.job_id))).unwrap(),
                )
                .unwrap();
                use std::io::Write;
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(record.workspace.final_path)
                    .unwrap()
                    .write_all(b"existing destination must survive")
                    .unwrap();
            }
            let _ = tx.send(s);
        }),
    );
    service.start_job(request(&output)).await.unwrap();
    let result = terminal(&mut rx).await;
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.error.unwrap().code, ErrorCode::OutputConflict);
    assert!(result.output_path.is_none());
    assert!(!result.cleanup_pending);
    let files: Vec<_> = fs::read_dir(&output)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        fs::read(&files[0]).unwrap(),
        b"existing destination must survive"
    );
    assert_eq!(
        file_identity(&source(), &CancellationToken::new())
            .await
            .unwrap(),
        before
    );
    service.shutdown().await.unwrap();
}

struct OwnedWorker(Child);
impl OwnedWorker {
    fn stop(&mut self) {
        #[cfg(windows)]
        {
            let status = Command::new("taskkill.exe")
                .args(["/PID", &self.0.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "could not stop owned worker tree");
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        self.0.wait().unwrap();
    }
}
impl Drop for OwnedWorker {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            self.stop();
        }
    }
}

// Break caught: startup cleanup leaves owned partial media or deletes foreign files.
// Kills only the owned test service + its tool tree; this does not launch the UI.
#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_crash_recovery_preserves_foreign_files() {
    let before = file_identity(&source(), &CancellationToken::new())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let output = output(root.path());
    let foreign = output.join("其他文件.mp4");
    fs::write(&foreign, b"unrelated user file").unwrap();
    let log = fs::File::create(root.path().join("worker.log")).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "crash_worker", "--ignored", "--nocapture"])
        .env("VIDEO_EDITOR_CRASH_ROOT", root.path())
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut worker = OwnedWorker(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    let ready = root.path().join("ready.json");
    let workspace: OutputWorkspace = loop {
        if let Ok(bytes) = fs::read(&ready) {
            if let Ok(workspace) = serde_json::from_slice(&bytes) {
                break workspace;
            }
        }
        assert!(
            Instant::now() < deadline,
            "worker not ready: {}",
            fs::read_to_string(root.path().join("worker.log")).unwrap()
        );
        assert!(
            worker.0.try_wait().unwrap().is_none(),
            "worker exited early"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    worker.stop();
    assert!(Path::new(&workspace.record_path).exists());
    assert!(fs::metadata(&workspace.temp_path).unwrap().len() > 0);
    assert!(!Path::new(&workspace.final_path).exists());
    let recovered = recover_incomplete_jobs(&root.path().join("records")).unwrap();
    assert_eq!(recovered.cleaned, vec![workspace.job_id]);
    assert!(recovered.pending.is_empty());
    assert!(recovered.errors.is_empty());
    assert!(!Path::new(&workspace.directory).exists());
    assert!(!Path::new(&workspace.record_path).exists());
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    assert_eq!(fs::read(&foreign).unwrap(), b"unrelated user file");
    assert_eq!(
        file_identity(&source(), &CancellationToken::new())
            .await
            .unwrap(),
        before
    );
}

#[tokio::test]
#[ignore = "subprocess entry used only by generated_crash_recovery_preserves_foreign_files"]
async fn crash_worker() {
    let Some(root) = std::env::var_os("VIDEO_EDITOR_CRASH_ROOT").map(PathBuf::from) else {
        return;
    };
    let callback_root = root.clone();
    let service = JobService::new(
        runner(true),
        root.join("records"),
        Arc::new(move |s| {
            if s.state == JobState::Running && s.progress.is_some_and(|p| p > 0.0) {
                let workspace = record(&callback_root, &s.job_id).workspace;
                if fs::metadata(&workspace.temp_path).is_ok_and(|m| m.len() > 0) {
                    fs::write(
                        callback_root.join("ready.json"),
                        serde_json::to_vec(&workspace).unwrap(),
                    )
                    .unwrap();
                }
            }
        }),
    );
    service
        .start_job(request(&root.join("输出 空格 '目录'")))
        .await
        .unwrap();
    std::future::pending::<()>().await;
}
