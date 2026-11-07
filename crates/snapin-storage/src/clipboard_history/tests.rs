use super::*;

#[test]
fn test_validated_image_path_rejects_traversal_and_nested_paths() {
    let dir = Path::new("/tmp/snapin-history");
    for invalid in [
        "../secret.png",
        "nested/image.png",
        "/tmp/secret.png",
        "image.jpg",
    ] {
        assert!(
            validated_image_path(dir, invalid).is_err(),
            "非法历史图像路径必须被拒绝: {invalid}"
        );
    }
    assert_eq!(
        validated_image_path(dir, "safe.png").expect("合法文件名应通过"),
        dir.join("safe.png")
    );
}

#[test]
fn test_prune_removes_entry_with_traversing_image_path() {
    let dir = tempfile::tempdir().expect("应可创建测试目录");
    let mut entries = vec![ClipboardEntry {
        id: 1,
        kind: ClipboardEntryKind::Image,
        text: None,
        image_file: Some("../outside.png".to_string()),
        created_at: now_secs(),
    }];

    prune_expired(&mut entries, dir.path());

    assert!(entries.is_empty(), "路径穿越条目必须在读取边界被丢弃");
}

#[test]
fn test_save_clipboard_image_creates_missing_directory() {
    let root = tempfile::tempdir().expect("应可创建测试目录");
    let history_dir = root.path().join("missing").join("clipboard-history");
    let image = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));

    let (_, file_name) = save_clipboard_image(&history_dir, &image).expect("首条图像应保存成功");
    let path = validated_image_path(&history_dir, &file_name).expect("生成路径必须合法");

    assert!(path.is_file(), "图像应写入新建的历史目录");
    assert_eq!(
        image::open(path).expect("保存的图像应可解码").to_rgba8(),
        image
    );
}

#[test]
fn test_load_rejects_oversized_history_index() {
    let dir = tempfile::tempdir().expect("应可创建测试目录");
    std::fs::write(
        dir.path().join("index.json"),
        vec![b' '; MAX_CLIPBOARD_INDEX_BYTES.saturating_add(1)],
    )
    .expect("应可写入超限索引");

    let error = load_and_prune(dir.path().to_path_buf()).expect_err("超限索引必须被拒绝");
    assert!(matches!(error, StorageError::Io { .. }));
}

#[test]
fn test_load_rejects_oversized_clipboard_image_dimensions() {
    let dir = tempfile::tempdir().expect("应可创建测试目录");
    let image = RgbaImage::from_pixel(1, MAX_CLIPBOARD_IMAGE_SIDE + 1, image::Rgba([0, 0, 0, 255]));
    let (id, file_name) = save_clipboard_image(dir.path(), &image).expect("图像应可保存");
    let entry = ClipboardEntry {
        id,
        kind: ClipboardEntryKind::Image,
        text: None,
        image_file: Some(file_name),
        created_at: now_secs(),
    };

    assert!(load_clipboard_image_from(dir.path(), &entry).is_err());
}

#[test]
fn test_load_rejects_oversized_clipboard_image_file() {
    let dir = tempfile::tempdir().expect("应可创建测试目录");
    let path = dir.path().join("huge.png");
    let file = std::fs::File::create(&path).expect("应可创建测试文件");
    file.set_len((MAX_CLIPBOARD_IMAGE_FILE_BYTES as u64).saturating_add(1))
        .expect("应可扩展测试文件");
    let entry = ClipboardEntry {
        id: 1,
        kind: ClipboardEntryKind::Image,
        text: None,
        image_file: Some("huge.png".to_string()),
        created_at: now_secs(),
    };

    assert!(load_clipboard_image_from(dir.path(), &entry).is_err());
}

#[cfg(unix)]
#[test]
fn test_load_rejects_clipboard_image_symlink() {
    let dir = tempfile::tempdir().expect("应可创建测试目录");
    let outside = dir.path().join("outside.png");
    std::fs::write(&outside, b"not a real image").expect("应可写入外部文件");
    let link = dir.path().join("linked.png");
    std::os::unix::fs::symlink(&outside, &link).expect("应可创建符号链接");
    let entry = ClipboardEntry {
        id: 1,
        kind: ClipboardEntryKind::Image,
        text: None,
        image_file: Some("linked.png".to_string()),
        created_at: now_secs(),
    };

    assert!(load_clipboard_image_from(dir.path(), &entry).is_err());
}

#[test]
fn test_history_lock_rejects_concurrent_owner_after_timeout() {
    let dir = tempfile::tempdir().expect("应可创建测试目录");
    let path = dir.path().join("clipboard-history.lock");
    let _first =
        HistoryLock::acquire_at(&path, Duration::from_millis(50), "首次").expect("首次加锁应成功");

    let error = HistoryLock::acquire_at(&path, Duration::from_millis(20), "并发")
        .expect_err("第二次加锁应超时");
    assert!(matches!(error, StorageError::LockTimeout { .. }));
}
