use video_editor::{
    contracts::ErrorCode,
    output::{
        publish::publish_no_replace,
        recovery::recover_incomplete_jobs,
        workspace::{cleanup_workspace, prepare_workspace},
    },
};
fn setup() -> (tempfile::TempDir, video_editor::contracts::OutputWorkspace) {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("原片.mp4");
    std::fs::write(&source, b"source").unwrap();
    let w = prepare_workspace(d.path(), &source, "test", &d.path().join("records")).unwrap();
    std::fs::write(&w.temp_path, b"validated media").unwrap();
    (d, w)
}
#[test]
fn existing_destination_is_never_overwritten() {
    let (_d, w) = setup();
    std::fs::write(&w.final_path, b"sentinel").unwrap();
    assert_eq!(
        publish_no_replace(&w).unwrap_err().code,
        ErrorCode::OutputConflict
    );
    assert_eq!(std::fs::read(&w.final_path).unwrap(), b"sentinel");
}
#[test]
fn unrelated_files_survive_recovery() {
    let (d, w) = setup();
    let other = d.path().join("important.mp4");
    std::fs::write(&other, b"keep").unwrap();
    let result = recover_incomplete_jobs(&d.path().join("records")).unwrap();
    assert_eq!(result.cleaned, vec!["test"]);
    assert_eq!(std::fs::read(&other).unwrap(), b"keep");
    assert!(!std::path::Path::new(&w.directory).exists());
}
#[cfg(unix)]
#[test]
fn symlink_workspace_is_not_followed() {
    let (d, w) = setup();
    let moved = d.path().join("moved");
    std::fs::rename(&w.directory, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &w.directory).unwrap();
    assert!(publish_no_replace(&w).is_err());
    assert!(cleanup_workspace(&w).is_err());
    assert!(moved.join("output.mp4").exists());
}
#[test]
fn cleanup_failure_keeps_pending_marker() {
    let (_d, w) = setup();
    let foreign = std::path::Path::new(&w.directory).join("foreign.txt");
    std::fs::write(&foreign, b"keep").unwrap();
    assert_eq!(
        cleanup_workspace(&w).unwrap_err().code,
        ErrorCode::CleanupPending
    );
    assert!(std::path::Path::new(&w.record_path).exists());
    assert!(foreign.exists());
}
#[test]
fn directory_swap_is_detected() {
    let (d, w) = setup();
    std::fs::rename(&w.directory, d.path().join("moved")).unwrap();
    std::fs::create_dir(&w.directory).unwrap();
    let foreign = std::path::Path::new(&w.directory).join("output.mp4");
    std::fs::write(&foreign, b"foreign").unwrap();
    assert!(publish_no_replace(&w).is_err());
    assert!(cleanup_workspace(&w).is_err());
    assert_eq!(std::fs::read(foreign).unwrap(), b"foreign");
}
#[test]
fn publication_moves_complete_file_once() {
    let (_d, w) = setup();
    let result = publish_no_replace(&w).unwrap();
    assert_eq!(std::fs::read(result).unwrap(), b"validated media");
    assert!(!std::path::Path::new(&w.temp_path).exists());
    cleanup_workspace(&w).unwrap();
    assert!(std::path::Path::new(&w.final_path).exists());
}
