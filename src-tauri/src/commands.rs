use crate::contracts::*;
use crate::jobs::log::LogExcerpt;
use crate::jobs::service::JobService;
use crate::license::{LicenseService, LicenseStatus};
use tauri::State;

pub struct DesktopState {
    pub jobs: JobService,
    pub tool_error: Option<AppError>,
    pub license: LicenseService,
}
#[tauri::command]
pub async fn get_license_status(state: State<'_, DesktopState>) -> Result<LicenseStatus, AppError> {
    Ok(state.license.status().await)
}
impl DesktopState {
    fn require_tools(&self) -> Result<(), AppError> {
        self.tool_error
            .as_ref()
            .map_or(Ok(()), |e| Err(e.for_user()))
    }
}
#[tauri::command]
pub async fn probe_input(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<MediaInfo, AppError> {
    state.require_tools()?;
    state.jobs.inspect_input(path).await
}
#[tauri::command]
pub async fn cancel_probe(state: State<'_, DesktopState>) -> Result<(), AppError> {
    state.jobs.cancel_inspection().await
}
#[tauri::command]
pub async fn get_queue_snapshot(state: State<'_, DesktopState>) -> Result<QueueSnapshot, AppError> {
    Ok(state.jobs.queue_snapshot().await)
}
#[tauri::command]
pub async fn import_folder(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<QueueSnapshot, AppError> {
    state.jobs.import_folder(path).await
}
#[tauri::command]
pub async fn import_paths(
    state: State<'_, DesktopState>,
    paths: Vec<String>,
) -> Result<QueueSnapshot, AppError> {
    state.jobs.import_paths(paths).await
}
#[tauri::command]
pub async fn remove_item(
    state: State<'_, DesktopState>,
    item_id: String,
) -> Result<QueueSnapshot, AppError> {
    state.jobs.remove_item(&item_id).await
}
#[tauri::command]
pub async fn start_batch(
    state: State<'_, DesktopState>,
    settings: serde_json::Value,
) -> Result<QueueSnapshot, AppError> {
    state.license.require_active().await?;
    state.require_tools()?;
    state
        .jobs
        .start_batch(decode_batch_settings(settings)?)
        .await
}
#[tauri::command]
pub async fn cancel_item(
    state: State<'_, DesktopState>,
    item_id: String,
) -> Result<QueueSnapshot, AppError> {
    state.jobs.cancel_item(&item_id).await
}
#[tauri::command]
pub fn list_presets() -> Vec<PresetSummary> {
    crate::presets::basic::list_presets()
}
#[tauri::command]
pub async fn start_job(
    state: State<'_, DesktopState>,
    request: serde_json::Value,
) -> Result<String, AppError> {
    state.license.require_active().await?;
    state.require_tools()?;
    state.jobs.start_job(decode_start_request(request)?).await
}
#[tauri::command]
pub async fn get_job_snapshot(
    state: State<'_, DesktopState>,
    job_id: String,
) -> Result<JobSnapshot, AppError> {
    state.jobs.snapshot(&job_id).await
}
#[tauri::command]
pub async fn get_current_job_snapshot(
    state: State<'_, DesktopState>,
) -> Result<Option<JobSnapshot>, AppError> {
    state.jobs.current_snapshot().await
}
#[tauri::command]
pub async fn cancel_job(
    state: State<'_, DesktopState>,
    job_id: String,
) -> Result<JobSnapshot, AppError> {
    state.jobs.cancel_job(&job_id).await
}
#[tauri::command]
pub async fn get_job_log(
    state: State<'_, DesktopState>,
    job_id: String,
) -> Result<LogExcerpt, AppError> {
    state.jobs.read_log(&job_id).await
}
#[tauri::command]
pub async fn reveal_output(state: State<'_, DesktopState>, job_id: String) -> Result<(), AppError> {
    let snapshot = state.jobs.snapshot(&job_id).await?;
    if snapshot.state != JobState::Succeeded {
        return Err(AppError::new(
            ErrorCode::ValidationFailed,
            "只有已验证并保存的结果可以定位",
        ));
    }
    let path = snapshot
        .output_path
        .ok_or_else(|| AppError::new(ErrorCode::OutputPermission, "结果路径缺失"))?;
    if !std::path::Path::new(&path).is_file() {
        return Err(AppError::new(
            ErrorCode::OutputPermission,
            "结果文件已移动或删除",
        ));
    }
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| {
        AppError::new(ErrorCode::OutputPermission, "无法定位结果文件").detail(e.to_string())
    })
}

#[tauri::command]
pub async fn get_custom_settings(
    state: State<'_, DesktopState>,
) -> Result<CustomSettingsStatus, AppError> {
    Ok(state.jobs.custom_settings().await)
}
#[tauri::command]
pub async fn save_custom_settings(
    state: State<'_, DesktopState>,
    settings: serde_json::Value,
) -> Result<CustomSettingsStatus, AppError> {
    state
        .jobs
        .save_custom_settings(decode_custom_settings(settings)?)
        .await
}
