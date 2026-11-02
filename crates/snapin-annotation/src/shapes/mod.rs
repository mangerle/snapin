//! 各类矢量标注图元的具体光栅化绘制实现。

pub mod arrow;
pub mod badge;
pub mod blur;
pub mod ellipse;
pub mod magnifier;
pub mod marker;
pub mod mosaic;
pub mod pen;
pub mod polyline;
pub mod rect;
pub mod spotlight;
pub mod text;
pub mod translation;

pub use arrow::ArrowAction;
pub use badge::BadgeAction;
pub use blur::BlurAction;
pub use ellipse::EllipseAction;
pub use magnifier::{MagnifierAction, MagnifierConnector, MagnifierShape};
pub use marker::{MarkerAction, MarkerBlendMode, MarkerShape, MarkerStrokeSpec};
pub use mosaic::MosaicAction;
pub use pen::{PenAction, PenStrokeSpec};
pub use polyline::PolylineAction;
pub use rect::RectAction;
pub use spotlight::{SpotlightAction, SpotlightShape};
pub use text::TextAction;
pub use translation::{TranslationAction, TranslationShapeItem};

use crate::geometry::{Point, Rect};

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

/// 过滤宽高非正的退化矩形
pub(crate) fn valid_rect(rect: Rect) -> Option<Rect> {
    (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

/// 计算点集的轴对齐包围盒；少于两个点视为无效几何
pub(crate) fn points_bounds(points: &[Point]) -> Option<Rect> {
    if points.len() < 2 {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for point in points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
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
pub(crate) fn map_point(from: Rect, to: Rect, point: Point) -> Point {
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
    Point::new(
        to.x + (point.x - from.x) * sx,
        to.y + (point.y - from.y) * sy,
    )
}

/// 按包围盒映射缩放矩形，结果宽高至少保留 1 像素
pub(crate) fn resize_rect(from: Rect, to: Rect, rect: Rect) -> Rect {
    let first = map_point(from, to, Point::new(rect.x, rect.y));
    let second = map_point(
        from,
        to,
        Point::new(rect.x + rect.width, rect.y + rect.height),
    );
    Rect {
        x: first.x.min(second.x),
        y: first.y.min(second.y),
        width: (second.x - first.x).abs().max(1.0),
        height: (second.y - first.y).abs().max(1.0),
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
