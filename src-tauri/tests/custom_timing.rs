#[allow(dead_code)]
#[path = "support/media_fixture.rs"]
mod media_fixture;
use media_fixture::media;
use video_editor::{
    contracts::*,
    media::{
        timeline::Timeline,
        timing::{project_video_pts, target_timing},
    },
};

fn source() -> MediaInfo {
    let mut source = media();
    source.video.time_base = Rational { num: 1, den: 1000 };
    source.video.start_pts = 5000;
    source.video.duration_ticks = 120;
    source.video.frame_count = 3;
    source.video.frame_rate = Rational { num: 25, den: 1 };
    let audio = source.audio.as_mut().unwrap();
    audio.time_base = Rational { num: 1, den: 48000 };
    audio.start_pts = -4800;
    audio.duration_ticks = 48000;
    source
}

#[test]
fn uniform_speed_projects_vfr_and_offsets() {
    let source = source();
    let policy = TimingPolicy::UniformSpeed {
        speed: Rational { num: 103, den: 100 },
    };
    let target = target_timing(&source, &policy).unwrap();
    assert_eq!(
        target.video_time_base,
        Rational {
            num: 1,
            den: 103000
        }
    );
    assert_eq!(target.video_start_pts, 515000);
    assert_eq!(target.video_duration_ticks, 12000);
    assert_eq!(target.frame_rate, Rational { num: 103, den: 4 });
    assert_eq!(target.audio_samples_48k, Some(46602));
    assert!((target.audio_start_seconds.unwrap() - (5.0 - 5.1 / 1.03)).abs() < 1e-12);
    for (pts, expected) in [(5000, 515000), (5033, 518300), (5080, 523000)] {
        assert_eq!(
            project_video_pts(&source.video, &policy, pts).unwrap(),
            expected
        );
    }
    let mut expected = Timeline::new(target.video_time_base, Some(527000));
    let mut projected = Timeline::with_timing(&source.video, &policy).unwrap();
    for (pts, transformed, duration) in [(5000, 515000, 33), (5033, 518300, 47), (5080, 523000, 40)]
    {
        projected
            .push(&format!("pts={pts}|duration={duration}"))
            .unwrap();
        expected
            .push(&format!("pts={transformed}|duration={}", duration * 100))
            .unwrap();
    }
    let report = projected.finish().unwrap();
    assert_eq!(
        report.timeline.timestamp_sha256,
        expected.finish().unwrap().timeline.timestamp_sha256
    );
    assert!(report.timeline.variable_frame_rate);
    assert_eq!(source.video.start_pts, 5000);
    assert!(source.video.timeline.is_none());
    let slow = TimingPolicy::UniformSpeed {
        speed: Rational { num: 97, den: 100 },
    };
    assert_eq!(
        project_video_pts(&source.video, &slow, 5033).unwrap(),
        488300
    );
    let mut nonunit = source.clone();
    nonunit.video.time_base.num = 3;
    assert_eq!(
        project_video_pts(&nonunit.video, &policy, 5033).unwrap(),
        1554900
    );
    assert_eq!(
        target_timing(&source, &TimingPolicy::Preserve)
            .unwrap()
            .video_start_pts,
        5000
    );
}

#[test]
fn reject_unrepresentable_timing() {
    for speed in [
        Rational { num: 0, den: 1 },
        Rational { num: -1, den: 1 },
        Rational { num: 1, den: 0 },
        Rational {
            num: i64::MAX,
            den: 1,
        },
    ] {
        assert!(target_timing(&source(), &TimingPolicy::UniformSpeed { speed }).is_err());
    }
    let policy = TimingPolicy::UniformSpeed {
        speed: Rational { num: 103, den: 100 },
    };
    let mut large = source();
    large.video.time_base.den = i32::MAX as u64;
    assert!(target_timing(&large, &policy).is_err());
    let mut large = source();
    large.video.start_pts = 1_i64 << 53;
    assert!(target_timing(&large, &policy).is_err());
    let mut large = source();
    large.video.duration_ticks = i64::MAX;
    assert!(target_timing(&large, &policy).is_err());
    let mut intermediate = source();
    intermediate.video.start_pts = 0;
    intermediate.video.duration_ticks = 88_000_000_000_000;
    assert!(target_timing(&intermediate, &policy).is_err());
    let mut large = source();
    large.video.time_base.num = i64::MAX;
    assert!(target_timing(&large, &policy).is_err());
}

#[allow(dead_code)]
#[path = "support/fake_runner.rs"]
mod fake_runner;

#[tokio::test]
async fn expected_timeline_streams_source_and_checks_identity() {
    use fake_runner::{FakeRunner, Response};
    use tokio_util::sync::CancellationToken;
    use video_editor::{
        media::{probe::file_identity, timing::expected_timeline},
        native::tools::Tool,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.mp4");
    std::fs::write(&path, b"source identity").unwrap();
    let mut source = source();
    let cancel = CancellationToken::new();
    source.identity = file_identity(&path, &cancel).await.unwrap();
    let policy = TimingPolicy::UniformSpeed {
        speed: Rational { num: 103, den: 100 },
    };
    let runner = FakeRunner::new(vec![Response {
        tool: Tool::Ffprobe,
        required_arg: "-show_frames".into(),
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
        lines: vec![
            "pts=5000|duration=33".into(),
            "pts=5033|duration=47".into(),
            "pts=5080|duration=40".into(),
        ],
    }]);
    let digest = expected_timeline(&runner, &source, &policy, cancel.clone())
        .await
        .unwrap()
        .unwrap();
    let mut expected = Timeline::new(
        Rational {
            num: 1,
            den: 103000,
        },
        Some(527000),
    );
    for (pts, duration) in [(515000, 3300), (518300, 4700), (523000, 4000)] {
        expected
            .push(&format!("pts={pts}|duration={duration}"))
            .unwrap();
    }
    assert_eq!(
        digest.timestamp_sha256,
        expected.finish().unwrap().timeline.timestamp_sha256
    );
    assert_eq!(runner.active_child_count(), 0);
    std::fs::write(&path, b"changed").unwrap();
    assert_eq!(
        expected_timeline(&runner, &source, &policy, cancel.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::InputChanged
    );
    cancel.cancel();
    assert_eq!(
        expected_timeline(&runner, &source, &policy, cancel)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Canceled
    );
}
