//! Snapin 全局统一规范化路径与运行时环境管理。
//!
//! 统一遵循 XDG Base Directory 规范收敛管理配置文件、缓存目录、IPC 套接字与安全运行时临时目录，
//! 消除各子系统分散拼接路径导致的不一致与误删隐患。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Snapin 统一路径管理中心
pub struct AppPaths;

impl AppPaths {
    /// 获取用户配置根目录：优先 `$XDG_CONFIG_HOME/snapin`，兜底 `~/.config/snapin`
    pub fn config_dir() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|_| PathBuf::from("."));
        let p = base.join("snapin");
        let _ = std::fs::create_dir_all(&p);
        p
    }

    /// 获取默认配置文件路径：`$XDG_CONFIG_HOME/snapin/config.toml`
    pub fn default_config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// 获取缓存根目录：优先 `$XDG_CACHE_HOME/snapin`，兜底 `~/.cache/snapin`
    pub fn cache_dir() -> PathBuf {
        let base = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(|_| PathBuf::from("."));
        let p = base.join("snapin");
        let _ = std::fs::create_dir_all(&p);
        p
    }

    /// 获取安全运行时临时目录（多用户 UID 隔离与 tmpfs 提速）：
    /// 优先 `$XDG_RUNTIME_DIR/snapin`，次选 `/dev/shm/snapin-$UID`，兜底系统临时目录
    pub fn runtime_dir() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let p = PathBuf::from(runtime_dir).join("snapin");
            let _ = std::fs::create_dir_all(&p);
            return p;
        }

        let uid = rustix::process::getuid().as_raw();
        let shm_dir = Path::new("/dev/shm");
        let base = if shm_dir.exists() {
            shm_dir.join(format!("snapin-{uid}"))
        } else {
            std::env::temp_dir().join(format!("snapin-{uid}"))
        };
        let _ = std::fs::create_dir_all(&base);
        base
    }

    /// 获取 IPC 通信套接字文件路径：优先 `$XDG_RUNTIME_DIR/snapin.sock`，兜底 `runtime_dir/snapin.sock`
    pub fn default_socket_path() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            PathBuf::from(runtime_dir).join("snapin.sock")
        } else {
            Self::runtime_dir().join("snapin.sock")
        }
    }

    /// 获取用户图片默认保存目录：优先 `~/Pictures`，兜底当前目录
    pub fn pictures_dir() -> PathBuf {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Pictures"))
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    /// 获取 GNOME Shell 扩展安装目标路径
    pub fn gnome_extension_dir(uuid: &str) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".local/share/gnome-shell/extensions")
            .join(uuid)
    }

    /// 生成高并发安全的 64 位全局唯一序列 ID
    pub fn generate_unique_id() -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let seq = SEQUENCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        now.wrapping_add(seq)
    }

    /// 生成防碰撞的安全临时文件绝对路径（位于安全运行时目录下）
    pub fn generate_temp_path(prefix: &str, ext: &str) -> PathBuf {
        let dir = Self::runtime_dir();
        let id = Self::generate_unique_id();
        let pid = std::process::id();
        dir.join(format!("{prefix}_{pid}_{id}.{ext}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_paths_generation() {
        let cfg_path = AppPaths::default_config_file();
        assert!(cfg_path.ends_with("config.toml"));

        let sock_path = AppPaths::default_socket_path();
        assert!(sock_path.ends_with("snapin.sock"));

        let temp_path = AppPaths::generate_temp_path("test", "raw");
        assert!(temp_path.to_string_lossy().contains("test_"));
        assert!(temp_path.to_string_lossy().ends_with(".raw"));
    }
}
