use std::fs;
use tempfile::tempdir;
use video_editor::{contracts::*, settings::SettingsStore};

#[test]
fn missing_file_uses_recommendation() {
    let root = tempdir().unwrap();
    let path = root.path().join("custom-variation-settings.json");
    let store = SettingsStore::load(path.clone());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.source, CustomSettingsSource::Default);
    assert_eq!(snapshot.settings.foreground_scale, 2);
    assert!(snapshot.notice.is_none());
    assert!(!path.exists());
}

#[test]
fn save_survives_reload() {
    let root = tempdir().unwrap();
    let path = root.path().join("custom-variation-settings.json");
    let mut store = SettingsStore::load(path.clone());
    let mut settings = CustomVariationSettings::recommended();
    settings.speed = 9;
    let saved = store.save(settings.clone()).unwrap();
    assert_eq!(saved.source, CustomSettingsSource::Saved);
    assert_eq!(
        SettingsStore::load(path.clone()).snapshot().settings,
        settings
    );
    let bytes = fs::read(&path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 15);
    assert_eq!(value["speed"], 9);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    settings.pitch = 3;
    store.save(settings.clone()).unwrap();
    assert_eq!(SettingsStore::load(path).snapshot().settings, settings);
}

#[test]
fn unsafe_or_future_settings_recover_without_rewrite() {
    let root = tempdir().unwrap();
    let path = root.path().join("custom-variation-settings.json");
    let mut future = serde_json::to_value(CustomVariationSettings::recommended()).unwrap();
    future["schemaVersion"] = 2.into();
    for bytes in [
        vec![b' '; 16385],
        b"broken".to_vec(),
        serde_json::to_vec(&future).unwrap(),
    ] {
        fs::write(&path, &bytes).unwrap();
        let snapshot = SettingsStore::load(path.clone()).snapshot();
        assert_eq!(snapshot.source, CustomSettingsSource::Recovered);
        assert!(snapshot.notice.is_some());
        assert_eq!(snapshot.settings.foreground_scale, 2);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let mut store = SettingsStore::load(path.clone());
    assert_eq!(store.snapshot().source, CustomSettingsSource::Recovered);
    assert!(store.save(CustomVariationSettings::recommended()).is_err());
    assert!(path.is_dir());
}

#[cfg(unix)]
#[test]
fn symbolic_link_is_never_read_or_replaced() {
    use std::os::unix::fs::symlink;
    let root = tempdir().unwrap();
    let target = root.path().join("external.json");
    let original = serde_json::to_vec(&CustomVariationSettings::recommended()).unwrap();
    fs::write(&target, &original).unwrap();
    let path = root.path().join("custom-variation-settings.json");
    symlink(&target, &path).unwrap();
    let mut store = SettingsStore::load(path.clone());
    assert_eq!(store.snapshot().source, CustomSettingsSource::Recovered);
    assert!(store.save(CustomVariationSettings::recommended()).is_err());
    assert_eq!(fs::read(&target).unwrap(), original);
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn failed_replace_keeps_previous() {
    let root = tempdir().unwrap();
    let path = root.path().join("custom-variation-settings.json");
    let mut store = SettingsStore::load(path.clone());
    let old = store.save(CustomVariationSettings::recommended()).unwrap();
    let original = fs::read(&path).unwrap();
    let backup = root.path().join("previous.json");
    fs::rename(&path, &backup).unwrap();
    fs::create_dir(&path).unwrap();
    let mut changed = old.settings.clone();
    changed.speed = 9;
    assert!(store.save(changed).is_err());
    assert_eq!(store.snapshot(), old);
    assert_eq!(fs::read(backup).unwrap(), original);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn invalid_save_does_not_create_files_or_update_memory() {
    let root = tempdir().unwrap();
    let path = root.path().join("custom-variation-settings.json");
    let mut store = SettingsStore::load(path);
    let old = store.snapshot();
    let mut invalid = old.settings.clone();
    invalid.encoding = 10;
    assert_eq!(
        store.save(invalid).unwrap_err().code,
        ErrorCode::InvalidSettings
    );
    assert_eq!(store.snapshot(), old);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    let mut memory = SettingsStore::memory();
    let mut settings = CustomVariationSettings::recommended();
    settings.speed = 4;
    assert_eq!(memory.save(settings).unwrap().settings.speed, 4);
}

#[cfg(windows)]
#[test]
fn junction_target_is_not_replaced() {
    let root = tempdir().unwrap();
    let target = root.path().join("external");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"untouched").unwrap();
    let path = root.path().join("custom-variation-settings.json");
    let result = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&path)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "junction setup failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut store = SettingsStore::load(path.clone());
    assert_eq!(store.snapshot().source, CustomSettingsSource::Recovered);
    assert!(store.save(CustomVariationSettings::recommended()).is_err());
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"untouched");
    fs::remove_dir(path).unwrap();
}
