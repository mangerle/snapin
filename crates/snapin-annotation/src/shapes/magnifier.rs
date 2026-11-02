//! 区域放大镜标注图元。
//!
//! 放大镜不保存原始像素快照，而是在绘制时从当前底图复制源区域，再缩放到目标区域。
//! 这样撤销、重做和后续标注重放始终基于同一张底图，历史数据也不会因缓存大块图像而膨胀。

use super::{resize_rect, SizeParam};
use crate::geometry::{Point, Rect};
use crate::style::{ColorRgba, LineStyle, StrokePathSpec};
use serde::{Deserialize, Serialize};
use tiny_skia::{
    FillRule, Mask, MaskType, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect as SkiaRect,
    Transform,
};

/// 放大镜外框形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MagnifierShape {
    /// 矩形放大区域
    #[default]
    Rectangle,
    /// 椭圆放大区域
    Ellipse,
}

/// 源区域与放大区域之间的连接线样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MagnifierConnector {
    /// 实线连接
    #[default]
    Solid,
    /// 点线连接
    Dotted,
    /// 线段连接两侧边框，并在线段两端绘制端帽
    Frame,
    /// 不绘制连接线
    None,
}

impl MagnifierConnector {
    /// 全部连接线样式，顺序与二级栏下拉菜单一致。
    pub const ALL: &'static [Self] = &[Self::Solid, Self::Dotted, Self::Frame, Self::None];

    /// 中文展示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "线段",
            Self::Dotted => "点线",
            Self::Frame => "框线",
            Self::None => "叉线",
        }
    }
}

/// 区域放大镜动作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MagnifierAction {
    /// 原始图像中的采样区域。
    pub source: Rect,
    /// 放大图像在画布上的目标区域。
    pub dest: Rect,
    /// 外框形状。
    #[serde(default)]
    pub shape: MagnifierShape,
    /// 外框描边宽度。
    #[serde(default = "default_border_width")]
    pub border_width: f32,
    /// 外框描边颜色。
    #[serde(default = "default_border_color")]
    pub border_color: ColorRgba,
    /// 是否使用双线性平滑放大。
    #[serde(default = "default_smooth")]
    pub smooth: bool,
    /// 是否绘制放大图像阴影。
    #[serde(default = "default_shadow")]
    pub shadow: bool,
    /// 源区域与放大区域之间的连接线样式。
    #[serde(default)]
    pub connector: MagnifierConnector,
}

fn default_border_width() -> f32 {
    2.0
}

fn default_border_color() -> ColorRgba {
    ColorRgba::opaque(255, 255, 255)
}

fn default_smooth() -> bool {
    true
}

fn default_shadow() -> bool {
    true
}

/// 放大镜连接线路径与端点。
struct ConnectorGeometry {
    path: tiny_skia::Path,
    start: Point,
    end: Point,
}

/// 放大镜连接线的端点、法向与端帽半长。
struct ConnectorLine {
    start: Point,
    end: Point,
    normal: (f32, f32),
    half: f32,
}

impl ConnectorLine {
    fn new(start: Point, end: Point, border_width: f32) -> Self {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt().max(1.0);
        Self {
            start,
            end,
            normal: (-dy / length, dx / length),
            half: (border_width * 4.0).clamp(4.0, 16.0),
        }
    }
}

