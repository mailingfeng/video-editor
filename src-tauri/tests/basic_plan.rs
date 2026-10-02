#[path = "support/media_fixture.rs"]
mod media_fixture;
use media_fixture::{media, request, workspace};
use video_editor::{contracts::*, presets::basic::build_plan};
fn plan(info: &MediaInfo) -> ProcessingPlan {
    build_plan(
        info,
        &request(&info.identity.canonical_path, "/output"),
        workspace(std::path::Path::new("/output")),
        "test",
    )
    .unwrap()
}
fn arg_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|p| p[0] == name)
        .map(|p| p[1].as_str())
}
#[test]
fn missing_video_bitrate_uses_only_crf18() {
    let mut m = media();
    m.video.bit_rate = None;
    let p = plan(&m);
    assert_eq!(arg_value(&p.args, "-crf"), Some("18"));
    assert!(!p.args.contains(&"-b:v".into()));
    assert_eq!(arg_value(&plan(&media()).args, "-b:v"), Some("1596000"));
    assert!(!plan(&media()).args.contains(&"-crf".into()));
}
#[test]
fn audio_bitrate_is_clamped() {
    for (input, expected) in [
        (Some(32000), "64000"),
        (Some(256000), "128000"),
        (None, "96000"),
    ] {
        let mut m = media();
        m.audio.as_mut().unwrap().bit_rate = input;
        assert_eq!(arg_value(&plan(&m).args, "-b:a"), Some(expected));
    }
}
#[test]
fn no_audio_emits_no_audio_policy() {
    let mut m = media();
    m.audio = None;
    let p = plan(&m);
    assert!(p.policy.channels.is_none());
    assert!(p.args.contains(&"-an".into()));
    assert!(!p.args.contains(&"-ar".into()));
}
#[test]
fn nonzero_starts_keep_relative_timeline() {
    let mut m = media();
    m.video.start_pts = 76800;
    m.audio.as_mut().unwrap().start_pts = 224910;
    let p = plan(&m);
    assert_eq!(p.source.video.start_pts, 76800);
    assert_eq!(p.source.audio.unwrap().start_pts, 224910);
    assert!(p.args.contains(&"-copyts".into()));
    assert_eq!(arg_value(&p.args, "-fps_mode:v"), Some("passthrough"));
    assert!(!p.args.contains(&"-r".into()));
    assert!(!p.args.contains(&"-vf".into()));
    assert_eq!(arg_value(&p.args, "-video_track_timescale"), Some("15360"));
}
#[test]
fn metadata_quotes_are_one_argument() {
    let m = media();
    let mut r = request(&m.identity.canonical_path, "/output");
    r.metadata = MetadataRequest::Override {
        title: Some("title ' \" $()".into()),
        comment: None,
    };
    let p = build_plan(&m, &r, workspace(std::path::Path::new("/output")), "test").unwrap();
    assert!(p.args.contains(&"title=title ' \" $()".into()));
}
#[test]
fn unknown_preset_is_rejected() {
    let m = media();
    let mut r = request(&m.identity.canonical_path, "/output");
    r.preset_id = "research".into();
    assert_eq!(
        build_plan(&m, &r, workspace(std::path::Path::new("/output")), "test")
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedInput
    );
}
#[test]
fn gop_follows_frame_rate() {
    assert_eq!(arg_value(&plan(&media()).args, "-g"), Some("300"));
    let mut m = media();
    m.video.frame_rate = Rational {
        num: 30000,
        den: 1001,
    };
    assert_eq!(arg_value(&plan(&m).args, "-g"), Some("300"));
}
