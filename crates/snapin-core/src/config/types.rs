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
    ///
    /// 解析完全基于 ASCII 字节进行，任何非 ASCII 字符都会直接返回 `None`，
    /// 避免按字节索引 UTF-8 字符串时落入字符中间导致 panic。
    pub fn from_hex(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
        let bytes = hex.as_bytes();

        match bytes.len() {
            3 => {
                let r = parse_hex_nibble(bytes[0])? * 17;
                let g = parse_hex_nibble(bytes[1])? * 17;
                let b = parse_hex_nibble(bytes[2])? * 17;
                Some(Self::new(r, g, b, 255))
            }
            6 => {
                let r = parse_hex_pair(bytes[0], bytes[1])?;
                let g = parse_hex_pair(bytes[2], bytes[3])?;
                let b = parse_hex_pair(bytes[4], bytes[5])?;
                Some(Self::new(r, g, b, 255))
            }
            8 => {
                let r = parse_hex_pair(bytes[0], bytes[1])?;
                let g = parse_hex_pair(bytes[2], bytes[3])?;
                let b = parse_hex_pair(bytes[4], bytes[5])?;
                let a = parse_hex_pair(bytes[6], bytes[7])?;
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

/// 解析单个 ASCII 十六进制半字节，非 ASCII 或非法字符返回 `None`
fn parse_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// 解析两个 ASCII 十六进制半字节组合为一个字节
fn parse_hex_pair(high: u8, low: u8) -> Option<u8> {
    Some(parse_hex_nibble(high)? * 16 + parse_hex_nibble(low)?)
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

/// 快捷键文本解析失败原因
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyParseError {
    /// 修饰键位置出现无法识别的令牌
    #[error(
        "快捷键 \"{binding}\" 中的 \"{token}\" 不是可识别的修饰键，仅支持 Ctrl/Alt/Shift/Super"
    )]
    UnknownModifier { binding: String, token: String },

    /// 只有修饰键或分隔符，缺少实际主键
    #[error("快捷键 \"{binding}\" 缺少主键，全局热键必须落在具体功能键或字符键上")]
    MissingMainKey { binding: String },

    /// 主键不在 GNOME 全局快捷键支持的按键集合内
    #[error(
        "快捷键 \"{binding}\" 的主键 \"{key}\" 不受支持，可用 F1-F24、字母、数字、空格及常用功能键"
    )]
    UnsupportedMainKey { binding: String, key: String },
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

    /// 解析人类可读文本为强类型结构（如 "Ctrl+Alt+A"、"F1"、"Ctrl+F3"）。
    ///
    /// 设计原理：旧实现静默丢弃无法识别的修饰键并接纳任意主键字符串，用户在设置中心录错键
    /// 或手工写坏 config.toml 时，系统会注册一个 GNOME 永远不会触发的空绑定，表现为
    /// "热键按了没反应"的静默失败。改为在配置边界一次性完成词法与合法性校验并显式报错，
    /// 让调用方能够保留旧值并给出中文提示（Parse, Don't Validate）。
    pub fn parse_str(s: &str) -> Result<Self, HotkeyParseError> {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "已禁用" || trimmed.eq_ignore_ascii_case("none") {
            return Ok(Self::new(false, false, false, false, ""));
        }

        let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
        let raw_key = parts.last().copied().unwrap_or("");
        if raw_key.is_empty() {
            return Err(HotkeyParseError::MissingMainKey {
                binding: trimmed.to_string(),
            });
        }

        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut meta = false;
        for &token in &parts[..parts.len().saturating_sub(1)] {
            let lowered = token.to_lowercase();
            match lowered.as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "super" | "win" | "meta" => meta = true,
                _ => {
                    return Err(HotkeyParseError::UnknownModifier {
                        binding: trimmed.to_string(),
                        token: token.to_string(),
                    })
                }
            }
        }

        let key =
            normalize_main_key(raw_key).ok_or_else(|| HotkeyParseError::UnsupportedMainKey {
                binding: trimmed.to_string(),
                key: raw_key.to_string(),
            })?;

        Ok(Self {
            ctrl,
            alt,
            shift,
            meta,
            key,
        })
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
}

/// 将原始主键文本归一化为标准展示名，不受支持的按键返回 `None`。
///
/// 接受集合与设置中心按键录制器输出的词表保持一致，并额外兼容 GNOME 侧常见别名，
/// 使用户手工编辑的配置文件同样能通过校验。
fn normalize_main_key(raw: &str) -> Option<String> {
    let token = raw.trim();
    if token.is_empty() {
        return None;
    }

    // 功能键 F1~F24（GNOME 自定义快捷键上限）
    if let Some(digits) = token.strip_prefix(['F', 'f']) {
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let index = digits.parse::<u32>().ok()?;
            return (1..=24).contains(&index).then(|| format!("F{index}"));
        }
    }

    // 单字符键：字母统一大写展示，数字与可打印 ASCII 符号原样保留
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphabetic() {
            return Some(c.to_ascii_uppercase().to_string());
        }
        if c.is_ascii_graphic() || c == ' ' {
            return Some(c.to_string());
        }
    }

    let label = match token.to_lowercase().as_str() {
        "space" => "Space",
        "enter" | "return" => "Enter",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "print" | "printscreen" | "sysrq" => "Print",
        "pause" | "break" => "Pause",
        "insert" => "Insert",
        "delete" | "del" => "Delete",
        "home" => "Home",
        "end" => "End",
        "pageup" | "page_up" => "PageUp",
        "pagedown" | "page_down" => "PageDown",
        "up" | "uparrow" => "Up",
        "down" | "downarrow" => "Down",
        "left" | "leftarrow" => "Left",
        "right" | "rightarrow" => "Right",
        "menu" => "Menu",
        _ => return None,
    };
    Some(label.to_string())
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

