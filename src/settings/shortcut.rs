//! 键盘组合快捷键捕获、解析与标准化格式化模块。
//!
//! 支持单键 (F1~F12, PrintScreen) 与多键修饰组合 (Ctrl, Alt, Shift, Super)，
//! 输出兼容 GNOME 与 XDG 标准的规范化热键字符串。

/// 将 Slint 键盘事件参数格式化为规范的展示文本（如 "Ctrl+Alt+A"、"F1"、"Ctrl+F3"）
///
/// 若未按下有效主键或仅按下修饰键，则返回 `None`。
pub fn format_key_combination(
    ctrl: bool,
    alt: bool,
    shift: bool,
    super_key: bool,
    text: &str,
) -> Option<String> {
    let key_label = parse_key_label(text)?;
    Some(format_with_modifiers(
        ctrl, alt, shift, super_key, &key_label,
    ))
}

/// 解析按键原始输入并返回标准单键展示名称
fn parse_key_label(text: &str) -> Option<String> {
    if text == " " {
        return Some("Space".to_string());
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(f_key) = parse_f_key(trimmed) {
        return Some(f_key);
    }
    if let Some(special) = parse_special_key(trimmed) {
        return Some(special);
    }
    parse_character_key(trimmed)
}

/// 解析功能键 (F1~F12 等)
fn parse_f_key(s: &str) -> Option<String> {
    if s.len() >= 2
        && (s.starts_with('F') || s.starts_with('f'))
        && s[1..].chars().all(|c| c.is_ascii_digit())
    {
        Some(format!("F{}", &s[1..]))
    } else {
        None
    }
}

/// 解析特殊按键与符号键
fn parse_special_key(s: &str) -> Option<String> {
    let label = match s {
        "space" | "Space" => "Space",
        "\n" | "\r" | "Return" | "Enter" => "Enter",
        "\t" | "Tab" => "Tab",
        "PrintScreen" | "Print" | "SysRq" => "Print",
        "Pause" | "Break" => "Pause",
        "Insert" => "Insert",
        "Delete" => "Delete",
        "Home" => "Home",
        "End" => "End",
        "PageUp" | "pageup" => "PageUp",
        "PageDown" | "pagedown" => "PageDown",
        "Up" | "up" | "UpArrow" => "Up",
        "Down" | "down" | "DownArrow" => "Down",
        "Left" | "left" | "LeftArrow" => "Left",
        "Right" | "right" | "RightArrow" => "Right",
        "." | "dot" | "period" => ".",
        "," | "comma" => ",",
        "-" | "minus" => "-",
        "=" | "equal" => "=",
        "/" | "slash" => "/",
        ";" | "semicolon" => ";",
        "`" | "grave" => "`",
        "[" | "leftbracket" => "[",
        "]" | "rightbracket" => "]",
        "\\" | "backslash" => "\\",
        "'" | "apostrophe" => "'",
        _ => return None,
    };
    Some(label.to_string())
}

/// 解析单字符字母与数字键
fn parse_character_key(s: &str) -> Option<String> {
    if s.len() == 1 {
        let c = s.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(c.to_ascii_uppercase().to_string());
        }
        if c.is_ascii_digit() {
            return Some(c.to_string());
        }
    }
    None
}

/// 组装修饰键与主键字符串
fn format_with_modifiers(ctrl: bool, alt: bool, shift: bool, super_key: bool, key: &str) -> String {
    let mut parts = Vec::with_capacity(5);
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    if super_key {
        parts.push("Super");
    }
    parts.push(key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_key_combinations() {
        assert_eq!(
            format_key_combination(false, false, false, false, "F1").as_deref(),
            Some("F1")
        );
        assert_eq!(
            format_key_combination(true, false, false, false, "F3").as_deref(),
            Some("Ctrl+F3")
        );
        assert_eq!(
            format_key_combination(true, true, false, false, "a").as_deref(),
            Some("Ctrl+Alt+A")
        );
        assert_eq!(
            format_key_combination(true, false, true, false, "s").as_deref(),
            Some("Ctrl+Shift+S")
        );
        assert_eq!(
            format_key_combination(false, false, false, false, "Print").as_deref(),
            Some("Print")
        );
        assert!(format_key_combination(true, false, false, false, "").is_none());
        assert!(format_key_combination(true, false, false, false, "\u{0001}").is_none());
    }
}
