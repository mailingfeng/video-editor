use crate::contracts::{AppError, ErrorCode};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct ToolPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Ffmpeg,
    Ffprobe,
}
pub fn resolve_tools(
    manifest_dir: &Path,
    target: &str,
    bundled_exe: Option<&Path>,
) -> Result<ToolPaths, AppError> {
    let paths = expected_tool_paths(manifest_dir, target, bundled_exe)?;
    for p in [&paths.ffmpeg, &paths.ffprobe] {
        if !p.is_file() {
            return Err(AppError::new(ErrorCode::ToolMissing, "媒体工具缺失")
                .detail(p.display().to_string()));
        }
    }
    Ok(paths)
}
// Fixed locations are also needed to start the UI when a package is incomplete.
// Commands report the resolver error instead of searching PATH or accepting a path from IPC.
pub fn expected_tool_paths(
    manifest_dir: &Path,
    target: &str,
    bundled_exe: Option<&Path>,
) -> Result<ToolPaths, AppError> {
    if ![
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ]
    .contains(&target)
    {
        return Err(AppError::new(
            ErrorCode::ToolMissing,
            "不支持的媒体工具架构",
        ));
    }
    let suffix = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let path = |name: &str| -> PathBuf {
        match bundled_exe.and_then(Path::parent) {
            Some(dir) => dir.join(format!("{name}{suffix}")),
            None => manifest_dir
                .join("binaries")
                .join(format!("{name}-{target}{suffix}")),
        }
    };
    let paths = ToolPaths {
        ffmpeg: path("ffmpeg"),
        ffprobe: path("ffprobe"),
    };
    Ok(paths)
}
pub fn current_target() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(not(any(
        all(
            target_os = "macos",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        "unsupported"
    }
}
