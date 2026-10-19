//! 强类型配置值模型（Parse, Don't Validate）。
//!
//! 在配置反序列化边界预先校验并解析十六进制颜色与键盘快捷键，
//! 内部系统流转完全基于强类型数据，消除运行期热点循环内的字符串重复解析。

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use tiny_skia::Color;

/// 强类型 HEX 色彩结构体
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl HexColor {
    /// 构造新的 RGBA 强类型色彩
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// 从十六进制字符串（如 "#00AEFF"、"#FFF"、"00AEFF"、"#00AEFFAA"）安全解析
    pub fn from_hex(s: &str) -> Option<Self> {
        let trimmed = s.trim().trim_start_matches('#');
        match trimmed.len() {
            3 => {
                let r = u8::from_str_radix(&trimmed[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&trimmed[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&trimmed[2..3], 16).ok()? * 17;
                Some(Self::new(r, g, b, 255))
            }
            6 => {
                let r = u8::from_str_radix(&trimmed[0..2], 16).ok()?;
                let g = u8::from_str_radix(&trimmed[2..4], 16).ok()?;
                let b = u8::from_str_radix(&trimmed[4..6], 16).ok()?;
                Some(Self::new(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&trimmed[0..2], 16).ok()?;
                let g = u8::from_str_radix(&trimmed[2..4], 16).ok()?;
                let b = u8::from_str_radix(&trimmed[4..6], 16).ok()?;
                let a = u8::from_str_radix(&trimmed[6..8], 16).ok()?;
                Some(Self::new(r, g, b, a))
            }
            _ => None,
        }
    }

    /// 转换为大写十六进制颜色字符串（如 "#00AEFF"）
    pub fn to_hex_string(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// 转换为 RGB 三元组
    pub const fn to_rgb(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    /// 转换为 RGBA 四元组
    pub const fn to_rgba(self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }

    /// 转换为 tiny_skia::Color 实例
    pub fn to_tiny_skia_color(self) -> Color {
        Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

impl fmt::Display for HexColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex_string())
    }
}

impl FromStr for HexColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s).ok_or_else(|| format!("无效的十六进制颜色格式: {s}"))
    }
}

impl Serialize for HexColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex_string())
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).ok_or_else(|| de::Error::custom(format!("无法解析十六进制颜色: {s}")))
    }
}

/// 强类型键盘快捷键组合结构体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: String,
}

impl Hotkey {
    /// 构造新的强类型快捷键
    pub fn new(ctrl: bool, alt: bool, shift: bool, meta: bool, key: impl Into<String>) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            meta,
            key: key.into(),
        }
    }

    /// 解析可读文本为强类型结构（如 "Ctrl+Alt+A"、"F1"、"Ctrl+F3"）
    pub fn parse_str(s: &str) -> Self {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "已禁用" || trimmed == "none" {
            return Self::new(false, false, false, false, "");
        }

        let parts: Vec<&str> = trimmed.split('+').map(|p| p.trim()).collect();
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut meta = false;

        let main_key = parts.last().copied().unwrap_or("");
        for &m in &parts[..parts.len().saturating_sub(1)] {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "super" | "win" | "meta" => meta = true,
                _ => {}
            }
        }

        Self::new(ctrl, alt, shift, meta, main_key)
    }

    /// 转换为人类可读的标准展示文本（如 "Ctrl+Alt+A"）
    pub fn to_display_string(&self) -> String {
        if self.key.is_empty() {
            return "已禁用".to_string();
        }
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.meta {
            parts.push("Super");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }

    /// 转换为 GNOME / GTK Accelerator 标准格式（如 "<Control><Alt>a"、"F1"）
    pub fn to_gnome_accelerator(&self) -> String {
        if self.key.is_empty() {
            return String::new();
        }

        let mut res = String::new();
        if self.ctrl {
            res.push_str("<Control>");
        }
        if self.alt {
            res.push_str("<Alt>");
        }
        if self.shift {
            res.push_str("<Shift>");
        }
        if self.meta {
            res.push_str("<Super>");
        }

        let formatted_key = match self.key.as_str() {
            k if k.len() == 1 && k.chars().next().unwrap_or(' ').is_ascii_alphabetic() => {
                k.to_ascii_lowercase()
            }
            "Space" | "space" => "space".to_string(),
            "PageUp" | "pageup" => "Page_Up".to_string(),
            "PageDown" | "pagedown" => "Page_Down".to_string(),
            other => other.to_string(),
        };

        res.push_str(&formatted_key);
        res
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

impl FromStr for Hotkey {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse_str(s))
    }
}

impl Serialize for Hotkey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_display_string())
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse_str(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct ColorHolder {
        color: HexColor,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct HotkeyHolder {
        hotkey: Hotkey,
    }

    #[test]
    fn test_hex_color_parsing_and_serde() {
        let col = HexColor::from_hex("#00AEFF").unwrap();
        assert_eq!(col.to_rgb(), (0, 174, 255));
        assert_eq!(col.to_hex_string(), "#00AEFF");

        let col_short = HexColor::from_hex("#FFF").unwrap();
        assert_eq!(col_short.to_rgb(), (255, 255, 255));

        let holder = ColorHolder { color: col };
        let toml_str = toml::to_string(&holder).unwrap();
        assert!(toml_str.contains("\"#00AEFF\""));
        let decoded: ColorHolder = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded, holder);
    }

    #[test]
    fn test_hotkey_parsing_and_gnome_conversion() {
        let hk = Hotkey::parse_str("Ctrl+Alt+A");
        assert!(hk.ctrl && hk.alt && !hk.shift);
        assert_eq!(hk.key, "A");
        assert_eq!(hk.to_gnome_accelerator(), "<Control><Alt>a");

        let f1 = Hotkey::parse_str("F1");
        assert_eq!(f1.to_gnome_accelerator(), "F1");

        let holder = HotkeyHolder { hotkey: hk };
        let toml_str = toml::to_string(&holder).unwrap();
        let decoded: HotkeyHolder = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded, holder);
    }
}
