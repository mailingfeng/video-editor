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
    assert_eq!(arg_value(&p.args, "-enc_time_base:v"), Some("demux"));
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
fn content_variation_freezes_pitch_layout_and_preserves_timing_contracts() {
    for channels in [None, Some(1), Some(2), Some(6), Some(8)] {
        let mut m = media();
        if let Some(channels) = channels {
            let a = m.audio.as_mut().unwrap();
            a.channels = channels;
            a.sample_rate = 8000;
            a.start_pts = 4000;
            a.time_base = Rational { num: 1, den: 8000 };
            a.duration_ticks = 16000;
            a.channel_layout = Some(
                match channels {
                    1 => "mono",
                    6 => "5.1",
                    8 => "7.1",
                    _ => "stereo",
                }
                .into(),
            );
        } else {
            m.audio = None;
        }
        let mut r = request(&m.identity.canonical_path, "/output");
        let w = workspace(std::path::Path::new("/output"));
        let basic = build_plan(&m, &r, w.clone(), "run-a").unwrap();
        r.preset_id = "content-variation-v1".into();
        let first = build_plan(&m, &r, w.clone(), "run-a").unwrap();
        let replay = build_plan(&m, &r, w.clone(), "run-a").unwrap();
        let second = build_plan(&m, &r, w, "run-b").unwrap();
        assert_eq!(first.version, 1);
        assert_eq!(first.args, replay.args);
        assert_ne!(
            arg_value(&first.args, "-vf"),
            arg_value(&second.args, "-vf")
        );
        assert_eq!(
            serde_json::to_value(&basic.policy).unwrap(),
            serde_json::to_value(&first.policy).unwrap()
        );
        assert_eq!(arg_value(&first.args, "-fps_mode:v"), Some("passthrough"));
        assert_eq!(arg_value(&first.args, "-enc_time_base:v"), Some("demux"));
        if channels.is_some() {
            let af = arg_value(&first.args, "-af").unwrap();
            assert!(af.contains("asetrate=") && af.contains("atempo="));
            assert!(af.contains("apad=whole_len=96000") && af.contains("atrim=end_sample=96000"));
            assert!(af.contains("asetpts=PTS+0.500000000/TB"));
            assert_ne!(Some(af), arg_value(&second.args, "-af"));
            assert_eq!(
                arg_value(&first.args, "-channel_layout:a"),
                m.audio.as_ref().unwrap().channel_layout.as_deref()
            );
        } else {
            assert!(!first.args.contains(&"-af".into()));
            assert!(first.args.contains(&"-an".into()));
        }
    }
    assert!(video_editor::presets::list_presets()
        .iter()
        .any(|p| p.preset_id == "content-variation-v1"));
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
#[test]
fn surround_channels_layout_and_bitrate_are_preserved() {
    let mut json = serde_json::to_value(media()).unwrap();
    json["audio"]["channels"] = 6.into();
    json["audio"]["channelLayout"] = "5.1".into();
    json["audio"]["bitRate"] = 384000.into();
    let source = serde_json::from_value(json).unwrap();
    let p = plan(&source);
    assert_eq!(arg_value(&p.args, "-ac"), Some("6"));
    assert_eq!(arg_value(&p.args, "-channel_layout:a"), Some("5.1"));
    assert_eq!(arg_value(&p.args, "-b:a"), Some("384000"));
}
#[test]
fn known_range_is_written_even_without_other_color_tags() {
    let mut source = media();
    source.video.color_space = None;
    source.video.color_primaries = None;
    source.video.color_transfer = None;
    assert_eq!(
        arg_value(&plan(&source).args, "-bsf:v"),
        Some("h264_metadata=video_full_range_flag=0")
    );
    source.video.color_range = Some("pc".into());
    assert_eq!(
        arg_value(&plan(&source).args, "-bsf:v"),
        Some("h264_metadata=video_full_range_flag=1")
    );
}

#[test]
fn sample_preset_is_available_and_keeps_media_and_metadata_contracts() {
    assert!(video_editor::presets::list_presets()
        .iter()
        .any(|p| p.preset_id == "sample-match-v1"));
    let m = media();
    let mut r = request(&m.identity.canonical_path, "/output");
    r.preset_id = "sample-match-v1".into();
    let p = build_plan(&m, &r, workspace(std::path::Path::new("/output")), "test").unwrap();
    assert_eq!(p.preset_id, r.preset_id);
    assert_eq!(p.policy.frame_count, m.video.frame_count);
    assert_eq!(p.policy.frame_rate, m.video.frame_rate);
    assert_eq!(p.policy.channels, Some(2));
    assert!(p
        .args
        .contains(&"comment=source; video-editor/sample-match-v1".into()));
    assert!(!p.args.contains(&"-r".into()));
    let mut silent = m;
    silent.audio = None;
    let p = build_plan(
        &silent,
        &r,
        workspace(std::path::Path::new("/output")),
        "test",
    )
    .unwrap();
    assert!(p.policy.channels.is_none());
    assert!(!p.args.contains(&"-af".into()));
}

#[test]
fn phase_preset_changes_only_audio_and_records_its_version() {
    let presets = video_editor::presets::list_presets();
    assert_eq!(presets[0].preset_id, "basic-transcode-v1");
    assert_eq!(
        presets
            .iter()
            .find(|p| p.preset_id == "sample-match-v2")
            .map(|p| p.version),
        Some(2)
    );
    for channels in [None, Some(2), Some(6), Some(8)] {
        let mut m = media();
        if let Some(channels) = channels {
            let audio = m.audio.as_mut().unwrap();
            audio.channels = channels;
            audio.channel_layout = Some(
                match channels {
                    6 => "5.1",
                    8 => "7.1",
                    _ => "stereo",
                }
                .into(),
            );
        } else {
            m.audio = None;
        }
        let mut r = request(&m.identity.canonical_path, "/output");
        // Keep metadata identical to isolate the media processing arguments.
        r.metadata = MetadataRequest::Override {
            title: None,
            comment: None,
        };
        r.preset_id = "sample-match-v1".into();
        let v1 = build_plan(&m, &r, workspace(std::path::Path::new("/output")), "test").unwrap();
        r.preset_id = "sample-match-v2".into();
        let v2 = build_plan(&m, &r, workspace(std::path::Path::new("/output")), "test").unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.preset_id, r.preset_id);
        assert_eq!(
            serde_json::to_value(&v1.policy).unwrap(),
            serde_json::to_value(&v2.policy).unwrap()
        );
        assert_eq!(v2.policy.channels, channels);
        let mut expected_args = v1.args.clone();
        if channels.is_some() {
            assert_ne!(arg_value(&v1.args, "-af"), arg_value(&v2.args, "-af"));
            let index = expected_args.iter().position(|arg| arg == "-af").unwrap();
            expected_args[index + 1] = arg_value(&v2.args, "-af").unwrap().into();
        } else {
            assert!(!v2.args.contains(&"-af".into()));
        }
        assert_eq!(expected_args, v2.args);
    }
}

