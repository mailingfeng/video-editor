use crate::contracts::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub schema_version: u8,
    pub workspace: OutputWorkspace,
    pub cleanup_pending: bool,
    pub plan: Option<ProcessingPlan>,
    pub snapshot: Option<JobSnapshot>,
}
pub fn prepare_workspace(
    output_dir: &Path,
    source: &Path,
    job_id: &str,
    record_dir: &Path,
) -> Result<OutputWorkspace, AppError> {
    if !valid_job_id(job_id) {
        return Err(AppError::new(ErrorCode::OutputPermission, "任务 ID 无效"));
    }
    let root = fs::canonicalize(output_dir).map_err(AppError::io)?;
    if !root.is_dir() {
        return Err(AppError::new(
            ErrorCode::OutputPermission,
            "输出位置必须为目录",
        ));
    }
    let source = fs::canonicalize(source).map_err(AppError::io)?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::new(ErrorCode::UnsupportedInput, "文件名无法表示"))?;
    fs::create_dir_all(record_dir).map_err(AppError::io)?;
    let records = fs::canonicalize(record_dir).map_err(AppError::io)?;
    let directory = root.join(format!(".video-editor-{job_id}"));
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = builder;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory).map_err(AppError::io)?;
    let result = (|| {
        let file = open_directory(&directory)?;
        let w = OutputWorkspace {
            job_id: job_id.into(),
            directory: path_text(&directory)?,
            temp_path: path_text(&directory.join("output.mp4"))?,
            final_path: path_text(&root.join(format!("{stem}_processed_{job_id}.mp4")))?,
            record_path: path_text(&records.join(format!("{job_id}.json")))?,
            directory_identity: directory_identity(&file)?,
        };
        if Path::new(&w.final_path) == source {
            return Err(AppError::new(ErrorCode::OutputConflict, "输出与输入冲突"));
        }
        create_json(&directory.join(".owner.json"), &w)?;
        create_json(
            Path::new(&w.record_path),
            &JobRecord {
                schema_version: 1,
                workspace: w.clone(),
                cleanup_pending: false,
                plan: None,
                snapshot: None,
            },
        )?;
        Ok(w)
    })();
    if result.is_err() {
        let _ = fs::remove_file(directory.join(".owner.json"));
        let _ = fs::remove_dir(&directory);
    }
    result
}
pub fn update_record(
    w: &OutputWorkspace,
    plan: Option<&ProcessingPlan>,
    snapshot: Option<&JobSnapshot>,
) -> Result<(), AppError> {
    let mut record: JobRecord =
        serde_json::from_slice(&fs::read(&w.record_path).map_err(AppError::io)?)
            .map_err(|e| AppError::new(ErrorCode::OutputPermission, e.to_string()))?;
    if record.workspace != *w {
        return Err(AppError::new(
            ErrorCode::CleanupPending,
            "任务记录身份不匹配",
        ));
    }
    if let Some(p) = plan {
        record.plan = Some(p.clone());
    }
    if let Some(s) = snapshot {
        record.snapshot = Some(s.clone());
        record.cleanup_pending = s.cleanup_pending;
    }
    save_record(w, &record)
}
fn save_record(w: &OutputWorkspace, record: &JobRecord) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|e| AppError::new(ErrorCode::OutputPermission, e.to_string()))?;
    let temporary =
        Path::new(&w.record_path).with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(AppError::io)?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(AppError::io)?;
    fs::rename(&temporary, &w.record_path).map_err(AppError::io)
}
pub fn cleanup_workspace(w: &OutputWorkspace) -> Result<(), AppError> {
    let result = cleanup_owned(w);
    if let Err(e) = result {
        if let Ok(bytes) = fs::read(&w.record_path) {
            if let Ok(mut record) = serde_json::from_slice::<JobRecord>(&bytes) {
                record.cleanup_pending = true;
                let _ = save_record(w, &record);
            }
        }
        return Err(
            AppError::new(ErrorCode::CleanupPending, "临时文件尚未清理").detail(e.to_string())
        );
    }
    Ok(())
}
fn cleanup_owned(w: &OutputWorkspace) -> Result<(), AppError> {
    match fs::symlink_metadata(&w.directory) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            remove_if_exists(Path::new(&w.record_path))?;
            return Ok(());
        }
        Err(e) => return Err(AppError::io(e)),
        Ok(_) => {}
    }
    let owned = open_owned(w)?;
    for entry in fs::read_dir(&w.directory).map_err(AppError::io)? {
        let entry = entry.map_err(AppError::io)?;
        if entry.file_name() != "output.mp4" && entry.file_name() != ".owner.json" {
            return Err(AppError::new(
                ErrorCode::CleanupPending,
                "任务目录包含非本工具文件",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        for name in ["output.mp4", ".owner.json"] {
            let name = std::ffi::CString::new(name).unwrap();
            if unsafe { libc::unlinkat(owned.as_raw_fd(), name.as_ptr(), 0) } != 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(AppError::io(e));
                }
            }
        }
    }
    #[cfg(windows)]
    {
        remove_if_exists(Path::new(&w.temp_path))?;
        remove_if_exists(&Path::new(&w.directory).join(".owner.json"))?;
    }
    drop(owned);
    fs::remove_dir(&w.directory).map_err(AppError::io)?;
    remove_if_exists(Path::new(&w.record_path))
}
pub(crate) fn open_owned(w: &OutputWorkspace) -> Result<File, AppError> {
    let directory = Path::new(&w.directory);
    if !valid_job_id(&w.job_id)
        || directory.file_name().and_then(|n| n.to_str())
            != Some(format!(".video-editor-{}", w.job_id).as_str())
        || Path::new(&w.temp_path) != directory.join("output.mp4")
        || Path::new(&w.final_path).parent() != directory.parent()
    {
        return Err(AppError::new(
            ErrorCode::CleanupPending,
            "任务工作区边界无效",
        ));
    }
    let file = open_directory(directory)?;
    if directory_identity(&file)? != w.directory_identity {
        return Err(AppError::new(ErrorCode::CleanupPending, "任务目录已被替换"));
    }
    let owner: OutputWorkspace =
        serde_json::from_slice(&fs::read(directory.join(".owner.json")).map_err(AppError::io)?)
            .map_err(|e| AppError::new(ErrorCode::CleanupPending, e.to_string()))?;
    if owner != *w {
        return Err(AppError::new(
            ErrorCode::CleanupPending,
            "任务目录标记不匹配",
        ));
    }
    Ok(file)
}
pub(crate) fn open_directory(path: &Path) -> Result<File, AppError> {
    let meta = fs::symlink_metadata(path).map_err(AppError::io)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(AppError::new(
            ErrorCode::CleanupPending,
            "任务目录不是独立的普通目录",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        if meta.file_attributes() & 0x400 != 0 {
            return Err(AppError::new(ErrorCode::CleanupPending, "目录包含重解析点"));
        }
        options.custom_flags(0x02000000 | 0x00200000).share_mode(3);
    }
    options.open(path).map_err(AppError::io)
}
pub(crate) fn directory_identity(file: &File) -> Result<DirectoryIdentity, AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().map_err(AppError::io)?;
        Ok(DirectoryIdentity {
            volume_id: m.dev().to_string(),
            file_id: m.ino().to_string(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
            return Err(AppError::io(std::io::Error::last_os_error()));
        }
        Ok(DirectoryIdentity {
            volume_id: info.dwVolumeSerialNumber.to_string(),
            file_id: ((info.nFileIndexHigh as u64) << 32 | info.nFileIndexLow as u64).to_string(),
        })
    }
}
pub(crate) fn valid_job_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn path_text(path: &Path) -> Result<String, AppError> {
    path.to_str()
        .map(String::from)
        .ok_or_else(|| AppError::new(ErrorCode::OutputPermission, "目录路径无法表示"))
}
fn create_json(path: &Path, value: &impl Serialize) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| AppError::new(ErrorCode::OutputPermission, e.to_string()))?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(AppError::io)?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(AppError::io)
}
fn remove_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::io(e)),
    }
}
