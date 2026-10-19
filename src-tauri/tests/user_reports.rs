#[path = "support/controlled_runner.rs"]
#[allow(dead_code)]
mod controlled_runner;
use async_trait::async_trait;
use controlled_runner::{BlockAt, ControlledRunner};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*,
    jobs::{service::JobService, state::is_terminal},
    native::process::{MediaRunner, ProcessEvent, RunExit, RunSpec},
};

struct DiagnosticRunner {
    inner: Arc<ControlledRunner>,
    scenario: Scenario,
}
#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Success,
    TranscodeFailure,
    InvalidDimensions,
    DecodeFailure,
    MissingOutput,
}
#[async_trait]
impl MediaRunner for DiagnosticRunner {
    async fn run(
        &self,
        spec: RunSpec,
        cancel: CancellationToken,
        events: mpsc::Sender<ProcessEvent>,
    ) -> Result<RunExit, AppError> {
        if spec.args.iter().any(|a| a == "-progress") {
            events
                .send(ProcessEvent::StderrLine(
                    "ffmpeg libx264 -vf rotate=0.01 -af asetrate=47000".into(),
                ))
                .await
                .unwrap();
            for frame in [6, 7, 12] {
                events
                    .send(ProcessEvent::StdoutLine(format!("frame={frame}")))
                    .await
                    .unwrap();
                events
                    .send(ProcessEvent::StdoutLine("progress=continue".into()))
                    .await
                    .unwrap();
            }
            if self.scenario == Scenario::TranscodeFailure {
                return Ok(RunExit {
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr_tail: "ffmpeg -vf rotate=0.01: No space left on device".into(),
                    canceled: false,
                });
            }
        }
        if self.scenario == Scenario::DecodeFailure && spec.args.iter().any(|a| a == "-xerror") {
            return Ok(RunExit {
                exit_code: Some(1),
                stdout: String::new(),
                stderr_tail: "ffmpeg -vf rotate=0.01: broken frame".into(),
                canceled: false,
            });
        }
        let invalid_output = self.scenario == Scenario::InvalidDimensions
            && spec.args.iter().any(|a| a == "-show_streams")
            && spec.args.last().is_some_and(|a| a.ends_with("output.mp4"));
        let remove_output = (self.scenario == Scenario::MissingOutput
            && spec.args.iter().any(|a| a == "-progress"))
        .then(|| spec.args.last().unwrap().clone());
        let mut result = self.inner.run(spec, cancel, events).await?;
        if let Some(path) = remove_output {
            std::fs::remove_file(path).unwrap();
        }
        if invalid_output {
            let mut metadata: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
            metadata["streams"][0]["width"] = 718.into();
            result.stdout = metadata.to_string();
        }
        Ok(result)
    }
}

async fn run(scenario: Scenario) -> (tempfile::TempDir, JobService, JobSnapshot) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("source.mp4");
    std::fs::write(&input, b"input").unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let service = JobService::new(
        Arc::new(DiagnosticRunner {
            inner: ControlledRunner::new(BlockAt::None, false),
            scenario,
        }),
        root.path().join("records"),
        Arc::new(move |snapshot| {
            let _ = tx.send(snapshot);
        }),
    );
    service
        .start_job(StartJobRequest {
            custom_settings: None,
            input_path: input.to_str().unwrap().into(),
            output_directory: root.path().to_str().unwrap().into(),
            preset_id: "content-variation-v1".into(),
            metadata: MetadataRequest::Preserve,
        })
        .await
        .unwrap();
    let end = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let snapshot = rx.recv().await.unwrap();
            if is_terminal(snapshot.state) {
                break snapshot;
            }
        }
    })
    .await
    .unwrap();
    (root, service, end)
}

fn assert_no_diagnostics(text: &str) {
    for secret in [
        "ffmpeg",
        "ffprobe",
        "libx264",
        "rotate=",
        "asetrate=",
        "参数：",
        "工具来源记录",
        "content-variation-v1",
        "Preparing",
    ] {
        assert!(!text.contains(secret), "technical detail leaked: {secret}");
    }
}

