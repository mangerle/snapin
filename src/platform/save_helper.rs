//! 图像输出存储、文件名模板解析与历史选区持久化辅助模块。

use crate::config::{ImageFormat, OutputConfig};
use crate::error::SnapinError;
use chrono::Local;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
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

/// 持久化缓存的历史选区矩形数据契约
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelectionRectRecord {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SelectionRectRecord {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 10.0 && self.height > 10.0 && self.x >= 0.0 && self.y >= 0.0
    }
}

/// 保存上次选区坐标至运行时缓存文件
pub fn save_last_selection(rect: (f32, f32, f32, f32)) {
    let record = SelectionRectRecord::new(rect.0, rect.1, rect.2, rect.3);
    if !record.is_valid() {
        return;
    }

    if let Some(cache_dir) = get_cache_dir() {
        let toml_path = cache_dir.join("last_selection.toml");
        if let Ok(toml_content) = toml::to_string(&record) {
            let _ = std::fs::write(toml_path, toml_content);
        }
    }
}

/// 读取上次保存的历史选区
pub fn load_last_selection() -> Option<(f32, f32, f32, f32)> {
    let cache_dir = get_cache_dir()?;
    let toml_path = cache_dir.join("last_selection.toml");

    // 优先读取强类型结构化的 TOML 缓存文件
    if let Ok(content) = std::fs::read_to_string(&toml_path) {
        if let Ok(record) = toml::from_str::<SelectionRectRecord>(&content) {
            if record.is_valid() {
                return Some((record.x, record.y, record.width, record.height));
            }
        }
    }

    // 兜底兼容旧版逗号拼接 txt 缓存文件
    let legacy_path = cache_dir.join("last_selection.txt");
    if let Ok(content) = std::fs::read_to_string(legacy_path) {
        let parts: Vec<&str> = content.split(',').collect();
        if parts.len() == 4 {
            let x: f32 = parts[0].trim().parse().ok()?;
            let y: f32 = parts[1].trim().parse().ok()?;
            let w: f32 = parts[2].trim().parse().ok()?;
            let h: f32 = parts[3].trim().parse().ok()?;
            let record = SelectionRectRecord::new(x, y, w, h);
            if record.is_valid() {
                return Some((x, y, w, h));
            }
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

    #[test]
    fn test_selection_record_serde_and_validation() {
        let record = SelectionRectRecord::new(10.0, 20.0, 100.0, 80.0);
        assert!(record.is_valid());

        let toml_str = toml::to_string(&record).expect("序列化选区记录成功");
        let decoded: SelectionRectRecord = toml::from_str(&toml_str).expect("反序列化选区记录成功");
        assert_eq!(record, decoded);

        let invalid = SelectionRectRecord::new(-1.0, 0.0, 5.0, 5.0);
        assert!(!invalid.is_valid());
    }
}
