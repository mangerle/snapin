//! OCR 模型配置与多语言字符字典加载解析。

use crate::error::ServiceError;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// 文本识别模型的配置契约 (对应 inference.yml)
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInferenceConfig {
    #[serde(rename = "PostProcess", default)]
    pub post_process: Option<PostProcessConfig>,
}

/// 后处理配置
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostProcessConfig {
    #[serde(default)]
    pub character_dict: Vec<String>,
}

/// 本地模型路径清单
#[derive(Debug, Clone)]
pub struct LocalOcrPaths {
    /// 识别模型 onnx 权重路径
    pub rec_model: PathBuf,
    /// 识别模型配置文件或字典路径 (inference.yml / dict.txt)
    pub rec_config: PathBuf,
    /// 检测模型 onnx 权重路径
    pub det_model: PathBuf,
}

impl LocalOcrPaths {
    /// 在指定根目录中探测并定位模型文件
    pub fn discover_in_root(root: &Path) -> Option<Self> {
        // 1. 优先探测完整命名的识别模型，次选兼容旧版或子目录结构
        let rec_candidates = [
            root.join("ppocrv6_small_rec.onnx"),
            root.join("rec").join("ppocrv6_small_rec.onnx"),
            root.join("rec").join("inference.onnx"),
            root.join("ch_PP-OCRv6_small_rec.onnx"),
        ];
        let rec_model = rec_candidates.into_iter().find(|p| is_non_empty_file(p))?;

        // 2. 优先探测完整命名的检测模型
        let det_candidates = [
            root.join("ppocrv6_small_det.onnx"),
            root.join("det").join("ppocrv6_small_det.onnx"),
            root.join("ppocrv6_det.onnx"),
            root.join("det").join("inference.onnx"),
            root.join("ch_PP-OCRv6_small_det.onnx"),
        ];
        let det_model = det_candidates.into_iter().find(|p| is_non_empty_file(p))?;

        // 3. 优先探测识别模型对应的完整命名配置文件与字典
        let config_candidates = [
            root.join("ppocrv6_small_rec.yml"),
            root.join("ppocrv6_small_rec.json"),
            root.join("rec").join("ppocrv6_small_rec.yml"),
            root.join("rec").join("inference.yml"),
            root.join("rec").join("ppocrv6_dict.txt"),
            root.join("rec").join("inference.json"),
            root.join("inference.yml"),
            root.join("ppocrv6_dict.txt"),
        ];
        let rec_config = config_candidates
            .into_iter()
            .find(|p| is_non_empty_file(p))?;

        Some(Self {
            rec_model,
            rec_config,
            det_model,
        })
    }

    /// 按 XDG 规范三级回退链搜索并定位模型资产
    pub fn resolve_model_paths(custom_dir: Option<&Path>) -> Result<Self, ServiceError> {
        if let Some(custom) = custom_dir {
            return Self::discover_required(custom);
        }
        if let Ok(custom) = std::env::var("SNAPIN_OCR_MODEL_DIR") {
            return Self::discover_required(Path::new(&custom));
        }
        if let Some(paths) = Self::discover_first(&model_search_roots()) {
            return Ok(paths);
        }
        Err(ServiceError::ModelNotFound {
            path: "/usr/lib/snapin/models/ppocrv6 或 /usr/share/snapin/models/ppocrv6".to_string(),
        })
    }

    /// 严格解析显式目录；配置错误必须直接报错，不能静默回退。
    fn discover_required(root: &Path) -> Result<Self, ServiceError> {
        Self::discover_in_root(root).ok_or_else(|| ServiceError::ModelNotFound {
            path: root.to_string_lossy().to_string(),
        })
    }

    /// 返回首个包含完整模型资产的候选目录。
    fn discover_first(roots: &[PathBuf]) -> Option<Self> {
        roots.iter().find_map(|root| Self::discover_in_root(root))
    }
}

