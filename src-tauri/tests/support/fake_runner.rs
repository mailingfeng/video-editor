use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::{AppError, ErrorCode},
    native::{
        process::{MediaRunner, ProcessEvent, RunExit, RunSpec},
        tools::Tool,
    },
};
pub struct Response {
    pub tool: Tool,
    pub required_arg: String,
    pub stdout: String,
    pub lines: Vec<String>,
    pub stderr: String,
    pub exit_code: i32,
}
pub struct FakeRunner {
    responses: Mutex<std::collections::VecDeque<Response>>,
    active: Arc<AtomicUsize>,
}
impl FakeRunner {
    pub fn new(responses: Vec<Response>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            active: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub fn active_child_count(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
}
#[async_trait]
impl MediaRunner for FakeRunner {
    async fn run(
        &self,
        spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError> {
        let r = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected media invocation");
        assert_eq!(spec.tool, r.tool);
        assert!(
            spec.args.contains(&r.required_arg),
            "missing required argument {}",
            r.required_arg
        );
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "canceled"));
        }
        self.active.fetch_add(1, Ordering::SeqCst);
        for line in r.lines {
            if events.send(ProcessEvent::StdoutLine(line)).await.is_err() {
                break;
            }
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(RunExit {
            exit_code: Some(r.exit_code),
            stdout: r.stdout,
            stderr_tail: r.stderr,
            canceled: cancel.is_cancelled(),
        })
    }
}
