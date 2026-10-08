use crate::contracts::{AppError, ErrorCode};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

pub struct InputCandidate {
    pub input_path: String,
    pub key: String,
}

pub fn collect_folder(folder: &Path) -> Result<Vec<InputCandidate>, AppError> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(folder).map_err(AppError::io)? {
        let entry = entry.map_err(AppError::io)?;
        let file_type = entry.file_type().map_err(AppError::io)?;
        if !(file_type.is_dir() || file_type.is_symlink() && entry.path().is_dir())
            && is_supported_video(&entry.path())
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    collect_paths(&paths)
}

pub fn collect_paths(paths: &[PathBuf]) -> Result<Vec<InputCandidate>, AppError> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for path in paths {
        if !is_supported_video(path) || path.is_dir() {
            return Err(AppError::new(
                ErrorCode::UnsupportedInput,
                "请选择 MP4、MOV、M4V、MKV 或 WebM 视频",
            ));
        }
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir().map_err(AppError::io)?.join(path)
        };
        let resolved = std::fs::canonicalize(&absolute).unwrap_or_else(|_| normalize(&absolute));
        let input_path = resolved
            .to_str()
            .ok_or_else(|| AppError::new(ErrorCode::UnsupportedInput, "输入路径无法表示"))?
            .to_owned();
        let key = path_key(&input_path);
        if seen.insert(key.clone()) {
            result.push(InputCandidate { input_path, key });
        }
    }
    Ok(result)
}

pub fn is_supported_video(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["mp4", "mov", "m4v", "mkv", "webm"]
            .iter()
            .any(|ext| s.eq_ignore_ascii_case(ext))
    })
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}
