//! 标注图元共用的描边线型与底板样式。

use serde::{Deserialize, Serialize};
use tiny_skia::{LineCap, Paint, Path, PixmapMut, Stroke, StrokeDash, Transform};

/// 标注图元使用的 RGBA 颜色。
///
/// 使用具名新类型而不是四元组，避免与坐标、尺寸等 `(u8, u8, u8, u8)` 参数混淆；
/// 序列化布局与历史四元组保持一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRgba(pub u8, pub u8, pub u8, pub u8);

impl ColorRgba {
    /// 构造 RGBA 颜色。
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(r, g, b, a)
    }

    /// 构造不透明 RGB 颜色。
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }

    /// 转为四元组，供仍在边界使用元组的第三方接口。
    pub const fn to_tuple(self) -> (u8, u8, u8, u8) {
        (self.0, self.1, self.2, self.3)
    }
}

impl From<(u8, u8, u8, u8)> for ColorRgba {
    fn from((r, g, b, a): (u8, u8, u8, u8)) -> Self {
        Self(r, g, b, a)
    }
}

/// 描边线型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    /// 实线
    #[default]
    Solid,
    /// 虚线
    Dashed,
    /// 点线
    Dotted,
}

impl LineStyle {
    /// 全部线型，顺序与工具栏下拉列表一致
    pub const ALL: &'static [LineStyle] = &[Self::Solid, Self::Dashed, Self::Dotted];

    /// 中文展示名
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "实线",
            Self::Dashed => "虚线",
            Self::Dotted => "点线",
        }
    }

    /// 生成与线宽成比例的虚线参数。
    ///
    /// 段长与间隔都按线宽缩放，保证同一线型在 1px 与 20px 下的疏密观感一致；
    /// 点线用极短段长配合圆头笔帽绘制成圆点。线宽为 0 时按 1px 处理，避免除零与零长段。
    fn dash(self, width: f32) -> Option<StrokeDash> {
        let unit = width.max(1.0);
        match self {
            Self::Solid => None,
            Self::Dashed => StrokeDash::new(vec![unit * 3.0, unit * 2.0], 0.0),
            Self::Dotted => StrokeDash::new(vec![unit * 0.15, unit * 1.8], 0.0),
        }
    }

    /// 按当前线型描边给定路径
    pub fn stroke_path(self, pixmap: &mut PixmapMut, path: &Path, paint: &Paint, width: f32) {
        let mut stroke = Stroke {
            width: width.max(1.0),
            ..Default::default()
        };
        if let Some(dash) = self.dash(width) {
            stroke.dash = Some(dash);
            stroke.line_cap = LineCap::Round;
        }
        pixmap.stroke_path(path, paint, &stroke, Transform::identity(), None);
    }
}

/// 序号徽标底板样式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BadgeStyle {
    /// 实心圆底 + 白色数字
    #[default]
    Filled,
    /// 空心描边圆环 + 彩色数字，压在复杂底图上时不遮挡内容
    Outline,
}

/// 箭头形态样式（三选一，不是可叠加的线型属性）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArrowStyle {
    /// 实线箭杆 + 实心头，粗细均匀
    Solid,
    /// 虚线箭杆 + 实心头
    Dashed,
    /// 锥形箭杆：尾细头粗，整体填充
    #[default]
    Tapered,
}

impl ArrowStyle {
    /// 全部样式，顺序与下拉菜单一致
    pub const ALL: &'static [ArrowStyle] = &[Self::Solid, Self::Dashed, Self::Tapered];

    /// 中文展示名
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "实线箭头",
            Self::Dashed => "虚线箭头",
            Self::Tapered => "锥形箭头",
        }
    }

    /// 描边箭杆对应的线型；锥形是填充多边形，不使用描边
    pub fn shaft_line_style(self) -> Option<LineStyle> {
        match self {
            Self::Solid => Some(LineStyle::Solid),
            Self::Dashed => Some(LineStyle::Dashed),
            Self::Tapered => None,
        }
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    /// 新类型必须保持历史四元组的序列化布局，避免旧图元缓存失效。
    #[test]
    fn test_color_rgba_serializes_like_tuple() {
        let color = ColorRgba::new(1, 2, 3, 4);
        let encoded = serde_json::to_string(&color).expect("颜色应可序列化");
        assert_eq!(encoded, "[1,2,3,4]");
        let decoded: ColorRgba = serde_json::from_str(&encoded).expect("颜色应可反序列化");
        assert_eq!(decoded, color);
    }
}
