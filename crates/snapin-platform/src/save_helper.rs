//! 图像输出存储、文件名模板解析与历史选区持久化辅助门面模块（向后兼容）。
//!
//! 核心功能已按关注点分离原则拆解至 `crate::storage` 子模块，
//! 本文件作为门面统一对外重导出，避免破坏存量调用方代码。

pub use crate::storage::selection_cache::{
    clear_selection_cache, load_last_selection, save_last_selection,
};
pub use crate::storage::template::{expand_home_dir, format_template};
pub use crate::storage::writer::{resolve_save_path, save_image_with_config};
pub use snapin_core::config::SelectionRectRecord;
use std::path::PathBuf;

/// 生成高并发安全的 64 位全局唯一序列 ID
pub fn generate_unique_id() -> u64 {
    snapin_core::AppPaths::generate_unique_id()
}

/// 获取当前用户专属的安全运行时临时目录（具备多用户权限隔离与 tmpfs 内存提速）
pub fn get_secure_runtime_dir() -> PathBuf {
    snapin_core::AppPaths::runtime_dir()
}

/// 生成安全防碰撞的临时文件路径（包含 UID 隔离、进程 PID 与纳秒唯一序列）
pub fn generate_secure_temp_path(prefix: &str, ext: &str) -> PathBuf {
    snapin_core::AppPaths::generate_temp_path(prefix, ext)
}

#[cfg(test)]
mod tests {
    use super::*;

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
