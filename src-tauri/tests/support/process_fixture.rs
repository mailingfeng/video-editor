use std::{path::PathBuf, process::Command};
use video_editor::native::tools::ToolPaths;
pub struct ProcessFixture {
    dir: tempfile::TempDir,
    exe: PathBuf,
    pub mode: String,
}
impl ProcessFixture {
    pub fn new(mode: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("fixture.rs");
        std::fs::write(&source, include_str!("process_fixture_main.rs")).unwrap();
        let exe = dir.path().join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        assert!(Command::new("rustc")
            .arg(source)
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap()
            .success());
        Self {
            dir,
            exe,
            mode: mode.into(),
        }
    }
    pub fn paths(&self) -> ToolPaths {
        ToolPaths {
            ffmpeg: self.exe.clone(),
            ffprobe: self.exe.clone(),
        }
    }
    pub fn args(&self) -> Vec<String> {
        vec![
            self.mode.clone(),
            self.dir.path().join("pid").to_str().unwrap().into(),
            self.dir.path().join("q").to_str().unwrap().into(),
        ]
    }
    pub fn q_seen(&self) -> bool {
        self.dir.path().join("q").exists()
    }
    pub fn is_alive(&self) -> bool {
        let Ok(s) = std::fs::read_to_string(self.dir.path().join("pid")) else {
            return false;
        };
        let Ok(pid) = s.parse::<i32>() else {
            return false;
        };
        #[cfg(unix)]
        {
            unsafe { libc::kill(pid, 0) == 0 }
        }
        #[cfg(windows)]
        {
            Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .unwrap()
                .stdout
                .windows(s.len())
                .any(|x| x == s.as_bytes())
        }
    }
}
