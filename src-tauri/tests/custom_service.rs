#[allow(dead_code)]
#[path = "support/controlled_runner.rs"]
mod controlled_runner;
#[allow(dead_code)]
#[path = "support/media_fixture.rs"]
mod media_fixture;
use controlled_runner::{BlockAt, ControlledRunner};
use media_fixture::zero_custom;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::mpsc;
use video_editor::{
    contracts::*,
    jobs::{service::JobService, state::is_terminal},
    presets::custom::FrozenCustomParameters,
    settings::SettingsStore,
};

async fn setup(
    count: usize,
) -> (
    tempfile::TempDir,
    Arc<ControlledRunner>,
    JobService,
    mpsc::UnboundedReceiver<JobSnapshot>,
    BatchSettings,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom-variation-settings.json");
    let runner = ControlledRunner::new(BlockAt::Probe, false);
    let (tx, rx) = mpsc::unbounded_channel();
    let service = JobService::with_queue_sink_and_settings(
        runner.clone(),
        dir.path().join("records"),
        Arc::new(move |s| {
            let _ = tx.send(s);
        }),
        Arc::new(|_| {}),
        SettingsStore::load(path),
    );
    let paths = (0..count)
        .map(|i| {
            let p = dir.path().join(format!("source-{i}.mp4"));
            std::fs::write(&p, b"source").unwrap();
            p.to_str().unwrap().into()
        })
        .collect();
    service.import_paths(paths).await.unwrap();
    let settings = BatchSettings {
        output_directory: dir.path().to_str().unwrap().into(),
        preset_id: "custom-variation-v1".into(),
        custom_settings: Some(zero_custom()),
        metadata: MetadataRequest::Preserve,
    };
    (dir, runner, service, rx, settings)
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
async fn invalid_batch_never_reserves_jobs() {
    let (_dir, runner, service, _rx, mut settings) = setup(3).await;
    let before = service.queue_snapshot().await;
    settings.custom_settings.as_mut().unwrap().speed = 10;
    assert_eq!(
        service
            .start_batch(settings.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidSettings
    );
    settings.custom_settings = None;
    assert_eq!(
        service
            .start_batch(settings.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidSettings
    );
    settings.custom_settings = Some(zero_custom());
    settings.preset_id = "content-variation-v1".into();
    assert_eq!(
        service.start_batch(settings).await.unwrap_err().code,
        ErrorCode::InvalidSettings
    );
    let after = service.queue_snapshot().await;
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    let request = StartJobRequest {
        input_path: "/input.mp4".into(),
        output_directory: "/out".into(),
        preset_id: "custom-variation-v1".into(),
        custom_settings: None,
        metadata: MetadataRequest::Preserve,
    };
    assert_eq!(
        service.start_job(request).await.unwrap_err().code,
        ErrorCode::InvalidSettings
    );
    assert!(service.current_snapshot().await.unwrap().is_none());
}
#[tokio::test]
async fn save_and_start_share_idle_boundary() {
    let (dir, runner, service, _rx, mut batch) = setup(3).await;
    let mut saved = zero_custom();
    saved.foreground_scale = 3;
    // A completed save wins the first boundary; the explicit request still
    // controls this batch. The probe barrier holds the active lifecycle.
    let writer = service.clone();
    let value = saved.clone();
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let ready = barrier.clone();
    let writing = tokio::spawn(async move {
        let result = writer.save_custom_settings(value).await;
        ready.wait().await;
        result
    });
    barrier.wait().await;
    assert_eq!(writing.await.unwrap().unwrap().settings, saved);
    let bytes = std::fs::read(dir.path().join("custom-variation-settings.json")).unwrap();
    batch.custom_settings.as_mut().unwrap().foreground_scale = 2;
    let active = service.start_batch(batch).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    assert_eq!(
        active
            .active_settings
            .unwrap()
            .custom_settings
            .unwrap()
            .foreground_scale,
        2
    );
    let mut changed = saved.clone();
    changed.foreground_scale = 9;
    assert_eq!(
        service
            .save_custom_settings(changed)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Busy
    );
    assert_eq!(
        std::fs::read(dir.path().join("custom-variation-settings.json")).unwrap(),
        bytes
    );
    assert_eq!(service.custom_settings().await.settings, saved);
    service.shutdown().await.unwrap();
}
#[tokio::test]
async fn refill_uses_frozen_levels() {
    let (dir, runner, service, mut rx, mut batch) = setup(3).await;
    let levels = batch.custom_settings.as_mut().unwrap();
    levels.foreground_scale = 2;
    levels.background_blur = 2;
    let frozen = levels.clone();
    service.start_batch(batch).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    let mut external = zero_custom();
    external.foreground_scale = 9;
    std::fs::write(
        dir.path().join("custom-variation-settings.json"),
        serde_json::to_vec(&external).unwrap(),
    )
    .unwrap();
    runner.resume();
    assert_eq!(end(&mut rx).await.state, JobState::Succeeded);
    entered(&runner).await;
    runner.resume();
    runner.resume();
    assert_eq!(end(&mut rx).await.state, JobState::Succeeded);
    assert_eq!(end(&mut rx).await.state, JobState::Succeeded);
    let q = service.queue_snapshot().await;
    assert!(!q.running);
    assert!(q.active_settings.is_none());
    let conversions = runner.conversions.lock().unwrap();
    assert_eq!(conversions.len(), 3);
    for item in &q.items {
        let id = item.job_id.as_ref().unwrap();
        let media = item.media.as_ref().unwrap();
        let args = conversions
            .iter()
            .find(|args| args.last().unwrap().contains(id))
            .unwrap();
        let vf = &args[args.iter().position(|a| a == "-vf").unwrap() + 1];
        assert_eq!(
            vf,
            &FrozenCustomParameters::resolve(&frozen, id)
                .unwrap()
                .video_filter(&media.video)
                .unwrap()
                .unwrap()
        );
        assert_ne!(
            vf,
            &FrozenCustomParameters::resolve(&external, id)
                .unwrap()
                .video_filter(&media.video)
                .unwrap()
                .unwrap()
        );
    }
}
#[tokio::test]
async fn remount_snapshot_contains_actual_selection() {
    let (_dir, runner, service, _rx, mut batch) = setup(3).await;
    batch.custom_settings.as_mut().unwrap().hue = 8;
    let started = service.start_batch(batch.clone()).await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    let restored = service.clone().queue_snapshot().await;
    assert_eq!(restored.version, started.version);
    assert_eq!(
        restored.active_settings.unwrap().custom_settings,
        batch.custom_settings
    );
    assert_eq!(service.custom_settings().await.settings.hue, 5);
    service.shutdown().await.unwrap();
    assert!(service.queue_snapshot().await.active_settings.is_none());
}

#[tokio::test]
async fn simultaneous_start_and_save_have_only_atomic_outcomes() {
    let (dir, runner, service, _rx, mut batch) = setup(3).await;
    batch.custom_settings.as_mut().unwrap().foreground_scale = 2;
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let starter = service.clone();
    let gate = barrier.clone();
    let start = tokio::spawn(async move {
        gate.wait().await;
        starter.start_batch(batch).await
    });
    let writer = service.clone();
    let gate = barrier.clone();
    let save = tokio::spawn(async move {
        let mut levels = zero_custom();
        levels.foreground_scale = 9;
        gate.wait().await;
        writer.save_custom_settings(levels).await
    });
    barrier.wait().await;
    let queue = start.await.unwrap().unwrap();
    let saved = save.await.unwrap();
    entered(&runner).await;
    entered(&runner).await;
    assert_eq!(
        queue
            .active_settings
            .unwrap()
            .custom_settings
            .unwrap()
            .foreground_scale,
        2
    );
    let path = dir.path().join("custom-variation-settings.json");
    match saved {
        Ok(value) => {
            assert_eq!(value.settings.foreground_scale, 9);
            assert_eq!(
                SettingsStore::load(path)
                    .snapshot()
                    .settings
                    .foreground_scale,
                9
            );
        }
        Err(error) => {
            assert_eq!(error.code, ErrorCode::Busy);
            assert!(!path.exists());
        }
    }
    service.shutdown().await.unwrap();
}
