//! 截图历史选区坐标持久化与缓存管理模块。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

    /// 过滤宽高不足或坐标为负的畸形记录，避免恢复时生成不可见的历史选区
    fn is_valid(&self) -> bool {
        self.width > 10.0 && self.height > 10.0 && self.x >= 0.0 && self.y >= 0.0
    }
}

/// 获取 Snapin 缓存目录
fn get_cache_dir() -> Option<PathBuf> {
    Some(snapin_core::AppPaths::cache_dir())
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
    }

    #[test]
    fn test_clear_selection_cache() {
        save_last_selection((50.0, 60.0, 200.0, 150.0));
        assert!(load_last_selection().is_some());
        let cleared = clear_selection_cache();
        assert!(cleared >= 1);
        assert!(load_last_selection().is_none());
    }
}
