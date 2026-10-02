use super::workspace::{open_directory, open_owned};
use crate::contracts::*;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub fn publish_no_replace(w: &OutputWorkspace) -> Result<PathBuf, AppError> {
    let _source_directory = open_owned(w)?;
    let meta = fs::symlink_metadata(&w.temp_path).map_err(AppError::io)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() == 0 {
        return Err(AppError::new(ErrorCode::ValidationFailed, "待保存媒体无效"));
    }
    let target = Path::new(&w.final_path);
    let _target_directory = open_directory(
        target
            .parent()
            .ok_or_else(|| AppError::new(ErrorCode::OutputPermission, "输出目录缺失"))?,
    )?;
    open_output_for_sync(Path::new(&w.temp_path))
        .and_then(|f| f.sync_all())
        .map_err(AppError::io)?;
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let src = std::ffi::CString::new("output.mp4").unwrap();
        let dst = std::ffi::CString::new(
            target
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| AppError::new(ErrorCode::OutputPermission, "输出名称无效"))?,
        )
        .map_err(|e| AppError::new(ErrorCode::OutputPermission, e.to_string()))?;
        if unsafe {
            libc::renameatx_np(
                _source_directory.as_raw_fd(),
                src.as_ptr(),
                _target_directory.as_raw_fd(),
                dst.as_ptr(),
                libc::RENAME_EXCL,
            )
        } != 0
        {
            return Err(move_error(std::io::Error::last_os_error()));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let src: Vec<u16> = Path::new(&w.temp_path)
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect();
        let dst: Vec<u16> = target.as_os_str().encode_wide().chain([0]).collect();
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(move_error(std::io::Error::last_os_error()));
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (_source_directory, _target_directory);
        return Err(AppError::new(
            ErrorCode::OutputPermission,
            "当前平台未实现排他保存",
        ));
    }
    #[cfg(any(target_os = "macos", windows))]
    Ok(target.to_path_buf())
}
fn move_error(e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::AlreadyExists || matches!(e.raw_os_error(), Some(80 | 183)) {
        AppError::new(ErrorCode::OutputConflict, "已有同名输出，未覆盖")
    } else {
        AppError::io(e)
    }
}
fn open_output_for_sync(path: &Path) -> std::io::Result<fs::File> {
    // Windows FlushFileBuffers requires GENERIC_WRITE after FFmpeg exits.
    fs::OpenOptions::new().read(true).write(true).open(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn flush_handle_has_windows_required_write_access() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.mp4");
        fs::write(&path, b"output").unwrap();
        let mut file = open_output_for_sync(&path).unwrap();
        file.write_all(b"output")
            .expect("FlushFileBuffers requires a writable handle");
        file.sync_all().unwrap();
        assert_eq!(fs::read(path).unwrap(), b"output");
    }
}
