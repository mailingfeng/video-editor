use video_editor::media::inputs::{collect_folder, collect_paths};

#[test]
fn folder_collects_current_level_mp4_and_preserves_unchecked_candidates() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("中文 a.mp4"), b"not probed at import").unwrap();
    std::fs::write(dir.path().join("b.MP4"), b"").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/hidden.mp4"), b"").unwrap();
    std::fs::create_dir(dir.path().join("directory.mp4")).unwrap();
    let items = collect_folder(dir.path()).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|i| i.input_path.ends_with("中文 a.mp4")));
    assert!(items.iter().any(|i| i.input_path.ends_with("b.MP4")));
    assert!(collect_folder(&dir.path().join("sub/missing")).is_err());
    std::fs::create_dir(dir.path().join("empty")).unwrap();
    assert!(collect_folder(&dir.path().join("empty"))
        .unwrap()
        .is_empty());
}

#[test]
fn keys_deduplicate_aliases_and_keep_same_names_in_different_folders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("a")).unwrap();
    std::fs::create_dir(dir.path().join("b")).unwrap();
    for folder in ["a", "b"] {
        std::fs::write(dir.path().join(folder).join("video.mp4"), b"").unwrap();
    }
    let paths = vec![
        dir.path().join("a/video.mp4"),
        dir.path().join("a/../a/video.mp4"),
        dir.path().join("b/video.mp4"),
    ];
    let items = collect_paths(&paths).unwrap();
    assert_eq!(items.len(), 2);
    assert_ne!(items[0].key, items[1].key);
    assert_eq!(
        collect_paths(&[dir.path().join("later-unreadable.mp4")])
            .unwrap()
            .len(),
        1
    );
}

#[cfg(windows)]
#[test]
fn windows_keys_deduplicate_case_and_separator_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("中文 Video.MP4");
    std::fs::write(&path, b"").unwrap();
    let alias = path.to_string_lossy().to_uppercase().replace('\\', "/");
    assert_eq!(collect_paths(&[path, alias.into()]).unwrap().len(), 1);
}
