use super::tools::{Tool, ToolPaths};
use crate::contracts::{AppError, ErrorCode};
use async_trait::async_trait;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;
const LOG_LIMIT: usize = 65536;
const JSON_LIMIT: usize = 1024 * 1024;
#[derive(Debug, Clone, Copy)]
pub enum StdoutPolicy {
    CaptureJson,
    Stream,
    Discard,
}
#[derive(Debug, Clone)]
pub struct RunSpec {
    pub tool: Tool,
    pub args: Vec<String>,
    pub stdout_policy: StdoutPolicy,
}
#[derive(Debug)]
pub struct RunExit {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr_tail: String,
    pub canceled: bool,
}
#[derive(Debug)]
pub enum ProcessEvent {
    StdoutLine(String),
    StderrLine(String),
}
#[async_trait]
pub trait MediaRunner: Send + Sync {
    async fn run(
        &self,
        spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError>;
}
#[derive(Clone)]
pub struct NativeRunner {
    paths: ToolPaths,
}
impl NativeRunner {
    pub fn new(paths: ToolPaths) -> Self {
        Self { paths }
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
#[async_trait]
impl MediaRunner for NativeRunner {
    async fn run(
        &self,
        spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError> {
        let token = cancel.child_token();
        let guard = CancelOnDrop(token.clone());
        let path = match spec.tool {
            Tool::Ffmpeg => self.paths.ffmpeg.clone(),
            Tool::Ffprobe => self.paths.ffprobe.clone(),
        };
        let result = tokio::spawn(run_owned(path, spec, token, events))
            .await
            .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))?;
        drop(guard);
        result
    }
}
async fn run_owned(
    path: PathBuf,
    spec: RunSpec,
    cancel: CancellationToken,
    events: mpsc::Sender<ProcessEvent>,
) -> Result<RunExit, AppError> {
    if cancel.is_cancelled() {
        return Ok(RunExit {
            exit_code: None,
            stdout: String::new(),
            stderr_tail: String::new(),
            canceled: true,
        });
    }
    let mut command = tokio::process::Command::new(path);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| {
        AppError::new(ErrorCode::ToolMissing, "无法启动媒体工具").detail(e.to_string())
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new(ErrorCode::ProcessExit, "缺失 stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new(ErrorCode::ProcessExit, "缺失 stderr"))?;
    let (errors_tx, mut errors_rx) = mpsc::channel(2);
    let out_cancel = cancel.clone();
    let out_events = events.clone();
    let out_err = errors_tx.clone();
    let out = tokio::spawn(async move {
        let r = read_pipe(stdout, spec.stdout_policy, false, out_events, out_cancel).await;
        if let Err(e) = &r {
            let _ = out_err.send(e.clone()).await;
        }
        r
    });
    let err_cancel = cancel.clone();
    let err_events = events.clone();
    let err = tokio::spawn(async move {
        let r = read_pipe(stderr, StdoutPolicy::Stream, true, err_events, err_cancel).await;
        if let Err(e) = &r {
            let _ = errors_tx.send(e.clone()).await;
        }
        r
    });
    let mut failure = None;
    let status = tokio::select! {
     s=child.wait()=>Some(s),
     _=cancel.cancelled()=>None,
     _=events.closed()=>{cancel.cancel();None},
        Some(e)=errors_rx.recv()=>{failure=Some(e);cancel.cancel();None},
    };
    let canceled = status.is_none();
    let status = match status {
        Some(s) => s,
        None => {
            if spec.tool == Tool::Ffmpeg {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"q\n").await;
                }
                match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                    Ok(s) => s,
                    Err(_) => {
                        let _ = child.start_kill();
                        child.wait().await
                    }
                }
            } else {
                let _ = child.start_kill();
                child.wait().await
            }
        }
    }
    .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))?;
    if canceled {
        cancel.cancel();
    }
    let stdout = out
        .await
        .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))??;
    let stderr_tail = err
        .await
        .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))??;
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(RunExit {
        exit_code: status.code(),
        stdout,
        stderr_tail,
        canceled,
    })
}
async fn read_pipe<R: AsyncRead + Unpin>(
    mut pipe: R,
    policy: StdoutPolicy,
    stderr: bool,
    events: mpsc::Sender<ProcessEvent>,
    cancel: CancellationToken,
) -> Result<String, AppError> {
    let mut captured = Vec::new();
    let mut line = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = pipe
            .read(&mut chunk)
            .await
            .map_err(|e| AppError::new(ErrorCode::ProcessExit, e.to_string()))?;
        if n == 0 {
            break;
        }
        if stderr {
            captured.extend_from_slice(&chunk[..n]);
            if captured.len() > LOG_LIMIT {
                captured.drain(..captured.len() - LOG_LIMIT);
            }
        } else if matches!(policy, StdoutPolicy::CaptureJson) {
            if captured.len() + n > JSON_LIMIT {
                return Err(AppError::new(
                    ErrorCode::ProcessExit,
                    "媒体探测 JSON 超过限制",
                ));
            }
            captured.extend_from_slice(&chunk[..n]);
        }
        if matches!(policy, StdoutPolicy::Stream) {
            for b in &chunk[..n] {
                if *b == b'\n' {
                    let s = String::from_utf8_lossy(&line)
                        .trim_end_matches('\r')
                        .to_owned();
                    line.clear();
                    let event = if stderr {
                        ProcessEvent::StderrLine(s)
                    } else {
                        ProcessEvent::StdoutLine(s)
                    };
                    tokio::select! {_=cancel.cancelled()=>{},r=events.send(event)=>{if r.is_err(){cancel.cancel();}}}
                } else {
                    if line.len() >= LOG_LIMIT {
                        return Err(AppError::new(
                            ErrorCode::ProcessExit,
                            "媒体日志单行超过限制",
                        ));
                    }
                    line.push(*b);
                }
            }
        }
    }
    if !line.is_empty() && !cancel.is_cancelled() {
        let s = String::from_utf8_lossy(&line).into_owned();
        let event = if stderr {
            ProcessEvent::StderrLine(s)
        } else {
            ProcessEvent::StdoutLine(s)
        };
        tokio::select! {_=cancel.cancelled()=>{},r=events.send(event)=>{if r.is_err(){cancel.cancel();}}}
    }
    let mut s = String::from_utf8_lossy(&captured).into_owned();
    if stderr && s.len() > LOG_LIMIT {
        let mut cut = s.len() - LOG_LIMIT;
        while !s.is_char_boundary(cut) {
            cut += 1;
        }
        s.drain(..cut);
    }
    Ok(s)
}
