//! 图像编码输出与防覆盖写盘模块。

use super::template::{expand_home_dir, format_template};
use crate::error::PlatformError;
use chrono::Local;
use image::RgbaImage;
use snapin_core::config::{ImageFormat, OutputConfig};
use std::path::{Path, PathBuf};

/// 根据输出配置生成目标文件绝对路径（具备重名自增防覆盖安全保障）
pub fn resolve_save_path(cfg: &OutputConfig) -> PathBuf {
    let base_dir = expand_home_dir(&cfg.quick_save_dir);
    if !base_dir.exists() {
        let _ = std::fs::create_dir_all(&base_dir);
    }

    let now = Local::now();
    let file_name = format_template(&cfg.file_name_template, &now);
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
) -> Result<PathBuf, PlatformError> {
    let out_path = resolve_save_path(cfg);
    encode_image_to_path(img, &out_path, cfg.default_format, cfg.jpeg_quality)?;
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
) -> Result<PathBuf, PlatformError> {
    let has_extension = path.extension().is_some();
    let format = match image_format_from_path(path) {
        Some(format) => format,
        None if !has_extension => cfg.default_format,
        None => {
            return Err(PlatformError::InvalidSavePath {
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
        return Err(PlatformError::InvalidSavePath {
            path: out_path.display().to_string(),
            message: "自动补齐扩展名后的目标文件已存在，请重新选择文件名".to_string(),
        });
    }

    encode_image_to_path(img, &out_path, format, cfg.jpeg_quality)?;
    Ok(out_path)
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

/// 将图像按指定格式编码到目标路径。
fn encode_image_to_path(
    img: &RgbaImage,
    out_path: &Path,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<(), PlatformError> {
    match format {
        ImageFormat::Jpeg => {
            let (w, h) = (img.width(), img.height());
            let mut rgb_bytes = Vec::with_capacity((w as usize) * (h as usize) * 3);
            for chunk in img.as_raw().as_chunks::<4>().0 {
                rgb_bytes.extend_from_slice(&chunk[0..3]);
            }
            let rgb =
                image::RgbImage::from_raw(w, h, rgb_bytes).ok_or_else(|| PlatformError::Io {
                    path: out_path.display().to_string(),
                    source: std::io::Error::other("构建 RGB 图像缓冲区失败"),
                })?;
            let mut file = std::fs::File::create(out_path).map_err(|e| PlatformError::Io {
                path: out_path.display().to_string(),
                source: e,
            })?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut file,
                jpeg_quality.clamp(1, 100),
            );
            encoder.encode_image(&rgb).map_err(|e| PlatformError::Io {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
        ImageFormat::Png | ImageFormat::Webp => {
            img.save(out_path).map_err(|e| PlatformError::Io {
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
        let unique_id = snapin_core::AppPaths::generate_unique_id();
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
        let unique_id = snapin_core::AppPaths::generate_unique_id();
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
        let unique_id = snapin_core::AppPaths::generate_unique_id();
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
