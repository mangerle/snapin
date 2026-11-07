//! 工具栏动作类别图标绘制实现。

use super::primitives::{add_round_rect, make_paint, RoundRect};
use super::IconPainter;
use crate::geometry::Rect;
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 图标 8: 向左回转撤销
pub(super) fn render_icon_undo(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 3.0 * s, cy - 6.5 * s);
    pb.line_to(cx - 6.5 * s, cy - 3.0 * s);
    pb.line_to(cx - 3.0 * s, cy + 0.5 * s);
    pb.move_to(cx - 5.5 * s, cy - 3.0 * s);
    pb.cubic_to(
        cx + 2.0 * s,
        cy - 3.0 * s,
        cx + 6.5 * s,
        cy + 0.5 * s,
        cx + 6.5 * s,
        cy + 6.0 * s,
    );
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 9: 向右回转重做
pub(super) fn render_icon_redo(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    pb.move_to(cx + 3.0 * s, cy - 6.5 * s);
    pb.line_to(cx + 6.5 * s, cy - 3.0 * s);
    pb.line_to(cx + 3.0 * s, cy + 0.5 * s);
    pb.move_to(cx + 5.5 * s, cy - 3.0 * s);
    pb.cubic_to(
        cx - 2.0 * s,
        cy - 3.0 * s,
        cx - 6.5 * s,
        cy + 0.5 * s,
        cx - 6.5 * s,
        cy + 6.0 * s,
    );
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 10: 关闭取消 X
pub(super) fn render_icon_close(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 5.0 * s, cy - 5.0 * s);
    pb.line_to(cx + 5.0 * s, cy + 5.0 * s);
    pb.move_to(cx + 5.0 * s, cy - 5.0 * s);
    pb.line_to(cx - 5.0 * s, cy + 5.0 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 12: 鼠标指针加斜杠 (贴图鼠标穿透开关)
pub(super) fn render_icon_mouse_through(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    // 指针轮廓
    pb.move_to(cx - 4.5 * s, cy - 7.0 * s);
    pb.line_to(cx - 4.5 * s, cy + 3.0 * s);
    pb.line_to(cx - 1.2 * s, cy + 0.5 * s);
    pb.line_to(cx + 1.2 * s, cy + 6.0 * s);
    pb.line_to(cx + 3.8 * s, cy + 4.8 * s);
    pb.line_to(cx + 1.4 * s, cy - 0.6 * s);
    pb.line_to(cx + 5.0 * s, cy - 1.2 * s);
    pb.close();
    // 穿透斜杠
    pb.move_to(cx - 7.0 * s, cy + 7.0 * s);
    pb.line_to(cx + 7.0 * s, cy - 7.0 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 11: 经典大头工字图钉 (贴图置顶)
pub(super) fn render_icon_pin(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    // 钉帽与钉身轮廓：平顶帽盖、内收腰身、展开托盘
    pb.move_to(cx - 5.5 * s, cy - 6.5 * s);
    pb.line_to(cx + 5.5 * s, cy - 6.5 * s);
    pb.line_to(cx + 4.0 * s, cy - 4.0 * s);
    pb.line_to(cx + 2.0 * s, cy - 1.0 * s);
    pb.line_to(cx + 5.2 * s, cy + 2.0 * s);
    pb.line_to(cx - 5.2 * s, cy + 2.0 * s);
    pb.line_to(cx - 2.0 * s, cy - 1.0 * s);
    pb.line_to(cx - 4.0 * s, cy - 4.0 * s);
    pb.close();
    // 垂直扎下的锋利尖针
    pb.move_to(cx, cy + 2.0 * s);
    pb.line_to(cx, cy + 7.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 12: 经典 3.5 寸软盘
pub(super) fn render_icon_save(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 6.5 * s, cy - 6.5 * s);
    pb.line_to(cx + 3.5 * s, cy - 6.5 * s);
    pb.line_to(cx + 6.5 * s, cy - 3.5 * s);
    pb.line_to(cx + 6.5 * s, cy + 6.5 * s);
    pb.line_to(cx - 6.5 * s, cy + 6.5 * s);
    pb.close();
    pb.move_to(cx - 3.0 * s, cy - 6.5 * s);
    pb.line_to(cx - 3.0 * s, cy - 2.5 * s);
    pb.line_to(cx + 1.5 * s, cy - 2.5 * s);
    pb.line_to(cx + 1.5 * s, cy - 6.5 * s);
    pb.move_to(cx - 4.0 * s, cy + 6.5 * s);
    pb.line_to(cx - 4.0 * s, cy + 2.0 * s);
    pb.line_to(cx + 4.0 * s, cy + 2.0 * s);
    pb.line_to(cx + 4.0 * s, cy + 6.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 13: 双重叠矩形纸张 (复制到剪贴板)
pub(super) fn render_icon_copy(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb_back = PathBuilder::new();
    add_round_rect(
        &mut pb_back,
        RoundRect {
            x: cx - 3.0 * s,
            y: cy - 6.5 * s,
            width: 9.5 * s,
            height: 9.5 * s,
            radius: 1.5 * s,
        },
    );
    if let Some(path) = pb_back.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
    let mut pb_front = PathBuilder::new();
    add_round_rect(
        &mut pb_front,
        RoundRect {
            x: cx - 6.5 * s,
            y: cy - 3.0 * s,
            width: 9.5 * s,
            height: 9.5 * s,
            radius: 1.5 * s,
        },
    );
    if let Some(path) = pb_front.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 14: 四角取景框内嵌大写 A (文字识别)
pub(super) fn render_icon_ocr(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let reach = 6.6 * s;
    let inner = 3.4 * s;
    let mut pb = PathBuilder::new();
    // 左上角
    pb.move_to(cx - reach, cy - inner);
    pb.line_to(cx - reach, cy - reach);
    pb.line_to(cx - inner, cy - reach);
    // 右上角
    pb.move_to(cx + inner, cy - reach);
    pb.line_to(cx + reach, cy - reach);
    pb.line_to(cx + reach, cy - inner);
    // 左下角
    pb.move_to(cx - reach, cy + inner);
    pb.line_to(cx - reach, cy + reach);
    pb.line_to(cx - inner, cy + reach);
    // 右下角
    pb.move_to(cx + inner, cy + reach);
    pb.line_to(cx + reach, cy + reach);
    pb.line_to(cx + reach, cy + inner);
    add_glyph_a(
        &mut pb,
        Rect::new(cx - 3.0 * s, cy - 3.5 * s, 6.0 * s, 7.0 * s),
    );
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 二维码图标：三个定位方块与中间简化数据点。
pub(super) fn render_icon_qr(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let (paint, stroke) = (&painter.paint, &painter.stroke);
    let mut pb = PathBuilder::new();
    for (x, y) in [
        (cx - 6.0 * s, cy - 6.0 * s),
        (cx + 1.0 * s, cy - 6.0 * s),
        (cx - 6.0 * s, cy + 1.0 * s),
    ] {
        add_round_rect(
            &mut pb,
            RoundRect {
                x,
                y,
                width: 5.0 * s,
                height: 5.0 * s,
                radius: 0.6 * s,
            },
        );
    }
    for (x, y) in [
        (cx + 1.5 * s, cy + 1.5 * s),
        (cx + 4.5 * s, cy + 1.5 * s),
        (cx + 1.5 * s, cy + 4.5 * s),
        (cx + 5.5 * s, cy + 5.5 * s),
    ] {
        pb.push_circle(x, y, 0.8 * s);
    }
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 15: 双叠层圆角方框内嵌 "A" 与 "文" (截图翻译)
///
/// 前框先以按钮当前底色填充再描边，遮住后框被压住的一角，
/// 从而在任意悬停/激活高亮状态下都呈现干净的纸张叠层效果。
pub(super) fn render_icon_translate(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let side = 9.6 * s;
    let radius = 1.5 * s;
    let back = TranslateFrame {
        origin_x: cx - 1.6 * s,
        origin_y: cy - 7.8 * s,
        side,
        radius,
        scale: s,
    };
    let front = TranslateFrame {
        origin_x: cx - 8.0 * s,
        origin_y: cy - 1.8 * s,
        side,
        radius,
        scale: s,
    };
    // 内嵌字符用细一号笔画，避免小字号下方框与字形粘连成实心块
    let glyph_stroke = Stroke {
        width: painter.stroke.width * 0.72,
        ..painter.stroke.clone()
    };

    draw_translate_back(pixmap, painter, back);
    draw_translate_front(pixmap, painter, front, &glyph_stroke);
}

/// 翻译图标中单个圆角方框的几何信息。
#[derive(Clone, Copy)]
struct TranslateFrame {
    origin_x: f32,
    origin_y: f32,
    side: f32,
    radius: f32,
    scale: f32,
}

impl TranslateFrame {
    fn center(self) -> (f32, f32) {
        (
            self.origin_x + self.side / 2.0,
            self.origin_y + self.side / 2.0,
        )
    }

    fn path(self) -> Option<tiny_skia::Path> {
        let mut builder = PathBuilder::new();
        add_round_rect(
            &mut builder,
            RoundRect {
                x: self.origin_x,
                y: self.origin_y,
                width: self.side,
                height: self.side,
                radius: self.radius,
            },
        );
        builder.finish()
    }
}

/// 绘制翻译图标的后层方框与字母 A。
fn draw_translate_back(pixmap: &mut PixmapMut, painter: &IconPainter, frame: TranslateFrame) {
    if let Some(path) = frame.path() {
        pixmap.stroke_path(
            &path,
            &painter.paint,
            &painter.stroke,
            Transform::identity(),
            None,
        );
    }

    let (center_x, center_y) = frame.center();
    let glyph_scale = frame.scale;
    let mut glyph = PathBuilder::new();
    add_glyph_a(
        &mut glyph,
        Rect::new(
            center_x - 2.2 * glyph_scale,
            center_y - 2.6 * glyph_scale,
            4.4 * glyph_scale,
            5.2 * glyph_scale,
        ),
    );
    if let Some(path) = glyph.finish() {
        let glyph_stroke = Stroke {
            width: painter.stroke.width * 0.72,
            ..painter.stroke.clone()
        };
        pixmap.stroke_path(
            &path,
            &painter.paint,
            &glyph_stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制翻译图标的前层遮罩方框与汉字“文”。
fn draw_translate_front(
    pixmap: &mut PixmapMut,
    painter: &IconPainter,
    frame: TranslateFrame,
    glyph_stroke: &Stroke,
) {
    let (center_x, center_y) = frame.center();
    let backdrop = sample_backdrop(pixmap, center_x, center_y);
    let Some(path) = frame.path() else {
        return;
    };
    pixmap.fill_path(
        &path,
        &make_paint(backdrop),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    pixmap.stroke_path(
        &path,
        &painter.paint,
        &painter.stroke,
        Transform::identity(),
        None,
    );

    let mut glyph = PathBuilder::new();
    add_glyph_wen(&mut glyph, center_x, center_y, frame.scale);
    if let Some(path) = glyph.finish() {
        pixmap.stroke_path(
            &path,
            &painter.paint,
            glyph_stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 向路径追加无衬线大写字母 A，尖顶居中、横梁位于字高约三分之二处
fn add_glyph_a(pb: &mut PathBuilder, rect: Rect) {
    let center_x = rect.x + rect.width / 2.0;
    let top = rect.y;
    let bottom = rect.y + rect.height;
    let half = rect.width / 2.0;
    let bar_y = top + rect.height * 0.68;
    let bar_half = half * 0.66;
    pb.move_to(center_x - half, bottom);
    pb.line_to(center_x, top);
    pb.line_to(center_x + half, bottom);
    pb.move_to(center_x - bar_half, bar_y);
    pb.line_to(center_x + bar_half, bar_y);
}

/// 向路径追加汉字 "文" 的骨架笔画 (点、横、撇、捺)
fn add_glyph_wen(pb: &mut PathBuilder, cx: f32, cy: f32, s: f32) {
    // 顶点
    pb.move_to(cx - 0.2 * s, cy - 2.6 * s);
    pb.line_to(cx + 0.2 * s, cy - 2.0 * s);
    // 横画
    pb.move_to(cx - 2.4 * s, cy - 1.3 * s);
    pb.line_to(cx + 2.4 * s, cy - 1.3 * s);
    // 撇画：自横画中部向左下展开
    pb.move_to(cx + 0.3 * s, cy - 1.3 * s);
    pb.quad_to(cx - 1.2 * s, cy + 0.7 * s, cx - 2.4 * s, cy + 2.7 * s);
    // 捺画：自横画中部向右下展开
    pb.move_to(cx - 0.3 * s, cy - 1.0 * s);
    pb.quad_to(cx + 1.3 * s, cy + 0.9 * s, cx + 2.3 * s, cy + 2.7 * s);
}

/// 采样按钮当前底色 (含悬停/激活高亮)，供叠层图标的前框作遮挡填充
fn sample_backdrop(pixmap: &PixmapMut, x: f32, y: f32) -> Color {
    let width = pixmap.width();
    let height = pixmap.height();
    let px = (x.max(0.0) as u32).min(width.saturating_sub(1));
    let py = (y.max(0.0) as u32).min(height.saturating_sub(1));
    pixmap
        .as_ref()
        .pixels()
        .get(py as usize * width as usize + px as usize)
        .map(|pixel| {
            let c = pixel.demultiply();
            Color::from_rgba8(c.red(), c.green(), c.blue(), c.alpha())
        })
        .unwrap_or(Color::TRANSPARENT)
}

/// 图标: 屏幕录制（显示器外框与实心录制圆点）
pub(super) fn render_icon_screen_recording(pixmap: &mut PixmapMut, painter: &IconPainter) {
    let (cx, cy, s) = (painter.center_x, painter.center_y, painter.scale);
    let mut frame = PathBuilder::new();
    add_round_rect(
        &mut frame,
        RoundRect {
            x: cx - 6.0 * s,
            y: cy - 4.5 * s,
            width: 12.0 * s,
            height: 8.5 * s,
            radius: 1.5 * s,
        },
    );
    if let Some(path) = frame.finish() {
        pixmap.stroke_path(
            &path,
            &painter.paint,
            &painter.stroke,
            Transform::identity(),
            None,
        );
    }
    let mut dot = PathBuilder::new();
    dot.push_circle(cx, cy - 0.2 * s, 2.3 * s);
    if let Some(path) = dot.finish() {
        let paint = make_paint(Color::from_rgba8(239, 68, 68, 255));
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
