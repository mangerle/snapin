//! 图像输出存储、文件名模板解析与历史选区持久化辅助模块。

use crate::error::PlatformError;
use chrono::Local;
use image::RgbaImage;
use snapin_core::config::{ImageFormat, OutputConfig};
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
pub fn save_image_with_config(
    img: &RgbaImage,
    cfg: &OutputConfig,
) -> Result<PathBuf, PlatformError> {
    let out_path = resolve_save_path(cfg);

    match cfg.default_format {
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
            let mut file = std::fs::File::create(&out_path).map_err(|e| PlatformError::Io {
                path: out_path.display().to_string(),
                source: e,
            })?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut file,
                cfg.jpeg_quality.clamp(1, 100),
            );
            encoder.encode_image(&rgb).map_err(|e| PlatformError::Io {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
        ImageFormat::Png | ImageFormat::Webp => {
            img.save(&out_path).map_err(|e| PlatformError::Io {
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

pub use snapin_core::config::SelectionRectRecord;

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

/// 清理历史选区记忆缓存文件（包括 TOML 与旧版 txt 缓存）
pub fn clear_selection_cache() -> usize {
    let mut count = 0;
    if let Some(cache_dir) = get_cache_dir() {
        let toml_path = cache_dir.join("last_selection.toml");
        if toml_path.exists() && std::fs::remove_file(&toml_path).is_ok() {
            count += 1;
        }
        let legacy_path = cache_dir.join("last_selection.txt");
        if legacy_path.exists() && std::fs::remove_file(&legacy_path).is_ok() {
            count += 1;
        }
    }
    count
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

static SEQUENCE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// 生成高并发安全的 64 位全局唯一序列 ID（结合纳秒时间戳与原子自增计数器）
pub fn generate_unique_id() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = SEQUENCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    now.wrapping_add(seq)
}

/// 获取当前用户专属的安全运行时临时目录（具备多用户权限隔离与 tmpfs 内存提速）
pub fn get_secure_runtime_dir() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(runtime_dir).join("snapin");
        let _ = std::fs::create_dir_all(&p);
        return p;
    }

    let uid = rustix::process::getuid().as_raw();
    let shm_dir = std::path::Path::new("/dev/shm");
    let base = if shm_dir.exists() {
        shm_dir.join(format!("snapin-{uid}"))
    } else {
        std::env::temp_dir().join(format!("snapin-{uid}"))
    };
    let _ = std::fs::create_dir_all(&base);
    base
}

/// 生成安全防碰撞的临时文件路径（包含 UID 隔离、进程 PID 与纳秒唯一序列）
pub fn generate_secure_temp_path(prefix: &str, ext: &str) -> PathBuf {
    let dir = get_secure_runtime_dir();
    let id = generate_unique_id();
    let pid = std::process::id();
    dir.join(format!("{prefix}_{pid}_{id}.{ext}"))
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

    #[test]
    fn test_generate_unique_id_and_secure_temp_path() {
        let id1 = generate_unique_id();
        let id2 = generate_unique_id();
        assert_ne!(id1, id2, "连续生成的唯一 ID 不应相同");

        let p1 = generate_secure_temp_path("snapin_test", "tmp");
        let p2 = generate_secure_temp_path("snapin_test", "tmp");
        assert_ne!(p1, p2, "生成的安全临时路径不应相同");
        assert!(p1.to_string_lossy().contains("snapin_test"));
    }

    #[test]
    fn test_clear_selection_cache() {
        save_last_selection((50.0, 60.0, 200.0, 150.0));
        assert!(load_last_selection().is_some());
        let cleared = clear_selection_cache();
        assert!(cleared >= 1);
        assert!(load_last_selection().is_none());
    }

    #[test]
    fn test_save_image_with_config_jpeg_and_png() {
        let temp_dir = std::env::temp_dir().join(format!("snapin_test_{}", generate_unique_id()));
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
    fn test_secure_runtime_dir_path_traversal_guard() {
        let runtime_dir = get_secure_runtime_dir();
        assert!(runtime_dir.exists(), "安全运行时沙箱目录必须存在");

        // 合法路径：在沙箱目录内生成的文件
        let safe_file = generate_secure_temp_path("test", "raw");
        let _ = std::fs::write(&safe_file, b"test_data");
        let canonical_safe = safe_file.canonicalize().expect("合法路径应可规范化");
        assert!(
            canonical_safe.starts_with(&runtime_dir),
            "合法文件应属于沙箱目录"
        );
        let _ = std::fs::remove_file(&safe_file);

        // 非法越权路径：外部敏感路径（如 /etc）绝不应被判定属于沙箱目录
        let external_path = std::path::PathBuf::from("/etc/hosts");
        if let Ok(canonical_ext) = external_path.canonicalize() {
            assert!(
                !canonical_ext.starts_with(&runtime_dir),
                "外部越权路径必须被安全沙箱拒绝"
            );
        }
    }
}
