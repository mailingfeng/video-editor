#[path = "support/controlled_runner.rs"]
mod controlled_runner;
use controlled_runner::{BlockAt, ControlledRunner};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::mpsc;
use video_editor::{
    contracts::*,
    jobs::{service::JobService, state::is_terminal},
};

fn setup(
    at: BlockAt,
    count: usize,
) -> (
    tempfile::TempDir,
    Arc<ControlledRunner>,
    JobService,
    mpsc::UnboundedReceiver<JobSnapshot>,
    BatchSettings,
    Vec<String>,
) {
    let dir = tempfile::tempdir().unwrap();
    let paths = (0..count)
        .map(|i| {
            let path = dir.path().join(format!("source-{i}.mp4"));
            std::fs::write(&path, format!("input-{i}")).unwrap();
            path.to_str().unwrap().to_owned()
        })
        .collect();
    let runner = ControlledRunner::new(at, false);
    let (tx, rx) = mpsc::unbounded_channel();
    let service = JobService::new(
        runner.clone(),
        dir.path().join("records"),
        Arc::new(move |s| {
            let _ = tx.send(s);
        }),
    );
    let settings = BatchSettings {
        output_directory: dir.path().to_str().unwrap().into(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    };
    (dir, runner, service, rx, settings, paths)
}

async fn entered(runner: &ControlledRunner) {
    tokio::time::timeout(Duration::from_secs(3), runner.wait_entered())
        .await
        .unwrap();
}
async fn end(rx: &mut mpsc::UnboundedReceiver<JobSnapshot>) -> JobSnapshot {
    tokio::time::timeout(Duration::from_secs(3), async {
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
async fn two_overlap_third_waits_and_completion_refills_without_restarting_terminal_rows() {
    let (_dir, runner, service, mut rx, settings, paths) = setup(BlockAt::Probe, 4);
    let imported = service.import_paths(paths.clone()).await.unwrap();
    assert_eq!(service.import_paths(paths).await.unwrap().items.len(), 4);
    let started = service.start_batch(settings.clone()).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    assert_eq!(runner.active_child_count(), 2);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        started.items.iter().filter(|i| i.job_id.is_some()).count(),
        2
    );
    assert_eq!(
        service
            .start_batch(settings.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Busy
    );
    assert_eq!(
        service
            .import_paths(vec![imported.items[0].input_path.clone()])
            .await
            .unwrap_err()
            .code,
        ErrorCode::Busy
    );
    assert_eq!(
        service
            .remove_item(&imported.items[0].item_id)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Busy
    );
    runner.resume();
    assert_eq!(end(&mut rx).await.state, JobState::Succeeded);
    entered(&runner).await;
    runner.resume();
    runner.resume();
    end(&mut rx).await;
    end(&mut rx).await;
    entered(&runner).await;
    runner.resume();
    end(&mut rx).await;
    let complete = service.queue_snapshot().await;
    assert!(!complete.running);
    assert!(complete
        .items
        .iter()
        .all(|i| i.snapshot.as_ref().unwrap().state == JobState::Succeeded));
    assert_eq!(
        service.start_batch(settings).await.unwrap_err().code,
        ErrorCode::UnsupportedInput
    );
    assert_eq!(runner.active_child_count(), 0);
}

#[tokio::test]
async fn validation_occupies_slot_and_waiting_cancel_never_launches() {
    let (_dir, runner, service, mut rx, settings, paths) = setup(BlockAt::Validation, 4);
    let items = service.import_paths(paths).await.unwrap().items;
    service.start_batch(settings).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    let q = service.queue_snapshot().await;
    assert_eq!(
        q.items[0].snapshot.as_ref().unwrap().state,
        JobState::Validating
    );
    assert_eq!(
        q.items[1].snapshot.as_ref().unwrap().state,
        JobState::Validating
    );
    assert!(q.items[2].job_id.is_none());
    let canceled = service.cancel_item(&items[2].item_id).await.unwrap();
    assert_eq!(canceled.items[2].state, QueueItemState::Canceled);
    service.cancel_item(&items[0].item_id).await.unwrap();
    assert_eq!(end(&mut rx).await.state, JobState::Canceled);
    entered(&runner).await;
    runner.resume();
    runner.resume();
    end(&mut rx).await;
    end(&mut rx).await;
    assert!(service.queue_snapshot().await.items[2].job_id.is_none());
}

#[tokio::test]
async fn shutdown_reaps_both_and_does_not_dispatch_waiting_items() {
    let (_dir, runner, service, _rx, settings, paths) = setup(BlockAt::Probe, 4);
    service.import_paths(paths).await.unwrap();
    service.start_batch(settings).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    tokio::time::timeout(Duration::from_secs(3), service.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runner.active_child_count(), 0);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    let q = service.queue_snapshot().await;
    assert!(!q.running);
    assert!(q.items[..2]
        .iter()
        .all(|i| i.snapshot.as_ref().unwrap().state == JobState::Canceled));
    assert!(q.items[2..].iter().all(|i| i.job_id.is_none()));
}

#[tokio::test]
async fn unreadable_input_fails_once_other_items_continue_and_reimport_gets_new_identity() {
    let (dir, _runner, service, mut rx, settings, mut paths) = setup(BlockAt::None, 2);
    paths.insert(0, dir.path().join("missing.mp4").to_str().unwrap().into());
    let items = service.import_paths(paths.clone()).await.unwrap().items;
    service.start_batch(settings).await.unwrap();
    let mut ends = vec![end(&mut rx).await, end(&mut rx).await, end(&mut rx).await];
    ends.sort_by_key(|s| s.state == JobState::Succeeded);
    assert_eq!(ends[0].state, JobState::Failed);
    assert_eq!(ends[1].state, JobState::Succeeded);
    assert_eq!(ends[2].state, JobState::Succeeded);
    assert!(service
        .read_log(&ends[0].job_id)
        .await
        .unwrap()
        .text
        .contains("无法读取"));
    for s in &ends {
        let value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                dir.path()
                    .join("records/history")
                    .join(format!("{}.json", s.job_id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["snapshot"]["jobId"], s.job_id);
    }
    service.remove_item(&items[1].item_id).await.unwrap();
    assert!(std::path::Path::new(&paths[1]).is_file());
    let q = service.import_paths(vec![paths[1].clone()]).await.unwrap();
    let new_item = q.items.last().unwrap();
    assert_ne!(new_item.item_id, items[1].item_id);
    assert!(new_item.job_id.is_none());
}

#[tokio::test]
async fn per_input_gates_prove_isolation_and_cancel_reaping_holds_the_slot() {
    let (_dir, runner, service, mut rx, settings, paths) = setup(BlockAt::Probe, 3);
    let paths: Vec<String> = paths
        .iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let gates: Vec<_> = paths
        .iter()
        .map(|path| {
            let gate = Arc::new(tokio::sync::Semaphore::new(0));
            runner
                .gates
                .lock()
                .unwrap()
                .insert(path.clone(), gate.clone());
            gate
        })
        .collect();
    runner.hold_cancel.store(true, Ordering::SeqCst);
    let items = service.import_paths(paths.clone()).await.unwrap().items;
    service.start_batch(settings).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    let mut entered_inputs = runner.inputs.lock().unwrap().clone();
    entered_inputs.sort();
    assert_eq!(entered_inputs, paths[..2]);
    service.cancel_item(&items[0].item_id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), runner.reaping.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    assert_eq!(runner.active_child_count(), 2);
    assert!(service.queue_snapshot().await.items[2].job_id.is_none());
    runner.reap_release.add_permits(1);
    assert_eq!(end(&mut rx).await.state, JobState::Canceled);
    entered(&runner).await;
    assert_eq!(runner.inputs.lock().unwrap().len(), 3);
    gates[1].add_permits(1);
    gates[2].add_permits(1);
    end(&mut rx).await;
    end(&mut rx).await;
    let queue = service.queue_snapshot().await;
    for item in &queue.items[1..] {
        assert_eq!(
            item.media.as_ref().unwrap().identity.canonical_path,
            item.input_path
        );
        let log = service
            .read_log(item.job_id.as_deref().unwrap())
            .await
            .unwrap()
            .text;
        assert!(log.contains(&item.input_path));
        assert!(!log.contains(&queue.items[0].input_path));
    }
    let conversions = runner.conversions.lock().unwrap();
    assert_eq!(conversions.len(), 2);
    for args in conversions.iter() {
        let input = &args[args.iter().position(|a| a == "-i").unwrap() + 1];
        assert!(args.contains(&format!(
            "title={}",
            std::path::Path::new(input)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        )));
    }
}

#[tokio::test]
async fn shutdown_before_reserved_tasks_run_still_signals_completion() {
    let (_dir, runner, service, _rx, settings, paths) = setup(BlockAt::Probe, 3);
    service.import_paths(paths).await.unwrap();
    service.start_batch(settings).await.unwrap();
    // The current-thread runtime has not yielded since reservation and spawn.
    tokio::time::timeout(Duration::from_secs(3), service.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    assert_eq!(runner.active_child_count(), 0);
}

#[tokio::test]
async fn override_is_frozen_for_each_input_and_legacy_entry_remains_exclusive() {
    let (_dir, runner, service, mut rx, mut settings, paths) = setup(BlockAt::Probe, 3);
    settings.metadata = MetadataRequest::Override {
        title: Some("batch title".into()),
        comment: Some("batch comment".into()),
    };
    service.import_paths(paths.clone()).await.unwrap();
    service.start_batch(settings.clone()).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    assert_eq!(
        service
            .inspect_input(paths[0].clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Busy
    );
    let request = StartJobRequest {
        input_path: paths[0].clone(),
        output_directory: settings.output_directory,
        preset_id: settings.preset_id,
        metadata: MetadataRequest::Preserve,
    };
    assert_eq!(
        service.start_job(request).await.unwrap_err().code,
        ErrorCode::Busy
    );
    runner.resume();
    runner.resume();
    end(&mut rx).await;
    entered(&runner).await;
    runner.resume();
    end(&mut rx).await;
    end(&mut rx).await;
    let conversions = runner.conversions.lock().unwrap();
    assert_eq!(conversions.len(), 3);
    for args in conversions.iter() {
        assert!(args.contains(&"title=batch title".into()));
        assert!(args.contains(&"comment=batch comment".into()));
    }
}