/// 模型目录搜索顺序：可执行文件旁、用户数据目录、系统安装目录。
fn model_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(5);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            roots.push(exe_dir.join("models").join("ppocrv6"));
            if let Some(resources) = exe_dir.parent() {
                roots.push(resources.join("Resources").join("models").join("ppocrv6"));
            }
        }
    }
    #[cfg(target_os = "windows")]
    let user_data_base = std::env::var("LOCALAPPDATA").map(PathBuf::from).ok();
    #[cfg(not(target_os = "windows"))]
    let user_data_base = std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".local").join("share"))
        .ok();
    if let Some(base) = user_data_base {
        roots.push(base.join("snapin").join("models").join("ppocrv6"));
    }
    roots.push(PathBuf::from("/usr/lib/snapin/models/ppocrv6"));
    roots.push(PathBuf::from("/usr/share/snapin/models/ppocrv6"));
    roots
}

/// 模型资产必须是存在、非目录且非空的常规文件。
fn is_non_empty_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

/// 加载字符字典 (支持从 inference.yml 或 dict.txt 自动适配)
pub fn load_character_dict(config_path: &Path) -> Result<Vec<String>, ServiceError> {
    let content = read_dictionary_text(config_path)?;
    if config_path.extension().and_then(|value| value.to_str()) == Some("txt") {
        return Ok(content.lines().map(str::to_string).collect());
    }
    if let Some(dict) = parse_yaml_character_dict(&content) {
        return Ok(dict);
    }
    if let Some(dict) = parse_json_character_dict(&content) {
        return Ok(dict);
    }
    Err(ServiceError::ConfigParseError {
        path: config_path.to_string_lossy().to_string(),
        reason: "未能从配置文件中解析出有效的 character_dict 字符列表".to_string(),
    })
}

/// 读取并校验模型字典文本。
fn read_dictionary_text(config_path: &Path) -> Result<String, ServiceError> {
    if !config_path.exists() {
        return Err(ServiceError::ModelNotFound {
            path: config_path.to_string_lossy().to_string(),
        });
    }
    let content = fs::read_to_string(config_path).map_err(|error| ServiceError::IoError {
        path: config_path.to_string_lossy().to_string(),
        source: error,
    })?;
    if content.trim().is_empty() {
        return Err(ServiceError::ConfigParseError {
            path: config_path.to_string_lossy().to_string(),
            reason: "模型字典文件为空".to_string(),
        });
    }
    Ok(content)
}

/// 从 YAML 推理配置中提取字符字典。
fn parse_yaml_character_dict(content: &str) -> Option<Vec<String>> {
    let config = serde_yaml::from_str::<ModelInferenceConfig>(content).ok()?;
    config
        .post_process
        .map(|post_process| post_process.character_dict)
        .filter(|dict| !dict.is_empty())
}

/// 从 JSON 推理配置中提取字符字典。
fn parse_json_character_dict(content: &str) -> Option<Vec<String>> {
    #[derive(Deserialize)]
    struct JsonDictWrapper {
        #[serde(alias = "character_dict", alias = "character_list", default)]
        character_dict: Vec<String>,
    }
    serde_json::from_str::<JsonDictWrapper>(content)
        .ok()
        .map(|config| config.character_dict)
        .filter(|dict| !dict.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录、空文件与缺失文件都不能被当作模型资产。
    #[test]
    fn test_discover_in_root_rejects_invalid_assets() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        assert!(LocalOcrPaths::discover_in_root(dir.path()).is_none());

        let rec_dir = dir.path().join("ppocrv6_small_rec.onnx");
        std::fs::create_dir(&rec_dir).expect("创建同名目录失败");
        assert!(LocalOcrPaths::discover_in_root(dir.path()).is_none());

        std::fs::remove_dir(&rec_dir).expect("删除目录失败");
        std::fs::write(&rec_dir, b"").expect("写入空文件失败");
        assert!(LocalOcrPaths::discover_in_root(dir.path()).is_none());

        std::fs::write(&rec_dir, b"model").expect("写入模型占位内容失败");
        std::fs::write(dir.path().join("ppocrv6_small_det.onnx"), b"model")
            .expect("写入检测模型占位内容失败");
        std::fs::write(dir.path().join("ppocrv6_small_rec.yml"), b"PostProcess:\n")
            .expect("写入配置占位内容失败");
        assert!(LocalOcrPaths::discover_in_root(dir.path()).is_some());
    }
}
