use std::path::Path;
use video_editor::contracts::*;
pub fn media() -> MediaInfo {
    MediaInfo {
        identity: FileIdentity {
            canonical_path: "/input/original.mp4".into(),
            size_bytes: 1000,
            modified_ns: "100".into(),
            sha256: "source".into(),
        },
        container: "mp4".into(),
        video: VideoInfo {
            stream_index: 0,
            codec: "h264".into(),
            width: 720,
            height: 1280,
            bit_depth: 8,
            pixel_format: "yuv420p".into(),
            frame_rate: Rational { num: 30, den: 1 },
            time_base: Rational { num: 1, den: 15360 },
            frame_count: 60,
            start_pts: 0,
            duration_ticks: 30720,
            bit_rate: Some(1596000),
            color_range: Some("tv".into()),
            color_space: Some("bt709".into()),
            color_primaries: Some("bt709".into()),
            color_transfer: Some("bt709".into()),
        },
        audio: Some(AudioInfo {
            stream_index: 1,
            codec: "aac".into(),
            sample_rate: 44100,
            channels: 2,
            time_base: Rational { num: 1, den: 44100 },
            start_pts: 0,
            duration_ticks: 88200,
            bit_rate: Some(74000),
        }),
        title: None,
        comment: Some("source".into()),
    }
}
pub fn request(input: &str, output: &str) -> StartJobRequest {
    StartJobRequest {
        input_path: input.into(),
        output_directory: output.into(),
        preset_id: "basic-transcode-v1".into(),
        metadata: MetadataRequest::Preserve,
    }
}
pub fn workspace(root: &Path) -> OutputWorkspace {
    OutputWorkspace {
        job_id: "test".into(),
        directory: root.to_str().unwrap().into(),
        temp_path: root.join("output.mp4").to_str().unwrap().into(),
        final_path: root
            .join("original_processed_test.mp4")
            .to_str()
            .unwrap()
            .into(),
        record_path: root.join("record.json").to_str().unwrap().into(),
        directory_identity: DirectoryIdentity {
            volume_id: "0".into(),
            file_id: "0".into(),
        },
    }
}
