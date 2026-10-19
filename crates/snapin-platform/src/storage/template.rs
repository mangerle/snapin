//! 路径与文件名时间模板解析辅助模块。

use chrono::Local;
use std::path::PathBuf;

/// 将路径中的 ~ 符号安全展开为真实用户主目录
pub fn expand_home_dir(path_str: &str) -> PathBuf {
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    } else if path_str == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path_str)
}

/// 格式化时间戳占位符模板
pub fn format_template(template: &str, dt: &chrono::DateTime<Local>) -> String {
    template
        .replace("{YYYY}", &dt.format("%Y").to_string())
        .replace("{MM}", &dt.format("%m").to_string())
        .replace("{DD}", &dt.format("%d").to_string())
        .replace("{HH}", &dt.format("%H").to_string())
        .replace("{mm}", &dt.format("%M").to_string())
        .replace("{ss}", &dt.format("%S").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_template() {
        let dt = Local::now();
        let res = format_template("Snapin_{YYYY}{MM}{DD}", &dt);
        let y = dt.format("%Y").to_string();
        assert!(res.starts_with(&format!("Snapin_{y}")));
    }

    #[test]
    fn test_expand_home_dir() {
        let orig = "~/Pictures/Screenshots";
        let expanded = expand_home_dir(orig);
        if let Ok(home) = std::env::var("HOME") {
            assert!(expanded.starts_with(home));
        }
    }
}