#[tokio::test]
async fn report_keeps_timed_stages_and_coarse_progress_without_tool_output() {
    let (root, service, end) = run(Scenario::Success).await;
    assert_eq!(end.state, JobState::Succeeded);
    let report = service.read_log(&end.job_id).await.unwrap().text;
    assert_no_diagnostics(&report);
    for stage in [
        "检查视频",
        "准备任务",
        "处理中：10%",
        "处理中：20%",
        "校验结果",
        "保存结果",
        "结果已保存",
    ] {
        assert!(report.contains(stage), "missing stage: {stage}");
    }
    assert_eq!(report.matches("处理中：10%").count(), 1);
    for line in report.lines() {
        let stamp = &line[..21];
        assert!(stamp.starts_with('[') && stamp.ends_with(']'));
        assert_eq!(stamp.as_bytes()[5], b'-');
        assert_eq!(stamp.as_bytes()[8], b'-');
        assert_eq!(stamp.as_bytes()[11], b' ');
        assert_eq!(stamp.as_bytes()[14], b':');
        assert_eq!(stamp.as_bytes()[17], b':');
    }
    let restored = JobService::new(
        ControlledRunner::new(BlockAt::None, false),
        root.path().join("records"),
        Arc::new(|_| {}),
    );
    assert_eq!(restored.read_log(&end.job_id).await.unwrap().text, report);
}

#[tokio::test]
async fn report_and_snapshot_keep_disk_full_advice_without_raw_failure_details() {
    let (_root, service, end) = run(Scenario::TranscodeFailure).await;
    assert_eq!(end.state, JobState::Failed);
    let error = end.error.unwrap();
    assert_eq!(error.code, ErrorCode::DiskFull);
    assert!(error.message.contains("空间不足") && error.message.contains("释放空间"));
    assert!(error.details.is_none());
    let report = service.read_log(&end.job_id).await.unwrap().text;
    assert_no_diagnostics(&report);
    assert!(report.contains("释放空间"));
}

#[tokio::test]
async fn validation_mismatch_or_decode_failure_is_logged_but_output_is_saved_and_history_restores()
{
    for (scenario, reason) in [
        (Scenario::InvalidDimensions, "视频尺寸与处理计划不一致"),
        (Scenario::DecodeFailure, "输出完整解码失败"),
    ] {
        let (root, service, end) = run(scenario).await;
        assert_eq!(end.state, JobState::Succeeded);
        assert_eq!(end.progress, Some(1.0));
        assert!(end.error.is_none());
        assert_eq!(
            std::fs::read(end.output_path.as_ref().unwrap()).unwrap(),
            b"converted"
        );
        let report = service.read_log(&end.job_id).await.unwrap().text;
        assert!(report.contains(reason), "{report}");
        assert!(
            report.contains("校验提醒")
                && report.contains("将继续保存新视频")
                && report.contains("结果已保存")
        );
        assert!(!report.contains("未保存新视频"));
        assert_no_diagnostics(&report);
        let restored = JobService::new(
            ControlledRunner::new(BlockAt::None, false),
            root.path().join("records"),
            Arc::new(|_| {}),
        );
        assert_eq!(restored.read_log(&end.job_id).await.unwrap().text, report);
        let snapshot = restored.current_snapshot().await.unwrap().unwrap();
        assert_eq!(snapshot.state, JobState::Succeeded);
        assert!(snapshot.error.is_none());
        assert_eq!(snapshot.output_path, end.output_path);
        let stored: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.path().join("records/history/latest.json")).unwrap(),
        )
        .unwrap();
        assert!(
            stored["validation"].is_null(),
            "failed validation must not be stored as passed"
        );
    }
}

#[tokio::test]
async fn missing_output_still_fails_publication_after_validation_warning() {
    let (_root, service, end) = run(Scenario::MissingOutput).await;
    assert_eq!(end.state, JobState::Failed);
    assert!(end.output_path.is_none());
    let report = service.read_log(&end.job_id).await.unwrap().text;
    assert!(report.contains("校验提醒"));
    assert!(!report.contains("结果已保存"));
}

