//! 截图历史选区坐标持久化与缓存管理模块。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 选区缓存文件允许读取的最大字节数。
const MAX_SELECTION_CACHE_BYTES: usize = 16 * 1024;

/// 持久化缓存的历史选区矩形数据契约。
///
/// 该结构只描述缓存文件本身的落盘格式，既非用户偏好也不跨模块共享，因此收敛为本模块私有类型，
/// 避免核心配置层为单一存储实现承担对外 API 义务。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct SelectionRectRecord {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl SelectionRectRecord {
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 过滤宽高不足、坐标越界或非有限数的畸形记录。
    fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 10.0
            && self.height > 10.0
            && self.x >= 0.0
            && self.y >= 0.0
    }
}

/// 获取 Snapin 缓存目录。
fn get_cache_dir() -> PathBuf {
    snapin_foundation::AppPaths::cache_dir()
}

/// 保存上次选区坐标至运行时缓存文件
pub fn save_last_selection(rect: (f32, f32, f32, f32)) {
    save_last_selection_in(&get_cache_dir(), rect);
}

/// 将历史选区写入指定缓存目录；目录由调用方负责存在。
fn save_last_selection_in(cache_dir: &std::path::Path, rect: (f32, f32, f32, f32)) {
    let record = SelectionRectRecord::new(rect.0, rect.1, rect.2, rect.3);
    if !record.is_valid() {
        return;
    }
    let _ = std::fs::create_dir_all(cache_dir);
    let toml_path = cache_dir.join("last_selection.toml");
    let Ok(toml_content) = toml::to_string(&record) else {
        return;
    };

    // 同目录临时文件 + rename，避免进程崩溃留下半截 TOML 被静默当成损坏缓存。
    let temp_path = cache_dir.join(format!(
        ".last_selection.{}.{}.tmp",
        std::process::id(),
        snapin_foundation::AppPaths::generate_unique_id()
    ));
    if snapin_foundation::AppPaths::write_secure_file(&temp_path, toml_content.as_bytes()).is_err()
    {
        let _ = std::fs::remove_file(&temp_path);
        return;
    }
    if snapin_foundation::AppPaths::replace_file_atomically(&temp_path, &toml_path).is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
}

/// 读取上次保存的历史选区
pub fn load_last_selection() -> Option<(f32, f32, f32, f32)> {
    load_last_selection_from(&get_cache_dir())
}

/// 从指定缓存目录读取历史选区。
fn load_last_selection_from(cache_dir: &std::path::Path) -> Option<(f32, f32, f32, f32)> {
    let toml_path = cache_dir.join("last_selection.toml");

    // 优先读取强类型结构化的 TOML 缓存文件
    if let Ok(content) =
        snapin_foundation::read_utf8_file_limited(&toml_path, MAX_SELECTION_CACHE_BYTES)
    {
        if let Ok(record) = toml::from_str::<SelectionRectRecord>(&content) {
            if record.is_valid() {
                return Some((record.x, record.y, record.width, record.height));
            }
        }
    }

    // 兜底兼容旧版逗号拼接 txt 缓存文件
    let legacy_path = cache_dir.join("last_selection.txt");
    if let Ok(content) =
        snapin_foundation::read_utf8_file_limited(&legacy_path, MAX_SELECTION_CACHE_BYTES)
    {
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
    clear_selection_cache_in(&get_cache_dir())
}

/// 清理指定缓存目录中的选区缓存文件。
fn clear_selection_cache_in(cache_dir: &std::path::Path) -> usize {
    let mut count = 0;
    let toml_path = cache_dir.join("last_selection.toml");
    if toml_path.exists() && std::fs::remove_file(&toml_path).is_ok() {
        count += 1;
    }
    let legacy_path = cache_dir.join("last_selection.txt");
    if legacy_path.exists() && std::fs::remove_file(&legacy_path).is_ok() {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_record_serde_and_validation() {
        let record = SelectionRectRecord::new(100.0, 200.0, 800.0, 600.0);
        assert!(record.is_valid());

        let toml_str = toml::to_string(&record).expect("序列化选区记录成功");
        let decoded: SelectionRectRecord = toml::from_str(&toml_str).expect("反序列化选区记录成功");
        assert_eq!(record, decoded);

        let invalid = SelectionRectRecord::new(-1.0, 0.0, 5.0, 5.0);
        assert!(!invalid.is_valid());
        assert!(!SelectionRectRecord::new(0.0, 0.0, f32::INFINITY, 20.0).is_valid());
        assert!(!SelectionRectRecord::new(0.0, 0.0, 20.0, f32::NAN).is_valid());
    }

    /// 原子写入成功后不得残留同目录临时文件。
    #[test]
    fn test_save_last_selection_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("应可创建测试缓存目录");
        save_last_selection_in(dir.path(), (10.0, 20.0, 300.0, 200.0));
        assert!(load_last_selection_from(dir.path()).is_some());

        let temp_exists = std::fs::read_dir(dir.path())
            .expect("读取缓存目录失败")
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".last_selection.")
            });
        assert!(!temp_exists, "成功写入后不应残留临时文件");
    }

    #[test]
    fn test_clear_selection_cache_is_hermetic() {
        let dir = tempfile::tempdir().expect("应可创建测试缓存目录");
        save_last_selection_in(dir.path(), (50.0, 60.0, 200.0, 150.0));
        assert!(load_last_selection_from(dir.path()).is_some());

        let cleared = clear_selection_cache_in(dir.path());
        assert_eq!(cleared, 1);
        assert!(load_last_selection_from(dir.path()).is_none());
    }

    #[test]
    fn test_clear_selection_cache_removes_legacy_file() {
        let dir = tempfile::tempdir().expect("应可创建测试缓存目录");
        std::fs::write(dir.path().join("last_selection.txt"), "1,2,300,400")
            .expect("应可写入旧版缓存");
        assert_eq!(clear_selection_cache_in(dir.path()), 1);
        assert!(!dir.path().join("last_selection.txt").exists());
    }

    #[test]
    fn test_oversized_selection_cache_is_ignored() {
        let dir = tempfile::tempdir().expect("应可创建测试缓存目录");
        std::fs::write(
            dir.path().join("last_selection.toml"),
            vec![b' '; MAX_SELECTION_CACHE_BYTES.saturating_add(1)],
        )
        .expect("应可写入超限缓存");

        assert!(load_last_selection_from(dir.path()).is_none());
    }
}
