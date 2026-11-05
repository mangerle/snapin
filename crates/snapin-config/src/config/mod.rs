//! 配置管理模块门面。
//!
//! 负责配置文件的定位、自动创建默认配置、TOML 反序列化与安全保存。

pub mod defaults;
pub mod enums;
mod load;
pub mod model;
mod persist;
pub mod sections;
pub mod types;

pub use load::{get_default_config_path, load_or_create_config, parse_imported_config};
pub use model::{
    normalize_file_name_template, BeautifyBackground, BeautifyConfig, ColorFormat, Config,
    ConfigFile, CoverMode, DoubleClickAction, GeneralConfig, HotkeyConfig, ImageFormat,
    InterfaceConfig, LogLevel, OcrConfig, OutputConfig, PasterConfig, SnipConfig, SocialSize,
    TargetLanguage, TranslateConfig, TranslateProvider, UiLanguage, ZoomAnchor,
    CONFIG_SCHEMA_VERSION,
};
pub use persist::save_config;
pub use types::{HexColor, Hotkey, HotkeyParseError, SecretString};

#[cfg(test)]
mod tests {
    use super::load::{parse_config_file, MAX_CONFIG_FILE_BYTES};
    use super::*;
    use crate::error::CoreError;
    use std::path::Path;
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

    /// 无效 TOML 必须保留原文件并返回解析错误，不得静默替换为默认配置。
    #[test]
    fn test_invalid_config_is_preserved_without_rewrite() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        let original = "this is not toml = [";
        std::fs::write(&config_file, original).expect("写入损坏配置失败");

        let error = load_or_create_config(Some(&config_file)).expect_err("损坏配置必须显式报错");

        assert!(matches!(error, CoreError::ConfigParseError { .. }));
        assert_eq!(
            std::fs::read_to_string(&config_file).expect("读取原配置失败"),
            original,
            "解析失败不得改写用户原配置"
        );
        assert!(
            !config_file.with_extension("toml.bak").exists(),
            "保留原文件时不得额外生成备份副本"
        );
    }

    /// 未知字段必须保留原文件并返回可定位错误，由用户显式修复。
    #[test]
    fn test_unknown_field_config_is_preserved_without_rewrite() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        let original = "schema_version = 1\n[general]\nunknown_field = 1\n";
        std::fs::write(&config_file, original).expect("写入未知字段配置失败");

        let error = load_or_create_config(Some(&config_file)).expect_err("未知字段必须显式报错");

        assert!(matches!(error, CoreError::ConfigParseError { .. }));
        assert_eq!(
            std::fs::read_to_string(&config_file).expect("读取原配置失败"),
            original,
            "未知字段不得触发默认配置重建"
        );
    }

    /// 已保存的完整配置必须可以再次加载，所有顶层板块都不得被误判为未知字段。
    #[test]
    fn test_saved_config_roundtrip_preserves_all_sections() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config_file = temp_dir.path().join("config.toml");
        let mut config = Config::default();
        config.beautify.corner_radius = 33.0;
        config.general.update_check_interval_hours = 12;

        save_config(&config, &config_file).expect("保存配置失败");
        let loaded = load_or_create_config(Some(&config_file)).expect("重新加载配置失败");

        assert_eq!(loaded, config.sanitized());
        assert!(
            !config_file.with_extension("toml.bak").exists(),
            "合法配置不得被备份后重建"
        );
    }

    /// 导出的配置文本必须能够被导入流程完整解析，不能因顶层板块白名单漂移而失败。
    #[test]
    fn test_exported_config_can_be_imported() {
        let config = Config::default();
        let exported = toml::to_string_pretty(&ConfigFile::current(config.clone()))
            .expect("序列化导出配置失败");
        let imported = parse_imported_config(&exported, Path::new("exported-config.toml"))
            .expect("导出配置应可重新导入");

        assert_eq!(imported, config.sanitized());
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

    #[test]
    fn test_load_rejects_oversized_config_file() {
        let dir = tempdir().expect("应可创建临时目录");
        let config_file = dir.path().join("config.toml");
        std::fs::write(
            &config_file,
            vec![b' '; MAX_CONFIG_FILE_BYTES.saturating_add(1)],
        )
        .expect("应可写入超限配置");

        let error = load_or_create_config(Some(&config_file)).expect_err("超限配置必须被拒绝");
        assert!(matches!(error, CoreError::IoError { .. }));
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
