//! 配置管理模块门面。
//!
//! 负责配置文件的定位、自动创建默认配置、TOML 反序列化与安全保存。

pub mod enums;
pub mod model;
pub mod sections;
pub mod types;

use crate::error::CoreError;
pub use model::{
    ColorFormat, Config, CoverMode, DefaultTool, DoubleClickAction, GeneralConfig, HotkeyConfig,
    ImageFormat, InterfaceConfig, LogLevel, OcrConfig, OcrEngineKind, OutputConfig, PasterConfig,
    SnipConfig, TargetLanguage, Theme, TranslateConfig, TranslateProvider, TrayTheme, ZoomAnchor,
};
use std::fs;
use std::path::{Path, PathBuf};
pub use types::{HexColor, Hotkey, SelectionRectRecord};

/// 获取 Snapin 默认配置文件存储路径：优先 `$XDG_CONFIG_HOME/snapin/config.toml`，兜底 `~/.config/snapin/config.toml`
pub fn get_default_config_path() -> Result<PathBuf, CoreError> {
    Ok(crate::paths::AppPaths::default_config_file())
}

/// 加载配置文件；若文件不存在，则自动初始化并保存一套默认配置到磁盘
pub fn load_or_create_config(custom_path: Option<&Path>) -> Result<Config, CoreError> {
    let target_path = match custom_path {
        Some(p) => p.to_path_buf(),
        None => get_default_config_path()?,
    };

    if !target_path.exists() {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| CoreError::IoError {
                path: parent.to_string_lossy().to_string(),
                source: e,
            })?;
        }

        let default_config = Config::default();
        save_config(&default_config, &target_path)?;
        tracing::info!("未检测到配置文件，已初始化默认配置至: {:?}", target_path);
        return Ok(default_config);
    }

    let content = fs::read_to_string(&target_path).map_err(|e| CoreError::IoError {
        path: target_path.to_string_lossy().to_string(),
        source: e,
    })?;

    let config: Config = toml::from_str(&content).map_err(|e| CoreError::ConfigParseError {
        path: target_path.to_string_lossy().to_string(),
        source: e,
    })?;

    tighten_config_permissions(&target_path);
    tracing::debug!("成功加载配置文件: {:?}", target_path);
    Ok(config)
}

/// 将已存在配置文件的权限收紧为 0600，避免历史版本创建的 0644 文件泄露 API 密钥。
fn tighten_config_permissions(path: &Path) {
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
pub fn save_config(config: &Config, target_path: &Path) -> Result<(), CoreError> {
    let toml_string = toml::to_string_pretty(config)?;

    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| CoreError::IoError {
                path: parent.to_string_lossy().to_string(),
                source: e,
            })?;
        }
    }

    // 同目录临时文件 + 原子 rename，避免写入中断导致配置被截断；
    // 临时文件使用 0600 权限，防止包含 API Key 的配置被其他本机用户读取。
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

    if let Err(error) =
        crate::paths::AppPaths::write_secure_file(&temp_path, toml_string.as_bytes())
    {
        let _ = fs::remove_file(&temp_path);
        return Err(CoreError::IoError {
            path: temp_path.to_string_lossy().to_string(),
            source: error,
        });
    }

    if let Err(error) = fs::rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(CoreError::IoError {
            path: target_path.to_string_lossy().to_string(),
            source: error,
        });
    }

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
    fn test_translate_config_enum_serde_and_unknown_rejection() {
        let raw = r#"
[translate]
provider = "google_free"
target_language = "ja"
cover_mode = "frosted_glass"
"#;
        let cfg: Config = toml::from_str(raw).expect("强类型翻译配置应解析成功");
        assert_eq!(cfg.translate.provider, TranslateProvider::GoogleFree);
        assert_eq!(cfg.translate.target_language, TargetLanguage::Ja);
        assert_eq!(cfg.translate.cover_mode, CoverMode::FrostedGlass);

        let invalid = r#"
[translate]
provider = "deepl"
"#;
        assert!(
            toml::from_str::<Config>(invalid).is_err(),
            "未知翻译服务商必须显式报错，不能静默回退"
        );
    }

    #[test]
    fn test_load_or_create_new_file() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("sub_dir").join("config.toml");

        assert!(!config_file.exists());
        let loaded = load_or_create_config(Some(&config_file)).expect("加载默认配置失败");
        assert!(config_file.exists());
        assert_eq!(loaded, Config::default());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&config_file)
                .expect("读取配置文件元数据失败")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "配置文件必须仅当前用户可读写");
        }

        let temp_files: Vec<_> = std::fs::read_dir(config_file.parent().expect("父目录存在"))
            .expect("读取配置目录失败")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(temp_files.is_empty(), "原子写入后不得遗留临时文件");
    }
}
