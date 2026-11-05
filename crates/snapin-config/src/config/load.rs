//! 配置文件读取、严格反序列化与 schema 迁移入口。

use super::model::{config_file_top_level_keys, Config, ConfigFile};
use super::persist::{save_config, tighten_config_permissions};
use crate::error::CoreError;
use std::fs;
use std::path::{Path, PathBuf};

/// 配置文件允许读取的最大字节数，防止异常文件一次性耗尽内存。
pub(super) const MAX_CONFIG_FILE_BYTES: usize = 1024 * 1024;

/// 获取 Snapin 默认配置文件存储路径：优先 `$XDG_CONFIG_HOME/snapin/config.toml`，兜底 `~/.config/snapin/config.toml`
///
/// # Errors
///
/// 路径解析、语义校验、序列化或原子持久化失败时返回 [`CoreError`]；当前默认路径实现本身不会失败。
pub fn get_default_config_path() -> Result<PathBuf, CoreError> {
    Ok(crate::paths::AppPaths::default_config_file())
}

/// 加载配置文件；若文件不存在，则自动初始化并保存一套默认配置到磁盘。
///
/// # Errors
///
/// 文件读取失败、TOML 语法或字段非法、schema 版本过新，以及重复快捷键等语义
/// 校验失败时返回 [`CoreError`]。已存在的无效配置始终保留原文件并拒绝继续，
/// 不自动替换为默认值，避免用户偏好和翻译凭据被静默清除。
pub fn load_or_create_config(custom_path: Option<&Path>) -> Result<Config, CoreError> {
    if custom_path.is_none() {
        crate::paths::AppPaths::ensure_config_dir().map_err(|source| CoreError::IoError {
            path: crate::paths::AppPaths::config_dir(),
            source,
        })?;
    }
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

    let content = snapin_foundation::read_utf8_file_limited(&target_path, MAX_CONFIG_FILE_BYTES)
        .map_err(|e| CoreError::IoError {
            path: target_path.clone(),
            source: e,
        })?;

    let file = parse_config_file(&content).map_err(|reason| CoreError::ConfigParseError {
        path: target_path.clone(),
        reason,
    })?;
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
pub(super) fn parse_config_file(content: &str) -> Result<ConfigFile, String> {
    let mut table = parse_toml_table(content)?;
    reject_unknown_top_level_keys(&table)?;
    let schema_version = take_schema_version(&mut table)?;
    let config = deserialize_config_strict(table)?;
    Ok(ConfigFile {
        schema_version,
        config,
    })
}

/// 把原始 TOML 解析为可编辑表。
fn parse_toml_table(content: &str) -> Result<toml::Table, String> {
    let raw: toml::Value =
        toml::from_str(content).map_err(|error| format!("TOML 解析失败: {error}"))?;
    raw.as_table()
        .cloned()
        .ok_or_else(|| "配置文件顶层必须是 TOML 表".to_string())
}

/// 检查顶层板块白名单，防止 flatten 吞掉未知板块。
fn reject_unknown_top_level_keys(table: &toml::Table) -> Result<(), String> {
    let allowed = config_file_top_level_keys();
    let unknown_fields: Vec<String> = table
        .keys()
        .filter(|key| !allowed.iter().any(|candidate| candidate == *key))
        .cloned()
        .collect();
    if !unknown_fields.is_empty() {
        return Err(format!(
            "配置文件包含未知字段: {}",
            unknown_fields.join(", ")
        ));
    }
    Ok(())
}

/// 取出并校验 schema_version；缺失时按历史 v0 处理。
fn take_schema_version(table: &mut toml::Table) -> Result<u32, String> {
    let schema_version = match table.remove("schema_version") {
        None => 0,
        Some(value) => value
            .as_integer()
            .ok_or_else(|| "schema_version 必须是整数".to_string())?,
    };
    u32::try_from(schema_version)
        .map_err(|_| format!("schema_version 超出 u32 范围: {schema_version}"))
}

/// 在保留嵌套字段检查的前提下把表反序列化为强类型配置。
fn deserialize_config_strict(table: toml::Table) -> Result<Config, String> {
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
    Ok(config)
}
