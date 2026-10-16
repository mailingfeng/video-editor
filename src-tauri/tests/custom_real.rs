#[allow(dead_code)]
#[path = "support/custom_media.rs"]
mod custom_media;
#[allow(dead_code)]
#[path = "support/media_fixture.rs"]
mod media_fixture;
use custom_media::*;
use media_fixture::zero_custom;
use std::path::Path;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*, media::validate::validate_output, native::process::NativeRunner,
    presets::custom::FrozenCustomParameters,
};

fn settings9() -> CustomVariationSettings {
    let mut value = serde_json::to_value(zero_custom()).unwrap();
    for (key, value) in value.as_object_mut().unwrap() {
        if key != "schemaVersion" {
            *value = 9.into();
        }
    }
    serde_json::from_value(value).unwrap()
}
fn output(result: &ValidatedCustomOutput) -> &Path {
    Path::new(&result.plan.workspace.temp_path)
}
fn directions(settings: &CustomVariationSettings) -> [String; 2] {
    let mut faster = None;
    let mut slower = None;
    for i in 0..100 {
        let id = format!("speed-{i}");
        let TimingPolicy::UniformSpeed { speed } = FrozenCustomParameters::resolve(settings, &id)
            .unwrap()
            .timing()
        else {
            panic!()
        };
        if speed.num as u64 > speed.den {
            faster = Some(id);
        } else {
            slower = Some(id);
        }
        if faster.is_some() && slower.is_some() {
            break;
        }
    }
    [faster.unwrap(), slower.unwrap()]
}
#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_custom_speed_preserves_frames_and_pitch() {
    let mut settings = zero_custom();
    settings.speed = 9;
    for id in directions(&settings) {
        for name in [
            "短片 空格 '引号'.mp4",
            "vfr.mp4",
            "av-offset.mp4",
            "audio-leads.mp4",
            "custom-tones-2.mkv",
        ] {
            let result = run_custom(&fixtures().join(name), settings.clone(), &id)
                .await
                .unwrap_or_else(|e| panic!("{name} {id}: {e:?}"));
            assert_eq!(
                result.result.decoded_frames,
                result.plan.source.video.frame_count
            );
            if name == "custom-tones-2.mkv" {
                let pcm = decode_pcm(output(&result)).await;
                for (channel, expected) in [610.0, 730.0].into_iter().enumerate() {
                    let actual = frequency(&pcm, channel, 0.3, 1.8);
                    assert!(
                        (actual / expected - 1.0).abs() <= 0.02,
                        "speed changed pitch: {actual} vs {expected}"
                    );
                }
                let mut wrong = result.plan.clone();
                wrong.timing = TimingPolicy::UniformSpeed {
                    speed: Rational { num: 111, den: 100 },
                };
                assert!(
                    validate_output(
                        &NativeRunner::new(tool_paths()),
                        &wrong,
                        CancellationToken::new()
                    )
                    .await
                    .is_err(),
                    "wrong target must fail"
                );
            }
        }
    }
}
#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_custom_pitch_surround_and_tail() {
    for channels in [1, 2, 6, 8] {
        for speed in [0, 9] {
            let mut settings = zero_custom();
            settings.pitch = 9;
            settings.speed = speed;
            let result = run_custom(
                &fixtures().join(format!("custom-tones-{channels}.mkv")),
                settings,
                &format!("pitch-{channels}-{speed}"),
            )
            .await
            .unwrap_or_else(|e| panic!("channels {channels} speed {speed}: {e:?}"));
            let pcm = decode_pcm(output(&result)).await;
            assert_eq!(pcm.channels, channels);
            let rate = arg(&result.plan, "-af")
                .unwrap()
                .split(":pitch=")
                .nth(1)
                .unwrap()
                .split(':')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap();
            let samples = result.plan.policy.audio_target_samples.unwrap();
            assert!(
                pcm.samples.len() / usize::from(channels) >= samples.saturating_sub(1) as usize
            );
            let end = samples as f64 / 48000.0;
            for (channel, base) in [610.0, 730.0, 850.0, 90.0, 1090.0, 1230.0, 1370.0, 1490.0]
                .into_iter()
                .take(usize::from(channels))
                .enumerate()
            {
                let expected = base * rate;
                let actual = frequency(&pcm, channel, 0.3, 1.8);
                assert!(
                    (actual / expected - 1.0).abs() <= 0.02,
                    "channel {channel}: {actual} vs {expected}"
                );
                assert!(
                    rms(&pcm, channel, end - 0.08, end - 0.03) > rms(&pcm, channel, 0.5, 0.9) * 1.5,
                    "tail marker missing in channel {channel}"
                );
            }
            assert!(pcm.samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            let effective =
                &pcm.samples[..(samples as usize * usize::from(channels)).min(pcm.samples.len())];
            let silent = effective
                .chunks_exact(usize::from(channels))
                .rev()
                .take_while(|frame| frame.iter().all(|s| s.abs() < 0.0001))
                .count();
            assert!(silent <= 1024, "too much synthetic tail: {silent}");
        }
    }
}
fn left_foreground(frame: &[u8], width: usize, height: usize) -> usize {
    (0..width / 3)
        .find(|&x| {
            let rgb = &frame[(height / 2 * width + x) * 3..][..3];
            rgb[0] > 230 && rgb[1] < 100 && rgb[2] < 100
        })
        .expect("red foreground border")
}
fn edge_energy(frame: &[u8], width: usize, height: usize) -> f64 {
    (0..6)
        .map(|x| {
            let a = frame[(height / 2 * width + x) * 3] as f64;
            let b = frame[(height / 2 * width + x + 1) * 3] as f64;
            (a - b).powi(2)
        })
        .sum()
}
#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_custom_layout_and_crop() {
    for (name, width, height) in [
        ("custom-landscape.mp4", 320, 240),
        ("custom-portrait.mp4", 240, 320),
    ] {
        let input = fixtures().join(name);
        let original = decode_frames(&input).await;
        let zero = run_custom(&input, zero_custom(), "layout").await.unwrap();
        let frame = decode_frames(output(&zero)).await;
        for (x, y) in [
            (0, height / 2),
            (width - 1, height / 2),
            (width / 2, 0),
            (width / 2, height - 1),
        ] {
            let i = (y * width + x) * 3;
            for component in 0..3 {
                assert!(
                    (i16::from(frame[i + component]) - i16::from(original[i + component])).abs()
                        < 30,
                    "crop=0 lost border"
                );
            }
        }
        let mut mild = zero_custom();
        mild.foreground_scale = 2;
        mild.background_blur = 2;
        let a = run_custom(&input, mild.clone(), "layout").await.unwrap();
        let mut strong = mild.clone();
        strong.foreground_scale = 5;
        strong.background_blur = 5;
        let b = run_custom(&input, strong, "layout").await.unwrap();
        let a_frame = decode_frames(output(&a)).await;
        let b_frame = decode_frames(output(&b)).await;
        assert!(
            left_foreground(&a_frame, width, height) < left_foreground(&b_frame, width, height)
        );
        mild.background_blur = 5;
        let blurred = run_custom(&input, mild, "layout").await.unwrap();
        let blur_frame = decode_frames(output(&blurred)).await;
        assert!(
            edge_energy(&a_frame, width, height) > edge_energy(&blur_frame, width, height),
            "recommended background should preserve more edge detail"
        );
        let content = run_preset(&input, "content-variation-v1", None, "layout")
            .await
            .unwrap();
        let content_frame = decode_frames(output(&content)).await;
        assert!(
            left_foreground(&a_frame, width, height)
                < left_foreground(&content_frame, width, height)
        );
    }
    let input = fixtures().join("custom-sar.mp4");
    let mut settings = zero_custom();
    settings.crop_scale = 9;
    let result = run_custom(&input, settings, "crop-sar").await.unwrap();
    let json = tokio::process::Command::new(tool_paths().ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=sample_aspect_ratio",
            "-of",
            "json",
            output(&result).to_str().unwrap(),
        ])
        .output()
        .await
        .unwrap();
    assert!(json.status.success());
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["streams"][0]["sample_aspect_ratio"], "4:3");
}
#[tokio::test]
#[ignore = "requires native tools and generated fixtures"]
async fn generated_custom_all_levels_and_boundaries() {
    let input = fixtures().join("短片 空格 '引号'.mp4");
    let mut cases = vec![
        zero_custom(),
        CustomVariationSettings::recommended(),
        settings9(),
    ];
    let zero = serde_json::to_value(zero_custom()).unwrap();
    for key in zero
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.as_str() != "schemaVersion")
    {
        let mut settings = zero.clone();
        settings[key] = 9.into();
        if key == "backgroundBlur" || key == "dynamicOffset" {
            settings["foregroundScale"] = 2.into();
        }
        cases.push(serde_json::from_value(settings).unwrap());
    }
    for (i, settings) in cases.into_iter().enumerate() {
        run_custom(&input, settings, &format!("boundary-{i}"))
            .await
            .unwrap_or_else(|e| panic!("case {i}: {e:?}"));
    }
    for name in [
        "custom-tiny.mp4",
        "full-range.mp4",
        "no-audio.mp4",
        "surround.mkv",
        "surround-71.mp4",
    ] {
        run_custom(&fixtures().join(name), settings9(), "maximum")
            .await
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    for name in ["damaged.mp4", "hdr.mp4", "rgb.mp4"] {
        assert!(run_custom(&fixtures().join(name), settings9(), "invalid")
            .await
            .is_err());
    }
}
