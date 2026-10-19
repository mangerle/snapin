//! 图像输出存储、文件名模板解析与历史选区持久化辅助模块。

use crate::config::{ImageFormat, OutputConfig};
use crate::error::SnapinError;
use chrono::Local;
use image::RgbaImage;
use std::path::PathBuf;

/// 根据输出配置生成目标文件绝对路径
pub fn resolve_save_path(cfg: &OutputConfig) -> PathBuf {
    let base_dir = expand_home_dir(&cfg.quick_save_dir);
    if !base_dir.exists() {
        let _ = std::fs::create_dir_all(&base_dir);
    }

    let now = Local::now();
    let file_name = format_template(&cfg.file_name_template, &now);
    let ext = cfg.default_format.extension();

    base_dir.join(format!("{file_name}.{ext}"))
}

/// 按照用户配置格式将图像编码并写入磁盘
pub fn save_image_with_config(img: &RgbaImage, cfg: &OutputConfig) -> Result<PathBuf, SnapinError> {
    let out_path = resolve_save_path(cfg);

    match cfg.default_format {
        ImageFormat::Jpeg => {
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            let mut file = std::fs::File::create(&out_path).map_err(|e| SnapinError::IoError {
                path: out_path.display().to_string(),
                source: e,
            })?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut file,
                cfg.jpeg_quality.clamp(1, 100),
            );
            encoder
                .encode_image(&rgb)
                .map_err(|e| SnapinError::IoError {
                    path: out_path.display().to_string(),
                    source: std::io::Error::other(e.to_string()),
                })?;
        }
        ImageFormat::Png | ImageFormat::Webp => {
            img.save(&out_path).map_err(|e| SnapinError::IoError {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
    }

    Ok(out_path)
}

/// 将路径中的 ~ 符号展开为真实用户主目录
fn expand_home_dir(path_str: &str) -> PathBuf {
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    } else if path_str == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path_str)
}

/// 格式化时间戳占位符模板
fn format_template(template: &str, dt: &chrono::DateTime<Local>) -> String {
    template
        .replace("{YYYY}", &dt.format("%Y").to_string())
        .replace("{MM}", &dt.format("%m").to_string())
        .replace("{DD}", &dt.format("%d").to_string())
        .replace("{HH}", &dt.format("%H").to_string())
        .replace("{mm}", &dt.format("%M").to_string())
        .replace("{ss}", &dt.format("%S").to_string())
}

/// 保存上次选区坐标至运行时缓存文件
pub fn save_last_selection(rect: (f32, f32, f32, f32)) {
    if let Some(cache_dir) = get_cache_dir() {
        let path = cache_dir.join("last_selection.txt");
        let content = format!("{},{},{},{}", rect.0, rect.1, rect.2, rect.3);
        let _ = std::fs::write(path, content);
    }
}

/// 读取上次保存的历史选区
pub fn load_last_selection() -> Option<(f32, f32, f32, f32)> {
    let cache_dir = get_cache_dir()?;
    let path = cache_dir.join("last_selection.txt");
    let content = std::fs::read_to_string(path).ok()?;
    let parts: Vec<&str> = content.split(',').collect();
    if parts.len() == 4 {
        let x: f32 = parts[0].trim().parse().ok()?;
        let y: f32 = parts[1].trim().parse().ok()?;
        let w: f32 = parts[2].trim().parse().ok()?;
        let h: f32 = parts[3].trim().parse().ok()?;
        if w > 10.0 && h > 10.0 {
            return Some((x, y, w, h));
        }
    }
    None
}

/// 获取 Snapin 缓存目录
fn get_cache_dir() -> Option<PathBuf> {
    let dir = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .ok()?
        .join("snapin");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_template() {
        let dt = Local::now();
        let res = format_template("Snapin_{YYYY}{MM}{DD}", &dt);
        let y = dt.format("%Y").to_string();
        assert!(res.starts_with(&format!("Snapin_{y}")));
    }

    #[test]
    fn test_expand_home_dir() {
        let res = expand_home_dir("~/Pictures");
        assert!(!res.to_string_lossy().starts_with('~'));
    }
}
