//! ONNX Runtime 动态库发现、全局环境初始化与进程内单例管理。
//!
//! 通过系统安装的 `libonnxruntime.so` 动态加载推理后端，避免在二进制中静态捆绑数十兆的
//! 原生运行时。环境只能创建一次，因此使用全局 `OnceLock` 缓存并配合互斥锁防止并发重复初始化。
//! 若系统未安装运行时，初始化会返回带安装提示的领域错误，由上层决定禁用 OCR 或提示用户。

use crate::error::ServiceError;
use ort::environment::Environment;
use ort::logging::LogLevel;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

static ORT_ENVIRONMENT: OnceLock<Environment> = OnceLock::new();
static ORT_INIT_LOCK: Mutex<()> = Mutex::new(());

/// 获取进程内唯一的 ONNX Runtime 环境；首次调用时会动态加载系统运行时。
///
/// # Errors
///
/// 当系统中找不到 `libonnxruntime.so`、动态库加载失败或 ONNX Runtime 环境创建失败时，
/// 返回 [`ServiceError::InferenceError`]，并在错误信息中附带具体阶段与系统安装提示。
pub fn environment() -> Result<Environment, ServiceError> {
    if let Some(env) = ORT_ENVIRONMENT.get() {
        return Ok(env.clone());
    }

    // 锁中毒不应阻断初始化，直接取回内部守卫继续执行。
    let _guard = match ORT_INIT_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(env) = ORT_ENVIRONMENT.get() {
        return Ok(env.clone());
    }

    let library = discover_library()?;
    tracing::debug!("正在加载 ONNX Runtime 动态库: {:?}", library);

    let env = ort::init_from(&library)
        .map_err(|e| ServiceError::InferenceError {
            stage: "onnx_runtime_load".to_string(),
            reason: format!(
                "动态加载 ONNX Runtime 失败，库路径: {}，原因: {e}",
                library.display()
            ),
        })?
        .with_name("snapin-ocr")
        .with_telemetry(false)
        .with_logger(Arc::new(|level, category, _id, _location, message| {
            // ONNX Runtime 内部日志统一降级到 debug，避免污染用户终端；真正的推理失败由调用方记录。
            if matches!(level, LogLevel::Error | LogLevel::Fatal) {
                tracing::debug!("ONNX Runtime [{category}]: {message}");
            }
        }))
        .build()
        .map_err(|e| ServiceError::InferenceError {
            stage: "onnx_runtime_init".to_string(),
            reason: format!("创建 ONNX Runtime 环境失败: {e}"),
        })?;

    if ORT_ENVIRONMENT.set(env.clone()).is_err() {
        return ORT_ENVIRONMENT
            .get()
            .cloned()
            .ok_or_else(|| ServiceError::InferenceError {
                stage: "onnx_runtime_init".to_string(),
                reason: "ONNX Runtime 环境已存在但无法读取".to_string(),
            });
    }

    tracing::info!("ONNX Runtime 动态加载完成，库路径: {:?}", library);
    Ok(env)
}

/// 快速判断当前系统是否具备 ONNX Runtime 动态库（仅检查文件存在，不触发加载）。
pub fn is_runtime_available() -> bool {
    discover_library().is_ok()
}

/// 按优先级探测系统 ONNX Runtime 动态库路径。
fn discover_library() -> Result<PathBuf, ServiceError> {
    for variable in ["SNAPIN_ORT_DYLIB", "ORT_DYLIB_PATH"] {
        if let Ok(value) = std::env::var(variable) {
            let path = PathBuf::from(&value);
            if !value.contains('/') {
                // 允许直接传入 soname，由动态链接器在标准搜索路径中解析。
                return Ok(path);
            }
            if path.exists() {
                return Ok(path);
            }
        }
    }

    let mut search_dirs: Vec<PathBuf> = Vec::with_capacity(12);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            search_dirs.push(parent.to_path_buf());
            search_dirs.push(parent.join("lib"));
            search_dirs.push(parent.join("../lib/snapin"));
        }
    }

    search_dirs.extend([
        PathBuf::from("/usr/lib/snapin"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/lib"),
    ]);

    let multiarch = match std::env::consts::ARCH {
        "x86_64" => Some("x86_64-linux-gnu"),
        "aarch64" => Some("aarch64-linux-gnu"),
        "arm" => Some("arm-linux-gnueabihf"),
        _ => None,
    };
    if let Some(triplet) = multiarch {
        search_dirs.push(PathBuf::from(format!("/usr/lib/{triplet}")));
        search_dirs.push(PathBuf::from(format!("/lib/{triplet}")));
    }

    for dir in &search_dirs {
        if let Some(path) = find_library_in_dir(dir) {
            return Ok(path);
        }
    }

    Err(ServiceError::InferenceError {
        stage: "onnx_runtime_load".to_string(),
        reason: "未找到 libonnxruntime.so，请安装系统包 libonnxruntime1.23，或通过 SNAPIN_ORT_DYLIB 指定动态库路径"
            .to_string(),
    })
}

/// 在指定目录中查找 `libonnxruntime.so` 或带版本号的动态库。
fn find_library_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut exact: Option<PathBuf> = None;
    let mut versioned: Vec<(String, PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == "libonnxruntime.so" {
            exact = Some(entry.path());
        } else if name.starts_with("libonnxruntime.so.") {
            versioned.push((name.to_string(), entry.path()));
        }
    }

    if exact.is_some() {
        return exact;
    }

    versioned.sort_by(|left, right| right.0.cmp(&left.0));
    versioned.into_iter().next().map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_library_in_temp_dir_returns_none() {
        let temp = tempfile::tempdir().expect("创建临时目录失败");
        assert!(find_library_in_dir(temp.path()).is_none());
    }
}
