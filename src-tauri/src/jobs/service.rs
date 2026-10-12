use super::{
    log::{phase_message, LogExcerpt, LogTail},
    progress::{apply_progress, ProgressParser},
    state::{is_terminal, transition},
};
use crate::{
    contracts::*,
    media::{
        inputs::{collect_folder, collect_paths, InputCandidate},
        probe::probe_input,
        validate::{validate_output, ValidationResult},
    },
    native::{
        process::{MediaRunner, ProcessEvent, RunSpec, StdoutPolicy},
        tools::Tool,
    },
    output::{
        publish::publish_no_replace,
        workspace::{cleanup_workspace, prepare_workspace, update_record},
    },
    presets::basic::build_plan,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;
pub type SnapshotSink = Arc<dyn Fn(JobSnapshot) + Send + Sync>;
pub type QueueSink = Arc<dyn Fn(QueueSnapshot) + Send + Sync>;
#[derive(Clone)]
pub struct JobService {
    runner: Arc<dyn MediaRunner>,
    record_dir: PathBuf,
    state: Arc<Mutex<Inner>>,
    emit: SnapshotSink,
    emit_queue: QueueSink,
}
struct Inner {
    active: HashMap<String, Activity>,
    jobs: HashMap<String, JobData>,
    current: Option<String>,
    closing: bool,
    queue: Vec<QueueEntry>,
    queue_version: u64,
    batch: Option<BatchSettings>,
}
struct Activity {
    inspection: bool,
    cancel: CancellationToken,
    done: CancellationToken,
}
struct QueueEntry {
    key: String,
    item: QueueItem,
}
struct Launch {
    id: String,
    request: StartJobRequest,
    cancel: CancellationToken,
}
struct JobData {
    snapshot: JobSnapshot,
    log: LogTail,
    plan: Option<ProcessingPlan>,
    validation: Option<ValidationResult>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredJob {
    snapshot: JobSnapshot,
    log: LogExcerpt,
    validation: Option<ValidationResult>,
    #[serde(default)]
    report_version: u32,
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
impl JobService {
    pub fn new(runner: Arc<dyn MediaRunner>, record_dir: PathBuf, emit: SnapshotSink) -> Self {
        Self::with_queue_sink(runner, record_dir, emit, Arc::new(|_| {}))
    }
    pub fn with_queue_sink(
        runner: Arc<dyn MediaRunner>,
        record_dir: PathBuf,
        emit: SnapshotSink,
        emit_queue: QueueSink,
    ) -> Self {
        if let Err(error) = scrub_history(&record_dir) {
            eprintln!("历史记录整理未完成：{:?}", error.code);
        }
        let mut inner = Inner {
            active: HashMap::new(),
            jobs: HashMap::new(),
            current: None,
            closing: false,
            queue: Vec::new(),
            queue_version: 0,
            batch: None,
        };
        if let Ok(bytes) = std::fs::read(record_dir.join("history/latest.json")) {
            if let Ok(mut stored) = serde_json::from_slice::<StoredJob>(&bytes) {
                if is_terminal(stored.snapshot.state) {
                    stored.snapshot.error = stored.snapshot.error.map(|e| e.for_user());
                    let log = if stored.report_version == 2 {
                        LogTail::from_excerpt(stored.log)
                    } else {
                        LogTail::from_legacy(&stored.snapshot)
                    };
                    let id = stored.snapshot.job_id.clone();
                    inner.current = Some(id.clone());
                    inner.jobs.insert(
                        id,
                        JobData {
                            snapshot: stored.snapshot,
                            log,
                            plan: None,
                            validation: stored.validation,
                        },
                    );
                }
            }
        }
        Self {
            runner,
            record_dir,
            state: Arc::new(Mutex::new(inner)),
            emit,
            emit_queue,
        }
    }
    pub async fn queue_snapshot(&self) -> QueueSnapshot {
        queue_snapshot(&*self.state.lock().await)
    }
    pub async fn import_folder(&self, path: String) -> Result<QueueSnapshot, AppError> {
        ensure_idle(&*self.state.lock().await)?;
        let inputs = tokio::task::spawn_blocking(move || collect_folder(Path::new(&path)))
            .await
            .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))??;
        if inputs.is_empty() {
            return Err(AppError::new(
                ErrorCode::UnsupportedInput,
                "所选文件夹当前层没有支持的视频文件（MP4、MOV、M4V、MKV、WebM）",
            ));
        }
        self.append_inputs(inputs).await
    }
    pub async fn import_paths(&self, paths: Vec<String>) -> Result<QueueSnapshot, AppError> {
        ensure_idle(&*self.state.lock().await)?;
        let inputs = tokio::task::spawn_blocking(move || {
            collect_paths(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())
        })
        .await
        .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))??;
        self.append_inputs(inputs).await
    }
    async fn append_inputs(&self, inputs: Vec<InputCandidate>) -> Result<QueueSnapshot, AppError> {
        let snapshot = {
            let mut state = self.state.lock().await;
            ensure_idle(&state)?;
            for input in inputs {
                if state.queue.iter().any(|e| e.key == input.key) {
                    continue;
                }
                state.queue.push(QueueEntry {
                    key: input.key,
                    item: QueueItem {
                        item_id: uuid::Uuid::new_v4().to_string(),
                        input_path: input.input_path,
                        state: QueueItemState::Waiting,
                        job_id: None,
                        snapshot: None,
                        media: None,
                    },
                });
            }
            state.queue_version += 1;
            queue_snapshot(&state)
        };
        (self.emit_queue)(snapshot.clone());
        Ok(snapshot)
    }
    pub async fn remove_item(&self, item_id: &str) -> Result<QueueSnapshot, AppError> {
        let snapshot = {
            let mut state = self.state.lock().await;
            ensure_idle(&state)?;
            let index = state
                .queue
                .iter()
                .position(|e| e.item.item_id == item_id)
                .ok_or_else(unknown_job)?;
            state.queue.remove(index);
            state.queue_version += 1;
            queue_snapshot(&state)
        };
        (self.emit_queue)(snapshot.clone());
        Ok(snapshot)
    }
    pub async fn start_batch(&self, settings: BatchSettings) -> Result<QueueSnapshot, AppError> {
        let (snapshot, launches) = {
            let mut state = self.state.lock().await;
            ensure_idle(&state)?;
            if !state
                .queue
                .iter()
                .any(|e| e.item.state == QueueItemState::Waiting)
            {
                return Err(AppError::new(ErrorCode::UnsupportedInput, "没有待处理文件"));
            }
            state.batch = Some(settings);
            let launches = reserve_next(&mut state);
            state.queue_version += 1;
            (queue_snapshot(&state), launches)
        };
        self.launch(launches);
        (self.emit_queue)(snapshot.clone());
        Ok(snapshot)
    }
    pub async fn cancel_item(&self, item_id: &str) -> Result<QueueSnapshot, AppError> {
        let job_id = {
            let mut state = self.state.lock().await;
            let entry = state
                .queue
                .iter_mut()
                .find(|e| e.item.item_id == item_id)
                .ok_or_else(unknown_job)?;
            let job_id = entry.item.job_id.clone();
            if entry.item.state == QueueItemState::Waiting {
                entry.item.state = QueueItemState::Canceled;
                state.queue_version += 1;
            }
            job_id
        };
        if let Some(id) = job_id {
            self.cancel_job(&id).await?;
        }
        let snapshot = self.queue_snapshot().await;
        (self.emit_queue)(snapshot.clone());
        Ok(snapshot)
    }
    fn launch(&self, launches: Vec<Launch>) {
        for launch in launches {
            let service = self.clone();
            tokio::spawn(async move {
                service
                    .execute(&launch.id, launch.request, launch.cancel)
                    .await;
            });
        }
    }
    pub async fn inspect_input(&self, path: String) -> Result<MediaInfo, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        let done = CancellationToken::new();
        {
            let mut state = self.state.lock().await;
            ensure_idle(&state)?;
            state.active.insert(
                id.clone(),
                Activity {
                    inspection: true,
                    cancel: cancel.clone(),
                    done: done.clone(),
                },
            );
        }
        let guard = CancelOnDrop(cancel.clone());
        let (tx, rx) = oneshot::channel();
        let service = self.clone();
        tokio::spawn(async move {
            let result = probe_input(service.runner.as_ref(), Path::new(&path), cancel).await;
            service.release_activity(&id).await;
            done.cancel();
            let _ = tx.send(result);
        });
        let result = rx
            .await
            .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))?;
        drop(guard);
        result
    }
    pub async fn cancel_inspection(&self) -> Result<(), AppError> {
        let active = {
            let state = self.state.lock().await;
            state
                .active
                .values()
                .find(|a| a.inspection)
                .map(|a| (a.cancel.clone(), a.done.clone()))
        };
        if let Some((cancel, done)) = active {
            cancel.cancel();
            done.cancelled().await;
        }
        Ok(())
    }
    pub async fn start_job(&self, request: StartJobRequest) -> Result<String, AppError> {
        let (launch, snapshot) = {
            let mut state = self.state.lock().await;
            ensure_idle(&state)?;
            let launch = reserve_job(&mut state, request);
            let snapshot = state.jobs[&launch.id].snapshot.clone();
            (launch, snapshot)
        };
        let id = launch.id.clone();
        self.launch(vec![launch]);
        (self.emit)(snapshot);
        Ok(id)
    }
    pub async fn snapshot(&self, job_id: &str) -> Result<JobSnapshot, AppError> {
        self.state
            .lock()
            .await
            .jobs
            .get(job_id)
            .map(|j| j.snapshot.clone())
            .ok_or_else(unknown_job)
    }
    pub async fn current_snapshot(&self) -> Result<Option<JobSnapshot>, AppError> {
        let state = self.state.lock().await;
        Ok(state
            .current
            .as_ref()
            .and_then(|id| state.jobs.get(id))
            .map(|j| j.snapshot.clone()))
    }
    pub async fn cancel_job(&self, job_id: &str) -> Result<JobSnapshot, AppError> {
        let snapshot = {
            let mut state = self.state.lock().await;
            let current = state
                .jobs
                .get(job_id)
                .ok_or_else(unknown_job)?
                .snapshot
                .clone();
            if is_terminal(current.state) || current.state == JobState::Committing {
                return Ok(current);
            }
            let token = state
                .active
                .get(job_id)
                .filter(|a| !a.inspection)
                .map(|a| a.cancel.clone())
                .ok_or_else(unknown_job)?;
            let job = state.jobs.get_mut(job_id).ok_or_else(unknown_job)?;
            transition(&mut job.snapshot, JobState::Canceling);
            job.log.append(phase_message(JobState::Canceling));
            token.cancel();
            job.snapshot.clone()
        };
        (self.emit)(snapshot.clone());
        Ok(snapshot)
    }
    pub async fn read_log(&self, job_id: &str) -> Result<LogExcerpt, AppError> {
        self.state
            .lock()
            .await
            .jobs
            .get(job_id)
            .map(|j| j.log.excerpt())
            .ok_or_else(unknown_job)
    }
    pub async fn shutdown(&self) -> Result<(), AppError> {
        let done = {
            let mut state = self.state.lock().await;
            state.closing = true;
            state.batch = None;
            state.queue_version += 1;
            state
                .active
                .iter()
                .map(|(id, a)| {
                    if a.inspection
                        || state.jobs.get(id).is_some_and(|j| {
                            j.snapshot.state != JobState::Committing
                                && !is_terminal(j.snapshot.state)
                        })
                    {
                        a.cancel.cancel();
                    }
                    a.done.clone()
                })
                .collect::<Vec<_>>()
        };
        for done in done {
            done.cancelled().await;
        }
        Ok(())
    }
    async fn release_activity(&self, id: &str) {
        let mut state = self.state.lock().await;
        if let Some(activity) = state.active.remove(id) {
            activity.done.cancel();
        }
    }
    async fn set_phase(
        &self,
        id: &str,
        phase: JobState,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let snapshot = {
            let mut state = self.state.lock().await;
            let job = state.jobs.get_mut(id).ok_or_else(unknown_job)?;
            if cancel.is_cancelled() || job.snapshot.state == JobState::Canceling {
                return Err(canceled());
            }
            if !transition(&mut job.snapshot, phase) {
                return Err(AppError::new(ErrorCode::ProcessExit, "任务状态转换无效"));
            }
            job.log.append(phase_message(phase));
            if let Some(plan) = &job.plan {
                update_record(&plan.workspace, Some(&job.snapshot))?;
            }
            job.snapshot.clone()
        };
        (self.emit)(snapshot);
        Ok(())
    }
    async fn execute(&self, id: &str, request: StartJobRequest, cancel: CancellationToken) {
        let mut workspace = None;
        let result: Result<ValidationResult, AppError> = async {
            if cancel.is_cancelled() {
                return Err(canceled());
            }
            let info = probe_input(
                self.runner.as_ref(),
                Path::new(&request.input_path),
                cancel.clone(),
            )
            .await?;
            let queue = {
                let mut state = self.state.lock().await;
                if let Some(entry) = state
                    .queue
                    .iter_mut()
                    .find(|e| e.item.job_id.as_deref() == Some(id))
                {
                    entry.item.media = Some(info.clone());
                    state.queue_version += 1;
                    Some(queue_snapshot(&state))
                } else {
                    None
                }
            };
            if let Some(queue) = queue {
                (self.emit_queue)(queue);
            }
            self.set_phase(id, JobState::Preparing, &cancel).await?;
            let w = prepare_workspace(
                Path::new(&request.output_directory),
                Path::new(&info.identity.canonical_path),
                id,
                &self.record_dir,
            )?;
            workspace = Some(w.clone());
            let plan = build_plan(&info, &request, w, id)?;
            {
                let mut state = self.state.lock().await;
                let job = state.jobs.get_mut(id).ok_or_else(unknown_job)?;
                update_record(&plan.workspace, Some(&job.snapshot))?;
                job.plan = Some(plan.clone());
            }
            self.set_phase(id, JobState::Running, &cancel).await?;
            let (tx, mut rx) = mpsc::channel(256);
            let service = self.clone();
            let job_id = id.to_owned();
            let frames = plan.policy.frame_count;
            let events = tokio::spawn(async move {
                let mut parser = ProgressParser::default();
                while let Some(event) = rx.recv().await {
                    service.on_event(&job_id, event, &mut parser, frames).await;
                }
            });
            let exit = self
                .runner
                .run(
                    RunSpec {
                        tool: Tool::Ffmpeg,
                        args: plan.args.clone(),
                        stdout_policy: StdoutPolicy::Stream,
                    },
                    cancel.clone(),
                    tx,
                )
                .await;
            let _ = events.await;
            let exit = exit?;
            if cancel.is_cancelled() || exit.canceled {
                return Err(canceled());
            }
            if exit.exit_code != Some(0) {
                let code = if exit.stderr_tail.contains("No space left on device") {
                    ErrorCode::DiskFull
                } else {
                    ErrorCode::ProcessExit
                };
                return Err(AppError::new(code, "视频处理进程退出").detail(exit.stderr_tail));
            }
            self.set_phase(id, JobState::Validating, &cancel).await?;
            let mut validation =
                validate_output(self.runner.as_ref(), &plan, cancel.clone()).await?;
            self.set_phase(id, JobState::Committing, &cancel).await?;
            validation.output_path = publish_no_replace(&plan.workspace)?
                .to_str()
                .ok_or_else(|| AppError::new(ErrorCode::OutputPermission, "输出路径无法表示"))?
                .to_owned();
            Ok(validation)
        }
        .await;
        let cleanup = workspace.as_ref().map(cleanup_workspace).transpose();
        let (snapshot, queue, launches) = {
            let mut state = self.state.lock().await;
            let Some(job) = state.jobs.get_mut(id) else {
                return;
            };
            match result {
                Ok(validation) => {
                    transition(&mut job.snapshot, JobState::Succeeded);
                    job.snapshot.progress = Some(1.0);
                    job.snapshot.output_path = Some(validation.output_path.clone());
                    job.validation = Some(validation);
                    job.log.append(phase_message(JobState::Succeeded));
                }
                Err(error) => {
                    let cancelled = job.snapshot.state == JobState::Canceling
                        || (cancel.is_cancelled() && job.snapshot.state != JobState::Committing)
                        || error.code == ErrorCode::Canceled;
                    if cancelled {
                        if job.snapshot.state != JobState::Canceling {
                            transition(&mut job.snapshot, JobState::Canceling);
                        }
                        transition(&mut job.snapshot, JobState::Canceled);
                    } else {
                        transition(&mut job.snapshot, JobState::Failed);
                    }
                    let error = if cancelled { canceled() } else { error }.for_user();
                    job.log.append(&format!(
                        "{}：{}",
                        phase_message(job.snapshot.state),
                        error.message
                    ));
                    job.snapshot.error = Some(error);
                }
            }
            if let Err(error) = cleanup {
                let error = error.for_user();
                job.snapshot.cleanup_pending = true;
                job.log.append(&error.to_string());
                if job.snapshot.error.is_none() {
                    job.snapshot.error = Some(error);
                }
                if let Some(plan) = &job.plan {
                    let _ = update_record(&plan.workspace, Some(&job.snapshot));
                }
            }
            job.snapshot.ended_at_ms = Some(now_ms());
            job.snapshot.version += 1;
            if let Err(error) = self.save_history(job) {
                let error = error.for_user();
                job.log
                    .append("处理记录暂时无法保存，请检查磁盘空间与权限。");
                if job.snapshot.error.is_none() {
                    job.snapshot.error = Some(error);
                }
            }
            let snapshot = job.snapshot.clone();
            if let Some(activity) = state.active.remove(id) {
                activity.done.cancel();
            }
            let launches = reserve_next(&mut state);
            state.queue_version += 1;
            (snapshot, queue_snapshot(&state), launches)
        };
        self.launch(launches);
        (self.emit_queue)(queue);
        (self.emit)(snapshot);
    }
    async fn on_event(
        &self,
        id: &str,
        event: ProcessEvent,
        parser: &mut ProgressParser,
        frames: u64,
    ) {
        let snapshot = {
            let mut state = self.state.lock().await;
            let Some(job) = state.jobs.get_mut(id) else {
                return;
            };
            if is_terminal(job.snapshot.state) {
                return;
            }
            match event {
                ProcessEvent::StderrLine(_) => None,
                ProcessEvent::StdoutLine(line) => {
                    let previous_bucket = (job.snapshot.progress.unwrap_or(0.0) * 10.0) as u32;
                    parser
                        .push(&line)
                        .filter(|n| apply_progress(&mut job.snapshot, *n, frames))
                        .map(|_| {
                            let bucket = (job.snapshot.progress.unwrap_or(0.0) * 10.0) as u32;
                            if bucket > previous_bucket {
                                job.log.append(&format!("处理中：{}%", bucket * 10));
                            }
                            job.snapshot.clone()
                        })
                }
            }
        };
        if let Some(snapshot) = snapshot {
            (self.emit)(snapshot);
        }
    }
    fn save_history(&self, job: &JobData) -> Result<(), AppError> {
        let root = self.record_dir.join("history");
        std::fs::create_dir_all(&root).map_err(AppError::io)?;
        let data = serde_json::to_vec_pretty(&StoredJob {
            snapshot: job.snapshot.clone(),
            log: job.log.excerpt(),
            validation: job.validation.clone(),
            report_version: 2,
        })
        .map_err(|e| AppError::new(ErrorCode::OutputPermission, e.to_string()))?;
        for target in [
            root.join(format!("{}.json", job.snapshot.job_id)),
            root.join("latest.json"),
        ] {
            let tmp = root.join(format!("{}.tmp", uuid::Uuid::new_v4()));
            std::fs::write(&tmp, &data).map_err(AppError::io)?;
            std::fs::rename(tmp, target).map_err(AppError::io)?;
        }
        Ok(())
    }
}
// Older versions persisted the frozen plan. Read only the user-facing fields,
// then atomically replace every regular history record, including latest.json.
fn scrub_history(records: &Path) -> Result<(), AppError> {
    let root = records.join("history");
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError::io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::new(ErrorCode::OutputPermission, "历史目录无效"));
    }
    for entry in std::fs::read_dir(&root).map_err(AppError::io)? {
        let entry = entry.map_err(AppError::io)?;
        let path = entry.path();
        if !entry.file_type().map_err(AppError::io)?.is_file()
            || path.extension().and_then(|ext| ext.to_str()) != Some("json")
        {
            continue;
        }
        let old = std::fs::read(&path).map_err(AppError::io)?;
        let Ok(mut stored) = serde_json::from_slice::<StoredJob>(&old) else {
            continue;
        };
        stored.snapshot.error = stored.snapshot.error.map(|error| error.for_user());
        if stored.report_version != 2 {
            stored.log = LogTail::from_legacy(&stored.snapshot).excerpt();
            stored.report_version = 2;
        }
        let data = serde_json::to_vec_pretty(&stored)
            .map_err(|error| AppError::new(ErrorCode::OutputPermission, error.to_string()))?;
        if data != old {
            let tmp = root.join(format!("{}.tmp", uuid::Uuid::new_v4()));
            std::fs::write(&tmp, data).map_err(AppError::io)?;
            if let Err(error) = std::fs::rename(&tmp, &path) {
                let _ = std::fs::remove_file(tmp);
                return Err(AppError::io(error));
            }
        }
    }
    Ok(())
}
fn queue_snapshot(state: &Inner) -> QueueSnapshot {
    QueueSnapshot {
        version: state.queue_version,
        running: state.batch.is_some(),
        items: state
            .queue
            .iter()
            .map(|entry| {
                let mut item = entry.item.clone();
                item.snapshot = item
                    .job_id
                    .as_ref()
                    .and_then(|id| state.jobs.get(id))
                    .map(|job| job.snapshot.clone());
                item
            })
            .collect(),
    }
}

