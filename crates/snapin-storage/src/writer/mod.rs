//! 图像编码输出与防覆盖写盘模块。

use super::template::{expand_home_dir, format_template};
use crate::error::StorageError;
use chrono::Local;
use image::RgbaImage;
use snapin_config::config::{normalize_file_name_template, ImageFormat, OutputConfig};
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// 保存对话框建议的目标位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSuggestion {
    /// 建议文件名（含输出格式扩展名）
    pub file_name: String,
    /// 建议目录；目录可能尚不存在，需要落盘时由调用方显式创建
    pub directory: PathBuf,
}

impl SaveSuggestion {
    /// 建议目录与文件名拼接后的完整路径。
    pub fn path(&self) -> PathBuf {
        self.directory.join(&self.file_name)
    }
}

/// 计算保存对话框建议的文件名与目录，不创建目录、不写文件。
///
/// 默认文件名已被占用时追加递增序号（`_1`、`_2`），减少用户手动改名的次数；
/// 这个存在性检查只影响建议文案，真正的落盘由 [`save_image_with_config`] 以
/// `create_new` 原子预占，避免“检查后写入”的竞态。
pub fn suggest_save_path(cfg: &OutputConfig) -> SaveSuggestion {
    let directory = expand_home_dir(&cfg.quick_save_dir);
    let stem = rendered_file_name(cfg);
    let ext = cfg.default_format.extension();
    let mut counter = 0usize;
    loop {
        let file_name = if counter == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}_{counter}.{ext}")
        };
        if !directory.join(&file_name).exists() {
            return SaveSuggestion {
                file_name,
                directory,
            };
        }
        counter = counter.saturating_add(1);
        if counter == usize::MAX {
            return SaveSuggestion {
                file_name: format!("{stem}.{ext}"),
                directory,
            };
        }
    }
}

/// 确保快捷保存目录存在，并返回其绝对路径。
///
/// 目录创建从 [`suggest_save_path`] 中剥离为显式副作用入口，避免“生成建议”
/// 这类读路径在用户未确认保存前就改动文件系统。
///
/// # Errors
///
/// 目录创建失败或目标路径不是可写目录时返回 [`StorageError::Io`]。
pub fn ensure_output_directory(cfg: &OutputConfig) -> Result<PathBuf, StorageError> {
    let directory = expand_home_dir(&cfg.quick_save_dir);
    std::fs::create_dir_all(&directory).map_err(|error| StorageError::Io {
        path: directory.display().to_string(),
        source: error,
    })?;
    Ok(directory)
}

/// 按照用户配置格式将图像编码并写入磁盘
///
/// # Errors
///
/// 目标目录创建、图像编码、临时文件写入或原子替换失败时返回 [`StorageError`]。
pub fn save_image_with_config(
    img: &RgbaImage,
    cfg: &OutputConfig,
) -> Result<PathBuf, StorageError> {
    // 通过 create_new 原子抢占文件名，消除“先 exists 再写入”的 TOCTOU；
    // 编码先落到同目录临时文件，成功后再原子替换占位文件，失败不会留下半截图像。
    let out_path = reserve_output_path(cfg)?;
    if let Err(error) =
        encode_image_atomically(img, &out_path, cfg.default_format, cfg.jpeg_quality)
    {
        let _ = std::fs::remove_file(&out_path);
        return Err(error);
    }
    Ok(out_path)
}

/// 将图像保存到用户在“另存为”对话框中选择的路径。
///
/// 格式优先按目标扩展名推断。无扩展名时补齐全局默认格式的扩展名，但若补齐后的
/// 文件已存在则拒绝写入，因为 Portal 的覆盖确认针对的是用户最初选择的路径；
/// 不支持的扩展名直接报错，避免静默把文件改写成另一种格式。
///
/// # Errors
///
/// 目标目录创建、图像编码、临时文件写入或原子替换失败时返回 [`StorageError`]。
pub fn save_image_to_path(
    img: &RgbaImage,
    path: &Path,
    cfg: &OutputConfig,
) -> Result<PathBuf, StorageError> {
    let has_extension = path.extension().is_some();
    let format = match image_format_from_path(path) {
        Some(format) => format,
        None if !has_extension => cfg.default_format,
        None => {
            return Err(StorageError::InvalidSavePath {
                path: path.display().to_string(),
                message: "仅支持 .png、.jpg、.jpeg 或 .webp 扩展名".to_string(),
            });
        }
    };

    let out_path = if has_extension {
        path.to_path_buf()
    } else {
        path.with_extension(format.extension())
    };
    if !has_extension && out_path.exists() {
        return Err(StorageError::InvalidSavePath {
            path: out_path.display().to_string(),
            message: "自动补齐扩展名后的目标文件已存在，请重新选择文件名".to_string(),
        });
    }

    encode_image_atomically(img, &out_path, format, cfg.jpeg_quality)?;
    Ok(out_path)
}

