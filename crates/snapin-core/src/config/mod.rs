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
    normalize_file_name_template, BeautifyConfig, ColorFormat, Config, ConfigFile, CoverMode,
    DoubleClickAction, GeneralConfig, HotkeyConfig, ImageFormat, InterfaceConfig, LogLevel,
    OcrConfig, OutputConfig, PasterConfig, SnipConfig, TargetLanguage, TranslateConfig,
    TranslateProvider, UiLanguage, ZoomAnchor, CONFIG_SCHEMA_VERSION,
};
use std::fs;
use std::path::{Path, PathBuf};
pub use types::{HexColor, Hotkey, HotkeyParseError, SecretString};

/// 获取 Snapin 默认配置文件存储路径：优先 `$XDG_CONFIG_HOME/snapin/config.toml`，兜底 `~/.config/snapin/config.toml`
pub fn get_default_config_path() -> Result<PathBuf, CoreError> {
    Ok(crate::paths::AppPaths::default_config_file())
}

/// 加载配置文件；若文件不存在，则自动初始化并保存一套默认配置到磁盘。
///
/// # Errors
///
/// 文件读取失败、TOML 语法或字段非法、schema 版本过新，以及重复快捷键等语义
/// 校验失败时返回 [`CoreError`]。语法损坏会先备份再重建；语义错误则保留原文件
/// 并拒绝继续，避免把用户可修复的配置静默替换成默认值。
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

    let file = match parse_config_file(&content) {
        Ok(file) => file,
        Err(reason) => {
            // 配置损坏或含未知字段时先保留原始文件，再重建默认配置并告警，
            // 用户仍可从 .bak 中手工恢复。
            return recover_invalid_config(&target_path, &reason);
        }
    };
    let file = file.migrate(&target_path)?;
    let config = file.config.sanitized();
    config.validate()?;

    tighten_config_permissions(&target_path);
    tracing::debug!("成功加载配置文件: {:?}", target_path);
    Ok(config)
}

/// 严格解析导入的配置文本，不修改源文件，也不在失败时创建备份或默认配置。
///
/// # Errors
///
/// 未知字段、非法快捷键、未来 schema 版本或语义校验失败时返回 [`CoreError`]。
pub fn parse_imported_config(content: &str, source_path: &Path) -> Result<Config, CoreError> {
    let file = parse_config_file(content).map_err(|reason| CoreError::ConfigParseError {
        path: source_path.to_path_buf(),
        reason,
    })?;
    let config = file.migrate(source_path)?.config.sanitized();
    config.validate()?;
    Ok(config)
}

/// 解析配置并检查未知字段。
///
/// `ConfigFile` 使用 `flatten` 兼容扁平板块布局，但 `serde` 的
/// `deny_unknown_fields` 与 flatten 组合不可靠，因此用 `serde_ignored` 显式
/// 收集未知键；拼写错误或未来版本字段会被当成损坏配置备份并重建，而不是静默忽略。
fn parse_config_file(content: &str) -> Result<ConfigFile, String> {
    // flatten 会把顶层未知板块吞进 Config，serde_ignored 未必能回报；先按 TOML
    // 表结构做一次顶层键白名单检查，再把 schema_version 摘掉后直接反序列化 Config，
    // 让 serde_ignored 能发现嵌套未知字段。
    const KNOWN_TOP_LEVEL: &[&str] = &[
        "schema_version",
        "general",
        "interface",
        "snip",
        "paster",
        "output",
        "hotkeys",
        "ocr",
        "translate",
    ];
    let raw: toml::Value =
        toml::from_str(content).map_err(|error| format!("TOML 解析失败: {error}"))?;
    let mut table = raw
        .as_table()
        .cloned()
        .ok_or_else(|| "配置文件顶层必须是 TOML 表".to_string())?;

    let mut unknown_fields: Vec<String> = Vec::new();
    for key in table.keys() {
        if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
            unknown_fields.push(key.clone());
        }
    }
    if !unknown_fields.is_empty() {
        return Err(format!(
            "配置文件包含未知字段: {}",
            unknown_fields.join(", ")
        ));
    }

    let schema_version = match table.remove("schema_version") {
        None => 0,
        Some(value) => value
            .as_integer()
            .ok_or_else(|| "schema_version 必须是整数".to_string())?,
    };
    let schema_version = u32::try_from(schema_version)
        .map_err(|_| format!("schema_version 超出 u32 范围: {schema_version}"))?;

    let config_toml = toml::to_string(&toml::Value::Table(table))
        .map_err(|error| format!("配置规范化失败: {error}"))?;
    let deserializer = toml::de::Deserializer::new(&config_toml);
    let mut unknown_fields = Vec::new();
    let config: Config =
        serde_ignored::deserialize(deserializer, |path| unknown_fields.push(path.to_string()))
            .map_err(|error| format!("TOML 解析失败: {error}"))?;
    if !unknown_fields.is_empty() {
        return Err(format!(
            "配置文件包含未知字段: {}",
            unknown_fields.join(", ")
        ));
    }

    Ok(ConfigFile {
        schema_version,
        config,
    })
}

