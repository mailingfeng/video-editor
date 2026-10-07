use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::AppError,
    native::{
        process::{MediaRunner, ProcessEvent, RunExit, RunSpec},
        tools::Tool,
    },
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlockAt {
    None,
    Probe,
    Validation,
}
pub struct ControlledRunner {
    pub at: BlockAt,
    pub entered: Semaphore,
    pub release: Semaphore,
    pub calls: AtomicUsize,
    pub active: AtomicUsize,
    pub add_foreign: bool,
    pub inputs: Mutex<Vec<String>>,
    pub conversions: Mutex<Vec<Vec<String>>>,
    pub gates: Mutex<std::collections::HashMap<String, Arc<Semaphore>>>,
    pub hold_cancel: AtomicBool,
    pub reaping: Semaphore,
    pub reap_release: Semaphore,
}
impl ControlledRunner {
    pub fn new(at: BlockAt, add_foreign: bool) -> Arc<Self> {
        Arc::new(Self {
            at,
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            add_foreign,
            inputs: Mutex::new(Vec::new()),
            conversions: Mutex::new(Vec::new()),
            gates: Mutex::new(std::collections::HashMap::new()),
            hold_cancel: AtomicBool::new(false),
            reaping: Semaphore::new(0),
            reap_release: Semaphore::new(0),
        })
    }
    pub async fn wait_entered(&self) {
        self.entered.acquire().await.unwrap().forget();
    }
    pub fn resume(&self) {
        self.release.add_permits(1);
    }
    pub fn active_child_count(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait]
impl MediaRunner for ControlledRunner {
    async fn run(
        &self,
        spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst);
        let _active = Active(&self.active);
        let path = spec.args.last().unwrap();
        let output = path.ends_with("output.mp4");
        let metadata = spec.args.iter().any(|a| a == "-show_streams");
        if metadata && !output {
            self.inputs.lock().unwrap().push(path.clone());
        }
        if metadata
            && match self.at {
                BlockAt::Probe => !output,
                BlockAt::Validation => output,
                BlockAt::None => false,
            }
        {
            self.entered.add_permits(1);
            let gate = self.gates.lock().unwrap().get(path).cloned();
            let release = gate.as_deref().unwrap_or(&self.release);
            tokio::select! {p=release.acquire()=>{p.unwrap().forget();},_=cancel.cancelled()=>{
                if self.hold_cancel.load(Ordering::SeqCst) {
                    self.reaping.add_permits(1);
                    self.reap_release.acquire().await.unwrap().forget();
                }
                return Ok(RunExit{exit_code:None,stdout:String::new(),stderr_tail:String::new(),canceled:true});
            }}
        }
        if cancel.is_cancelled() {
            return Ok(RunExit {
                exit_code: None,
                stdout: String::new(),
                stderr_tail: String::new(),
                canceled: true,
            });
        }
        let mut stdout = String::new();
        if spec.tool == Tool::Ffprobe && metadata {
            let mut j: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tests/fixtures/validation/valid.json"
            ))
            .unwrap();
            if !output {
                j["format"]["tags"]["title"] = serde_json::json!(std::path::Path::new(path)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap());
                j["streams"][1]["sample_rate"] = serde_json::json!("44100");
                j["streams"][1]["time_base"] = serde_json::json!("1/44100");
                j["streams"][1]["duration_ts"] = serde_json::json!(88200);
            }
            stdout = j.to_string();
        } else if spec.tool == Tool::Ffprobe {
            for i in 0..60 {
                let _ = events
                    .send(ProcessEvent::StdoutLine(format!(
                        "best_effort_timestamp={}|duration=512",
                        i * 512
                    )))
                    .await;
            }
        } else if spec.args.iter().any(|a| a == "-progress") {
            self.conversions.lock().unwrap().push(spec.args.clone());
            let input = &spec.args[spec.args.iter().position(|a| a == "-i").unwrap() + 1];
            let _ = events
                .send(ProcessEvent::StderrLine(format!("input: {input}")))
                .await;
            std::fs::write(path, b"converted").unwrap();
            if self.add_foreign {
                std::fs::write(
                    std::path::Path::new(path)
                        .parent()
                        .unwrap()
                        .join("foreign.txt"),
                    b"keep",
                )
                .unwrap();
            }
            for s in ["frame=60", "progress=end"] {
                let _ = events.send(ProcessEvent::StdoutLine(s.into())).await;
            }
        }
        Ok(RunExit {
            exit_code: Some(0),
            stdout,
            stderr_tail: String::new(),
            canceled: false,
        })
    }
}
