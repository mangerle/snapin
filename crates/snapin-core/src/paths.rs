//! Snapin 全局统一规范化路径与运行时环境管理。
//!
//! 统一遵循 XDG Base Directory 规范收敛管理配置文件、缓存目录、IPC 套接字与安全运行时临时目录，
//! 消除各子系统分散拼接路径导致的不一致与误删隐患。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Snapin 统一路径管理中心
pub struct AppPaths;

impl AppPaths {
    /// 获取用户配置根目录：
    /// - Linux: 优先 `$XDG_CONFIG_HOME/snapin`，兜底 `~/.config/snapin`
    /// - Windows: 优先 `%APPDATA%\snapin`，兜底系统临时目录
    /// - macOS: `~/Library/Application Support/snapin`
    pub fn config_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|h| PathBuf::from(h).join("AppData").join("Roaming"))
            })
            .unwrap_or_else(|_| std::env::temp_dir());

        #[cfg(target_os = "macos")]
        let base = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
            .unwrap_or_else(|_| PathBuf::from("."));

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|_| PathBuf::from("."));

        base.join("snapin")
    }

    /// 确保配置目录存在并收紧为私有权限。
    ///
    /// 目录创建从 getter 中拆出，避免仅读取路径或测试布局时产生磁盘副作用。
    pub fn ensure_config_dir() -> PathBuf {
        let path = Self::config_dir();
        ensure_private_dir(&path);
        path
    }

    /// 确保缓存目录存在并收紧为私有权限。
    pub fn ensure_cache_dir() -> PathBuf {
        let path = Self::cache_dir();
        ensure_private_dir(&path);
        path
    }

    /// 确保运行时目录存在并收紧为私有权限。
    pub fn ensure_runtime_dir() -> PathBuf {
        let path = Self::runtime_dir();
        ensure_private_dir(&path);
        path
    }

    /// 获取默认配置文件路径：`$XDG_CONFIG_HOME/snapin/config.toml` 或系统对应配置目录
    pub fn default_config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// 获取缓存根目录：
    /// - Linux: 优先 `$XDG_CACHE_HOME/snapin`，兜底 `~/.cache/snapin`
    /// - Windows: 优先 `%LOCALAPPDATA%\snapin\cache`
    /// - macOS: `~/Library/Caches/snapin`
    pub fn cache_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        let base = std::env::var("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("snapin").join("cache"))
            .unwrap_or_else(|_| std::env::temp_dir().join("snapin-cache"));

        #[cfg(target_os = "macos")]
        let base = std::env::var("HOME")
            .map(|h| {
                PathBuf::from(h)
                    .join("Library")
                    .join("Caches")
                    .join("snapin")
            })
            .unwrap_or_else(|_| PathBuf::from(".snapin-cache"));

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let base = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("snapin");

        base
    }

    /// 获取安全运行时临时目录（多用户隔离与 tmpfs 提速）：
    /// - Linux: 优先 `$XDG_RUNTIME_DIR/snapin`，次选 `/dev/shm/snapin-$UID`，兜底系统临时目录
    /// - Windows: `%TEMP%\snapin-<USERNAME>`
    /// - macOS: 优先 `$TMPDIR/snapin`，兜底系统临时目录
    pub fn runtime_dir() -> PathBuf {
        #[cfg(unix)]
        {
            if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
                return PathBuf::from(runtime_dir).join("snapin");
            }

            let uid = rustix::process::getuid().as_raw();
            let shm_dir = Path::new("/dev/shm");
            let base = if shm_dir.exists() {
                shm_dir.join(format!("snapin-{uid}"))
            } else {
                std::env::temp_dir().join(format!("snapin-{uid}"))
            };
            base
        }

        #[cfg(windows)]
        {
            let username = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
            std::env::temp_dir().join(format!("snapin-{username}"))
        }
    }

    /// 获取 IPC 通信套接字文件路径：始终位于 `runtime_dir()` 私有目录内。
    ///
    /// 设计原理：套接字与截屏原始像素临时文件同属运行时私有数据，必须共享同一 0700 目录，
    /// 否则既会把可被同用户其他进程抢占的句柄暴露在共享 `$XDG_RUNTIME_DIR` 下，
    /// 又会让"清空临时缓存"逻辑因路径不在同一目录而无法识别并保护在线套接字。
    pub fn default_socket_path() -> PathBuf {
        Self::runtime_dir().join("snapin.sock")
    }

    /// 获取当前用户主目录；Windows 优先 `USERPROFILE`，Unix/macOS 使用 `HOME`。
    pub fn home_dir() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::env::var_os("HOME").map(PathBuf::from)
        }
    }

    /// 获取用户图片默认保存目录：优先操作系统图片目录，兜底当前目录
    pub fn pictures_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(profile) = std::env::var("USERPROFILE") {
                return PathBuf::from(profile).join("Pictures");
            }
        }

        #[cfg(unix)]
        {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join("Pictures");
            }
        }

        PathBuf::from(".")
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
        let dir = Self::ensure_runtime_dir();
        let id = Self::generate_unique_id();
        let pid = std::process::id();
        dir.join(format!("{prefix}_{pid}_{id}.{ext}"))
    }

    /// 以仅当前用户可读写的权限（0600）写入文件。
    ///
    /// # Errors
    ///
    /// 当父目录不存在、文件创建失败或写入磁盘失败时返回底层 I/O 错误。
    pub fn write_secure_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(bytes)?;
        file.sync_all()
    }

    /// 以平台一致的语义将临时文件替换到目标路径。
    ///
    /// Unix 的 `rename` 本身支持原子覆盖；Windows 的 `std::fs::rename` 不会覆盖
    /// 已存在文件，因此使用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`。配置、选区缓存、
    /// 剪贴板索引和工作区状态都必须在“已有旧文件”时成功提交，否则 Windows 上第二次
    /// 保存就会稳定失败。
    ///
    /// # Errors
    ///
    /// 源文件不存在、目标路径不可写、跨文件系统移动失败或 Windows API 返回错误时，
    /// 返回底层 I/O 错误。
    pub fn replace_file_atomically(source: &Path, target: &Path) -> std::io::Result<()> {
        #[cfg(not(windows))]
        {
            std::fs::rename(source, target)
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };

            fn wide_path(path: &Path) -> std::io::Result<Vec<u16>> {
                let mut encoded: Vec<u16> = path.as_os_str().encode_wide().collect();
                if encoded.contains(&0) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "路径包含 Windows API 不允许的空字符",
                    ));
                }
                encoded.push(0);
                Ok(encoded)
            }

            let source = wide_path(source)?;
            let target = wide_path(target)?;
            // SAFETY: 两个路径缓冲区都以空字符结尾，并在调用期间保持有效；MoveFileExW
            // 只读取这两个指针，不保留其生命周期。标志位组合在函数契约允许范围内。
            let replaced = unsafe {
                MoveFileExW(
                    source.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if replaced == 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }
}

