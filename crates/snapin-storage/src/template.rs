//! 路径与文件名时间模板解析辅助模块。

use chrono::Local;
use snapin_core::AppPaths;
use std::path::PathBuf;

/// 将路径开头的 `~` 展开为真实用户主目录，兼容 Unix `/` 与 Windows `\` 分隔符。
pub(crate) fn expand_home_dir(path_str: &str) -> PathBuf {
    let Some(home) = AppPaths::home_dir() else {
        return PathBuf::from(path_str);
    };
    if path_str == "~" {
        return home;
    }
    if let Some(stripped) = path_str
        .strip_prefix("~/")
        .or_else(|| path_str.strip_prefix("~\\"))
    {
        return home.join(stripped);
    }
    PathBuf::from(path_str)
}

/// 格式化时间戳占位符模板
pub(crate) fn format_template(template: &str, dt: &chrono::DateTime<Local>) -> String {
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
        let Some(home) = AppPaths::home_dir() else {
            return;
        };
        assert_eq!(
            expand_home_dir("~/Pictures/Screenshots"),
            home.join("Pictures/Screenshots")
        );
        assert_eq!(
            expand_home_dir(r"~\Pictures\Screenshots"),
            home.join("Pictures\\Screenshots")
        );
        assert_eq!(expand_home_dir("~"), home);
    }
}
