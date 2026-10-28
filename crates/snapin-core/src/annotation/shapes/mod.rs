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

use super::action::ShapeRect;

/// 过滤宽高非正的退化矩形
pub(crate) fn valid_rect(rect: ShapeRect) -> Option<ShapeRect> {
    (rect.2 > 0.0 && rect.3 > 0.0).then_some(rect)
}

/// 计算点集的轴对齐包围盒；少于两个点视为无效几何
pub(crate) fn points_bounds(points: &[(f32, f32)]) -> Option<ShapeRect> {
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
    let width = (max_x - min_x).max(1.0);
    let height = (max_y - min_y).max(1.0);
    Some((min_x, min_y, width, height))
}

/// 将点从源包围盒线性映射到目标包围盒。
///
/// 源包围盒某一维为零时该维不做缩放，只跟随目标原点平移，避免除零与退化。
pub(crate) fn map_point(from: ShapeRect, to: ShapeRect, point: (f32, f32)) -> (f32, f32) {
    let sx = if from.2.abs() > f32::EPSILON {
        to.2 / from.2
    } else {
        1.0
    };
    let sy = if from.3.abs() > f32::EPSILON {
        to.3 / from.3
    } else {
        1.0
    };
    (
        to.0 + (point.0 - from.0) * sx,
        to.1 + (point.1 - from.1) * sy,
    )
}

/// 按包围盒映射缩放矩形，结果宽高至少保留 1 像素
pub(crate) fn resize_rect(from: ShapeRect, to: ShapeRect, rect: ShapeRect) -> ShapeRect {
    let (x0, y0) = map_point(from, to, (rect.0, rect.1));
    let (x1, y1) = map_point(from, to, (rect.0 + rect.2, rect.1 + rect.3));
    let (x, y) = (x0.min(x1), y0.min(y1));
    (x, y, (x1 - x0).abs().max(1.0), (y1 - y0).abs().max(1.0))
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
