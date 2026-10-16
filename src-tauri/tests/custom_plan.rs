#[path = "support/media_fixture.rs"]
mod media_fixture;
use media_fixture::{media, request, workspace, zero_custom};
use std::path::Path;
use video_editor::{
    contracts::*,
    presets::{
        basic::build_plan,
        custom::{FrozenCustomParameters, ID},
    },
};
fn custom(settings: CustomVariationSettings, source: &MediaInfo) -> ProcessingPlan {
    let mut request = request("/input.mp4", "/output");
    request.preset_id = ID.into();
    request.custom_settings = Some(settings);
    request.metadata = MetadataRequest::Override {
        title: None,
        comment: None,
    };
    build_plan(
        source,
        &request,
        workspace(Path::new("/output")),
        "fixed-job",
    )
    .unwrap()
}
fn arg<'a>(plan: &'a ProcessingPlan, key: &str) -> Option<&'a str> {
    plan.args
        .windows(2)
        .find(|a| a[0] == key)
        .map(|a| a[1].as_str())
}
#[test]
fn defaults_are_milder_than_content_layout() {
    let source = media();
    let recommended = CustomVariationSettings::recommended();
    let mut stronger = recommended.clone();
    stronger.foreground_scale = 5;
    stronger.background_blur = 5;
    let mild = custom(recommended, &source);
    let strong = custom(stronger, &source);
    assert_ne!(arg(&mild, "-vf"), arg(&strong, "-vf"));
    assert!(arg(&mild, "-vf").unwrap().contains("split=2"));
    assert_eq!(mild.policy.audio_target_samples, Some(96000));
}
#[test]
fn zero_has_no_extra_filters() {
    let source = media();
    let zero = custom(zero_custom(), &source);
    let mut request = request("/input.mp4", "/output");
    request.metadata = MetadataRequest::Override {
        title: None,
        comment: None,
    };
    let basic = build_plan(
        &source,
        &request,
        workspace(Path::new("/output")),
        "fixed-job",
    )
    .unwrap();
    assert_eq!(zero.args, basic.args);
    assert_eq!(zero.timing, TimingPolicy::Preserve);
    assert_eq!(zero.policy.audio_target_samples, None);
    let mut settings = zero_custom();
    settings.background_blur = 9;
    settings.dynamic_offset = 9;
    assert_eq!(custom(settings, &source).args, basic.args);
}
#[test]
fn each_level_is_finite_and_independent() {
    let source = media();
    let zero = serde_json::to_value(zero_custom()).unwrap();
    for key in zero
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.as_str() != "schemaVersion")
    {
        for level in 0..=9 {
            let mut value = zero.clone();
            value[key] = level.into();
            let settings: CustomVariationSettings = serde_json::from_value(value).unwrap();
            let plan = custom(settings, &source);
            assert!(
                !plan
                    .args
                    .iter()
                    .any(|a| a.contains("NaN") || a.contains("inf")),
                "{key} {level}"
            );
        }
    }
    let mut settings = CustomVariationSettings::recommended();
    settings.speed = 9;
    let a = FrozenCustomParameters::resolve(&settings, "fixed-job").unwrap();
    let mut changed = settings.clone();
    changed.hue = 9;
    let b = FrozenCustomParameters::resolve(&changed, "fixed-job").unwrap();
    assert_eq!(a.timing(), b.timing());
    assert_eq!(
        a.audio_filter(source.audio.as_ref().unwrap(), &source.video)
            .unwrap(),
        b.audio_filter(source.audio.as_ref().unwrap(), &source.video)
            .unwrap()
    );
    assert_eq!(
        a,
        FrozenCustomParameters::resolve(&settings, "fixed-job").unwrap()
    );
    assert_ne!(
        a,
        FrozenCustomParameters::resolve(&settings, "other-job").unwrap()
    );
}
#[test]
fn small_geometry_stays_valid() {
    let mut value = serde_json::to_value(zero_custom()).unwrap();
    for (key, v) in value.as_object_mut().unwrap() {
        if key != "schemaVersion" {
            *v = 9.into();
        }
    }
    let settings: CustomVariationSettings = serde_json::from_value(value).unwrap();
    for (width, height) in [(2, 2), (2, 128), (128, 2), (720, 1280), (1280, 720)] {
        let mut source = media();
        source.video.width = width;
        source.video.height = height;
        let plan = custom(settings.clone(), &source);
        let filter = arg(&plan, "-vf").unwrap();
        assert!(!filter.contains("scale=0:"));
        assert!(filter.contains("setsar=sar*"));
        assert_eq!((plan.policy.width, plan.policy.height), (width, height));
    }
}
#[test]
fn speed_and_pitch_have_separate_time_controls() {
    let mut source = media();
    for channels in [1, 2, 6, 8] {
        source.audio.as_mut().unwrap().channels = channels;
        let mut settings = zero_custom();
        settings.speed = 9;
        let speed = custom(settings.clone(), &source);
        assert!(matches!(speed.timing, TimingPolicy::UniformSpeed { .. }));
        assert_ne!(arg(&speed, "-enc_time_base:v"), Some("demux"));
        let filter = arg(&speed, "-af").unwrap();
        assert!(filter.contains("atempo="));
        assert!(!filter.contains("asetrate="));
        assert!(!filter.contains("alimiter="));
        assert!(!filter.contains("whole_len="));
        settings.speed = 0;
        settings.pitch = 9;
        let pitch = custom(settings, &source);
        assert_eq!(pitch.timing, TimingPolicy::Preserve);
        let filter = arg(&pitch, "-af").unwrap();
        assert!(filter.contains("asetrate="));
        assert!(filter.contains("pad_len=1024"));
        if channels > 1 {
            assert!(filter.contains(&format!("pan=mono|c0=c{}", channels - 1)));
        }
    }
    let mut settings = zero_custom();
    settings.speed = 9;
    source.audio = None;
    assert!(arg(&custom(settings, &source), "-af").is_none());
}

#[test]
fn encoding_levels_control_actual_encoder_args() {
    let source = media();
    let mut settings = zero_custom();
    settings.encoding = 5;
    let parameters = FrozenCustomParameters::resolve(&settings, "fixed-job").unwrap();
    let expected = parameters.encoding(&source).unwrap().unwrap();
    let plan = custom(settings, &source);
    assert_eq!(arg(&plan, "-crf"), Some(expected.crf.to_string().as_str()));
    assert_eq!(arg(&plan, "-g"), Some(expected.gop.to_string().as_str()));
    assert_eq!(arg(&plan, "-b:a"), Some("192000"));
    assert!(arg(&plan, "-b:v").is_none());
}
