//! 标注图元命令模式与撤销/重做控制。
//!
//! 将每一次绘制操作封装为具备 Execute 与 Undo 能力的 Command 对象，
//! 采用命令模式维护 Undo/Redo 历史栈。

use crate::geometry::Rect;
use crate::shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MagnifierAction, MarkerAction,
    MosaicAction, PenAction, PolylineAction, RectAction, SizeParam, SpotlightAction, TextAction,
    TranslationAction,
};
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 所有支持的闭合矢量标注图元枚举 (代数数据类型，天然支持无堆分配存储、值拷贝与 Serde 序列化)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationShape {
    Rect(RectAction),
    Ellipse(EllipseAction),
    Arrow(ArrowAction),
    Pen(PenAction),
    Polyline(PolylineAction),
    Marker(MarkerAction),
    Mosaic(MosaicAction),
    Blur(BlurAction),
    Spotlight(SpotlightAction),
    Magnifier(MagnifierAction),
    Badge(BadgeAction),
    Text(TextAction),
    Translation(TranslationAction),
}

impl AnnotationShape {
    /// 执行带坐标偏移量的绘制
    pub fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        match self {
            Self::Rect(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Ellipse(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Arrow(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Pen(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Polyline(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Marker(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Mosaic(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Blur(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Spotlight(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Magnifier(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Badge(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Text(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Translation(a) => a.draw_offset(pixmap, offset_x, offset_y),
        }
    }

    /// 执行绘制（偏置量默认为 0.0）
    pub fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }

    /// 动态调节矩形圆角半径，仅矩形图元生效
    pub fn adjust_corner_radius(&mut self, radius: f32) {
        if let AnnotationShape::Rect(action) = self {
            action.adjust_corner_radius(radius);
        }
    }

    /// 按尺寸语义调节图元：描边类只接受 [`SizeParam::Stroke`]，
    /// 文本类只接受 [`SizeParam::Font`]，传错语义时不做任何修改。
    pub fn adjust_size(&mut self, param: SizeParam) {
        match self {
            Self::Rect(a) => a.adjust_size(param),
            Self::Ellipse(a) => a.adjust_size(param),
            Self::Arrow(a) => a.adjust_size(param),
            Self::Pen(a) => a.adjust_size(param),
            Self::Polyline(a) => a.adjust_size(param),
            Self::Marker(a) => a.adjust_size(param),
            Self::Mosaic(a) => a.adjust_size(param),
            Self::Blur(a) => a.adjust_size(param),
            Self::Spotlight(a) => a.adjust_size(param),
            Self::Magnifier(a) => a.adjust_size(param),
            Self::Badge(a) => a.adjust_size(param),
            Self::Text(a) => a.adjust_size(param),
            Self::Translation(a) => a.adjust_size(param),
        }
    }

    /// 返回图元的轴对齐包围盒
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Rect(a) => a.bounds(),
            Self::Ellipse(a) => a.bounds(),
            Self::Arrow(a) => a.bounds(),
            Self::Pen(a) => a.bounds(),
            Self::Polyline(a) => a.bounds(),
            Self::Marker(a) => a.bounds(),
            Self::Mosaic(a) => a.bounds(),
            Self::Blur(a) => a.bounds(),
            Self::Spotlight(a) => a.bounds(),
            Self::Magnifier(a) => a.bounds(),
            Self::Badge(a) => a.bounds(),
            Self::Text(a) => a.bounds(),
            Self::Translation(a) => a.bounds(),
        }
    }

    /// 整体平移图元
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Rect(a) => a.translate(dx, dy),
            Self::Ellipse(a) => a.translate(dx, dy),
            Self::Arrow(a) => a.translate(dx, dy),
            Self::Pen(a) => a.translate(dx, dy),
            Self::Polyline(a) => a.translate(dx, dy),
            Self::Marker(a) => a.translate(dx, dy),
            Self::Mosaic(a) => a.translate(dx, dy),
            Self::Blur(a) => a.translate(dx, dy),
            Self::Spotlight(a) => a.translate(dx, dy),
            Self::Magnifier(a) => a.translate(dx, dy),
            Self::Badge(a) => a.translate(dx, dy),
            Self::Text(a) => a.translate(dx, dy),
            Self::Translation(a) => a.translate(dx, dy),
        }
    }

    /// 按包围盒映射缩放图元
    pub fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        match self {
            Self::Rect(a) => a.resize_from_bounds(from, to),
            Self::Ellipse(a) => a.resize_from_bounds(from, to),
            Self::Arrow(a) => a.resize_from_bounds(from, to),
            Self::Pen(a) => a.resize_from_bounds(from, to),
            Self::Polyline(a) => a.resize_from_bounds(from, to),
            Self::Marker(a) => a.resize_from_bounds(from, to),
            Self::Mosaic(a) => a.resize_from_bounds(from, to),
            Self::Blur(a) => a.resize_from_bounds(from, to),
            Self::Spotlight(a) => a.resize_from_bounds(from, to),
            Self::Magnifier(a) => a.resize_from_bounds(from, to),
            Self::Badge(a) => a.resize_from_bounds(from, to),
            Self::Text(a) => a.resize_from_bounds(from, to),
            Self::Translation(a) => a.resize_from_bounds(from, to),
        }
    }

    /// 是否允许通过包围盒手柄拉伸缩放
    ///
    /// 原位翻译块是复合图元，只支持整体移动；其余图元均支持手柄拉伸。
    pub fn supports_resize(&self) -> bool {
        !matches!(self, Self::Translation(_))
    }
}

impl From<TranslationAction> for AnnotationShape {
    fn from(action: TranslationAction) -> Self {
        Self::Translation(action)
    }
}


#[cfg(test)]
mod tests;
