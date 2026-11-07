use super::*;

#[test]
fn test_save_image_with_config_jpeg_and_png() {
    let unique_id = snapin_foundation::AppPaths::generate_unique_id();
    let temp_dir = std::env::temp_dir().join(format!("snapin_test_{unique_id}"));
    let mut cfg = OutputConfig {
        quick_save_dir: temp_dir.to_string_lossy().to_string(),
        file_name_template: "test_{YYYY}".to_string(),
        default_format: ImageFormat::Jpeg,
        jpeg_quality: 85,
        notify_on_save: false,
        ..OutputConfig::default()
    };

    // 构造测试 RGBA 图像 (10x10)
    let img = RgbaImage::from_pixel(10, 10, image::Rgba([120, 200, 255, 255]));

    // 1. 测试 JPEG 格式保存（验证无 clone chunk 编码路径）
    let jpeg_path = save_image_with_config(&img, &cfg).expect("保存 JPEG 图像成功");
    assert!(jpeg_path.exists());
    assert_eq!(jpeg_path.extension().and_then(|s| s.to_str()), Some("jpg"));
    let jpeg_bytes = std::fs::read(&jpeg_path).expect("读取生成的 JPEG 文件成功");
    assert!(!jpeg_bytes.is_empty());
    let _ = std::fs::remove_file(&jpeg_path);

    // 2. 测试 PNG 格式保存
    cfg.default_format = ImageFormat::Png;
    let png_path = save_image_with_config(&img, &cfg).expect("保存 PNG 图像成功");
    assert!(png_path.exists());
    assert_eq!(png_path.extension().and_then(|s| s.to_str()), Some("png"));
    let _ = std::fs::remove_file(&png_path);

    // 3. 测试 WebP 格式保存
    cfg.default_format = ImageFormat::Webp;
    let webp_path = save_image_with_config(&img, &cfg).expect("保存 WebP 图像成功");
    assert!(webp_path.exists());
    assert_eq!(webp_path.extension().and_then(|s| s.to_str()), Some("webp"));
    let _ = std::fs::remove_file(&webp_path);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_save_image_to_path_supports_formats_and_guards_overwrite() {
    let unique_id = snapin_foundation::AppPaths::generate_unique_id();
    let temp_dir = std::env::temp_dir().join(format!("snapin_save_as_{unique_id}"));
    std::fs::create_dir_all(&temp_dir).expect("创建测试目录失败");
    let cfg = OutputConfig {
        default_format: ImageFormat::Png,
        ..OutputConfig::default()
    };
    let img = RgbaImage::from_pixel(8, 6, image::Rgba([30, 120, 220, 255]));

    let png_path = temp_dir.join("chosen.png");
    let saved = save_image_to_path(&img, &png_path, &cfg).expect("保存指定 PNG 路径失败");
    assert_eq!(saved, png_path);
    assert!(saved.exists());

    let jpeg_path = temp_dir.join("chosen.jpg");
    let saved = save_image_to_path(&img, &jpeg_path, &cfg).expect("保存指定 JPEG 路径失败");
    assert_eq!(saved, jpeg_path);
    assert!(saved.exists());

    let no_extension = temp_dir.join("extensionless");
    let saved = save_image_to_path(&img, &no_extension, &cfg).expect("保存无扩展名路径失败");
    assert_eq!(saved, temp_dir.join("extensionless.png"));
    assert!(saved.exists());

    // 不支持的扩展名必须报错，且不得改写同名的默认格式文件
    let unsupported_fallback = temp_dir.join("unsupported.png");
    std::fs::write(&unsupported_fallback, b"keep").expect("写入占位文件失败");
    let unsupported = temp_dir.join("unsupported.txt");
    let error =
        save_image_to_path(&img, &unsupported, &cfg).expect_err("不支持的扩展名必须拒绝写入");
    assert!(error.to_string().contains("仅支持"));
    assert_eq!(
        std::fs::read(&unsupported_fallback).expect("读取占位文件失败"),
        b"keep"
    );

    // 无扩展名补齐后若目标已存在，同样必须拒绝而不是静默覆盖
    let collision_base = temp_dir.join("collision");
    let collision_target = temp_dir.join("collision.png");
    std::fs::write(&collision_target, b"keep").expect("写入冲突占位文件失败");
    let error = save_image_to_path(&img, &collision_base, &cfg)
        .expect_err("补齐扩展名后的目标已存在必须拒绝");
    assert!(error.to_string().contains("已存在"));
    assert_eq!(
        std::fs::read(&collision_target).expect("读取冲突文件失败"),
        b"keep"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_suggest_save_path_auto_increment_when_exists() {
    let unique_id = snapin_foundation::AppPaths::generate_unique_id();
    let temp_dir = std::env::temp_dir().join(format!("snapin_inc_{unique_id}"));
    let cfg = OutputConfig {
        quick_save_dir: temp_dir.to_string_lossy().to_string(),
        file_name_template: "snapin_fixed".to_string(),
        default_format: ImageFormat::Png,
        ..OutputConfig::default()
    };

    std::fs::create_dir_all(&temp_dir).expect("创建测试目录失败");

    // 第一次生成：文件不存在，返回固定原名
    let path1 = suggest_save_path(&cfg).path();
    assert_eq!(path1, temp_dir.join("snapin_fixed.png"));
    std::fs::write(&path1, b"first").expect("创建占位文件1成功");

    // 第二次生成：原文件已存在，自增生成 _1 后缀
    let path2 = suggest_save_path(&cfg).path();
    assert_eq!(path2, temp_dir.join("snapin_fixed_1.png"));
    std::fs::write(&path2, b"second").expect("创建占位文件2成功");

    // 第三次生成：_1 也存在，自增生成 _2 后缀
    let path3 = suggest_save_path(&cfg).path();
    assert_eq!(path3, temp_dir.join("snapin_fixed_2.png"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

mod template_extension_tests {
    use super::*;
    use image::Rgba;
    use snapin_config::config::ImageFormat;
    use tempfile::tempdir;

    /// 历史模板带 `.png` 时不得再追加一次扩展名。
    #[test]
    fn test_template_with_extension_does_not_duplicate_suffix() {
        let dir = tempdir().expect("应可创建临时目录");
        let cfg = OutputConfig {
            quick_save_dir: dir.path().to_string_lossy().to_string(),
            file_name_template: "shot.png".to_string(),
            default_format: ImageFormat::Png,
            ..OutputConfig::default()
        };
        let path = suggest_save_path(&cfg).path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("shot.png")
        );

        let img = RgbaImage::from_pixel(1, 1, Rgba([1, 2, 3, 255]));
        let saved = save_image_with_config(&img, &cfg).expect("应可保存图像");
        assert_eq!(saved.extension().and_then(|ext| ext.to_str()), Some("png"));
    }

    /// 模板中的路径分隔符不得把文件写出用户选择的保存目录。
    #[test]
    fn test_template_path_traversal_stays_inside_save_dir() {
        let dir = tempdir().expect("应可创建临时目录");
        let cfg = OutputConfig {
            quick_save_dir: dir.path().to_string_lossy().to_string(),
            file_name_template: "../../escape_{YYYY}".to_string(),
            default_format: ImageFormat::Png,
            ..OutputConfig::default()
        };
        let path = suggest_save_path(&cfg).path();
        assert_eq!(path.parent(), Some(dir.path()));
        assert!(path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("escape_")));
    }

    /// 模板扩展名与输出格式不一致时，应以输出格式为准。
    #[test]
    fn test_template_extension_is_replaced_by_selected_format() {
        let dir = tempdir().expect("应可创建临时目录");
        let cfg = OutputConfig {
            quick_save_dir: dir.path().to_string_lossy().to_string(),
            file_name_template: "shot.png".to_string(),
            default_format: ImageFormat::Webp,
            ..OutputConfig::default()
        };
        let path = suggest_save_path(&cfg).path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("shot.webp")
        );
    }

    /// OCR 文本导出必须自动避让同名文件，并使用目标扩展名。
    #[test]
    fn test_save_text_with_config_avoids_overwrite() {
        let dir = tempdir().expect("应可创建临时目录");
        let cfg = OutputConfig {
            quick_save_dir: dir.path().to_string_lossy().to_string(),
            file_name_template: "ocr-fixed".to_string(),
            ..OutputConfig::default()
        };
        let first = save_text_with_config("第一份", "md", &cfg).expect("首次导出成功");
        let second = save_text_with_config("第二份", "md", &cfg).expect("二次导出成功");
        assert_ne!(first, second);
        assert_eq!(
            second.extension().and_then(|value| value.to_str()),
            Some("md")
        );
        assert_eq!(
            std::fs::read_to_string(&second).expect("读取导出文件失败"),
            "第二份"
        );
    }

    /// 连续保存同名模板时必须原子占名，不能互相覆盖。
    #[test]
    fn test_save_with_config_reserves_unique_paths() {
        let dir = tempdir().expect("应可创建临时目录");
        let cfg = OutputConfig {
            quick_save_dir: dir.path().to_string_lossy().to_string(),
            file_name_template: "fixed".to_string(),
            default_format: ImageFormat::Png,
            ..OutputConfig::default()
        };
        let img = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));

        let first = save_image_with_config(&img, &cfg).expect("首次保存应成功");
        let second = save_image_with_config(&img, &cfg).expect("第二次保存应成功");
        assert_ne!(first, second, "同名模板必须自动递增，不能覆盖");
        assert!(first.exists() && second.exists());
    }
}