impl FromStr for Hotkey {
    type Err = HotkeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s)
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
        Self::parse_str(&s).map_err(de::Error::custom)
    }
}

/// 持久化缓存的历史选区矩形数据契约
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelectionRectRecord {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SelectionRectRecord {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 10.0 && self.height > 10.0 && self.x >= 0.0 && self.y >= 0.0
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
    fn test_hex_color_non_ascii_is_rejected_without_panic() {
        for input in ["#红色", "#测试", "#ééé", "#💥💥", "红色", "💥"] {
            assert_eq!(
                HexColor::from_hex(input),
                None,
                "非 ASCII 输入必须安全返回 None: {input}"
            );
        }

        let rgba = HexColor::from_hex("#00AEFF80").expect("8 位颜色应解析成功");
        assert_eq!(rgba.to_rgba(), (0, 174, 255, 128));
    }

    #[test]
    fn test_hotkey_parsing_and_serde() {
        let hk = Hotkey::parse_str("Ctrl+Alt+A").expect("合法组合键应解析成功");
        assert!(hk.ctrl && hk.alt && !hk.shift);
        assert_eq!(hk.key, "A");
        assert_eq!(hk.to_display_string(), "Ctrl+Alt+A");

        let f1 = Hotkey::parse_str("F1").expect("单功能键应解析成功");
        assert_eq!(f1.to_display_string(), "F1");

        let holder = HotkeyHolder { hotkey: hk };
        let toml_str = toml::to_string(&holder).unwrap();
        let decoded: HotkeyHolder = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded, holder);
    }

    /// 录制器输出的全部合法词表必须通过校验，且展示名可稳定往返
    #[test]
    fn test_hotkey_accepts_recorder_vocabulary() {
        let accepted = [
            "F1",
            "f12",
            "F24",
            "A",
            "7",
            "Space",
            "space",
            "Enter",
            "Tab",
            "Print",
            "Pause",
            "Insert",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Up",
            "Down",
            "Left",
            "Right",
            ",",
            ".",
            ";",
            "Ctrl+F3",
            "Shift+F3",
            "Ctrl+Shift+S",
            "Super+A",
            "Win+D",
            "Meta+F10",
            "Alt+Space",
            "已禁用",
            "none",
            "",
        ];
        for binding in accepted {
            let hotkey = Hotkey::parse_str(binding)
                .unwrap_or_else(|e| panic!("快捷键 {binding} 应通过校验，实际失败: {e}"));
            let display = hotkey.to_display_string();
            let round_trip = Hotkey::parse_str(&display)
                .unwrap_or_else(|e| panic!("快捷键展示名 {display} 应可往返解析，实际失败: {e}"));
            assert_eq!(round_trip, hotkey, "快捷键 {binding} 往返不一致");
        }
    }

    /// 拼错的修饰键、缺失主键与超范围功能键必须显式报错，杜绝静默注册无效热键
    #[test]
    fn test_hotkey_rejects_invalid_bindings() {
        assert_eq!(
            Hotkey::parse_str("Crtl+A"),
            Err(HotkeyParseError::UnknownModifier {
                binding: "Crtl+A".to_string(),
                token: "Crtl".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("Ctrl+"),
            Err(HotkeyParseError::MissingMainKey {
                binding: "Ctrl+".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("Ctrl+F25"),
            Err(HotkeyParseError::UnsupportedMainKey {
                binding: "Ctrl+F25".to_string(),
                key: "F25".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("F0"),
            Err(HotkeyParseError::UnsupportedMainKey {
                binding: "F0".to_string(),
                key: "F0".to_string(),
            })
        );
        assert_eq!(
            Hotkey::parse_str("Ctrl+日本語"),
            Err(HotkeyParseError::UnsupportedMainKey {
                binding: "Ctrl+日本語".to_string(),
                key: "日本語".to_string(),
            })
        );
        assert!(matches!(
            Hotkey::parse_str("<Control>F3"),
            Err(HotkeyParseError::UnsupportedMainKey { .. })
        ));
    }

    /// 配置反序列化必须把非法快捷键转成带中文原因的 serde 错误，而不是静默接受无效键
    #[test]
    fn test_hotkey_deserialize_rejects_invalid_binding() {
        let error = toml::from_str::<HotkeyHolder>("hotkey = \"Hyper+B\"")
            .expect_err("非法修饰键应导致配置解析失败");
        assert!(error.message().contains("Hyper"));
    }
}