fn reserve_job(state: &mut Inner, request: StartJobRequest) -> Launch {
    let id = uuid::Uuid::new_v4().to_string();
    let cancel = CancellationToken::new();
    state.active.insert(
        id.clone(),
        Activity {
            inspection: false,
            cancel: cancel.clone(),
            done: CancellationToken::new(),
        },
    );
    state.current = Some(id.clone());
    let mut log = LogTail::default();
    log.append(phase_message(JobState::Probing));
    state.jobs.insert(
        id.clone(),
        JobData {
            snapshot: JobSnapshot {
                job_id: id.clone(),
                version: 1,
                state: JobState::Probing,
                progress: None,
                started_at_ms: now_ms(),
                ended_at_ms: None,
                output_path: None,
                error: None,
                cleanup_pending: false,
            },
            log,
            plan: None,
            validation: None,
        },
    );
    Launch {
        id,
        request,
        cancel,
    }
}

fn reserve_next(state: &mut Inner) -> Vec<Launch> {
    let mut launches = Vec::new();
    if state.closing {
        return launches;
    }
    let Some(settings) = state.batch.clone() else {
        return launches;
    };
    while state.active.len() < 2 {
        let Some(index) = state
            .queue
            .iter()
            .position(|e| e.item.state == QueueItemState::Waiting)
        else {
            break;
        };
        let request = StartJobRequest {
            input_path: state.queue[index].item.input_path.clone(),
            output_directory: settings.output_directory.clone(),
            preset_id: settings.preset_id.clone(),
            metadata: settings.metadata.clone(),
        };
        let launch = reserve_job(state, request);
        state.queue[index].item.state = QueueItemState::Started;
        state.queue[index].item.job_id = Some(launch.id.clone());
        launches.push(launch);
    }
    if state.active.is_empty() {
        state.batch = None;
    }
    launches
}

fn ensure_idle(state: &Inner) -> Result<(), AppError> {
    if state.closing {
        Err(AppError::new(ErrorCode::Busy, "应用正在关闭"))
    } else if !state.active.is_empty() || state.batch.is_some() {
        Err(AppError::new(ErrorCode::Busy, "请先结束当前检查或处理任务"))
    } else {
        Ok(())
    }
}
fn unknown_job() -> AppError {
    AppError::new(ErrorCode::UnsupportedInput, "未知任务")
}
fn canceled() -> AppError {
    AppError::new(ErrorCode::Canceled, "任务已取消")
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
