//! 剪贴板历史持久化。
//!
//! 历史索引使用 JSON，图像保存为独立 PNG 文件。默认最多保留 10 条和 1 天，
//! 不加密；所有文件位于 Snapin 私有配置目录。

use crate::error::StorageError;
use crate::raw_image::MAX_RAW_PIXELS;
use image::{ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use snapin_foundation::AppPaths;
use std::fs::OpenOptions;
use std::io::{BufWriter, Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 剪贴板历史最大条数。
pub const CLIPBOARD_HISTORY_LIMIT: usize = 10;
/// 剪贴板历史保留秒数。
pub const CLIPBOARD_HISTORY_TTL_SECS: u64 = 24 * 60 * 60;
/// 剪贴板历史索引允许读取的最大字节数。
const MAX_CLIPBOARD_INDEX_BYTES: usize = 1024 * 1024;
/// 单张剪贴板历史图像允许读取的最大字节数。
const MAX_CLIPBOARD_IMAGE_FILE_BYTES: usize = 64 * 1024 * 1024;
/// 剪贴板历史图像允许的最大宽度与高度。
const MAX_CLIPBOARD_IMAGE_SIDE: u32 = 16_384;

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
    let (id, file_name) = save_clipboard_image(&dir, image)?;
    let image_path = validated_image_path(&dir, &file_name)?;
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
    match trim_and_save(dir.clone(), entries) {
        Ok(entries) => Ok(entries),
        Err(error) => {
            let _ = std::fs::remove_file(image_path);
            Err(error)
        }
    }
}

/// 读取条目对应的图像文件。
pub fn load_clipboard_image(entry: &ClipboardEntry) -> Result<RgbaImage, StorageError> {
    load_clipboard_image_from(&clipboard_history_dir(), entry)
}

fn load_clipboard_image_from(
    dir: &Path,
    entry: &ClipboardEntry,
) -> Result<RgbaImage, StorageError> {
    let file_name = entry
        .image_file
        .as_deref()
        .ok_or_else(|| StorageError::InvalidSavePath {
            path: "<missing>".to_string(),
            message: "图像历史缺少文件名".to_string(),
        })?;
    let path = validated_image_path(dir, file_name)?;
    let bytes = snapin_foundation::read_file_limited(&path, MAX_CLIPBOARD_IMAGE_FILE_BYTES)
        .map_err(|source| StorageError::Io {
            path: path.display().to_string(),
            source,
        })?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| StorageError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(format!("识别剪贴板图像格式失败: {error}")),
        })?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_CLIPBOARD_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_CLIPBOARD_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_RAW_PIXELS.saturating_mul(4));
    reader.limits(limits);
    reader
        .decode()
        .map(|image| image.to_rgba8())
        .map_err(|error| StorageError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(format!("解码剪贴板图像失败: {error}")),
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
    let content = match snapin_foundation::read_utf8_file_limited(&index, MAX_CLIPBOARD_INDEX_BYTES)
    {
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
    if let Err(source) = snapin_foundation::AppPaths::replace_file_atomically(&temp, &index) {
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
    entries.retain(|entry| entry.created_at >= cutoff && entry_payload_is_valid(entry, dir));
}

fn remove_orphan_images(dir: &Path, entries: &[ClipboardEntry]) {
    let keep: Vec<PathBuf> = entries
        .iter()
        .filter_map(|entry| {
            entry
                .image_file
                .as_deref()
                .and_then(|file| validated_image_path(dir, file).ok())
        })
        .collect();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for image in read_dir.flatten() {
        let path = image.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("png")
            && !keep.contains(&path)
        {
            let _ = std::fs::remove_file(image.path());
        }
    }
}

/// 在历史目录内生成唯一图像文件，并以私有临时文件完成编码后原子提交。
fn save_clipboard_image(dir: &Path, image: &RgbaImage) -> Result<(u64, String), StorageError> {
    std::fs::create_dir_all(dir).map_err(|source| StorageError::Io {
        path: dir.display().to_string(),
        source,
    })?;

    let id = AppPaths::generate_unique_id();
    let file_name = format!("{id}.png");
    let target = validated_image_path(dir, &file_name)?;
    let temp = dir.join(format!(
        ".{file_name}.{}.tmp",
        AppPaths::generate_unique_id()
    ));

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = match options.open(&temp) {
        Ok(file) => file,
        Err(source) => {
            return Err(StorageError::Io {
                path: temp.display().to_string(),
                source,
            })
        }
    };
    let mut writer = BufWriter::new(file);
    let encoded = image
        .write_to(&mut writer, ImageFormat::Png)
        .map_err(|error| StorageError::Io {
            path: temp.display().to_string(),
            source: std::io::Error::other(format!("编码剪贴板图像失败: {error}")),
        })
        .and_then(|()| {
            writer.flush().map_err(|source| StorageError::Io {
                path: temp.display().to_string(),
                source,
            })
        })
        .and_then(|()| {
            writer
                .get_ref()
                .sync_all()
                .map_err(|source| StorageError::Io {
                    path: temp.display().to_string(),
                    source,
                })
        });
    drop(writer);
    if let Err(error) = encoded {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }

    if let Err(source) = AppPaths::replace_file_atomically(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(StorageError::Io {
            path: target.display().to_string(),
            source,
        });
    }
    Ok((id, file_name))
}

/// 校验历史条目载荷与其类型一致，防止损坏索引把文本和图像字段同时带入业务层。
fn entry_payload_is_valid(entry: &ClipboardEntry, dir: &Path) -> bool {
    match entry.kind {
        ClipboardEntryKind::Text | ClipboardEntryKind::Color => {
            entry.image_file.is_none() && entry.text.as_deref().is_some_and(|text| !text.is_empty())
        }
        ClipboardEntryKind::Image => {
            entry.text.is_none()
                && entry.image_file.as_deref().is_some_and(|file| {
                    validated_image_path(dir, file).is_ok_and(|path| path.is_file())
                })
        }
    }
}

/// 只接受历史目录下的单段 `.png` 文件名，彻底阻断损坏索引构造的路径穿越。
fn validated_image_path(dir: &Path, file_name: &str) -> Result<PathBuf, StorageError> {
    let relative = Path::new(file_name);
    let mut components = relative.components();
    let single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    let is_png = relative
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("png");
    if !single_normal_component || !is_png {
        return Err(StorageError::InvalidSavePath {
            path: file_name.to_string(),
            message: "剪贴板图像文件名必须是历史目录内的单段 .png 文件名".to_string(),
        });
    }
    Ok(dir.join(relative))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validated_image_path_rejects_traversal_and_nested_paths() {
        let dir = Path::new("/tmp/snapin-history");
        for invalid in [
            "../secret.png",
            "nested/image.png",
            "/tmp/secret.png",
            "image.jpg",
        ] {
            assert!(
                validated_image_path(dir, invalid).is_err(),
                "非法历史图像路径必须被拒绝: {invalid}"
            );
        }
        assert_eq!(
            validated_image_path(dir, "safe.png").expect("合法文件名应通过"),
            dir.join("safe.png")
        );
    }

    #[test]
    fn test_prune_removes_entry_with_traversing_image_path() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let mut entries = vec![ClipboardEntry {
            id: 1,
            kind: ClipboardEntryKind::Image,
            text: None,
            image_file: Some("../outside.png".to_string()),
            created_at: now_secs(),
        }];

        prune_expired(&mut entries, dir.path());

        assert!(entries.is_empty(), "路径穿越条目必须在读取边界被丢弃");
    }

    #[test]
    fn test_save_clipboard_image_creates_missing_directory() {
        let root = tempfile::tempdir().expect("应可创建测试目录");
        let history_dir = root.path().join("missing").join("clipboard-history");
        let image = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));

        let (_, file_name) =
            save_clipboard_image(&history_dir, &image).expect("首条图像应保存成功");
        let path = validated_image_path(&history_dir, &file_name).expect("生成路径必须合法");

        assert!(path.is_file(), "图像应写入新建的历史目录");
        assert_eq!(
            image::open(path).expect("保存的图像应可解码").to_rgba8(),
            image
        );
    }

    #[test]
    fn test_load_rejects_oversized_history_index() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        std::fs::write(
            dir.path().join("index.json"),
            vec![b' '; MAX_CLIPBOARD_INDEX_BYTES.saturating_add(1)],
        )
        .expect("应可写入超限索引");

        let error = load_and_prune(dir.path().to_path_buf()).expect_err("超限索引必须被拒绝");
        assert!(matches!(error, StorageError::Io { .. }));
    }

    #[test]
    fn test_load_rejects_oversized_clipboard_image_dimensions() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let image =
            RgbaImage::from_pixel(1, MAX_CLIPBOARD_IMAGE_SIDE + 1, image::Rgba([0, 0, 0, 255]));
        let (id, file_name) = save_clipboard_image(dir.path(), &image).expect("图像应可保存");
        let entry = ClipboardEntry {
            id,
            kind: ClipboardEntryKind::Image,
            text: None,
            image_file: Some(file_name),
            created_at: now_secs(),
        };

        assert!(load_clipboard_image_from(dir.path(), &entry).is_err());
    }

    #[test]
    fn test_load_rejects_oversized_clipboard_image_file() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("huge.png");
        let file = std::fs::File::create(&path).expect("应可创建测试文件");
        file.set_len((MAX_CLIPBOARD_IMAGE_FILE_BYTES as u64).saturating_add(1))
            .expect("应可扩展测试文件");
        let entry = ClipboardEntry {
            id: 1,
            kind: ClipboardEntryKind::Image,
            text: None,
            image_file: Some("huge.png".to_string()),
            created_at: now_secs(),
        };

        assert!(load_clipboard_image_from(dir.path(), &entry).is_err());
    }
}
