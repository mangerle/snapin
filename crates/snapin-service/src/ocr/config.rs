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
        let rec_model = root.join("rec").join("inference.onnx");
        let det_model = root.join("det").join("inference.onnx");

        if !rec_model.exists() || !det_model.exists() {
            // 兼容扁平目录结构
            let flat_rec = root.join("ppocrv6_small_rec.onnx");
            let flat_det = root.join("ppocrv6_det.onnx");
            if flat_rec.exists() && flat_det.exists() {
                let rec_cfg = root.join("inference.yml");
                return Some(Self {
                    rec_model: flat_rec,
                    rec_config: rec_cfg,
                    det_model: flat_det,
                });
            }
            return None;
        }

        // 优先探测 yml，其次探测 txt 或 json
        let rec_yml = root.join("rec").join("inference.yml");
        let rec_dict = root.join("rec").join("ppocrv6_dict.txt");
        let rec_config = if rec_yml.exists() {
            rec_yml
        } else if rec_dict.exists() {
            rec_dict
        } else {
            root.join("rec").join("inference.json")
        };

        Some(Self {
            rec_model,
            rec_config,
            det_model,
        })
    }

    /// 按照 XDG 规范三级回退链搜索并定位模型资产
    pub fn resolve_model_paths(custom_dir: Option<&Path>) -> Result<Self, ServiceError> {
        // 1. 自定义显式指定路径
        if let Some(custom) = custom_dir {
            if let Some(paths) = Self::discover_in_root(custom) {
                return Ok(paths);
            }
        }

        // 2. 用户数据目录: ~/.local/share/snapin/models/ppocrv6
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