/// 创建目录并收紧为仅当前用户可访问的 0700 权限。
///
/// 权限设置失败时不会阻断主流程（只记录警告），避免在不支持权限位的文件系统上崩溃。
fn ensure_private_dir(path: &Path) {
    if let Err(error) = std::fs::create_dir_all(path) {
        tracing::warn!("创建私有目录失败，路径: {:?}，原因: {error}", path);
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)) {
            tracing::warn!("收紧目录权限失败，路径: {:?}，原因: {error}", path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_paths_generation() {
        let cfg_path = AppPaths::default_config_file();
        assert!(cfg_path.ends_with("config.toml"));

        // 套接字必须落在 0700 私有运行时目录内，保证清理临时缓存时可识别并保护在服套接字
        let sock_path = AppPaths::default_socket_path();
        assert!(sock_path.ends_with("snapin.sock"));
        assert_eq!(sock_path.parent(), Some(AppPaths::runtime_dir().as_path()));

        let temp_path = AppPaths::generate_temp_path("test", "raw");
        assert!(temp_path.to_string_lossy().contains("test_"));
        assert!(temp_path.to_string_lossy().ends_with(".raw"));
    }

    #[test]
    fn test_replace_file_atomically_overwrites_existing_target() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let source = dir.path().join("new.toml");
        let target = dir.path().join("config.toml");
        std::fs::write(&source, b"new").expect("应可写入源文件");
        std::fs::write(&target, b"old").expect("应可写入旧目标文件");

        AppPaths::replace_file_atomically(&source, &target).expect("替换已有文件必须成功");

        assert_eq!(std::fs::read(&target).expect("应可读取目标文件"), b"new");
        assert!(!source.exists(), "替换成功后临时源文件不应继续存在");
    }
}
