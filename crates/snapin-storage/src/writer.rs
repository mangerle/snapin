//! 图像编码输出与防覆盖写盘模块。

use super::template::{expand_home_dir, format_template};
use crate::error::StorageError;
use chrono::Local;
use image::RgbaImage;
use snapin_config::config::{normalize_file_name_template, ImageFormat, OutputConfig};
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// 根据输出配置生成目标文件绝对路径（具备重名自增防覆盖安全保障）
pub fn resolve_save_path(cfg: &OutputConfig) -> PathBuf {
    let base_dir = expand_home_dir(&cfg.quick_save_dir);
    if !base_dir.exists() {
        let _ = std::fs::create_dir_all(&base_dir);
    }

    let now = Local::now();
    let template = normalize_file_name_template(&cfg.file_name_template);
    let rendered = format_template(&template, &now);
    let file_name = std::path::Path::new(&rendered)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Snapin");
    let ext = cfg.default_format.extension();

    let initial_path = base_dir.join(format!("{file_name}.{ext}"));
    if !initial_path.exists() {
        return initial_path;
    }

    // 防静默覆盖：若目标文件已存在，递增寻找未占用的序号文件名（如 _1, _2）
    let mut counter = 1usize;
    loop {
        let candidate = base_dir.join(format!("{file_name}_{counter}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        counter = counter.saturating_add(1);
    }
}

/// 按照用户配置格式将图像编码并写入磁盘
pub fn save_image_with_config(
    img: &RgbaImage,
    cfg: &OutputConfig,
) -> Result<PathBuf, StorageError> {
    // 通过 create_new 原子抢占文件名，消除“先 exists 再写入”的 TOCTOU；
    // 编码先落到同目录临时文件，成功后再原子替换占位文件，失败不会留下半截图像。
    let out_path = reserve_output_path(cfg)?;
    if let Err(error) =
        encode_image_atomically(img, &out_path, cfg.default_format, cfg.jpeg_quality)
    {
        let _ = std::fs::remove_file(&out_path);
        return Err(error);
    }
    Ok(out_path)
}

/// 将图像保存到用户在“另存为”对话框中选择的路径。
///
/// 格式优先按目标扩展名推断。无扩展名时补齐全局默认格式的扩展名，但若补齐后的
/// 文件已存在则拒绝写入，因为 Portal 的覆盖确认针对的是用户最初选择的路径；
/// 不支持的扩展名直接报错，避免静默把文件改写成另一种格式。
pub fn save_image_to_path(
    img: &RgbaImage,
    path: &Path,
    cfg: &OutputConfig,
) -> Result<PathBuf, StorageError> {
    let has_extension = path.extension().is_some();
    let format = match image_format_from_path(path) {
        Some(format) => format,
        None if !has_extension => cfg.default_format,
        None => {
            return Err(StorageError::InvalidSavePath {
                path: path.display().to_string(),
                message: "仅支持 .png、.jpg、.jpeg 或 .webp 扩展名".to_string(),
            });
        }
    };

    let out_path = if has_extension {
        path.to_path_buf()
    } else {
        path.with_extension(format.extension())
    };
    if !has_extension && out_path.exists() {
        return Err(StorageError::InvalidSavePath {
            path: out_path.display().to_string(),
            message: "自动补齐扩展名后的目标文件已存在，请重新选择文件名".to_string(),
        });
    }

    encode_image_atomically(img, &out_path, format, cfg.jpeg_quality)?;
    Ok(out_path)
}

/// 将文本导出到快捷保存目录，并生成不覆盖已有文件的唯一路径。
///
/// OCR 导出的内容可能包含用户截图中的敏感文字，因此文件使用 `create_new` 防止
/// 静默覆盖，并在 Unix 上以 0600 权限创建。写入失败会删除半成品文件。
///
/// # Errors
///
/// 扩展名不是 `md`/`json`、目录创建失败、文件已存在或写入失败时返回
/// [`StorageError`]。
pub fn save_text_with_config(
    content: &str,
    extension: &str,
    cfg: &OutputConfig,
) -> Result<PathBuf, StorageError> {
    let extension = extension.to_ascii_lowercase();
    if !matches!(extension.as_str(), "md" | "json") {
        return Err(StorageError::InvalidSavePath {
            path: extension,
            message: "OCR 文本仅支持 .md 或 .json 扩展名".to_string(),
        });
    }

    let base_dir = expand_home_dir(&cfg.quick_save_dir);
    std::fs::create_dir_all(&base_dir).map_err(|error| StorageError::Io {
        path: base_dir.display().to_string(),
        source: error,
    })?;
    let file_name = format!("{}_ocr", rendered_file_name(cfg));

    for counter in 0usize.. {
        let candidate = if counter == 0 {
            base_dir.join(format!("{file_name}.{extension}"))
        } else {
            base_dir.join(format!("{file_name}_{counter}.{extension}"))
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(mut file) => {
                if let Err(error) = std::io::Write::write_all(&mut file, content.as_bytes()) {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(StorageError::Io {
                        path: candidate.display().to_string(),
                        source: error,
                    });
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(StorageError::Io {
                    path: candidate.display().to_string(),
                    source: error,
                });
            }
        }
    }

    Err(StorageError::Io {
        path: base_dir.display().to_string(),
        source: std::io::Error::other("无法找到可用的 OCR 导出文件名"),
    })
}

/// 原子预占一个不存在的输出路径，返回可写入的目标路径。
fn reserve_output_path(cfg: &OutputConfig) -> Result<PathBuf, StorageError> {
    let base_dir = expand_home_dir(&cfg.quick_save_dir);
    std::fs::create_dir_all(&base_dir).map_err(|error| StorageError::Io {
        path: base_dir.display().to_string(),
        source: error,
    })?;

    let file_name = rendered_file_name(cfg);
    let ext = cfg.default_format.extension();

    for counter in 0usize.. {
        let candidate = if counter == 0 {
            base_dir.join(format!("{file_name}.{ext}"))
        } else {
            base_dir.join(format!("{file_name}_{counter}.{ext}"))
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(StorageError::Io {
                    path: candidate.display().to_string(),
                    source: error,
                })
            }
        }
    }

    Err(StorageError::Io {
        path: base_dir.display().to_string(),
        source: std::io::Error::other("无法找到可用的输出文件名"),
    })
}

/// 提取经过模板渲染与路径净化后的基础文件名。
fn rendered_file_name(cfg: &OutputConfig) -> String {
    let template = normalize_file_name_template(&cfg.file_name_template);
    let rendered = format_template(&template, &Local::now());
    Path::new(&rendered)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Snapin")
        .to_string()
}

/// 根据文件扩展名推断受支持的图像格式。
fn image_format_from_path(path: &Path) -> Option<ImageFormat> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        _ => None,
    }
}

