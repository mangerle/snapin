//! 工具栏动作类别图标绘制实现。

use super::primitives::{add_round_rect, make_paint};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 图标 8: 向左回转撤销
pub fn render_icon_undo(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
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
pub fn render_icon_redo(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
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
pub fn render_icon_close(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 5.0 * s, cy - 5.0 * s);
    pb.line_to(cx + 5.0 * s, cy + 5.0 * s);
    pb.move_to(cx + 5.0 * s, cy - 5.0 * s);
    pb.line_to(cx - 5.0 * s, cy + 5.0 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 11: 经典大头工字图钉 (贴图置顶)
pub fn render_icon_pin(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
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
pub fn render_icon_save(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
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
pub fn render_icon_copy(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb_back = PathBuilder::new();
    add_round_rect(
        &mut pb_back,
        cx - 3.0 * s,
        cy - 6.5 * s,
        9.5 * s,
        9.5 * s,
        1.5 * s,
    );
    if let Some(path) = pb_back.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
    let mut pb_front = PathBuilder::new();
    add_round_rect(
        &mut pb_front,
        cx - 6.5 * s,
        cy - 3.0 * s,
        9.5 * s,
        9.5 * s,
        1.5 * s,
    );
    if let Some(path) = pb_front.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 14: 四角取景框内嵌大写 A (文字识别)
pub fn render_icon_ocr(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
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
    add_glyph_a(&mut pb, cx, cy, 7.0 * s, 6.0 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 15: 双叠层圆角方框内嵌 "A" 与 "文" (截图翻译)
///
/// 前框先以按钮当前底色填充再描边，遮住后框被压住的一角，
/// 从而在任意悬停/激活高亮状态下都呈现干净的纸张叠层效果。
pub fn render_icon_translate(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let side = 9.6 * s;
    let radius = 1.5 * s;
    let back = (cx - 1.6 * s, cy - 7.8 * s);
    let front = (cx - 8.0 * s, cy - 1.8 * s);
    // 内嵌字符用细一号笔画，避免小字号下方框与字形粘连成实心块
    let glyph_stroke = Stroke {
        width: stroke.width * 0.72,
        ..stroke.clone()
    };
    let backdrop = sample_backdrop(pixmap, front.0 + side / 2.0, front.1 + side / 2.0);

    // 1. 后框轮廓
    let mut pb_back = PathBuilder::new();
    add_round_rect(&mut pb_back, back.0, back.1, side, side, radius);
    if let Some(path) = pb_back.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }

    // 2. 后框字母 A
    let mut pb_a = PathBuilder::new();
    add_glyph_a(
        &mut pb_a,
        back.0 + side / 2.0,
        back.1 + side / 2.0,
        5.2 * s,
        4.4 * s,
    );
    if let Some(path) = pb_a.finish() {
        pixmap.stroke_path(&path, paint, &glyph_stroke, Transform::identity(), None);
    }

    // 3. 前框底色填充遮蔽后框左下角，随后描边
    let mut pb_front = PathBuilder::new();
    add_round_rect(&mut pb_front, front.0, front.1, side, side, radius);
    if let Some(path) = pb_front.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(backdrop),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }

    // 4. 前框汉字 "文"
    let mut pb_wen = PathBuilder::new();
    add_glyph_wen(&mut pb_wen, front.0 + side / 2.0, front.1 + side / 2.0, s);
    if let Some(path) = pb_wen.finish() {
        pixmap.stroke_path(&path, paint, &glyph_stroke, Transform::identity(), None);
    }
}

/// 向路径追加无衬线大写字母 A，尖顶居中、横梁位于字高约三分之二处
fn add_glyph_a(pb: &mut PathBuilder, cx: f32, cy: f32, height: f32, width: f32) {
    let top = cy - height / 2.0;
    let bottom = cy + height / 2.0;
    let half = width / 2.0;
    let bar_y = top + height * 0.68;
    let bar_half = half * 0.66;
    pb.move_to(cx - half, bottom);
    pb.line_to(cx, top);
    pb.line_to(cx + half, bottom);
    pb.move_to(cx - bar_half, bar_y);
    pb.line_to(cx + bar_half, bar_y);
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