#[test]
fn unknown_validation_errors_do_not_expose_raw_messages_or_details() {
    for message in [
        "ffmpeg -vf rotate=0.01",
        "视频帧数与处理计划不一致 -vf rotate=0.01",
        "处理结果未通过完整性检查：ffmpeg -vf rotate=0.01。未保存新视频。",
        "视频帧数与处理计划不一致",
    ] {
        let error = AppError::new(ErrorCode::ValidationFailed, message)
            .detail("asetrate=47000")
            .for_user();
        assert_no_diagnostics(&error.message);
        assert!(error.details.is_none());
        assert_eq!(error.for_user().message, error.message);
        let warning = AppError::new(ErrorCode::ValidationFailed, message)
            .detail("asetrate=47000")
            .validation_warning();
        assert_no_diagnostics(&warning);
        assert!(warning.contains("将继续保存新视频"));
        assert!(!warning.contains("未保存新视频"));
    }
}

#[tokio::test]
async fn old_history_is_rebuilt_from_saved_times_and_result_without_replaying_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let history = root.path().join("history");
    std::fs::create_dir(&history).unwrap();
    let snapshot = JobSnapshot {
        job_id: "legacy".into(),
        version: 4,
        state: JobState::Failed,
        progress: None,
        started_at_ms: 1_791_561_600_000,
        ended_at_ms: Some(1_791_561_610_000),
        output_path: None,
        error: Some(
            AppError::new(ErrorCode::ProcessExit, "ffmpeg libx264 exit").detail("-vf rotate=0.01"),
        ),
        cleanup_pending: true,
    };
    std::fs::write(history.join("latest.json"), serde_json::to_vec(&serde_json::json!({
        "snapshot": snapshot, "log": {"text": "参数：ffmpeg -vf rotate=0.01\n阶段：Preparing", "truncated": true},
        "plan": null, "validation": null,
    })).unwrap()).unwrap();
    let service = JobService::new(
        ControlledRunner::new(BlockAt::None, false),
        root.path().to_owned(),
        Arc::new(|_| {}),
    );
    let report = service.read_log("legacy").await.unwrap();
    assert_no_diagnostics(&report.text);
    assert!(
        report.text.contains("[2026-10-10 00:00:00]")
            && report.text.contains("[2026-10-10 00:00:10]")
    );
    assert!(report.text.contains("重试") && report.text.contains("临时文件"));
    assert!(!report.truncated);
    let restored = service.current_snapshot().await.unwrap().unwrap();
    let error = restored.error.unwrap();
    assert_no_diagnostics(&error.message);
    assert!(error.details.is_none());
}

#[tokio::test]
async fn time_only_history_gets_dates_from_the_saved_task_times() {
    let root = tempfile::tempdir().unwrap();
    let history = root.path().join("history");
    std::fs::create_dir(&history).unwrap();
    std::fs::write(history.join("latest.json"), serde_json::to_vec(&serde_json::json!({
        "reportVersion": 1,
        "snapshot": {
            "jobId": "time-only", "version": 4, "state": "succeeded", "progress": 1.0,
            "startedAtMs": 1735660799000u64, "endedAtMs": 1735660800000u64,
            "outputPath": "/output/result.mp4", "error": null, "cleanupPending": false,
        },
        "log": {"text": "[23:59:59] 开始检查视频\n[00:00:00] 处理完成，结果已保存\n", "truncated": false},
        "plan": null, "validation": null,
    })).unwrap()).unwrap();
    let service = JobService::new(
        ControlledRunner::new(BlockAt::None, false),
        root.path().to_owned(),
        Arc::new(|_| {}),
    );
    let report = service.read_log("time-only").await.unwrap();
    assert_eq!(
        report.text,
        "[2024-12-31 23:59:59] 开始检查视频\n[2025-01-01 00:00:00] 处理完成，结果已保存\n"
    );
}

#[tokio::test]
async fn completed_history_does_not_persist_processing_arguments() {
    let (root, _, end) = run(Scenario::Success).await;
    let history = root.path().join("records/history");
    for name in [format!("{}.json", end.job_id), "latest.json".into()] {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(history.join(name)).unwrap()).unwrap();
        assert!(value.get("plan").is_none(), "processing plan persisted");
        assert_eq!(value["snapshot"]["state"], "succeeded");
    }
}

