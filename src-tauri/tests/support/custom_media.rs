use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use video_editor::{
    contracts::*,
    media::{
        probe::{probe_input, run_capture},
        validate::{validate_output, ValidationResult},
    },
    native::{
        process::{NativeRunner, RunSpec, StdoutPolicy},
        tools::{current_target, resolve_tools, Tool, ToolPaths},
    },
    output::workspace::prepare_workspace,
    presets::basic::build_plan,
};

pub fn tool_paths() -> ToolPaths {
    let installed = std::env::var_os("VIDEO_EDITOR_INSTALLED_EXE").map(PathBuf::from);
    resolve_tools(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        current_target(),
        installed.as_deref(),
    )
    .unwrap()
}
pub fn fixtures() -> PathBuf {
    std::env::var_os("VIDEO_EDITOR_FIXTURE_DIR")
        .expect("run npm run test:media")
        .into()
}
pub struct DecodedPcm {
    pub sample_rate: u32,
    pub channels: u8,
    pub samples: Vec<f32>,
}
pub async fn decode(args: &[&str]) -> Vec<u8> {
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new(tool_paths().ffmpeg)
            .kill_on_drop(true)
            .args(args)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
pub async fn decode_pcm(path: &Path) -> DecodedPcm {
    let media = probe_input(
        &NativeRunner::new(tool_paths()),
        path,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let bytes = decode(&[
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-map",
        "0:a:0",
        "-ar",
        "48000",
        "-f",
        "f32le",
        "pipe:1",
    ])
    .await;
    assert_eq!(bytes.len() % 4, 0);
    DecodedPcm {
        sample_rate: 48000,
        channels: media.audio.unwrap().channels,
        samples: bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect(),
    }
}
pub async fn decode_frames(path: &Path) -> Vec<u8> {
    decode(&[
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ])
    .await
}
pub struct ValidatedCustomOutput {
    pub result: ValidationResult,
    pub plan: ProcessingPlan,
    pub directory: tempfile::TempDir,
}
pub async fn run_custom(
    input: &Path,
    settings: CustomVariationSettings,
    job_id: &str,
) -> Result<ValidatedCustomOutput, AppError> {
    Box::pin(run_preset(
        input,
        "custom-variation-v1",
        Some(settings),
        job_id,
    ))
    .await
}
pub async fn run_preset(
    input: &Path,
    preset_id: &str,
    custom_settings: Option<CustomVariationSettings>,
    job_id: &str,
) -> Result<ValidatedCustomOutput, AppError> {
    let directory = tempfile::tempdir().unwrap();
    let runner = NativeRunner::new(tool_paths());
    let cancel = CancellationToken::new();
    let source = probe_input(&runner, input, cancel.clone()).await?;
    let workspace = prepare_workspace(
        directory.path(),
        input,
        job_id,
        &directory.path().join("records"),
    )?;
    let request = StartJobRequest {
        input_path: input.display().to_string(),
        output_directory: directory.path().display().to_string(),
        preset_id: preset_id.into(),
        custom_settings,
        metadata: MetadataRequest::Preserve,
    };
    let plan = build_plan(&source, &request, workspace, job_id)?;
    let result = run_capture(
        &runner,
        RunSpec {
            tool: Tool::Ffmpeg,
            args: plan.args.clone(),
            stdout_policy: StdoutPolicy::Discard,
        },
        cancel.clone(),
    )
    .await?;
    if result.canceled {
        return Err(AppError::new(ErrorCode::Canceled, "conversion canceled"));
    }
    if result.exit_code != Some(0) {
        return Err(
            AppError::new(ErrorCode::ProcessExit, "conversion failed").detail(result.stderr_tail)
        );
    }
    let result = match validate_output(&runner, &plan, cancel.clone()).await {
        Ok(result) => result,
        Err(error) => {
            let actual = probe_input(&runner, Path::new(&plan.workspace.temp_path), cancel).await;
            if let Ok(actual) = actual {
                eprintln!(
                    "validation {job_id}: target={:?}, actual video={:?}, audio={:?}",
                    video_editor::media::timing::target_timing(&plan.source, &plan.timing),
                    actual.video,
                    actual.audio
                );
            }
            return Err(error);
        }
    };
    Ok(ValidatedCustomOutput {
        result,
        plan,
        directory,
    })
}
pub fn arg<'a>(plan: &'a ProcessingPlan, key: &str) -> Option<&'a str> {
    plan.args
        .windows(2)
        .find(|a| a[0] == key)
        .map(|a| a[1].as_str())
}
pub fn frequency(pcm: &DecodedPcm, channel: usize, from: f64, to: f64) -> f64 {
    let stride = 6;
    let rate = f64::from(pcm.sample_rate) / stride as f64;
    let start = (from * f64::from(pcm.sample_rate)) as usize * usize::from(pcm.channels) + channel;
    let count = ((to - from) * rate) as usize;
    let values: Vec<f64> = pcm
        .samples
        .iter()
        .skip(start)
        .step_by(usize::from(pcm.channels) * stride)
        .take(count)
        .enumerate()
        .map(|(i, &v)| {
            f64::from(v)
                * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (count - 1) as f64).cos())
        })
        .collect();
    assert!(
        values.iter().any(|v| v.abs() > 0.0001),
        "missing channel {channel}"
    );
    let power = |frequency: f64| {
        let coefficient = 2.0 * (std::f64::consts::TAU * frequency / rate).cos();
        let (mut a, mut b) = (0.0, 0.0);
        for &sample in &values {
            let next = sample + coefficient * a - b;
            b = a;
            a = next;
        }
        a * a + b * b - coefficient * a * b
    };
    let mut best = (40.0, 0.0);
    for hz in (40..2400).step_by(5) {
        let hz = f64::from(hz);
        let p = power(hz);
        if p > best.1 {
            best = (hz, p);
        }
    }
    let coarse = best.0;
    for i in -20..=20 {
        let hz = coarse + f64::from(i) * 0.25;
        let p = power(hz);
        if p > best.1 {
            best = (hz, p);
        }
    }
    best.0
}
pub fn rms(pcm: &DecodedPcm, channel: usize, from: f64, to: f64) -> f64 {
    let start = (from * f64::from(pcm.sample_rate)) as usize;
    let count = ((to - from) * f64::from(pcm.sample_rate)) as usize;
    let values = pcm
        .samples
        .iter()
        .skip(start * usize::from(pcm.channels) + channel)
        .step_by(usize::from(pcm.channels))
        .take(count);
    let (sum, count) = values.fold((0.0, 0), |(sum, count), &v| {
        (sum + f64::from(v).powi(2), count + 1)
    });
    assert!(count > 0);
    (sum / count as f64).sqrt()
}

#[test]
fn frequency_estimator_handles_modulated_amplitude_without_losing_cycles() {
    let samples = (0..96000)
        .map(|i| {
            let t = f64::from(i) / 48000.0;
            ((0.51 + 0.49 * (2.0 * std::f64::consts::PI * 7.0 * t).sin())
                * (2.0 * std::f64::consts::PI * 1190.0 * t).sin()
                * 0.1) as f32
        })
        .collect();
    let pcm = DecodedPcm {
        sample_rate: 48000,
        channels: 1,
        samples,
    };
    assert!((frequency(&pcm, 0, 0.3, 1.8) - 1190.0).abs() < 1.0);
}
