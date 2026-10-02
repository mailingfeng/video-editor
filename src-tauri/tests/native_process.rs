mod support;
#[tokio::test(start_paused = true)]
async fn hung_ffmpeg_is_killed_after_three_second_grace() {
    let f = ProcessFixture::new("q_hang");
    let r = NativeRunner::new(f.paths());
    let (tx, _rx) = mpsc::channel(256);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let args = f.args();
    let job = tokio::spawn(async move {
        r.run(
            RunSpec {
                tool: Tool::Ffmpeg,
                args,
                stdout_policy: StdoutPolicy::Discard,
            },
            cancel,
            tx,
        )
        .await
    });
    while !f.is_alive() {
        tokio::task::yield_now().await;
    }
    token.cancel();
    while !f.q_seen() {
        tokio::task::yield_now().await;
    }
    assert!(f.is_alive());
    tokio::time::advance(Duration::from_secs(3)).await;
    let result = job.await.unwrap().unwrap();
    assert!(result.canceled);
    assert!(!f.is_alive());
}
#[tokio::test]
async fn closed_pipes_before_exit_are_not_cancellation() {
    let f = ProcessFixture::new("closed");
    let r = NativeRunner::new(f.paths());
    let (tx, _rx) = mpsc::channel(256);
    let exit = r
        .run(
            RunSpec {
                tool: Tool::Ffprobe,
                args: f.args(),
                stdout_policy: StdoutPolicy::Discard,
            },
            CancellationToken::new(),
            tx,
        )
        .await
        .unwrap();
    assert!(!exit.canceled);
    assert_eq!(exit.exit_code, Some(0));
}
use support::process_fixture::ProcessFixture;
use tokio::{
    sync::mpsc,
    time::{timeout, Duration},
};
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::ErrorCode,
    native::{
        process::{MediaRunner, NativeRunner, ProcessEvent, RunSpec, StdoutPolicy},
        tools::{resolve_tools, Tool},
    },
};
#[tokio::test]
async fn chunked_stdout_is_reassembled() {
    let f = ProcessFixture::new("chunked");
    let r = NativeRunner::new(f.paths());
    let (tx, mut rx) = mpsc::channel(256);
    let j = tokio::spawn(async move {
        r.run(
            RunSpec {
                tool: Tool::Ffmpeg,
                args: f.args(),
                stdout_policy: StdoutPolicy::Stream,
            },
            CancellationToken::new(),
            tx,
        )
        .await
    });
    let mut lines = Vec::new();
    while let Some(ProcessEvent::StdoutLine(s)) = rx.recv().await {
        lines.push(s);
    }
    assert_eq!(lines, ["frame=3", "中文完成"]);
    assert_eq!(j.await.unwrap().unwrap().exit_code, Some(0));
}
#[tokio::test]
async fn stderr_flood_is_drained_and_truncated() {
    let f = ProcessFixture::new("flood");
    let r = NativeRunner::new(f.paths());
    let (tx, mut rx) = mpsc::channel(256);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let exit = r
        .run(
            RunSpec {
                tool: Tool::Ffmpeg,
                args: f.args(),
                stdout_policy: StdoutPolicy::CaptureJson,
            },
            CancellationToken::new(),
            tx,
        )
        .await
        .unwrap();
    drain.await.unwrap();
    assert_eq!(exit.exit_code, Some(0));
    assert!(exit.stderr_tail.len() <= 65536);
    assert!(exit.stdout.contains("finished"));
}
#[tokio::test]
async fn hung_child_is_reaped_after_cancel() {
    let f = ProcessFixture::new("hang");
    let r = NativeRunner::new(f.paths());
    let (tx, _rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    let args = f.args();
    let j = tokio::spawn(async move {
        r.run(
            RunSpec {
                tool: Tool::Ffprobe,
                args,
                stdout_policy: StdoutPolicy::Discard,
            },
            c,
            tx,
        )
        .await
    });
    while !f.is_alive() {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let exit = timeout(Duration::from_secs(4), j)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(exit.canceled);
    assert!(!f.is_alive());
}
#[test]
fn unknown_target_or_missing_tool_is_error() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(
        resolve_tools(d.path(), "bogus-target", None)
            .unwrap_err()
            .code,
        ErrorCode::ToolMissing
    );
}