#[tokio::test]
async fn startup_scrubs_all_old_history_plans_and_keeps_user_reports() {
    let (root, service, end) = run(Scenario::Success).await;
    let report = service.read_log(&end.job_id).await.unwrap().text;
    let history = root.path().join("records/history");
    let latest = history.join("latest.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).unwrap()).unwrap();
    value["plan"] =
        serde_json::json!({"args": ["-vf", "legacy-video-secret", "-af", "legacy-audio-secret"]});
    for path in [&latest, &history.join(format!("{}.json", end.job_id))] {
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let restored = JobService::new(
        ControlledRunner::new(BlockAt::None, false),
        root.path().join("records"),
        Arc::new(|_| {}),
    );
    assert_eq!(restored.read_log(&end.job_id).await.unwrap().text, report);
    for entry in std::fs::read_dir(history).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value.get("plan").is_none(), "old processing plan remains");
    }
}

#[tokio::test]
async fn active_recovery_record_does_not_persist_processing_arguments() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("source.mp4");
    std::fs::write(&input, b"input").unwrap();
    let runner = ControlledRunner::new(BlockAt::Validation, false);
    let records = root.path().join("records");
    let service = JobService::new(runner.clone(), records.clone(), Arc::new(|_| {}));
    let id = service
        .start_job(StartJobRequest {
            custom_settings: None,
            input_path: input.to_str().unwrap().into(),
            output_directory: root.path().to_str().unwrap().into(),
            preset_id: "content-variation-v1".into(),
            metadata: MetadataRequest::Preserve,
        })
        .await
        .unwrap();
    runner.wait_entered().await;
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(records.join(format!("{id}.json"))).unwrap())
            .unwrap();
    service.cancel_job(&id).await.unwrap();
    runner.resume();
    service.shutdown().await.unwrap();
    assert!(
        value.get("plan").is_none(),
        "active processing plan persisted"
    );
    assert!(value.get("workspace").is_some(), "recovery boundary lost");
}

#[tokio::test]
async fn custom_levels_are_saved_without_runtime_parameters_in_reports_or_records() {
    use video_editor::settings::SettingsStore;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("source.mp4");
    std::fs::write(&input, b"input").unwrap();
    let runner = ControlledRunner::new(BlockAt::Validation, false);
    let records = root.path().join("records");
    let settings_path = root.path().join("custom-variation-settings.json");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let service = JobService::with_queue_sink_and_settings(
        runner.clone(),
        records.clone(),
        Arc::new(move |s| {
            let _ = tx.send(s);
        }),
        Arc::new(|_| {}),
        SettingsStore::load(settings_path.clone()),
    );
    let settings = CustomVariationSettings::recommended();
    service
        .save_custom_settings(settings.clone())
        .await
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
    assert_eq!(saved.as_object().unwrap().len(), 15);
    assert_eq!(saved["foregroundScale"], 1);
    let id = service
        .start_job(StartJobRequest {
            input_path: input.to_str().unwrap().into(),
            output_directory: root.path().to_str().unwrap().into(),
            preset_id: "custom-variation-v1".into(),
            custom_settings: Some(settings),
            metadata: MetadataRequest::Preserve,
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), runner.wait_entered())
        .await
        .unwrap();
    fn check_records(directory: &std::path::Path) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check_records(&path);
            } else if path.extension().is_some_and(|e| e == "json") {
                let text = std::fs::read_to_string(path).unwrap();
                for secret in [
                    "customSettings",
                    "foregroundScale",
                    "setpts=",
                    "asetrate=",
                    "all_seed=",
                    "sigma=",
                    "\"args\"",
                    "\"plan\"",
                ] {
                    assert!(!text.contains(secret), "leaked: {secret}");
                }
            }
        }
    }
    check_records(&records);
    runner.resume();
    let end = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let s = rx.recv().await.unwrap();
            if is_terminal(s.state) {
                return s;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(end.state, JobState::Succeeded);
    check_records(&records);
    let text = service.read_log(&id).await.unwrap().text;
    assert_no_diagnostics(&text);
    assert!(!text.contains("custom-variation-v1"));
}
