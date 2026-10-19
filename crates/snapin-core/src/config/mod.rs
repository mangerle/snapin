//! 配置管理模块门面。
//!
//! 负责配置文件的定位、自动创建默认配置、TOML 反序列化与安全保存。

pub mod enums;
pub mod model;
pub mod sections;
pub mod types;

use crate::error::SnapinError;
pub use model::{
    ColorFormat, Config, DefaultTool, DoubleClickAction, GeneralConfig, HotkeyConfig, ImageFormat,
    InterfaceConfig, LogLevel, OutputConfig, PasterConfig, SnipConfig, Theme, TrayTheme,
    ZoomAnchor,
};
use std::fs;
use std::path::{Path, PathBuf};
pub use types::{HexColor, Hotkey};

/// 获取 Snapin 默认配置文件存储路径：`~/.config/snapin/config.toml`
pub fn get_default_config_path() -> Result<PathBuf, SnapinError> {
    let home = std::env::var("HOME").map_err(|e| SnapinError::IoError {
        path: "$HOME".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, e),
    })?;

    let config_dir = Path::new(&home).join(".config").join("snapin");
    Ok(config_dir.join("config.toml"))
}

/// 加载配置文件；若文件不存在，则自动初始化并保存一套默认配置到磁盘
pub fn load_or_create_config(custom_path: Option<&Path>) -> Result<Config, SnapinError> {
    let target_path = match custom_path {
        Some(p) => p.to_path_buf(),
        None => get_default_config_path()?,
    };

    if !target_path.exists() {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| SnapinError::IoError {
                path: parent.to_string_lossy().to_string(),
                source: e,
            })?;
        }

        let default_config = Config::default();
        save_config(&default_config, &target_path)?;
        tracing::info!("未检测到配置文件，已初始化默认配置至: {:?}", target_path);
        return Ok(default_config);
    }

    let content = fs::read_to_string(&target_path).map_err(|e| SnapinError::IoError {
        path: target_path.to_string_lossy().to_string(),
        source: e,
    })?;

    let config: Config = toml::from_str(&content).map_err(|e| SnapinError::ConfigParseError {
        path: target_path.to_string_lossy().to_string(),
        source: e,
    })?;

    tracing::debug!("成功加载配置文件: {:?}", target_path);
    Ok(config)
}

/// 将内存配置结构体持久化写入指定的 TOML 文件
pub fn save_config(config: &Config, target_path: &Path) -> Result<(), SnapinError> {
    let toml_string = toml::to_string_pretty(config)?;

    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| SnapinError::IoError {
                path: parent.to_string_lossy().to_string(),
                source: e,
            })?;
        }
    }

    fs::write(target_path, toml_string).map_err(|e| SnapinError::IoError {
        path: target_path.to_string_lossy().to_string(),
        source: e,
    })?;

    tracing::info!("配置文件保存就绪: {:?}", target_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config_serde_roundtrip() {
        let original = Config::default();
        let toml_str = toml::to_string_pretty(&original).expect("TOML 序列化不应失败");
        let decoded: Config = toml::from_str(&toml_str).expect("TOML 反序列化不应失败");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_load_or_create_new_file() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("sub_dir").join("config.toml");

        assert!(!config_file.exists());
        let loaded = load_or_create_config(Some(&config_file)).expect("加载默认配置失败");
        assert!(config_file.exists());
        assert_eq!(loaded, Config::default());
    }
}
