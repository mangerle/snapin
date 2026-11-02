//! 剪贴板历史持久化。
//!
//! 历史索引使用 JSON，图像保存为独立 PNG 文件。默认最多保留 10 条和 1 天，
//! 不加密；所有文件位于 Snapin 私有配置目录。

use crate::error::StorageError;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use snapin_core::AppPaths;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 剪贴板历史最大条数。
pub const CLIPBOARD_HISTORY_LIMIT: usize = 10;
/// 剪贴板历史保留秒数。
pub const CLIPBOARD_HISTORY_TTL_SECS: u64 = 24 * 60 * 60;

/// 剪贴板条目类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardEntryKind {
    Text,
    Image,
    Color,
}

/// 单条剪贴板历史。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: u64,
    pub kind: ClipboardEntryKind,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub image_file: Option<String>,
    pub created_at: u64,
}

/// 返回剪贴板历史目录。
pub fn clipboard_history_dir() -> PathBuf {
    AppPaths::config_dir().join("clipboard-history")
}

/// 读取并清理过期历史。
pub fn load_clipboard_history() -> Result<Vec<ClipboardEntry>, StorageError> {
    load_and_prune(clipboard_history_dir())
}

/// 追加文本或颜色条目。
pub fn append_clipboard_text(
    text: &str,
    kind: ClipboardEntryKind,
) -> Result<Vec<ClipboardEntry>, StorageError> {
    let dir = clipboard_history_dir();
    let mut entries = load_and_prune(dir.clone())?;
    if entries
        .first()
        .and_then(|entry| entry.text.as_deref())
        .is_some_and(|latest| latest == text)
    {
        return Ok(entries);
    }
    entries.insert(
        0,
        ClipboardEntry {
            id: AppPaths::generate_unique_id(),
            kind,
            text: Some(text.to_string()),
            image_file: None,
            created_at: now_secs(),
        },
    );
    trim_and_save(dir, entries)
}

/// 追加图像条目。
pub fn append_clipboard_image(image: &RgbaImage) -> Result<Vec<ClipboardEntry>, StorageError> {
    let dir = clipboard_history_dir();
    let mut entries = load_and_prune(dir.clone())?;
    let id = AppPaths::generate_unique_id();
    let file_name = format!("{id}.png");
    image
        .save(dir.join(&file_name))
        .map_err(|error| StorageError::Io {
            path: dir.join(&file_name).display().to_string(),
            source: std::io::Error::other(format!("保存剪贴板图像失败: {error}")),
        })?;
    entries.insert(
        0,
        ClipboardEntry {
            id,
            kind: ClipboardEntryKind::Image,
            text: None,
            image_file: Some(file_name),
            created_at: now_secs(),
        },
    );
    trim_and_save(dir, entries)
}

/// 读取条目对应的图像文件。
pub fn load_clipboard_image(entry: &ClipboardEntry) -> Result<RgbaImage, StorageError> {
    let path = clipboard_history_dir().join(entry.image_file.as_deref().ok_or_else(|| {
        StorageError::InvalidSavePath {
            path: "<missing>".to_string(),
            message: "图像历史缺少文件名".to_string(),
        }
    })?);
    image::open(&path)
        .map(|image| image.to_rgba8())
        .map_err(|error| StorageError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(format!("读取剪贴板图像失败: {error}")),
        })
}

/// 清空全部剪贴板历史。
pub fn clear_clipboard_history() -> Result<(), StorageError> {
    let dir = clipboard_history_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|source| StorageError::Io {
            path: dir.display().to_string(),
            source,
        })?;
    }
    Ok(())
}

fn load_and_prune(dir: PathBuf) -> Result<Vec<ClipboardEntry>, StorageError> {
    let index = dir.join("index.json");
    let content = match std::fs::read_to_string(&index) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(StorageError::Io {
                path: index.display().to_string(),
                source,
            })
        }
    };
    let mut entries: Vec<ClipboardEntry> =
        serde_json::from_str(&content).map_err(|error| StorageError::Io {
            path: index.display().to_string(),
            source: std::io::Error::other(format!("解析剪贴板历史失败: {error}")),
        })?;
    prune_expired(&mut entries, &dir);
    Ok(entries)
}

fn trim_and_save(
    dir: PathBuf,
    mut entries: Vec<ClipboardEntry>,
) -> Result<Vec<ClipboardEntry>, StorageError> {
    prune_expired(&mut entries, &dir);
    entries.truncate(CLIPBOARD_HISTORY_LIMIT);
    remove_orphan_images(&dir, &entries);
    std::fs::create_dir_all(&dir).map_err(|source| StorageError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let content = serde_json::to_string_pretty(&entries).map_err(|error| StorageError::Io {
        path: dir.join("index.json").display().to_string(),
        source: std::io::Error::other(format!("序列化剪贴板历史失败: {error}")),
    })?;
    let index = dir.join("index.json");
    let temp = dir.join(format!(".index.{}.tmp", AppPaths::generate_unique_id()));
    AppPaths::write_secure_file(&temp, content.as_bytes()).map_err(|source| StorageError::Io {
        path: temp.display().to_string(),
        source,
    })?;
    if let Err(source) = snapin_core::AppPaths::replace_file_atomically(&temp, &index) {
        let _ = std::fs::remove_file(&temp);
        return Err(StorageError::Io {
            path: index.display().to_string(),
            source,
        });
    }
    Ok(entries)
}

fn prune_expired(entries: &mut Vec<ClipboardEntry>, dir: &Path) {
    let cutoff = now_secs().saturating_sub(CLIPBOARD_HISTORY_TTL_SECS);
    entries.retain(|entry| entry.created_at >= cutoff);
    for entry in entries.iter_mut() {
        if let Some(file) = entry.image_file.as_deref() {
            if !dir.join(file).exists() {
                entry.created_at = 0;
            }
        }
    }
    entries.retain(|entry| entry.created_at >= cutoff);
}

fn remove_orphan_images(dir: &Path, entries: &[ClipboardEntry]) {
    let keep: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.image_file.as_deref())
        .collect();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for image in read_dir.flatten() {
        let name = image.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".png") && !keep.contains(&name.as_ref()) {
            let _ = std::fs::remove_file(image.path());
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