#[test]
fn repeated_processing_freezes_per_job_variation_and_keeps_basic_media_policy() {
    let m = media();
    let mut r = request(&m.identity.canonical_path, "/output");
    let w = workspace(std::path::Path::new("/output"));
    let basic = build_plan(&m, &r, w.clone(), "run-a").unwrap();
    r.preset_id = "repeat-variant-v1".into();
    let first = build_plan(&m, &r, w.clone(), "run-a").unwrap();
    let replay = build_plan(&m, &r, w.clone(), "run-a").unwrap();
    let second = build_plan(&m, &r, w, "run-b").unwrap();
    assert_eq!(first.args, replay.args);
    assert_ne!(
        arg_value(&first.args, "-vf"),
        arg_value(&second.args, "-vf")
    );
    assert!(arg_value(&first.args, "-vf").is_some());
    assert!(!first.args.contains(&"-af".into()));
    assert_eq!(
        serde_json::to_value(&basic.policy).unwrap(),
        serde_json::to_value(&first.policy).unwrap()
    );
    assert_eq!(arg_value(&first.args, "-b:v"), Some("1596000"));
    assert_eq!(arg_value(&first.args, "-b:a"), Some("74000"));
    assert_eq!(arg_value(&first.args, "-fps_mode:v"), Some("passthrough"));
    assert_eq!(arg_value(&first.args, "-enc_time_base:v"), Some("demux"));
    assert!(video_editor::presets::list_presets()
        .iter()
        .any(|p| p.preset_id == r.preset_id));
}

#[test]
fn combined_processing_freezes_video_audio_and_encoding_without_changing_media_policy() {
    for channels in [None, Some(2), Some(6), Some(8)] {
        let mut m = media();
        if let Some(channels) = channels {
            let audio = m.audio.as_mut().unwrap();
            audio.channels = channels;
            audio.sample_rate = 8000;
            audio.channel_layout = Some(
                match channels {
                    6 => "5.1",
                    8 => "7.1",
                    _ => "stereo",
                }
                .into(),
            );
        } else {
            m.audio = None;
        }
        let mut r = request(&m.identity.canonical_path, "/output");
        let w = workspace(std::path::Path::new("/output"));
        let basic = build_plan(&m, &r, w.clone(), "run-a").unwrap();
        r.preset_id = "repeat-combined-v2".into();
        let first = build_plan(&m, &r, w.clone(), "run-a").unwrap();
        let replay = build_plan(&m, &r, w.clone(), "run-a").unwrap();
        let second = build_plan(&m, &r, w, "run-b").unwrap();
        assert_eq!(first.version, 2);
        assert_eq!(first.args, replay.args);
        assert_ne!(
            arg_value(&first.args, "-vf"),
            arg_value(&second.args, "-vf")
        );
        let vf = arg_value(&first.args, "-vf").unwrap();
        for filter in ["rotate=", "scale=", "eq=", "unsharp=", "noise="] {
            assert!(vf.contains(filter), "missing {filter}: {vf}");
        }
        assert_eq!(
            serde_json::to_value(&basic.policy).unwrap(),
            serde_json::to_value(&first.policy).unwrap()
        );
        assert_eq!(arg_value(&first.args, "-profile:v"), Some("baseline"));
        assert!(!first.args.contains(&"-b:v".into()));
        assert!(arg_value(&first.args, "-crf").is_some());
        assert_eq!(arg_value(&first.args, "-fps_mode:v"), Some("passthrough"));
        assert_eq!(arg_value(&first.args, "-enc_time_base:v"), Some("demux"));
        if let Some(channels) = channels {
            let af = arg_value(&first.args, "-af").unwrap();
            assert!(af.starts_with("aresample=48000,"));
            assert!(
                af.contains("allpass=") && af.contains("acompressor=") && af.contains("latency=1")
            );
            assert_ne!(Some(af), arg_value(&second.args, "-af"));
            assert_eq!(
                arg_value(&first.args, "-ac"),
                Some(channels.to_string().as_str())
            );
        } else {
            assert!(arg_value(&first.args, "-af").is_none());
            assert!(first.args.contains(&"-an".into()));
        }
    }
    assert!(video_editor::presets::list_presets()
        .iter()
        .any(|p| p.preset_id == "repeat-combined-v2" && p.version == 2));
}
