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
        let rec_model = rec_candidates.into_iter().find(|p| p.exists())?;

        // 2. 优先探测完整命名的检测模型
        let det_candidates = [
            root.join("ppocrv6_small_det.onnx"),
            root.join("det").join("ppocrv6_small_det.onnx"),
            root.join("ppocrv6_det.onnx"),
            root.join("det").join("inference.onnx"),
            root.join("ch_PP-OCRv6_small_det.onnx"),
        ];
        let det_model = det_candidates.into_iter().find(|p| p.exists())?;

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
        let rec_config = config_candidates.into_iter().find(|p| p.exists())?;

        Some(Self {
            rec_model,
            rec_config,
            det_model,
        })
    }

    /// 按照 XDG 规范三级回退链搜索并定位模型资产
    pub fn resolve_model_paths(custom_dir: Option<&Path>) -> Result<Self, ServiceError> {
        // 1. 自定义显式指定路径：显式配置无效时必须直接报错，禁止静默回退到其他目录。
        if let Some(custom) = custom_dir {
            return Self::discover_in_root(custom).ok_or_else(|| ServiceError::ModelNotFound {
                path: custom.to_string_lossy().to_string(),
            });
        }

        // 2. 开发与调试环境变量覆盖
        if let Ok(custom) = std::env::var("SNAPIN_OCR_MODEL_DIR") {
            let dir = PathBuf::from(custom);
            if let Some(paths) = Self::discover_in_root(&dir) {
                return Ok(paths);
            }
            return Err(ServiceError::ModelNotFound {
                path: dir.to_string_lossy().to_string(),
            });
        }

        // 3. 用户数据目录: ~/.local/share/snapin/models/ppocrv6
        if let Ok(home) = std::env::var("HOME") {
            let user_dir = Path::new(&home)
                .join(".local")
                .join("share")
                .join("snapin")
                .join("models")
                .join("ppocrv6");
            if let Some(paths) = Self::discover_in_root(&user_dir) {
                return Ok(paths);
            }
        }

        // 3. 系统级标准安装目录: /usr/share/snapin/models/ppocrv6
        let sys_dir = Path::new("/usr/share/snapin/models/ppocrv6");
        if let Some(paths) = Self::discover_in_root(sys_dir) {
            return Ok(paths);
        }

        Err(ServiceError::ModelNotFound {
            path: custom_dir
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/usr/share/snapin/models/ppocrv6".to_string()),
        })
    }
}

/// 加载字符字典 (支持从 inference.yml 或 dict.txt 自动适配)
pub fn load_character_dict(config_path: &Path) -> Result<Vec<String>, ServiceError> {
    if !config_path.exists() {
        return Err(ServiceError::ModelNotFound {
            path: config_path.to_string_lossy().to_string(),
        });
    }

    let content = fs::read_to_string(config_path).map_err(|e| ServiceError::IoError {
        path: config_path.to_string_lossy().to_string(),
        source: e,
    })?;

    // 若为纯文本字典 (每行一个字符)
    if config_path.extension().and_then(|s| s.to_str()) == Some("txt") {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        return Ok(lines);
    }

    // 尝试按 YAML 格式解析 inference.yml
    if let Ok(cfg) = serde_yaml::from_str::<ModelInferenceConfig>(&content) {
        if let Some(pp) = cfg.post_process {
            if !pp.character_dict.is_empty() {
                return Ok(pp.character_dict);
            }
        }
    }

    // 尝试按 JSON 格式解析 inference.json
    #[derive(Deserialize)]
    struct JsonDictWrapper {
        #[serde(alias = "character_dict", alias = "character_list", default)]
        character_dict: Vec<String>,
    }

    if let Ok(json_cfg) = serde_json::from_str::<JsonDictWrapper>(&content) {
        if !json_cfg.character_dict.is_empty() {
            return Ok(json_cfg.character_dict);
        }
    }

    Err(ServiceError::ConfigParseError {
        path: config_path.to_string_lossy().to_string(),
        reason: "未能从配置文件中解析出有效的 character_dict 字符列表".to_string(),
    })
}
