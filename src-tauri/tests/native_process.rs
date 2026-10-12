mod support;
#[tokio::test]
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
    timeout(Duration::from_secs(5), async {
        while !f.is_alive() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture did not start");
    let started = std::time::Instant::now();
    token.cancel();
    timeout(Duration::from_secs(5), async {
        while !f.q_seen() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture did not acknowledge q");
    assert_eq!(std::fs::read(f.args()[2].as_str()).unwrap(), b"q\n");
    assert!(f.is_alive());
    // OS pipe writes and process exit do not follow Tokio's paused clock on Windows.
    let result = timeout(Duration::from_secs(6), job)
        .await
        .expect("hung FFmpeg was not reaped")
        .unwrap()
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(3));
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

fn filtered_args(
    fixture: &ProcessFixture,
) -> (
    tempfile::TempDir,
    video_editor::contracts::OutputWorkspace,
    Vec<String>,
) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("source.mp4");
    std::fs::write(&input, b"input").unwrap();
    let output = root.path().join("中文 空格 '输出'");
    std::fs::create_dir(&output).unwrap();
    let workspace = video_editor::output::workspace::prepare_workspace(
        &output,
        &input,
        "filter-test",
        &root.path().join("records"),
    )
    .unwrap();
    let mut args = fixture.args();
    args.extend([
        "--".into(),
        "-vf".into(),
        "hue=h='3*sin(t)'".into(),
        "-af".into(),
        "volume=0.95".into(),
        workspace.temp_path.clone(),
    ]);
    (root, workspace, args)
}

#[tokio::test]
async fn child_loads_exact_filters_without_inline_arguments_and_scripts_are_removed() {
    let fixture = ProcessFixture::new("filters");
    let (_root, workspace, args) = filtered_args(&fixture);
    let (tx, _rx) = mpsc::channel(256);
    let result = NativeRunner::new(fixture.paths())
        .run(
            RunSpec {
                tool: Tool::Ffmpeg,
                args,
                stdout_policy: StdoutPolicy::CaptureJson,
            },
            CancellationToken::new(),
            tx,
        )
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert!(!result
        .stdout
        .lines()
        .any(|line| line == "argument:hue=h='3*sin(t)'" || line == "argument:volume=0.95"));
    assert!(result.stdout.contains("loaded:video:hue=h='3*sin(t)'"));
    assert!(result.stdout.contains("loaded:audio:volume=0.95"));
    #[cfg(unix)]
    assert_eq!(result.stdout.matches("mode:384").count(), 2);
    assert_eq!(std::fs::read_dir(&workspace.directory).unwrap().count(), 1);
}

#[tokio::test]
async fn scripts_are_removed_after_first_frame_while_child_remains_running() {
    let fixture = ProcessFixture::new("filters_loaded");
    let (_root, workspace, args) = filtered_args(&fixture);
    let (tx, mut rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let runner = NativeRunner::new(fixture.paths());
    let job = tokio::spawn(async move {
        runner
            .run(
                RunSpec {
                    tool: Tool::Ffmpeg,
                    args,
                    stdout_policy: StdoutPolicy::Stream,
                },
                token,
                tx,
            )
            .await
    });
    timeout(Duration::from_secs(5), async {
        while let Some(event) = rx.recv().await {
            if matches!(event, ProcessEvent::StdoutLine(ref line) if line == "frame=1") {
                break;
            }
        }
        while std::fs::read_dir(&workspace.directory).unwrap().count() != 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("filter scripts were not removed after loading");
    assert!(fixture.is_alive());
    cancel.cancel();
    let result = timeout(Duration::from_secs(5), job)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(result.canceled);
    assert!(!fixture.is_alive());
}

#[tokio::test]
async fn failed_child_start_removes_filter_scripts() {
    let fixture = ProcessFixture::new("filters");
    let (_root, workspace, args) = filtered_args(&fixture);
    let mut paths = fixture.paths();
    paths.ffmpeg = std::path::Path::new(&workspace.directory).join("missing-tool");
    let (tx, _rx) = mpsc::channel(256);
    let result = NativeRunner::new(paths)
        .run(
            RunSpec {
                tool: Tool::Ffmpeg,
                args,
                stdout_policy: StdoutPolicy::CaptureJson,
            },
            CancellationToken::new(),
            tx,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(&workspace.directory).unwrap().count(), 1);
}

#[tokio::test]
async fn cancel_before_filter_progress_removes_private_scripts() {
    let fixture = ProcessFixture::new("filters_hold");
    let (_root, workspace, args) = filtered_args(&fixture);
    let report = fixture.args()[2].clone();
    let (tx, _rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let runner = NativeRunner::new(fixture.paths());
    let job = tokio::spawn(async move {
        runner
            .run(
                RunSpec {
                    tool: Tool::Ffmpeg,
                    args,
                    stdout_policy: StdoutPolicy::CaptureJson,
                },
                token,
                tx,
            )
            .await
    });
    timeout(Duration::from_secs(5), async {
        while !std::path::Path::new(&report).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read_dir(&workspace.directory).unwrap().count(), 3);
    #[cfg(windows)]
    {
        let file = std::path::Path::new(&workspace.directory).join(".v.dat");
        let path = file.display().to_string().replace('\'', "''");
        let check = std::process::Command::new("powershell.exe").args([
            "-NoProfile", "-NonInteractive", "-Command", &format!(
                "Import-Module ($PSHOME + '\\Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1') -ErrorAction Stop; $acl=Get-Acl -LiteralPath '{path}'; $acl.AreAccessRulesProtected; $raw=[System.Security.AccessControl.RawSecurityDescriptor]::new($acl.Sddl); $raw.DiscretionaryAcl.Count; $raw.DiscretionaryAcl[0].SecurityIdentifier.Value"
            ),
        ]).output().unwrap();
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&check.stdout)
                .replace('\r', "")
                .trim(),
            "True\n1\nS-1-3-4"
        );
    }
    cancel.cancel();
    let result = timeout(Duration::from_secs(5), job)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(result.canceled);
    assert_eq!(std::fs::read_dir(&workspace.directory).unwrap().count(), 1);
}
