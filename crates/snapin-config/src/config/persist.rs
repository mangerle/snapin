//! 配置文件原子写入与权限收敛。

use super::model::{Config, ConfigFile};
use crate::error::CoreError;
use std::fs;
use std::path::Path;

/// 将已存在配置文件的权限收紧为 0600，避免历史版本创建的 0644 文件泄露 API 密钥。
pub(super) fn tighten_config_permissions(path: &Path) {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                if let Err(error) =
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                {
                    tracing::warn!("收紧配置文件权限失败，路径: {:?}，原因: {error}", path);
                }
            }
        }
    }
}

/// 将内存配置结构体持久化写入指定的 TOML 文件
///
/// # Errors
///
/// 路径解析、语义校验、序列化或原子持久化失败时返回 [`CoreError`]；当前默认路径实现本身不会失败。
pub fn save_config(config: &Config, target_path: &Path) -> Result<(), CoreError> {
    if target_path == crate::paths::AppPaths::default_config_file() {
        crate::paths::AppPaths::ensure_config_dir().map_err(|source| CoreError::IoError {
            path: crate::paths::AppPaths::config_dir(),
            source,
        })?;
    }
    let toml_string = serialize_config(config)?;
    write_config_atomically(target_path, toml_string.as_bytes())
}

/// 收敛并校验内存配置，再序列化为带 schema 版本的 TOML。
fn serialize_config(config: &Config) -> Result<String, CoreError> {
    let sanitized = config.clone().sanitized();
    sanitized.validate()?;
    toml::to_string_pretty(&ConfigFile::current(sanitized)).map_err(|error| {
        CoreError::ConfigSerializeError {
            reason: error.to_string(),
        }
    })
}

/// 通过同目录临时文件和原子替换写入配置。
fn write_config_atomically(target_path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    ensure_parent_dir(target_path)?;
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let temp_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        crate::paths::AppPaths::generate_unique_id()
    ));

    if let Err(error) = crate::paths::AppPaths::write_secure_file(&temp_path, bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(CoreError::IoError {
            path: temp_path,
            source: error,
        });
    }

    if let Err(error) = crate::paths::AppPaths::replace_file_atomically(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(CoreError::IoError {
            path: target_path.to_path_buf(),
            source: error,
        });
    }

    tracing::info!("配置文件保存就绪: {:?}", target_path);
    Ok(())
}

/// 确保配置目标路径的父目录存在。
fn ensure_parent_dir(target_path: &Path) -> Result<(), CoreError> {
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|error| CoreError::IoError {
                path: parent.to_path_buf(),
                source: error,
            })?;
        }
    }
    Ok(())
}
