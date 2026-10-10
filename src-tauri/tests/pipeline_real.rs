use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*,
    jobs::service::JobService,
    media::probe::{file_identity, probe_input},
    native::{
        process::NativeRunner,
        tools::{current_target, resolve_tools, ToolPaths},
    },
};

fn runner() -> Arc<NativeRunner> {
    Arc::new(NativeRunner::new(tool_paths()))
}
fn tool_paths() -> ToolPaths {
    // CI can exercise the actual installed media tools after an NSIS installation.
    let installed = std::env::var_os("VIDEO_EDITOR_INSTALLED_EXE").map(PathBuf::from);
    resolve_tools(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        current_target(),
        installed.as_deref(),
    )
    .unwrap()
}
async fn decode(args: &[&str]) -> Vec<u8> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(tool_paths().ffmpeg)
            .kill_on_drop(true)
            .args(args)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}
fn request(input: &Path, output: &Path) -> StartJobRequest {
    StartJobRequest {
        input_path: input.display().to_string(),
        output_directory: output.display().to_string(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    }
}
async fn process(
    input: &Path,
    cancel_running: bool,
) -> (JobSnapshot, tempfile::TempDir, Vec<JobState>) {
    process_with_preset(input, cancel_running, "basic-transcode-v1").await
}
async fn process_with_preset(
    input: &Path,
    cancel_running: bool,
    preset_id: &str,
) -> (JobSnapshot, tempfile::TempDir, Vec<JobState>) {
    let d = tempfile::tempdir().unwrap();
    let output = d.path().join("输出 空格 '目录'");
    std::fs::create_dir(&output).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let service = JobService::new(
        runner(),
        d.path().join("records"),
        Arc::new(move |s| {
            let _ = tx.send(s);
        }),
    );
    let mut request = request(input, &output);
    request.preset_id = preset_id.into();
    let id = service.start_job(request).await.unwrap();
    let mut states = vec![];
    let snapshot = tokio::time::timeout(std::time::Duration::from_secs(300), async {
        while let Some(snapshot) = rx.recv().await {
            assert_eq!(snapshot.job_id, id);
            states.push(snapshot.state);
            if snapshot.state != JobState::Succeeded {
                assert!(snapshot.progress.is_none_or(|p| p < 1.0));
            }
            if cancel_running && snapshot.state == JobState::Running {
                service.cancel_job(&id).await.unwrap();
            }
            if matches!(
                snapshot.state,
                JobState::Succeeded | JobState::Failed | JobState::Canceled
            ) {
                return snapshot;
            }
        }
        panic!("missing terminal snapshot")
    })
    .await
    .unwrap();
    if snapshot.state == JobState::Succeeded {
        let log = service.read_log(&id).await.unwrap();
        assert!(log.text.contains("完整校验通过，结果已保存"));
    }
    service.shutdown().await.unwrap();
    (snapshot, d, states)
}
#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_media_end_to_end() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for name in [
        "短片 空格 '引号'.mp4",
        "no-audio.mp4",
        "av-offset.mp4",
        "full-range.mp4",
        "vfr.mp4",
        "phone.mov",
        "clip.m4v",
        "surround.mkv",
        "surround-71.mp4",
        "capture.webm",
    ] {
        let source = fixtures.join(name);
        let before = probe_input(runner().as_ref(), &source, CancellationToken::new())
            .await
            .unwrap();
        let (result, _dir, states) = process(&source, false).await;
        assert_eq!(
            result.state,
            JobState::Succeeded,
            "{name}: {:?}",
            result.error
        );
        assert!(states.contains(&JobState::Validating));
        assert!(states.contains(&JobState::Committing));
        let after = probe_input(
            runner().as_ref(),
            Path::new(result.output_path.as_ref().unwrap()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(before.video.frame_count, after.video.frame_count);
        assert_eq!(
            before.audio.as_ref().map(|a| a.channels),
            after.audio.as_ref().map(|a| a.channels)
        );
        assert_eq!(
            serde_json::to_value(&before.video).unwrap()["timeline"]["timestampSha256"],
            serde_json::to_value(&after.video).unwrap()["timeline"]["timestampSha256"]
        );
        assert_eq!(before.video.color_range, after.video.color_range);
        assert_eq!(
            after.audio.as_ref().map(|a| a.sample_rate),
            before.audio.as_ref().map(|_| 48000)
        );
        assert_eq!(
            file_identity(&source, &CancellationToken::new())
                .await
                .unwrap(),
            before.identity
        );
        assert_ne!(
            before.identity.canonical_path,
            after.identity.canonical_path
        );
    }
    for (name, code) in [
        ("hdr.mp4", ErrorCode::UnsupportedInput),
        ("rgb.mp4", ErrorCode::UnsupportedInput),
        ("damaged.mp4", ErrorCode::DamagedMedia),
    ] {
        let (result, _, _) = process(&fixtures.join(name), false).await;
        assert_eq!(result.state, JobState::Failed);
        assert_eq!(result.error.unwrap().code, code, "{name}");
        assert!(result.output_path.is_none());
    }
}
#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_sample_preset_preserves_vfr_offsets_surround_and_silent_inputs() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for preset in [
        "sample-match-v1",
        "sample-match-v2",
        "repeat-variant-v1",
        "repeat-combined-v2",
        "content-variation-v1",
    ] {
        for name in [
            "vfr.mp4",
            "av-offset.mp4",
            "surround.mkv",
            "surround-71.mp4",
            "no-audio.mp4",
            "full-range.mp4",
            "capture.webm",
        ] {
            let input = fixtures.join(name);
            let before = probe_input(runner().as_ref(), &input, CancellationToken::new())
                .await
                .unwrap();
            let (snapshot, _dir, _) = process_with_preset(&input, false, preset).await;
            assert_eq!(
                snapshot.state,
                JobState::Succeeded,
                "{preset}/{name}: {:?}",
                snapshot.error
            );
            let output = probe_input(
                runner().as_ref(),
                Path::new(snapshot.output_path.as_ref().unwrap()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
            assert_eq!(before.video.frame_count, output.video.frame_count);
            assert_eq!(
                before.video.timeline.as_ref().unwrap().timestamp_sha256,
                output.video.timeline.as_ref().unwrap().timestamp_sha256
            );
            assert_eq!(
                before.audio.as_ref().map(|a| a.channels),
                output.audio.as_ref().map(|a| a.channels)
            );
            assert_eq!(
                before
                    .audio
                    .as_ref()
                    .and_then(|a| a.channel_layout.as_ref()),
                output
                    .audio
                    .as_ref()
                    .and_then(|a| a.channel_layout.as_ref())
            );
            assert_eq!(
                before.identity,
                file_identity(&input, &CancellationToken::new())
                    .await
                    .unwrap()
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_repeat_preset_changes_decoded_frames_with_small_error_and_identical_audio() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for (name, has_audio) in [
        ("短片 空格 '引号'.mp4", true),
        ("sample-tones.mp4", true),
        ("full-range.mp4", false),
        ("no-audio.mp4", false),
    ] {
        let input = fixtures.join(name);
        let (basic, _basic_dir, _) = process_with_preset(&input, false, "basic-transcode-v1").await;
        let (first, _first_dir, _) = process_with_preset(&input, false, "repeat-variant-v1").await;
        let (second, _second_dir, _) =
            process_with_preset(&input, false, "repeat-variant-v1").await;
        let mut pictures = vec![];
        let mut audio = vec![];
        let original = decode(&[
            "-v",
            "error",
            "-i",
            input.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .await;
        for result in [&basic, &first, &second] {
            assert_eq!(
                result.state,
                JobState::Succeeded,
                "{name}: {:?}",
                result.error
            );
            let output = result.output_path.as_ref().unwrap();
            pictures.push(
                decode(&[
                    "-v",
                    "error",
                    "-i",
                    output,
                    "-map",
                    "0:v:0",
                    "-pix_fmt",
                    "yuv420p",
                    "-fps_mode",
                    "passthrough",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .await,
            );
            audio.push(if has_audio {
                decode(&[
                    "-v", "error", "-i", output, "-map", "0:a:0", "-f", "f32le", "pipe:1",
                ])
                .await
            } else {
                vec![]
            });
        }
        assert!(!pictures[0].is_empty());
        let rmse = |picture: &[u8]| {
            assert_eq!(picture.len(), original.len());
            (original
                .iter()
                .zip(picture)
                .map(|(a, b)| {
                    let error = f64::from(*a) - f64::from(*b);
                    error * error
                })
                .sum::<f64>()
                / picture.len() as f64)
                .sqrt()
        };
        let basic_error = rmse(&pictures[0]);
        assert!(
            pictures[1] != pictures[2],
            "{name}: decoded frames must vary between jobs"
        );
        for variant in &pictures[1..] {
            assert_eq!(variant.len(), pictures[0].len());
            // Compare both encodes with the source. Differences between two
            // lossy encodes alone also include their different bit allocation.
            let error = rmse(variant);
            eprintln!("{name}: source RMSE basic={basic_error}, variant={error}");
            assert!(
                error <= basic_error + 1.0,
                "{name}: source RMSE {error} exceeds basic {basic_error} + one 8-bit level"
            );
        }
        assert_eq!(!audio[0].is_empty(), has_audio);
        assert!(
            audio[0] == audio[1],
            "{name}: audio must match basic conversion"
        );
        assert!(
            audio[0] == audio[2],
            "{name}: audio must match basic conversion"
        );
    }
}

#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_combined_changes_decoded_picture_and_audio_with_bounded_content_error() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for name in [
        "短片 空格 '引号'.mp4",
        "sample-tones.mp4",
        "phase-surround.mkv",
        "phase-low-rate.mp4",
    ] {
        let input = fixtures.join(name);
        let source = probe_input(runner().as_ref(), &input, CancellationToken::new())
            .await
            .unwrap();
        let channels = source.audio.as_ref().unwrap().channels as usize;
        let (basic, _basic_dir, _) = process_with_preset(&input, false, "basic-transcode-v1").await;
        let (first, _first_dir, _) = process_with_preset(&input, false, "repeat-combined-v2").await;
        let (second, _second_dir, _) =
            process_with_preset(&input, false, "repeat-combined-v2").await;
        let original = decode(&[
            "-v",
            "error",
            "-i",
            input.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .await;
        let mut pictures = vec![];
        let mut audio = vec![];
        for result in [&basic, &first, &second] {
            assert_eq!(
                result.state,
                JobState::Succeeded,
                "{name}: {:?}",
                result.error
            );
            let output = result.output_path.as_ref().unwrap();
            pictures.push(
                decode(&[
                    "-v",
                    "error",
                    "-i",
                    output,
                    "-map",
                    "0:v:0",
                    "-pix_fmt",
                    "yuv420p",
                    "-fps_mode",
                    "passthrough",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .await,
            );
            audio.push(
                decode(&[
                    "-v", "error", "-i", output, "-map", "0:a:0", "-ss", "0.2", "-t", "0.5", "-f",
                    "f32le", "pipe:1",
                ])
                .await,
            );
        }
        assert!(!pictures[0].is_empty() && !audio[0].is_empty());
        assert!(
            pictures[1] != pictures[2],
            "{name}: decoded pictures must differ"
        );
        assert!(audio[1] != audio[2], "{name}: decoded PCM must differ");
        for variant in &pictures[1..] {
            assert_eq!(variant.len(), original.len());
            let mae = original
                .iter()
                .zip(variant)
                .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
                .sum::<f64>()
                / original.len() as f64;
            eprintln!("{name}: combined source MAE={mae}");
            assert!(
                mae <= 12.0,
                "{name}: mean absolute picture error {mae} exceeds 12 eight-bit levels"
            );
        }
        let samples = |pcm: &[u8]| {
            pcm.chunks_exact(4)
                .map(|b| f64::from(f32::from_le_bytes(b.try_into().unwrap())))
                .collect::<Vec<_>>()
        };
        let basic_samples = samples(&audio[0]);
        for pcm in &audio[1..] {
            assert_eq!(
                pcm.len(),
                audio[0].len(),
                "{name}: audio sample count changed"
            );
            let values = samples(pcm);
            assert!(
                values.iter().all(|v| v.is_finite() && v.abs() <= 1.01),
                "{name}: non-finite or clipped PCM"
            );
            for channel in 0..channels {
                let rms = |values: &[f64]| {
                    (values
                        .iter()
                        .skip(channel)
                        .step_by(channels)
                        .map(|v| v * v)
                        .sum::<f64>()
                        / (values.len() / channels) as f64)
                        .sqrt()
                };
                let ratio = rms(&values) / rms(&basic_samples);
                assert!(
                    (0.45..=1.2).contains(&ratio),
                    "{name}: channel {channel} RMS ratio {ratio}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_combined_preserves_non_square_pixel_aspect_ratio() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("anamorphic.mp4");
    decode(&[
        "-v",
        "error",
        "-i",
        fixtures.join("短片 空格 '引号'.mp4").to_str().unwrap(),
        "-vf",
        "setsar=4/3",
        "-c:v",
        "libx264",
        "-c:a",
        "copy",
        input.to_str().unwrap(),
    ])
    .await;
    for preset in ["repeat-combined-v2", "content-variation-v1"] {
        let (result, _output_dir, _) = process_with_preset(&input, false, preset).await;
        assert_eq!(result.state, JobState::Succeeded, "{:?}", result.error);
        let probe = tokio::process::Command::new(tool_paths().ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=sample_aspect_ratio",
                "-of",
                "default=nw=1:nk=1",
                result.output_path.as_ref().unwrap(),
            ])
            .output()
            .await
            .unwrap();
        assert!(
            probe.status.success(),
            "{}",
            String::from_utf8_lossy(&probe.stderr)
        );
        assert_eq!(String::from_utf8(probe.stdout).unwrap().trim(), "4:3");
    }
}

#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_content_variation_shifts_each_channel_pitch_without_losing_timing() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for (name, frequencies) in [
        (
            "phase-surround.mkv",
            vec![610.0, 730.0, 850.0, 90.0, 1090.0, 1230.0, 1370.0, 1490.0],
        ),
        ("phase-low-rate.mp4", vec![1700.0]),
    ] {
        let input = fixtures.join(name);
        let original = decode(&[
            "-v",
            "error",
            "-i",
            input.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .await;
        let mut outputs = vec![];
        for _ in 0..2 {
            let (result, _dir, _) =
                process_with_preset(&input, false, "content-variation-v1").await;
            assert_eq!(
                result.state,
                JobState::Succeeded,
                "{name}: {:?}",
                result.error
            );
            let output = result.output_path.as_ref().unwrap();
            let picture = decode(&[
                "-v",
                "error",
                "-i",
                output,
                "-map",
                "0:v:0",
                "-pix_fmt",
                "yuv420p",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .await;
            assert_eq!(picture.len(), original.len());
            assert!(
                picture != original,
                "{name}: layout must change decoded pixels"
            );
            let pcm = decode(&[
                "-v", "error", "-i", output, "-map", "0:a:0", "-ss", "0.2", "-t", "0.7", "-f",
                "f32le", "pipe:1",
            ])
            .await;
            let values: Vec<f64> = pcm
                .chunks_exact(4)
                .map(|b| f64::from(f32::from_le_bytes(b.try_into().unwrap())))
                .collect();
            assert_eq!(values.len(), 33600 * frequencies.len());
            assert!(values.iter().all(|v| v.is_finite() && v.abs() <= 1.01));
            for (channel, frequency) in frequencies.iter().enumerate() {
                let samples: Vec<f64> = values
                    .iter()
                    .skip(channel)
                    .step_by(frequencies.len())
                    .copied()
                    .collect();
                // WSOLA phase joins can confuse zero-crossing counts. Scan the
                // decoded signal's dominant frequency with a Hann window.
                let n = samples.len().div_ceil(4);
                let windowed: Vec<f64> = samples
                    .iter()
                    .step_by(4)
                    .enumerate()
                    .map(|(i, v)| {
                        v * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos())
                    })
                    .collect();
                let candidates = (frequency * 0.80 * 2.0) as u32..=(frequency * 1.24 * 2.0) as u32;
                let detected = candidates
                    .map(|half_hz| {
                        let hz = f64::from(half_hz) / 2.0;
                        let coefficient = 2.0 * (std::f64::consts::TAU * hz / 12000.0).cos();
                        let (mut prev, mut older) = (0.0, 0.0);
                        for sample in &windowed {
                            let current = sample + coefficient * prev - older;
                            older = prev;
                            prev = current;
                        }
                        (hz, prev * prev + older * older - coefficient * prev * older)
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap()
                    .0;
                let ratio = detected / frequency;
                eprintln!("{name}: channel {channel}, source={frequency} Hz, decoded={detected} Hz, ratio={ratio}");
                assert!(
                    (0.83..=0.90).contains(&ratio) || (1.11..=1.20).contains(&ratio),
                    "{name}: channel {channel} pitch ratio {ratio}"
                );
                let rms =
                    (samples.iter().map(|v| v * v).sum::<f64>() / samples.len() as f64).sqrt();
                assert!(
                    (0.06..=0.2).contains(&rms),
                    "{name}: channel {channel} RMS {rms}"
                );
            }
            let tail = decode(&[
                "-v", "error", "-i", output, "-map", "0:a:0", "-ss", "1.0", "-t", "0.15", "-f",
                "f32le", "pipe:1",
            ])
            .await;
            let tail: Vec<f64> = tail
                .chunks_exact(4)
                .map(|b| f64::from(f32::from_le_bytes(b.try_into().unwrap())))
                .collect();
            assert!(!tail.is_empty());
            for channel in 0..frequencies.len() {
                let rms = (tail
                    .iter()
                    .skip(channel)
                    .step_by(frequencies.len())
                    .map(|v| v * v)
                    .sum::<f64>()
                    / (tail.len() / frequencies.len()) as f64)
                    .sqrt();
                assert!(
                    rms > 0.04,
                    "{name}: channel {channel} padded away audio tail"
                );
            }
            outputs.push((picture, pcm));
        }
        assert!(outputs[0].0 != outputs[1].0 && outputs[0].1 != outputs[1].1);
    }
}

#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_sample_preset_changes_pixels_and_low_frequency_energy() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    let input = fixtures.join("sample-tones.mp4");
    let mut measurements = vec![];
    for preset in ["basic-transcode-v1", "sample-match-v1", "sample-match-v2"] {
        let (result, _dir, _) = process_with_preset(&input, false, preset).await;
        assert_eq!(
            result.state,
            JobState::Succeeded,
            "{preset}: {:?}",
            result.error
        );
        let output = result.output_path.unwrap();
        let frame = decode(&[
            "-v",
            "error",
            "-i",
            &output,
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .await;
        assert_eq!(frame.len(), 128 * 96 * 3 / 2);
        // Compare encoded Y values directly; conversion to gray can rescale a
        // stream whose color range is unspecified and confound this measurement.
        let brightness =
            frame[..128 * 96].iter().map(|b| f64::from(*b)).sum::<f64>() / (128 * 96) as f64;
        let pcm = decode(&[
            "-v", "error", "-i", &output, "-map", "0:a:0", "-ss", "0.3", "-t", "0.5", "-ac", "1",
            "-ar", "48000", "-f", "f32le", "pipe:1",
        ])
        .await;
        let samples: Vec<f64> = pcm
            .chunks_exact(4)
            .map(|b| f64::from(f32::from_le_bytes(b.try_into().unwrap())))
            .collect();
        assert_eq!(samples.len(), 24000);
        let amplitude = |frequency: f64| {
            let (real, imaginary) =
                samples
                    .iter()
                    .enumerate()
                    .fold((0.0, 0.0), |(real, imaginary), (i, value)| {
                        let angle = std::f64::consts::TAU * frequency * i as f64 / 48000.0;
                        (real + value * angle.cos(), imaginary + value * angle.sin())
                    });
            real.hypot(imaginary) / samples.len() as f64
        };
        measurements.push((brightness, amplitude(100.0) / amplitude(1000.0)));
    }
    let basic = measurements[0];
    assert!(
        basic.1 > 0.8 && basic.1 < 1.2,
        "basic conversion should retain the two-tone balance: {basic:?}"
    );
    for sample in &measurements[1..] {
        assert!(
            sample.1 < 0.5,
            "sample processing should attenuate the low tone: {sample:?}"
        );
        assert!(
            sample.0 < basic.0 - 0.5 && sample.0 > basic.0 - 4.0,
            "sample processing should gently lower luminance: basic={basic:?}, sample={sample:?}"
        );
    }
    assert_eq!(measurements[1].0, measurements[2].0, "v2 keeps v1 pixels");
}

#[tokio::test]
#[ignore = "requires prepared native tools and generated media"]
async fn generated_phase_preset_changes_each_channel_after_aac_encoding() {
    let fixtures = PathBuf::from(
        std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR").expect("run npm run test:media"),
    );
    for (name, frequencies) in [
        (
            "phase-surround.mkv",
            vec![610.0, 730.0, 850.0, 90.0, 1090.0, 1230.0, 1370.0, 1490.0],
        ),
        ("phase-low-rate.mkv", vec![1700.0]),
        ("phase-low-rate.mp4", vec![1700.0]),
    ] {
        let (result, _dir, _) =
            process_with_preset(&fixtures.join(name), false, "sample-match-v2").await;
        assert_eq!(result.state, JobState::Succeeded, "{:?}", result.error);
        let pcm = decode(&[
            "-v",
            "error",
            "-i",
            result.output_path.as_ref().unwrap(),
            "-map",
            "0:a:0",
            "-ss",
            "0.3",
            "-t",
            "0.5",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .await;
        let samples: Vec<f64> = pcm
            .chunks_exact(4)
            .map(|b| f64::from(f32::from_le_bytes(b.try_into().unwrap())))
            .collect();
        assert_eq!(samples.len(), 24000 * frequencies.len());
        for (channel, frequency) in frequencies.iter().copied().enumerate() {
            let (real, imaginary) = samples.chunks_exact(frequencies.len()).enumerate().fold(
                (0.0, 0.0),
                |(re, im), (i, frame)| {
                    let angle = std::f64::consts::TAU * frequency * i as f64 / 48000.0;
                    (
                        re + frame[channel] * angle.cos(),
                        im - frame[channel] * angle.sin(),
                    )
                },
            );
            // Independent analog two-pole response; tolerance covers bilinear warping
            // and AAC quantization. Every tone completes whole cycles in the window.
            let hp = frequency / 300.0;
            let lp = frequency / 5500.0;
            let hp_phase =
                std::f64::consts::PI - (std::f64::consts::SQRT_2 * hp).atan2(1.0 - hp * hp);
            let lp_phase = -(std::f64::consts::SQRT_2 * lp).atan2(1.0 - lp * lp);
            let expected_phase = -std::f64::consts::FRAC_PI_2 + hp_phase + lp_phase
                - std::f64::consts::TAU * frequency * 0.005;
            let phase_error = imaginary.atan2(real) - expected_phase;
            let phase_error = phase_error.sin().atan2(phase_error.cos()).abs();
            assert!(
                phase_error < 0.15,
                "{name} channel {channel}: phase error {phase_error} rad"
            );
            let expected_amplitude = 0.2 * 0.8 * hp * hp
                / (1.0 - hp * hp).hypot(std::f64::consts::SQRT_2 * hp)
                / (1.0 - lp * lp).hypot(std::f64::consts::SQRT_2 * lp);
            let amplitude = 2.0 * real.hypot(imaginary) / 24000.0;
            assert!(
                (amplitude / expected_amplitude - 1.0).abs() < 0.15,
                "channel {channel}: amplitude {amplitude}, expected {expected_amplitude}"
            );
        }
    }
}
#[tokio::test]
#[ignore = "requires an explicitly supplied compatibility sample"]
async fn provided_compatible_media_end_to_end() {
    let source =
        PathBuf::from(std::env::var_os("VIDEO_EDITOR_COMPAT_SAMPLE").expect("supply sample path"));
    let before = probe_input(runner().as_ref(), &source, CancellationToken::new())
        .await
        .unwrap();
    for preset in [
        "basic-transcode-v1",
        "sample-match-v1",
        "sample-match-v2",
        "repeat-variant-v1",
        "repeat-combined-v2",
        "content-variation-v1",
    ] {
        let (result, _dir, _) = process_with_preset(&source, false, preset).await;
        assert_eq!(
            result.state,
            JobState::Succeeded,
            "{preset}: {:?}",
            result.error
        );
        let after = probe_input(
            runner().as_ref(),
            Path::new(result.output_path.as_ref().unwrap()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(before.video.frame_count, after.video.frame_count);
        let source_timeline = serde_json::to_value(&before.video).unwrap()["timeline"].clone();
        assert!(source_timeline["timestampSha256"].is_string());
        assert_eq!(
            source_timeline["timestampSha256"],
            serde_json::to_value(&after.video).unwrap()["timeline"]["timestampSha256"]
        );
        assert_eq!(
            file_identity(&source, &CancellationToken::new())
                .await
                .unwrap(),
            before.identity
        );
    }
}
#[tokio::test]
#[ignore = "requires explicitly supplied read-only sample directory"]
async fn provided_sample_end_to_end() {
    let sample =
        PathBuf::from(std::env::var_os("VIDEO_EDITOR_SAMPLE_DIR").expect("supply --sample-dir"))
            .join("原视频.mp4");
    let before = probe_input(runner().as_ref(), &sample, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(before.video.frame_count, 2713);
    let (result, _dir, _) = process(&sample, false).await;
    assert_eq!(result.state, JobState::Succeeded, "{:?}", result.error);
    let output = probe_input(
        runner().as_ref(),
        Path::new(result.output_path.as_ref().unwrap()),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(output.video.frame_count, 2713);
    assert_eq!(output.audio.unwrap().sample_rate, 48000);
    assert_eq!(
        file_identity(&sample, &CancellationToken::new())
            .await
            .unwrap(),
        before.identity
    );
    let (canceled, dir, _) = process(&sample, true).await;
    assert_eq!(canceled.state, JobState::Canceled);
    assert!(canceled.output_path.is_none());
    assert!(!canceled.cleanup_pending);
    assert_eq!(
        std::fs::read_dir(dir.path().join("输出 空格 '目录'"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        file_identity(&sample, &CancellationToken::new())
            .await
            .unwrap(),
        before.identity
    );
}
