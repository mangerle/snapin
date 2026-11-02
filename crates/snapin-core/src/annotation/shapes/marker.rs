//! 荧光记号笔标注图元实现。
//!
//! 采用半透明或正片叠底（Multiply）混合模式，确保高亮文本时底部的黑色文字永不被遮蔽。

use super::{map_point, points_bounds, SizeParam};
use crate::annotation::style::LineStyle;
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use tiny_skia::{BlendMode, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 荧光笔绘制范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MarkerShape {
    #[default]
    Stroke,
    Rectangle,
}

impl MarkerShape {
    /// 二级栏和下拉菜单中的展示名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stroke => "线条",
            Self::Rectangle => "矩形",
        }
    }
}

/// 荧光笔混合模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MarkerBlendMode {
    #[default]
    Multiply,
    Normal,
}

impl MarkerBlendMode {
    /// 二级栏和下拉菜单中的展示名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Multiply => "正片叠底",
            Self::Normal => "半透明",
        }
    }
}

/// 荧光记号笔动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkerAction {
    pub points: Vec<(f32, f32)>,
    pub color: (u8, u8, u8),
    pub stroke_width: f32,
    #[serde(default)]
    pub shape: MarkerShape,
    #[serde(default)]
    pub blend_mode: MarkerBlendMode,
    #[serde(default)]
    pub line_style: LineStyle,
}

impl MarkerAction {
    /// 直接基于点集切片在 Pixmap 上绘制，避免在预览和高频路径中分配临时 Vec
    pub fn draw_points_offset(
        points: &[(f32, f32)],
        color: (u8, u8, u8),
        stroke_width: f32,
        pixmap: &mut PixmapMut,
        offset_x: f32,
        offset_y: f32,
    ) {
        if points.len() < 2 {
            return;
        }

        let mut paint = Paint::default();
        // 50% 半透明荧光色，搭配 Multiply 混合模式
        paint.set_color_rgba8(color.0, color.1, color.2, 110);
        paint.blend_mode = BlendMode::Multiply;

        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Square,
            line_join: LineJoin::Miter,
            ..Default::default()
        };

        let mut pb = PathBuilder::new();
        pb.move_to(points[0].0 + offset_x, points[0].1 + offset_y);
        for pt in &points[1..] {
            pb.line_to(pt.0 + offset_x, pt.1 + offset_y);
        }

        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

impl MarkerAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let mut paint = Paint::default();
        let alpha = if self.blend_mode == MarkerBlendMode::Multiply {
            110
        } else {
            180
        };
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, alpha);
        paint.blend_mode = if self.blend_mode == MarkerBlendMode::Multiply {
            BlendMode::Multiply
        } else {
            BlendMode::SourceOver
        };
        if self.shape == MarkerShape::Rectangle {
            if let Some(bounds) = points_bounds(&self.points) {
                if let Some(rect) = tiny_skia::Rect::from_xywh(
                    bounds.x + offset_x,
                    bounds.y + offset_y,
                    bounds.width,
                    bounds.height,
                ) {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
            return;
        }
        if self.points.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(self.points[0].0 + offset_x, self.points[0].1 + offset_y);
        for point in &self.points[1..] {
            pb.line_to(point.0 + offset_x, point.1 + offset_y);
        }
        if let Some(path) = pb.finish() {
            self.line_style
                .stroke_path(pixmap, &path, &paint, self.stroke_width);
        }
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.stroke_width = (width * 2.0).max(4.0);
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        points_bounds(&self.points)
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        for point in &mut self.points {
            point.0 += dx;
            point.1 += dy;
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        for point in &mut self.points {
            *point = map_point(from, to, *point);
        }
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl MarkerAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}