/// 备份损坏配置并写入默认配置。
///
/// 备份失败时不覆盖原文件，宁可把错误向上抛出，也不能在未保留用户数据的情况下
/// 用默认值静默替换包含翻译密钥的配置。
fn recover_invalid_config(target_path: &Path, reason: &str) -> Result<Config, CoreError> {
    let backup_path = target_path.with_extension("toml.bak");
    fs::rename(target_path, &backup_path).map_err(|error| CoreError::IoError {
        path: target_path.to_path_buf(),
        source: error,
    })?;

    let default_config = Config::default().sanitized();
    save_config(&default_config, target_path)?;
    tracing::warn!(
        "配置文件解析失败，已备份至 {:?} 并重建默认配置，原因: {reason}",
        backup_path
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
    sanitized.validate()?;
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

    /// 未知字段必须在解析边界报错，避免用户拼写错误被静默忽略。
    #[test]
    fn test_unknown_config_fields_are_rejected() {
        let unknown_section =
            parse_config_file("schema_version = 1\n[unknown_section]\nvalue = 1\n")
                .expect_err("未知板块必须报错");
        assert!(unknown_section.contains("unknown_section"));

        let unknown_field = parse_config_file(
            "schema_version = 1\n[general]\nautostart = true\nunknown_field = 1\n",
        )
        .expect_err("未知字段必须报错");
        assert!(unknown_field.contains("unknown_field"));
    }

    /// 语义校验必须拒绝重复快捷键与非法更新源，且不能把它们静默写回磁盘。
    #[test]
    fn test_validate_rejects_duplicate_hotkeys_and_invalid_url() {
        let mut config = Config::default();
        config.hotkeys.paste = config.hotkeys.snip.clone();
        assert!(matches!(
            config.validate(),
            Err(CoreError::ValidationError { .. })
        ));

        let mut config = Config::default();
        config.general.custom_update_url = Some("ftp://example.com/latest.json".to_string());
        assert!(matches!(
            config.validate(),
            Err(CoreError::ValidationError { .. })
        ));

        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        let mut duplicate = Config::default();
        duplicate.hotkeys.paste = duplicate.hotkeys.snip.clone();
        assert!(save_config(&duplicate, &config_file).is_err());
        assert!(!config_file.exists(), "校验失败不得写出配置文件");
    }

    /// 已有配置进入系统前必须完成语义校验，不能把重复快捷键带入运行期。
    #[test]
    fn test_load_rejects_semantically_invalid_config_without_rewriting() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        let original = format!(
            "schema_version = {CONFIG_SCHEMA_VERSION}\n[hotkeys]\nsnip = \"Ctrl+Alt+A\"\ntranslate = \"Ctrl+Alt+A\"\n"
        );
        std::fs::write(&config_file, &original).expect("写入重复快捷键配置失败");

        let error =
            load_or_create_config(Some(&config_file)).expect_err("重复快捷键必须在加载边界被拒绝");

        assert!(matches!(error, CoreError::ValidationError { .. }));
        assert_eq!(
            std::fs::read_to_string(&config_file).expect("读取原配置失败"),
            original,
            "语义校验失败不得改写用户原配置"
        );
    }

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

    /// 无版本号的历史配置必须按 v0 迁移，版本字段类型错误则必须拒绝而不是冒充当前版本。
    #[test]
    fn test_missing_schema_version_defaults_to_zero_and_non_integer_is_rejected() {
        let legacy =
            parse_config_file("[general]\nlanguage = \"zh_cn\"\n").expect("无版本号配置应可解析");
        assert_eq!(legacy.schema_version, 0, "旧配置必须从 schema v0 开始迁移");
        let migrated = legacy
            .migrate(Path::new("legacy.toml"))
            .expect("旧配置应迁移到当前版本");
        assert_eq!(migrated.schema_version, CONFIG_SCHEMA_VERSION);

        let error = parse_config_file("schema_version = \"3\"\n[general]\n")
            .expect_err("字符串版本号必须被拒绝");
        assert!(error.contains("schema_version 必须是整数"));
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

    /// 导入解析不得改写源文件，且必须严格拒绝非法语义。
    #[test]
    fn test_parse_imported_config_is_strict_and_non_destructive() {
        let path = Path::new("/tmp/imported-config.toml");
        let config = parse_imported_config(
            "schema_version = 1\n[hotkeys]\nsnip = \"Ctrl+Alt+A\"\n",
            path,
        )
        .expect("合法导入配置应解析成功");
        assert_eq!(config.hotkeys.snip.to_display_string(), "Ctrl+Alt+A");

        let error = parse_imported_config(
            "schema_version = 1\n[hotkeys]\nsnip = \"Ctrl+Alt+A\"\ntranslate = \"Ctrl+Alt+A\"\n",
            path,
        )
        .expect_err("重复快捷键必须在导入边界拒绝");
        assert!(error.to_string().contains("快捷键重复"));
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