impl MagnifierAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let source = offset_rect(self.source, offset_x, offset_y);
        let dest = offset_rect(self.dest, offset_x, offset_y);
        let Some(source) = clamp_rect(source, pixmap.width(), pixmap.height()) else {
            return;
        };
        let Some(dest) = clamp_rect(dest, pixmap.width(), pixmap.height()) else {
            return;
        };

        // 先从未叠加任何辅助线的原始像素构建放大图，避免源框虚线和连接线被复制进去。
        let Some(scaled) = self.build_scaled_region(pixmap, source, dest) else {
            return;
        };
        self.draw_shadow(pixmap, dest);
        self.draw_scaled_region(pixmap, dest, &scaled);
        if !rects_overlap(source, dest) && self.connector != MagnifierConnector::None {
            self.draw_connector(pixmap, source, dest);
        }
        // 源区域虚线必须最后绘制，重合时才能稳定显示在放大图像之上。
        self.draw_source_outline(pixmap, source);
        self.draw_border(pixmap, dest);
    }

    fn draw_connector(&self, pixmap: &mut PixmapMut, source: Rect, dest: Rect) {
        let Some(line) = self.connector_geometry(source, dest) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            self.border_color.0,
            self.border_color.1,
            self.border_color.2,
            self.border_color.3,
        );
        let style = match self.connector {
            MagnifierConnector::Solid => LineStyle::Solid,
            MagnifierConnector::Dotted => LineStyle::Dotted,
            MagnifierConnector::Frame | MagnifierConnector::None => LineStyle::Solid,
        };
        style.stroke_path(
            pixmap,
            &paint,
            StrokePathSpec::new(&line.path, self.border_width.max(1.0)),
        );
        if self.connector == MagnifierConnector::Frame {
            self.draw_connector_caps(
                pixmap,
                ConnectorLine::new(line.start, line.end, self.border_width),
                &paint,
            );
        }
    }

    /// 计算连接线端点与路径。
    fn connector_geometry(&self, source: Rect, dest: Rect) -> Option<ConnectorGeometry> {
        let source_center = rect_center(source);
        let dest_center = rect_center(dest);
        let direction = (
            dest_center.x - source_center.x,
            dest_center.y - source_center.y,
        );
        let (start, end) = match self.connector {
            MagnifierConnector::Frame => (
                boundary_point(
                    source_center,
                    source.width / 2.0,
                    source.height / 2.0,
                    direction,
                ),
                boundary_point(dest_center, dest.width / 2.0, dest.height / 2.0, direction),
            ),
            _ => (
                source_center,
                boundary_point(dest_center, dest.width / 2.0, dest.height / 2.0, direction),
            ),
        };
        let mut builder = PathBuilder::new();
        builder.move_to(start.x, start.y);
        builder.line_to(end.x, end.y);
        Some(ConnectorGeometry {
            path: builder.finish()?,
            start,
            end,
        })
    }

    fn draw_connector_caps(&self, pixmap: &mut PixmapMut, line: ConnectorLine, paint: &Paint) {
        let mut builder = PathBuilder::new();
        for point in [line.start, line.end] {
            builder.move_to(
                point.x + line.normal.0 * line.half,
                point.y + line.normal.1 * line.half,
            );
            builder.line_to(
                point.x - line.normal.0 * line.half,
                point.y - line.normal.1 * line.half,
            );
        }
        if let Some(path) = builder.finish() {
            pixmap.stroke_path(
                &path,
                paint,
                &tiny_skia::Stroke {
                    width: self.border_width.max(1.0),
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }

    fn draw_source_outline(&self, pixmap: &mut PixmapMut, source: Rect) {
        let Some(shape_rect) = SkiaRect::from_xywh(source.x, source.y, source.width, source.height)
        else {
            return;
        };
        let mut builder = PathBuilder::new();
        push_shape_path(&mut builder, self.shape, shape_rect);
        let Some(path) = builder.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            self.border_color.0,
            self.border_color.1,
            self.border_color.2,
            220,
        );
        let stroke = tiny_skia::Stroke {
            width: 1.2,
            dash: tiny_skia::StrokeDash::new(vec![5.0, 4.0], 0.0),
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    fn draw_shadow(&self, pixmap: &mut PixmapMut, dest: Rect) {
        if !self.shadow {
            return;
        }
        let Some(shape_rect) =
            SkiaRect::from_xywh(dest.x, dest.y, dest.width.max(1.0), dest.height.max(1.0))
        else {
            return;
        };
        let mut builder = PathBuilder::new();
        push_shape_path(&mut builder, self.shape, shape_rect);
        let Some(path) = builder.finish() else {
            return;
        };
        for (offset, alpha) in [(5.0, 20), (3.0, 28), (1.5, 36)] {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, alpha);
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::from_translate(offset, offset),
                None,
            );
        }
    }

    fn build_scaled_region(
        &self,
        pixmap: &mut PixmapMut,
        source: Rect,
        dest: Rect,
    ) -> Option<Pixmap> {
        let source_width = source.width.floor().max(1.0) as u32;
        let source_height = source.height.floor().max(1.0) as u32;
        let dest_width = dest.width.floor().max(1.0) as u32;
        let dest_height = dest.height.floor().max(1.0) as u32;
        let mut source_pixmap = Pixmap::new(source_width, source_height)?;
        let src_x = source.x.floor().max(0.0) as usize;
        let src_y = source.y.floor().max(0.0) as usize;
        let source_stride = pixmap.width() as usize * 4;
        let target_stride = source_width as usize * 4;
        let source_data = pixmap.data_mut();
        let target_data = source_pixmap.data_mut();
        for row in 0..source_height as usize {
            let src_start = (src_y + row) * source_stride + src_x * 4;
            let dst_start = row * target_stride;
            let src = source_data.get(src_start..src_start + target_stride)?;
            let dst = target_data.get_mut(dst_start..dst_start + target_stride)?;
            dst.copy_from_slice(src);
        }

        let mut scaled = Pixmap::new(dest_width, dest_height)?;
        let scale_x = dest_width as f32 / source_width as f32;
        let scale_y = dest_height as f32 / source_height as f32;
        let paint = PixmapPaint {
            quality: if self.smooth {
                tiny_skia::FilterQuality::Bilinear
            } else {
                tiny_skia::FilterQuality::Nearest
            },
            ..Default::default()
        };
        scaled.draw_pixmap(
            0,
            0,
            source_pixmap.as_ref(),
            &paint,
            Transform::from_scale(scale_x, scale_y),
            None,
        );
        Some(scaled)
    }

    fn draw_scaled_region(&self, pixmap: &mut PixmapMut, dest: Rect, scaled: &Pixmap) {
        let Some(mut mask_pixmap) = Pixmap::new(pixmap.width(), pixmap.height()) else {
            return;
        };
        let Some(shape_rect) =
            SkiaRect::from_xywh(dest.x, dest.y, dest.width.max(1.0), dest.height.max(1.0))
        else {
            return;
        };
        let mut builder = PathBuilder::new();
        push_shape_path(&mut builder, self.shape, shape_rect);
        let Some(path) = builder.finish() else {
            return;
        };
        let mut mask_paint = Paint::default();
        mask_paint.set_color(tiny_skia::Color::WHITE);
        mask_pixmap.fill_path(
            &path,
            &mask_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let mask = Mask::from_pixmap(mask_pixmap.as_ref(), MaskType::Alpha);
        pixmap.draw_pixmap(
            dest.x as i32,
            dest.y as i32,
            scaled.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }

    fn draw_border(&self, pixmap: &mut PixmapMut, dest: Rect) {
        let Some(shape_rect) =
            SkiaRect::from_xywh(dest.x, dest.y, dest.width.max(1.0), dest.height.max(1.0))
        else {
            return;
        };
        let mut builder = PathBuilder::new();
        push_shape_path(&mut builder, self.shape, shape_rect);
        let Some(path) = builder.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            self.border_color.0,
            self.border_color.1,
            self.border_color.2,
            self.border_color.3,
        );
        LineStyle::Solid.stroke_path(
            pixmap,
            &paint,
            StrokePathSpec::new(&path, self.border_width.max(1.0)),
        );
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.border_width = width.clamp(1.0, 12.0);
        }
    }

    /// 以当前放大图像中心为锚点调整倍率。
    pub fn adjust_scale(&mut self, scale: f32, preferred_center: Option<Point>) {
        let center = preferred_center.unwrap_or_else(|| rect_center(self.dest));
        let source = self.source;
        let width = source.width * scale.clamp(1.0, 8.0);
        let height = source.height * scale.clamp(1.0, 8.0);
        self.dest = Rect::new(
            center.x - width / 2.0,
            center.y - height / 2.0,
            width,
            height,
        );
    }

    /// 同步移动原区域与放大图像。
    pub fn translate_source(&mut self, dx: f32, dy: f32) {
        self.translate(dx, dy);
    }

    /// 仅移动放大图像。
    pub fn translate_dest(&mut self, dx: f32, dy: f32) {
        self.dest.x += dx;
        self.dest.y += dy;
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        union_rects(self.source, self.dest)
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.source.x += dx;
        self.source.y += dy;
        self.dest.x += dx;
        self.dest.y += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.source = resize_rect(from, to, self.source);
        self.dest = resize_rect(from, to, self.dest);
    }
}

fn push_shape_path(builder: &mut PathBuilder, shape: MagnifierShape, rect: SkiaRect) {
    match shape {
        MagnifierShape::Rectangle => builder.push_rect(rect),
        MagnifierShape::Ellipse => builder.push_oval(rect),
    }
}

fn offset_rect(rect: Rect, offset_x: f32, offset_y: f32) -> Rect {
    Rect {
        x: rect.x + offset_x,
        y: rect.y + offset_y,
        ..rect
    }
}

fn clamp_rect(rect: Rect, width: u32, height: u32) -> Option<Rect> {
    let left = rect.x.max(0.0).min(width as f32);
    let top = rect.y.max(0.0).min(height as f32);
    let right = (rect.x + rect.width).max(0.0).min(width as f32);
    let bottom = (rect.y + rect.height).max(0.0).min(height as f32);
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn union_rects(a: Rect, b: Rect) -> Option<Rect> {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// 计算矩形中心。
fn rect_center(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

/// 按方向求矩形边界交点；方向退化为零时返回中心。
fn boundary_point(
    center: Point,
    half_width: f32,
    half_height: f32,
    direction: (f32, f32),
) -> Point {
    let scale_x = if direction.0.abs() > f32::EPSILON {
        half_width / direction.0.abs()
    } else {
        f32::INFINITY
    };
    let scale_y = if direction.1.abs() > f32::EPSILON {
        half_height / direction.1.abs()
    } else {
        f32::INFINITY
    };
    let scale = scale_x.min(scale_y);
    if scale.is_finite() {
        Point::new(
            center.x + direction.0 * scale,
            center.y + direction.1 * scale,
        )
    } else {
        center
    }
}

/// 两个矩形是否存在面积重叠。
fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl MagnifierAction {
    pub fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 构造使用固定红色边框的测试放大镜，减少各视觉测试的重复参数。
    fn action(
        source: Rect,
        dest: Rect,
        connector: MagnifierConnector,
        shadow: bool,
    ) -> MagnifierAction {
        MagnifierAction {
            source,
            dest,
            shape: MagnifierShape::Rectangle,
            border_width: 2.0,
            border_color: ColorRgba::opaque(255, 0, 0),
            smooth: false,
            shadow,
            connector,
        }
    }

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * pixmap.width() + x) * 4) as usize;
        let data = &pixmap.data()[index..index + 4];
        [data[0], data[1], data[2], data[3]]
    }

    /// 放大镜应从源区域复制像素并按倍率写入目标区域，源区域保持原样。
    #[test]
    fn test_magnifier_scales_source_region() {
        let mut pixmap = Pixmap::new(24, 16).expect("创建测试位图失败");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));
        for y in 0..4 {
            for x in 0..4 {
                let index = (y * 24 + x) * 4;
                pixmap.data_mut()[index..index + 4].copy_from_slice(&[240, 30, 20, 255]);
            }
        }
        let source_index = (2 * 24 + 2) * 4;
        let source_before = pixmap.data()[source_index..source_index + 4].to_vec();
        let mut action = action(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Rect::new(8.0, 4.0, 8.0, 8.0),
            MagnifierConnector::None,
            false,
        );
        action.border_width = 1.0;

        action.draw(&mut pixmap.as_mut());

        let dest_index = (6 * 24 + 10) * 4;
        assert_eq!(
            &pixmap.data()[dest_index..dest_index + 4],
            &[240, 30, 20, 255]
        );
        assert_eq!(
            &pixmap.data()[source_index..source_index + 4],
            source_before.as_slice()
        );
    }

    /// 放大图与源区域重合时，源区域虚线框必须绘制在放大图上方。
    #[test]
    fn test_magnifier_source_outline_is_visible_over_destination() {
        let mut pixmap = Pixmap::new(24, 24).expect("创建测试位图失败");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));
        let action = action(
            Rect::new(4.0, 4.0, 8.0, 8.0),
            Rect::new(0.0, 0.0, 16.0, 16.0),
            MagnifierConnector::None,
            false,
        );

        action.draw(&mut pixmap.as_mut());

        let border_pixel = pixel(&pixmap, 4, 8);
        assert!(
            border_pixel[0] > 80 && border_pixel[0] > border_pixel[2],
            "源区域虚线框应覆盖在放大图上，实际像素: {border_pixel:?}"
        );
    }

    /// 实线连接线应连接源中心与放大图边框；无连接模式不得留下线条。
    #[test]
    fn test_magnifier_connector_modes_render_as_expected() {
        let draw_midpoint = |connector| {
            let mut pixmap = Pixmap::new(56, 28).expect("创建测试位图失败");
            pixmap.fill(Color::from_rgba8(10, 20, 30, 255));
            action(
                Rect::new(2.0, 10.0, 8.0, 8.0),
                Rect::new(36.0, 8.0, 12.0, 12.0),
                connector,
                false,
            )
            .draw(&mut pixmap.as_mut());
            pixel(&pixmap, 22, 14)
        };

        let solid = draw_midpoint(MagnifierConnector::Solid);
        let none = draw_midpoint(MagnifierConnector::None);
        assert!(solid[0] > 180, "实线连接线中点应为红色，实际: {solid:?}");
        assert!(none[0] < 80, "无连接线模式不得绘制线条，实际: {none:?}");
    }

    /// 调整倍率时保持目标区域中心不变，并夹取到 1.0 至 8.0。
    #[test]
    fn test_magnifier_adjust_scale_preserves_center_and_clamps() {
        let mut action = action(
            Rect::new(10.0, 10.0, 20.0, 10.0),
            Rect::new(0.0, 0.0, 40.0, 20.0),
            MagnifierConnector::Solid,
            true,
        );

        action.adjust_scale(20.0, Some(Point::new(50.0, 50.0)));

        assert_eq!(action.dest, Rect::new(-30.0, 10.0, 160.0, 80.0));
        assert_eq!(action.source, Rect::new(10.0, 10.0, 20.0, 10.0));
    }
}
