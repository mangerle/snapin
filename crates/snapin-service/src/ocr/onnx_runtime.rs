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

    let env = build_environment(&library)?;

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

/// 从指定动态库构造 ONNX Runtime 环境并配置统一日志策略。
fn build_environment(library: &Path) -> Result<Environment, ServiceError> {
    ort::init_from(library)
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
        })
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
            // 裸库名交给动态链接器搜索（Linux soname / Windows PATH / macOS dyld）。
            if !value.contains(['/', '\\']) {
                return Ok(path);
            }
            if path.exists() {
                return Ok(path);
            }
        }
    }

    let search_dirs = runtime_search_dirs();
    for dir in &search_dirs {
        if let Some(path) = find_library_in_dir(dir) {
            return Ok(path);
        }
    }
    Err(ServiceError::InferenceError {
        stage: "onnx_runtime_load".to_string(),
        reason: runtime_missing_reason().to_string(),
    })
}

/// 构建当前平台的运行时动态库搜索目录。
fn runtime_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::with_capacity(12);
    dirs.extend(executable_search_dirs());
    dirs.extend(platform_search_dirs());
    dirs
}

/// 程序可执行文件相邻目录，覆盖随包分发和开发构建目录。
fn executable_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(5);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
            dirs.push(parent.join("lib"));
            dirs.push(parent.join("../lib/snapin"));
            #[cfg(target_os = "macos")]
            {
                dirs.push(parent.join("../Frameworks"));
                dirs.push(parent.join("../Resources"));
            }
        }
    }
    dirs
}

/// 当前操作系统约定的动态库搜索目录。
fn platform_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(8);
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("snapin").join("lib"));
        }
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs.extend([
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/opt/homebrew/lib"),
            PathBuf::from("/usr/lib"),
        ]);
    }

    #[cfg(target_os = "linux")]
    {
        dirs.extend([
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
            dirs.push(PathBuf::from(format!("/usr/lib/{triplet}")));
            dirs.push(PathBuf::from(format!("/lib/{triplet}")));
        }
    }

    dirs
}

/// 当前平台缺失运行时时的安装提示。
fn runtime_missing_reason() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "未找到 onnxruntime.dll，请将 ONNX Runtime 放入程序目录或 PATH，或通过 SNAPIN_ORT_DYLIB 指定动态库路径"
    }
    #[cfg(target_os = "macos")]
    {
        "未找到 libonnxruntime.dylib，请通过 Homebrew 安装 onnxruntime，或通过 SNAPIN_ORT_DYLIB 指定动态库路径"
    }
    #[cfg(target_os = "linux")]
    {
        "未找到 libonnxruntime.so，请安装系统包 libonnxruntime1.23，或通过 SNAPIN_ORT_DYLIB 指定动态库路径"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "当前平台不支持 ONNX Runtime 动态库发现"
    }
}

/// 是否为当前平台的无版本动态库名。
fn is_exact_library_name(name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        name.eq_ignore_ascii_case("onnxruntime.dll")
    }
    #[cfg(target_os = "macos")]
    {
        name == "libonnxruntime.dylib" || name == "libonnxruntime.so"
    }
    #[cfg(target_os = "linux")]
    {
        name == "libonnxruntime.so"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = name;
        false
    }
}

/// 提取带版本动态库中的数字版本段，按组件比较而非字典序比较。
fn parse_library_version(name: &str) -> Option<Vec<u64>> {
    #[cfg(target_os = "windows")]
    let version = name
        .strip_prefix("onnxruntime")
        .or_else(|| name.strip_prefix("ONNXRUNTIME"))
        .and_then(|value| {
            value
                .strip_suffix(".dll")
                .or_else(|| value.strip_suffix(".DLL"))
        })?;
    #[cfg(target_os = "macos")]
    let version = name
        .strip_prefix("libonnxruntime.")?
        .strip_suffix(".dylib")?;
    #[cfg(target_os = "linux")]
    let version = name.strip_prefix("libonnxruntime.so.")?;
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let version = {
        let _ = name;
        return None;
    };

    let components: Option<Vec<u64>> = version
        .split('.')
        .map(|component| component.parse::<u64>().ok())
        .collect();
    components.filter(|components| !components.is_empty())
}

/// 在指定目录中查找当前平台的无版本或带版本动态库，带版本按名称降序优先。
fn find_library_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut exact: Option<PathBuf> = None;
    let mut versioned: Vec<(Vec<u64>, PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if is_exact_library_name(&name) {
            exact = Some(entry.path());
        } else if let Some(version) = parse_library_version(&name) {
            versioned.push((version, entry.path()));
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

    /// 当前平台必须能识别自己的动态库命名，且不会把其他平台名称误判。
    #[test]
    fn test_platform_library_names() {
        #[cfg(target_os = "windows")]
        {
            assert!(is_exact_library_name("onnxruntime.dll"));
            assert!(is_exact_library_name("ONNXRUNTIME.DLL"));
            assert!(parse_library_version("onnxruntime1.23.dll").is_some());
            assert!(!is_exact_library_name("libonnxruntime.so"));
        }
        #[cfg(target_os = "macos")]
        {
            assert!(is_exact_library_name("libonnxruntime.dylib"));
            assert!(parse_library_version("libonnxruntime.1.23.dylib").is_some());
            assert!(!is_exact_library_name("onnxruntime.dll"));
        }
        #[cfg(target_os = "linux")]
        {
            assert!(is_exact_library_name("libonnxruntime.so"));
            assert!(parse_library_version("libonnxruntime.so.1.23").is_some());
            assert!(!is_exact_library_name("onnxruntime.dll"));
        }
    }

    /// 无版本库优先于带版本库，带版本按数字组件降序选择。
    #[test]
    fn test_find_library_prefers_exact_then_newest_version() {
        let temp = tempfile::tempdir().expect("创建临时目录失败");
        let (old, new, exact) = if cfg!(target_os = "windows") {
            (
                "onnxruntime1.22.dll",
                "onnxruntime1.23.dll",
                "onnxruntime.dll",
            )
        } else if cfg!(target_os = "macos") {
            (
                "libonnxruntime.1.22.dylib",
                "libonnxruntime.1.23.dylib",
                "libonnxruntime.dylib",
            )
        } else {
            (
                "libonnxruntime.so.1.22",
                "libonnxruntime.so.1.23",
                "libonnxruntime.so",
            )
        };
        std::fs::File::create(temp.path().join(old)).expect("创建旧版本库");
        std::fs::File::create(temp.path().join(new)).expect("创建新版本库");
        assert_eq!(
            find_library_in_dir(temp.path()),
            Some(temp.path().join(new))
        );
        std::fs::File::create(temp.path().join(exact)).expect("创建无版本库");
        assert_eq!(
            find_library_in_dir(temp.path()),
            Some(temp.path().join(exact))
        );
    }

    #[test]
    fn test_version_selection_is_numeric_not_lexicographic() {
        let temp = tempfile::tempdir().expect("创建临时目录失败");
        let (old, new) = if cfg!(target_os = "windows") {
            ("onnxruntime1.9.dll", "onnxruntime1.23.dll")
        } else if cfg!(target_os = "macos") {
            ("libonnxruntime.1.9.dylib", "libonnxruntime.1.23.dylib")
        } else {
            ("libonnxruntime.so.1.9", "libonnxruntime.so.1.23")
        };
        std::fs::File::create(temp.path().join(old)).expect("创建旧版本库");
        std::fs::File::create(temp.path().join(new)).expect("创建新版本库");

        assert_eq!(
            find_library_in_dir(temp.path()),
            Some(temp.path().join(new))
        );
    }
}