/// 将文本导出到快捷保存目录，并生成不覆盖已有文件的唯一路径。
///
/// OCR 导出的内容可能包含用户截图中的敏感文字，因此文件使用 `create_new` 防止
/// 静默覆盖，并在 Unix 上以 0600 权限创建。写入失败会删除半成品文件。
///
/// # Errors
///
/// 扩展名不是 `md`/`json`、目录创建失败、文件已存在或写入失败时返回
/// [`StorageError`]。
pub fn save_text_with_config(
    content: &str,
    extension: &str,
    cfg: &OutputConfig,
) -> Result<PathBuf, StorageError> {
    let extension = extension.to_ascii_lowercase();
    if !matches!(extension.as_str(), "md" | "json") {
        return Err(StorageError::InvalidSavePath {
            path: extension,
            message: "OCR 文本仅支持 .md 或 .json 扩展名".to_string(),
        });
    }
    let base_dir = expand_home_dir(&cfg.quick_save_dir);
    std::fs::create_dir_all(&base_dir).map_err(|error| StorageError::Io {
        path: base_dir.display().to_string(),
        source: error,
    })?;
    create_unique_text_file(
        &base_dir,
        &format!("{}_ocr", rendered_file_name(cfg)),
        content,
        &extension,
    )
}

/// 在目标目录内使用 `create_new` 创建唯一文本文件并写入内容。
fn create_unique_text_file(
    base_dir: &Path,
    file_name: &str,
    content: &str,
    extension: &str,
) -> Result<PathBuf, StorageError> {
    for counter in 0usize.. {
        let candidate = if counter == 0 {
            base_dir.join(format!("{file_name}.{extension}"))
        } else {
            base_dir.join(format!("{file_name}_{counter}.{extension}"))
        };
        match open_new_private_file(&candidate) {
            Ok(mut file) => {
                if let Err(error) = std::io::Write::write_all(&mut file, content.as_bytes()) {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(StorageError::Io {
                        path: candidate.display().to_string(),
                        source: error,
                    });
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(StorageError::Io {
                    path: candidate.display().to_string(),
                    source: error,
                });
            }
        }
    }
    Err(StorageError::Io {
        path: base_dir.display().to_string(),
        source: std::io::Error::other("无法找到可用的 OCR 导出文件名"),
    })
}

/// 以仅创建模式打开文件，并在 Unix 上限制为仅当前用户可读写。
fn open_new_private_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// 原子预占一个不存在的输出路径，返回可写入的目标路径。
fn reserve_output_path(cfg: &OutputConfig) -> Result<PathBuf, StorageError> {
    let base_dir = ensure_output_directory(cfg)?;
    let file_name = rendered_file_name(cfg);
    let ext = cfg.default_format.extension();

    for counter in 0usize.. {
        let candidate = if counter == 0 {
            base_dir.join(format!("{file_name}.{ext}"))
        } else {
            base_dir.join(format!("{file_name}_{counter}.{ext}"))
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(StorageError::Io {
                    path: candidate.display().to_string(),
                    source: error,
                })
            }
        }
    }

    Err(StorageError::Io {
        path: base_dir.display().to_string(),
        source: std::io::Error::other("无法找到可用的输出文件名"),
    })
}

/// 提取经过模板渲染与路径净化后的基础文件名。
fn rendered_file_name(cfg: &OutputConfig) -> String {
    let template = normalize_file_name_template(&cfg.file_name_template);
    let rendered = format_template(&template, &Local::now());
    Path::new(&rendered)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Snapin")
        .to_string()
}

/// 根据文件扩展名推断受支持的图像格式。
fn image_format_from_path(path: &Path) -> Option<ImageFormat> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        _ => None,
    }
}

/// 先写同目录临时文件，再原子替换目标路径。
fn encode_image_atomically(
    img: &RgbaImage,
    out_path: &Path,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<(), StorageError> {
    let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = out_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("snapin");
    let temp_path = parent.join(format!(
        ".{stem}.{}.{}.{}",
        std::process::id(),
        snapin_foundation::AppPaths::generate_unique_id(),
        format.extension()
    ));

    let encoded = encode_image_to_path(img, &temp_path, format, jpeg_quality);
    if let Err(error) = encoded {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = snapin_foundation::AppPaths::replace_file_atomically(&temp_path, out_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(StorageError::Io {
            path: out_path.display().to_string(),
            source: error,
        });
    }
    Ok(())
}

/// 将图像按指定格式编码到目标路径。
fn encode_image_to_path(
    img: &RgbaImage,
    out_path: &Path,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<(), StorageError> {
    match format {
        ImageFormat::Jpeg => {
            let (w, h) = (img.width(), img.height());
            let mut rgb_bytes = Vec::with_capacity((w as usize) * (h as usize) * 3);
            for chunk in img.as_raw().as_chunks::<4>().0 {
                rgb_bytes.extend_from_slice(&chunk[0..3]);
            }
            let rgb =
                image::RgbImage::from_raw(w, h, rgb_bytes).ok_or_else(|| StorageError::Io {
                    path: out_path.display().to_string(),
                    source: std::io::Error::other("构建 RGB 图像缓冲区失败"),
                })?;
            let mut file = std::fs::File::create(out_path).map_err(|e| StorageError::Io {
                path: out_path.display().to_string(),
                source: e,
            })?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut file,
                jpeg_quality.clamp(1, 100),
            );
            encoder.encode_image(&rgb).map_err(|e| StorageError::Io {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
        ImageFormat::Png | ImageFormat::Webp => {
            img.save(out_path).map_err(|e| StorageError::Io {
                path: out_path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
    }

    Ok(())
}


#[cfg(test)]
mod tests;
