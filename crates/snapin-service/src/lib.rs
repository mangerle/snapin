//! Snapin 服务层与多引擎调度系统。
//!
//! 提供基于 PP-OCRv6 的离线文字检测识别与五大多服务商（百度、网易有道、阿里、微软 Bing、谷歌）机器翻译集成。

pub mod error;
pub mod ocr;
pub mod translate;

pub use error::ServiceError;
pub use ocr::{load_character_dict, LocalOcrPaths, OcrEngine, OcrResult, OcrTextBlock};
pub use translate::{TranslateOptions, TranslateResult, TranslationItem, Translator};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_load_character_dict_from_txt() {
        let dir = tempdir().unwrap();
        let txt_path = dir.path().join("dict.txt");
        let mut file = std::fs::File::create(&txt_path).unwrap();
        writeln!(file, "你").unwrap();
        writeln!(file, "好").unwrap();
        writeln!(file, "世").unwrap();
        writeln!(file, "界").unwrap();

        let dict = load_character_dict(&txt_path).unwrap();
        assert_eq!(dict, vec!["你", "好", "世", "界"]);
    }

    #[test]
    fn test_load_character_dict_from_yml() {
        let dir = tempdir().unwrap();
        let yml_path = dir.path().join("inference.yml");
        let mut file = std::fs::File::create(&yml_path).unwrap();
        writeln!(file, "PostProcess:").unwrap();
        writeln!(file, "  character_dict:").unwrap();
        writeln!(file, "    - '0'").unwrap();
        writeln!(file, "    - 'A'").unwrap();
        writeln!(file, "    - '中'").unwrap();

        let dict = load_character_dict(&yml_path).unwrap();
        assert_eq!(dict, vec!["0", "A", "中"]);
    }
}
