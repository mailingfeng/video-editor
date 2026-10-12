use super::{process::RunSpec, tools::Tool};
use crate::{contracts::*, output::workspace::open_owned};
use std::{fs::File, io::Write, path::Path};

pub(crate) const SCRIPT_NAMES: [&str; 2] = [".v.dat", ".a.dat"];

// The directory handle pins the verified workspace. Scripts never live in a
// shared temporary directory, and restart recovery recognizes only these names.
#[derive(Default)]
pub(super) struct FilterScripts {
    _directory: Option<File>,
    root: String,
    names: Vec<&'static str>,
}
impl FilterScripts {
    pub fn prepare(spec: &mut RunSpec) -> Result<Self, AppError> {
        let slots: Vec<_> = spec
            .args
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| match arg.as_str() {
                "-vf" | "-filter:v" => Some((index, 0, "-/filter:v")),
                "-af" | "-filter:a" => Some((index, 1, "-/filter:a")),
                _ => None,
            })
            .collect();
        if !matches!(spec.tool, Tool::Ffmpeg) || slots.is_empty() {
            return Ok(Self::default());
        }
        let output = spec.args.last().ok_or_else(invalid_workspace)?;
        let root = Path::new(output).parent().ok_or_else(invalid_workspace)?;
        let workspace: OutputWorkspace =
            serde_json::from_slice(&std::fs::read(root.join(".owner.json")).map_err(AppError::io)?)
                .map_err(|_| invalid_workspace())?;
        if output != &workspace.temp_path {
            return Err(invalid_workspace());
        }
        let mut scripts = Self {
            _directory: Some(open_owned(&workspace)?),
            root: workspace.directory,
            names: Vec::new(),
        };
        for (index, kind, flag) in slots {
            let filter = spec.args.get(index + 1).ok_or_else(invalid_workspace)?;
            let name = SCRIPT_NAMES[kind];
            let mut file = scripts.create(name)?;
            scripts.names.push(name);
            restrict_access(&file).map_err(AppError::io)?;
            file.write_all(filter.as_bytes()).map_err(AppError::io)?;
            spec.args[index] = flag.into();
            spec.args[index + 1] = Path::new(&scripts.root)
                .join(name)
                .to_str()
                .ok_or_else(invalid_workspace)?
                .into();
        }
        Ok(scripts)
    }
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
    fn create(&self, name: &str) -> Result<File, AppError> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = std::ffi::CString::new(name).unwrap();
            let fd = unsafe {
                libc::openat(
                    self._directory.as_ref().unwrap().as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(AppError::io(std::io::Error::last_os_error()));
            }
            Ok(unsafe { File::from_raw_fd(fd) })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // GENERIC_WRITE | WRITE_DAC; set the protected DACL before writing.
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .access_mode(0x40000000 | 0x00040000)
                .open(Path::new(&self.root).join(name))
                .map_err(AppError::io)
        }
    }
    pub fn clear(&mut self) -> Result<(), AppError> {
        for name in &self.names {
            #[cfg(unix)]
            let result = {
                use std::os::fd::AsRawFd;
                let name = std::ffi::CString::new(*name).unwrap();
                if unsafe {
                    libc::unlinkat(
                        self._directory.as_ref().unwrap().as_raw_fd(),
                        name.as_ptr(),
                        0,
                    )
                } == 0
                {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            };
            #[cfg(windows)]
            let result = std::fs::remove_file(Path::new(&self.root).join(name));
            if let Err(error) = result {
                if error.kind() != std::io::ErrorKind::NotFound {
                    return Err(AppError::io(error));
                }
            }
        }
        self.names.clear();
        Ok(())
    }
}
impl Drop for FilterScripts {
    fn drop(&mut self) {
        let _ = self.clear();
    }
}
fn invalid_workspace() -> AppError {
    AppError::new(ErrorCode::OutputPermission, "任务工作区无效")
}
#[cfg(unix)]
fn restrict_access(_: &File) -> std::io::Result<()> {
    Ok(())
}
#[cfg(windows)]
fn restrict_access(file: &File) -> std::io::Result<()> {
    use std::{os::windows::io::AsRawHandle, ptr};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo,
                SE_FILE_OBJECT,
            },
            GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION,
        },
    };
    // Owner Rights grants the file owner access without inheriting broad ACLs.
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)\0".encode_utf16().collect();
    let mut descriptor = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl = ptr::null_mut();
    let result =
        if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted) }
            == 0
        {
            Err(std::io::Error::last_os_error())
        } else if present == 0 || acl.is_null() {
            Err(std::io::Error::other("缺失文件访问权限"))
        } else {
            let error = unsafe {
                SetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    acl,
                    ptr::null_mut(),
                )
            };
            if error == 0 {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(error as i32))
            }
        };
    unsafe {
        LocalFree(descriptor);
    }
    result
}
