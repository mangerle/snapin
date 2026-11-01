//! 各类矢量标注图元的具体光栅化绘制实现。

pub mod arrow;
pub mod badge;
pub mod blur;
pub mod ellipse;
pub mod marker;
pub mod mosaic;
pub mod pen;
pub mod rect;
pub mod spotlight;
pub mod text;
pub mod translation;

pub use arrow::ArrowAction;
pub use badge::BadgeAction;
pub use blur::BlurAction;
pub use ellipse::EllipseAction;
pub use marker::MarkerAction;
pub use mosaic::MosaicAction;
pub use pen::PenAction;
pub use rect::RectAction;
pub use spotlight::SpotlightAction;
pub use text::TextAction;
pub use translation::{TranslationAction, TranslationShapeItem};

use crate::geometry::Rect;

/// 图元尺寸调整参数。
///
/// 描边粗细与字号是互斥能力，用枚举替代 `adjust_size(stroke, font)` 的
/// `0.0` 哨兵，避免调用方传错参数时静默改错字段。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeParam {
    /// 调整描边粗细或等价视觉尺寸
    Stroke(f32),
    /// 调整文字字号
    Font(f32),
}

/// 将历史结构体中的元组矩形转换为共享 [`Rect`]。
///
/// 标注图元为了 Serde 兼容仍保存四元组，但所有新几何计算统一走结构体。
pub(crate) fn rect_from_tuple(rect: (f32, f32, f32, f32)) -> Rect {
    Rect {
        x: rect.0,
        y: rect.1,
        width: rect.2,
        height: rect.3,
    }
}

/// 将共享 [`Rect`] 写回图元的历史四元组表示。
pub(crate) fn tuple_from_rect(rect: Rect) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.width, rect.height)
}

/// 过滤宽高非正的退化矩形
pub(crate) fn valid_rect(rect: Rect) -> Option<Rect> {
    (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

/// 计算点集的轴对齐包围盒；少于两个点视为无效几何
pub(crate) fn points_bounds(points: &[(f32, f32)]) -> Option<Rect> {
    if points.len() < 2 {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    Some(Rect {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(1.0),
        height: (max_y - min_y).max(1.0),
    })
}

/// 将点从源包围盒线性映射到目标包围盒。
///
/// 源包围盒某一维为零时该维不做缩放，只跟随目标原点平移，避免除零与退化。
pub(crate) fn map_point(from: Rect, to: Rect, point: (f32, f32)) -> (f32, f32) {
    let sx = if from.width.abs() > f32::EPSILON {
        to.width / from.width
    } else {
        1.0
    };
    let sy = if from.height.abs() > f32::EPSILON {
        to.height / from.height
    } else {
        1.0
    };
    (
        to.x + (point.0 - from.x) * sx,
        to.y + (point.1 - from.y) * sy,
    )
}

/// 按包围盒映射缩放矩形，结果宽高至少保留 1 像素
pub(crate) fn resize_rect(from: Rect, to: Rect, rect: Rect) -> Rect {
    let (x0, y0) = map_point(from, to, (rect.x, rect.y));
    let (x1, y1) = map_point(from, to, (rect.x + rect.width, rect.y + rect.height));
    Rect {
        x: x0.min(x1),
        y: y0.min(y1),
        width: (x1 - x0).abs().max(1.0),
        height: (y1 - y0).abs().max(1.0),
    }
}

/// 估算单行文本的渲染宽度。
///
/// cosmic-text 的实际排版宽度需要持有 FontSystem 才能精确测量，标注包围盒只服务
/// 于选中框与手柄命中，使用与画布状态提示一致的字符宽度经验值即可。
pub(crate) fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|ch| if ch.is_ascii() { 0.58 } else { 1.0 })
        .sum::<f32>()
        * font_size
}
