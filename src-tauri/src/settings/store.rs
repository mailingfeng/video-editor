use crate::contracts::{
    decode_custom_settings, AppError, CustomSettingsSource, CustomSettingsStatus,
    CustomVariationSettings, ErrorCode,
};
use std::{
    fs::{self, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 16_384;
const RECOVERED: &str = "已恢复推荐设置，请检查后保存";

pub struct SettingsStore {
    path: Option<PathBuf>,
    status: CustomSettingsStatus,
}

impl SettingsStore {
    pub fn memory() -> Self {
        Self {
            path: None,
            status: status(
                CustomSettingsSource::Default,
                CustomVariationSettings::recommended(),
            ),
        }
    }

    pub fn load(path: PathBuf) -> Self {
        let loaded = match read_settings(&path) {
            Ok(Some(settings)) => status(CustomSettingsSource::Saved, settings),
            Ok(None) => status(
                CustomSettingsSource::Default,
                CustomVariationSettings::recommended(),
            ),
            Err(_) => status(
                CustomSettingsSource::Recovered,
                CustomVariationSettings::recommended(),
            ),
        };
        Self {
            path: Some(path),
            status: loaded,
        }
    }

    pub fn snapshot(&self) -> CustomSettingsStatus {
        self.status.clone()
    }

    pub fn save(
        &mut self,
        settings: CustomVariationSettings,
    ) -> Result<CustomSettingsStatus, AppError> {
        settings.validate()?;
        if let Some(path) = &self.path {
            persist(path, &settings)?;
        }
        self.status = status(CustomSettingsSource::Saved, settings);
        Ok(self.snapshot())
    }
}

fn status(source: CustomSettingsSource, settings: CustomVariationSettings) -> CustomSettingsStatus {
    CustomSettingsStatus {
        settings,
        recommended: CustomVariationSettings::recommended(),
        source,
        notice: (source == CustomSettingsSource::Recovered).then(|| RECOVERED.into()),
    }
}

fn unsafe_path() -> AppError {
    AppError::new(
        ErrorCode::OutputPermission,
        "无法安全读写设置，请检查应用数据目录。",
    )
}

fn is_link(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return true;
        }
    }
    false
}

fn check_target(path: &Path) -> Result<Option<Metadata>, AppError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !is_link(&meta) => Ok(Some(meta)),
        Ok(_) => Err(unsafe_path()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::io(error)),
    }
}

fn read_settings(path: &Path) -> Result<Option<CustomVariationSettings>, AppError> {
    if check_target(path)?.is_none() {
        return Ok(None);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(AppError::io)?;
    let meta = file.metadata().map_err(AppError::io)?;
    if !meta.is_file() || is_link(&meta) {
        return Err(unsafe_path());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(AppError::io)?;
    if bytes.len() as u64 > LIMIT {
        return Err(unsafe_path());
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| crate::contracts::invalid_settings())?;
    decode_custom_settings(value).map(Some)
}

struct PendingFile(PathBuf);
impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn persist(path: &Path, settings: &CustomVariationSettings) -> Result<(), AppError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(unsafe_path)?;
    let meta = fs::symlink_metadata(parent).map_err(AppError::io)?;
    if !meta.is_dir() || is_link(&meta) {
        return Err(unsafe_path());
    }
    check_target(path)?;
    let temporary = parent.join(format!(".custom-settings-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(AppError::io)?;
    let pending = PendingFile(temporary);
    let bytes = serde_json::to_vec(settings).map_err(|_| crate::contracts::invalid_settings())?;
    file.write_all(&bytes).map_err(AppError::io)?;
    file.sync_all().map_err(AppError::io)?;
    drop(file);
    check_target(path)?;
    atomic_replace(&pending.0, path).map_err(AppError::io)?;
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let src: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            src.as_ptr(),
            dst.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_atomic_replace_cleans_only_its_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("occupied");
        fs::create_dir(&destination).unwrap();
        let unrelated = root.path().join("previous.json");
        fs::write(&unrelated, b"previous settings").unwrap();
        let temporary = root.path().join(".custom-settings-test.tmp");
        fs::write(&temporary, b"new settings").unwrap();
        {
            let pending = PendingFile(temporary.clone());
            assert!(atomic_replace(&pending.0, &destination).is_err());
        }
        assert!(!temporary.exists());
        assert_eq!(fs::read(unrelated).unwrap(), b"previous settings");
        assert!(destination.is_dir());
    }
}
