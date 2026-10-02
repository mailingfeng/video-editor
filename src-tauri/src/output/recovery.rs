use super::workspace::{cleanup_workspace, valid_job_id, JobRecord};
use crate::contracts::*;
use std::{fs, path::Path};
pub struct RecoverySummary {
    pub cleaned: Vec<String>,
    pub pending: Vec<String>,
    pub errors: Vec<AppError>,
}
pub fn recover_incomplete_jobs(record_dir: &Path) -> Result<RecoverySummary, AppError> {
    let mut summary = RecoverySummary {
        cleaned: vec![],
        pending: vec![],
        errors: vec![],
    };
    if !record_dir.exists() {
        return Ok(summary);
    }
    let root = fs::canonicalize(record_dir).map_err(AppError::io)?;
    for entry in fs::read_dir(&root).map_err(AppError::io)? {
        let entry = entry.map_err(AppError::io)?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json")
            || entry.file_type().map_err(AppError::io)?.is_symlink()
        {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_slice::<JobRecord>(&bytes) else {
            continue;
        };
        let w = record.workspace;
        if record.schema_version != 1
            || !valid_job_id(&w.job_id)
            || path.file_stem().and_then(|s| s.to_str()) != Some(&w.job_id)
            || Path::new(&w.record_path) != path
        {
            continue;
        }
        match cleanup_workspace(&w) {
            Ok(()) => summary.cleaned.push(w.job_id),
            Err(e) => {
                summary.pending.push(w.job_id);
                summary.errors.push(e);
            }
        }
    }
    Ok(summary)
}
