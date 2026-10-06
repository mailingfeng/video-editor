use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{Emitter, Manager};
use video_editor::{
    commands::{self, DesktopState},
    jobs::service::JobService,
    native::{
        process::NativeRunner,
        tools::{current_target, expected_tool_paths, resolve_tools},
    },
    output::recovery::recover_incomplete_jobs,
};

fn main() {
    let closing = Arc::new(AtomicBool::new(false));
    let ready_to_exit = Arc::new(AtomicBool::new(false));
    let close_flag = closing.clone();
    let exit_flag = ready_to_exit.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .setup(|app| {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
            let exe = std::env::current_exe()?;
            let bundled = if cfg!(debug_assertions) {
                None
            } else {
                Some(exe.as_path())
            };
            let tool_error = resolve_tools(manifest, current_target(), bundled).err();
            let tools =
                expected_tool_paths(manifest, current_target(), bundled).map_err(|e| e.message)?;
            let records = app.path().app_data_dir()?.join("job-records");
            std::fs::create_dir_all(&records)?;
            match recover_incomplete_jobs(&records) {
                Ok(recovery) if !recovery.pending.is_empty() || !recovery.errors.is_empty() => {
                    eprintln!(
                        "startup cleanup pending: {:?}; errors: {:?}",
                        recovery.pending, recovery.errors
                    );
                }
                Err(error) => eprintln!("startup cleanup: {error:?}"),
                _ => {}
            }
            let handle = app.handle().clone();
            let jobs = JobService::new(
                Arc::new(NativeRunner::new(tools)),
                records,
                Arc::new(move |snapshot| {
                    let _ = handle.emit("job_snapshot", snapshot);
                }),
            );
            app.manage(DesktopState { jobs, tool_error });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_input,
            commands::cancel_probe,
            commands::list_presets,
            commands::start_job,
            commands::get_job_snapshot,
            commands::get_current_job_snapshot,
            commands::cancel_job,
            commands::get_job_log,
            commands::reveal_output
        ])
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !exit_flag.load(Ordering::Acquire) {
                    api.prevent_close();
                    begin_shutdown(window.app_handle(), &close_flag, &exit_flag);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("desktop runtime failed");
    app.run(move |handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !ready_to_exit.load(Ordering::Acquire) {
                api.prevent_exit();
                begin_shutdown(handle, &closing, &ready_to_exit);
            }
        }
    });
}
fn begin_shutdown(handle: &tauri::AppHandle, closing: &Arc<AtomicBool>, ready: &Arc<AtomicBool>) {
    if closing.swap(true, Ordering::AcqRel) {
        return;
    }
    let jobs = handle.state::<DesktopState>().jobs.clone();
    let handle = handle.clone();
    let ready = ready.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = jobs.shutdown().await {
            eprintln!("shutdown: {error:?}");
        }
        ready.store(true, Ordering::Release);
        handle.exit(0);
    });
}
