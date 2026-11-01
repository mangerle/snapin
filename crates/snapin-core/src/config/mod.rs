//! 配置管理模块门面。
//!
//! 负责配置文件的定位、自动创建默认配置、TOML 反序列化与安全保存。

pub mod defaults;
pub mod enums;
pub mod model;
pub mod sections;
pub mod types;

use crate::error::CoreError;
pub use model::{
    normalize_file_name_template, ColorFormat, Config, ConfigFile, CoverMode, DoubleClickAction,
    GeneralConfig, HotkeyConfig, ImageFormat, InterfaceConfig, LogLevel, OcrConfig, OutputConfig,
    PasterConfig, SnipConfig, TargetLanguage, TranslateConfig, TranslateProvider, ZoomAnchor,
    CONFIG_SCHEMA_VERSION,
};
use std::fs;
use std::path::{Path, PathBuf};
pub use types::{HexColor, Hotkey, HotkeyParseError, SecretString};

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
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let default_config = Config::default().sanitized();
        save_config(&default_config, &target_path)?;
        tracing::info!("未检测到配置文件，已初始化默认配置至: {:?}", target_path);
        return Ok(default_config);
    }

    let content = fs::read_to_string(&target_path).map_err(|e| CoreError::IoError {
        path: target_path.clone(),
        source: e,
    })?;

    let file: ConfigFile = match toml::from_str(&content) {
        Ok(file) => file,
        Err(error) => {
            // 配置损坏不应让所有依赖配置的进程永久无法启动：先保留原始文件，
            // 再写入默认配置并告警，用户仍可从 .bak 中手工恢复。
            return recover_invalid_config(&target_path, error);
        }
    };
    if !file.is_supported() {
        return Err(CoreError::UnsupportedConfigVersion {
            path: target_path.clone(),
            found: file.schema_version,
            supported: CONFIG_SCHEMA_VERSION,
        });
    }
    let config = file.config.sanitized();

    tighten_config_permissions(&target_path);
    tracing::debug!("成功加载配置文件: {:?}", target_path);
    Ok(config)
}

/// 备份损坏配置并写入默认配置。
///
/// 备份失败时不覆盖原文件，宁可把错误向上抛出，也不能在未保留用户数据的情况下
/// 用默认值静默替换包含翻译密钥的配置。
fn recover_invalid_config(
    target_path: &Path,
    parse_error: toml::de::Error,
) -> Result<Config, CoreError> {
    let backup_path = target_path.with_extension("toml.bak");
    fs::rename(target_path, &backup_path).map_err(|error| CoreError::IoError {
        path: target_path.to_path_buf(),
        source: error,
    })?;

    let default_config = Config::default().sanitized();
    save_config(&default_config, target_path)?;
    tracing::warn!(
        "配置文件解析失败，已备份至 {:?} 并重建默认配置，原因: {}",
        backup_path,
        parse_error
    );
    Ok(default_config)
}

/// 将已存在配置文件的权限收紧为 0600，避免历史版本创建的 0644 文件泄露 API 密钥。
fn tighten_config_permissions(path: &Path) {
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
pub fn save_config(config: &Config, target_path: &Path) -> Result<(), CoreError> {
    // 保存前再次收敛外部输入，保证 UI 手工输入与历史配置都不会把越界值写回磁盘。
    let sanitized = config.clone().sanitized();
    let toml_string = toml::to_string_pretty(&ConfigFile::current(sanitized)).map_err(|error| {
        CoreError::ConfigSerializeError {
            reason: error.to_string(),
        }
    })?;

    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| CoreError::IoError {
                path: parent.to_path_buf(),
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
            path: temp_path.clone(),
            source: error,
        });
    }

    if let Err(error) = fs::rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(CoreError::IoError {
            path: target_path.to_path_buf(),
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

    /// 保存时必须写入 schema 版本，加载时能识别并拒绝未来版本。
    #[test]
    fn test_schema_version_is_written_and_newer_version_is_rejected() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        let config = Config::default();
        save_config(&config, &config_file).expect("保存配置失败");

        let raw = std::fs::read_to_string(&config_file).expect("读取配置失败");
        assert!(
            raw.contains(&format!("schema_version = {CONFIG_SCHEMA_VERSION}")),
            "配置文件必须写入当前 schema 版本，实际内容: {raw}"
        );

        std::fs::write(
            &config_file,
            format!(
                "schema_version = {}\n[general]\n",
                CONFIG_SCHEMA_VERSION + 1
            ),
        )
        .expect("写入未来版本配置失败");
        let error = load_or_create_config(Some(&config_file)).expect_err("未来版本必须被拒绝");
        assert!(matches!(error, CoreError::UnsupportedConfigVersion { .. }));
    }

    /// 损坏配置必须保留 .bak 并重建默认配置，不能阻断所有依赖配置的进程启动。
    #[test]
    fn test_corrupt_config_is_backed_up_and_replaced() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        std::fs::write(&config_file, "this is not toml = [").expect("写入损坏配置失败");

        let loaded = load_or_create_config(Some(&config_file)).expect("损坏配置应降级为默认值");
        assert_eq!(loaded, Config::default());
        assert!(
            config_file.with_extension("toml.bak").exists(),
            "损坏配置必须先备份再重建"
        );
        let rebuilt = std::fs::read_to_string(&config_file).expect("重建配置应存在");
        assert!(rebuilt.contains("schema_version"));
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

#[cfg(test)]
mod sanitize_tests {
    use super::*;

    /// 越界的数值配置必须在加载边界收敛，避免进入渲染与编码路径后触发异常。
    #[test]
    fn test_sanitized_clamps_out_of_range_values() {
        let mut config = Config::default();
        config.general.update_check_interval_hours = 0;
        config.interface.toolbar_scale = 0.0;
        config.interface.magnifier_scale = 0;
        config.snip.default_stroke_width = 0.0;
        config.snip.default_font_size = 1000.0;
        config.paster.initial_opacity = 5.0;
        config.output.jpeg_quality = 200;
        config.output.quick_save_dir = "   ".to_string();
        config.output.file_name_template = String::new();
        config.general.custom_update_url = Some("   ".to_string());

        let config = config.sanitized();
        assert_eq!(config.general.update_check_interval_hours, 1);
        assert_eq!(config.interface.toolbar_scale, 0.5);
        assert_eq!(config.interface.magnifier_scale, 1);
        assert_eq!(config.snip.default_stroke_width, 1.0);
        assert_eq!(config.snip.default_font_size, 96.0);
        assert_eq!(config.paster.initial_opacity, 1.0);
        assert_eq!(config.output.jpeg_quality, 100);
        assert_eq!(config.output.quick_save_dir, "~/Pictures/Screenshots");
        assert_eq!(
            config.output.file_name_template,
            "Snapin_{YYYY}{MM}{DD}_{HH}{mm}{ss}"
        );
        assert!(config.general.custom_update_url.is_none());
    }

    /// 非有限浮点值必须回退到安全默认值，不能把 NaN 传入 clamp 或布局计算。
    #[test]
    fn test_sanitized_replaces_non_finite_floats() {
        let mut config = Config::default();
        config.interface.toolbar_scale = f32::NAN;
        config.snip.default_stroke_width = f32::INFINITY;
        config.paster.initial_opacity = f32::NEG_INFINITY;

        let config = config.sanitized();
        assert_eq!(config.interface.toolbar_scale, 1.0);
        assert_eq!(config.snip.default_stroke_width, 4.0);
        assert_eq!(config.paster.initial_opacity, 1.0);
    }
}