/// 先写同目录临时文件，再原子替换目标路径。
fn encode_image_atomically(
    img: &RgbaImage,
    out_path: &Path,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<(), StorageError> {
    let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = out_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("snapin");
    let temp_path = parent.join(format!(
        ".{stem}.{}.{}.{}",
        std::process::id(),
        snapin_foundation::AppPaths::generate_unique_id(),
        format.extension()
    ));

    let encoded = encode_image_to_path(img, &temp_path, format, jpeg_quality);
    if let Err(error) = encoded {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = snapin_foundation::AppPaths::replace_file_atomically(&temp_path, out_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(StorageError::Io {
            path: out_path.display().to_string(),
            source: error,
        });
    }
    Ok(())
}

/// 将图像按指定格式编码到目标路径。
fn encode_image_to_path(
    img: &RgbaImage,
    out_path: &Path,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<(), StorageError> {
    match format {
        ImageFormat::Jpeg => {
            let (w, h) = (img.width(), img.height());
            let mut rgb_bytes = Vec::with_capacity((w as usize) * (h as usize) * 3);
            for chunk in img.as_raw().as_chunks::<4>().0 {
                rgb_bytes.extend_from_slice(&chunk[0..3]);
            }
            let rgb =
                image::RgbImage::from_raw(w, h, rgb_bytes).ok_or_else(|| StorageError::Io {
                    path: out_path.display().to_string(),
                    source: std::io::Error::other("构建 RGB 图像缓冲区失败"),
                })?;
            let mut file = std::fs::File::create(out_path).map_err(|e| StorageError::Io {
                path: out_path.display().to_string(),
                source: e,
            })?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut file,
                jpeg_quality.clamp(1, 100),
            );
            encoder.encode_image(&rgb).map_err(|e| StorageError::Io {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
        ImageFormat::Png | ImageFormat::Webp => {
            img.save(out_path).map_err(|e| StorageError::Io {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
    fn test_resolve_save_path_auto_increment_when_exists() {
        let unique_id = snapin_foundation::AppPaths::generate_unique_id();
        let temp_dir = std::env::temp_dir().join(format!("snapin_inc_{unique_id}"));
        let cfg = OutputConfig {
            quick_save_dir: temp_dir.to_string_lossy().to_string(),
            file_name_template: "snapin_fixed".to_string(),
            default_format: ImageFormat::Png,
            ..OutputConfig::default()
        };

        // 第一次生成：文件不存在，返回固定原名
        let path1 = resolve_save_path(&cfg);
        assert_eq!(path1, temp_dir.join("snapin_fixed.png"));
        std::fs::write(&path1, b"first").expect("创建占位文件1成功");

        // 第二次生成：原文件已存在，自增生成 _1 后缀
        let path2 = resolve_save_path(&cfg);
        assert_eq!(path2, temp_dir.join("snapin_fixed_1.png"));
        std::fs::write(&path2, b"second").expect("创建占位文件2成功");

        // 第三次生成：_1 也存在，自增生成 _2 后缀
        let path3 = resolve_save_path(&cfg);
        assert_eq!(path3, temp_dir.join("snapin_fixed_2.png"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

#[cfg(test)]
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
        let path = resolve_save_path(&cfg);
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
        let path = resolve_save_path(&cfg);
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
        let path = resolve_save_path(&cfg);
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
