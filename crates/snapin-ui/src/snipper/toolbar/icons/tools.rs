//! 标注工具类别图标绘制实现。

use super::primitives::{add_ellipse, add_round_rect, make_paint};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 图标: 纯椭圆几何造型
pub fn render_icon_ellipse(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut epb = PathBuilder::new();
    add_ellipse(&mut epb, cx, cy, 8.5 * s, 6.0 * s);
    if let Some(path) = epb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 1: 几何图形 (矩形与扁椭圆相交组合)
pub fn render_icon_rect(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        cx - 8.0 * s,
        cy - 7.5 * s,
        10.0 * s,
        8.5 * s,
        1.5 * s,
    );
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
    let ecx = cx + 2.5 * s;
    let ecy = cy + 2.5 * s;
    let mut epb = PathBuilder::new();
    add_ellipse(&mut epb, ecx, ecy, 5.5 * s, 4.0 * s);
    if let Some(path) = epb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 2: 坚挺斜向指示箭头 + 右上当前所选颜色微圆点
pub fn render_icon_arrow(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    active_color: Color,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    // 主干箭杆：从左下坚挺刺向右上
    pb.move_to(cx - 6.5 * s, cy + 6.5 * s);
    pb.line_to(cx + 4.5 * s, cy - 4.5 * s);
    // 锐利对称箭头翼
    pb.move_to(cx - 0.5 * s, cy - 4.5 * s);
    pb.line_to(cx + 4.5 * s, cy - 4.5 * s);
    pb.line_to(cx + 4.5 * s, cy + 0.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }

    let mut dpb = PathBuilder::new();
    dpb.push_circle(cx + 6.5 * s, cy - 6.5 * s, 2.2 * s);
    if let Some(dpath) = dpb.finish() {
        let dot_paint = make_paint(active_color);
        pixmap.fill_path(
            &dpath,
            &dot_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 图标 3: 45度涂鸦画笔 + 笔尖波浪涂鸦轨迹
pub fn render_icon_pen(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    // 笔身与金属箍
    pb.move_to(cx - 1.5 * s, cy + 1.5 * s);
    pb.line_to(cx + 4.5 * s, cy - 4.5 * s);
    pb.line_to(cx + 6.5 * s, cy - 2.5 * s);
    pb.line_to(cx + 0.5 * s, cy + 3.5 * s);
    pb.close();
    // 聚拢笔尖
    pb.move_to(cx - 1.5 * s, cy + 1.5 * s);
    pb.line_to(cx - 4.0 * s, cy + 4.0 * s);
    pb.line_to(cx + 0.5 * s, cy + 3.5 * s);
    // 笔尖下方涂鸦轨迹线条
    pb.move_to(cx - 6.5 * s, cy + 6.5 * s);
    pb.quad_to(cx - 3.5 * s, cy + 4.5 * s, cx - 1.5 * s, cy + 7.0 * s);
    pb.quad_to(cx + 0.5 * s, cy + 9.0 * s, cx + 3.5 * s, cy + 6.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 4: 荧光记号笔 (倒梯形宽笔头 + 笔身)
pub fn render_icon_marker(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 6.5 * s, cy + 4.5 * s);
    pb.line_to(cx - 3.5 * s, cy + 7.5 * s);
    pb.line_to(cx - 1.0 * s, cy + 5.0 * s);
    pb.line_to(cx - 4.0 * s, cy + 2.0 * s);
    pb.close();
    pb.move_to(cx - 2.0 * s, cy + 3.0 * s);
    pb.line_to(cx + 5.5 * s, cy - 4.5 * s);
    pb.line_to(cx + 7.5 * s, cy - 2.5 * s);
    pb.line_to(cx, cy + 5.0 * s);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 5: 2x2 黑白对角棋盘格马赛克
pub fn render_icon_mosaic(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let sz = 12.0 * s;
    let hsz = sz / 2.0;
    let x0 = cx - hsz;
    let y0 = cy - hsz;
    let mut pb = PathBuilder::new();
    add_round_rect(&mut pb, x0, y0, sz, sz, 1.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }

    if let Some(r1) = tiny_skia::Rect::from_xywh(x0, y0, hsz, hsz) {
        let mut p1 = PathBuilder::new();
        p1.push_rect(r1);
        if let Some(path) = p1.finish() {
            pixmap.fill_path(
                &path,
                paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    if let Some(r2) = tiny_skia::Rect::from_xywh(x0 + hsz, y0 + hsz, hsz, hsz) {
        let mut p2 = PathBuilder::new();
        p2.push_rect(r2);
        if let Some(path) = p2.finish() {
            pixmap.fill_path(
                &path,
                paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 图标 6: 现代无衬线粗体大写 T
pub fn render_icon_text(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 6.5 * s, cy - 6.0 * s);
    pb.line_to(cx + 6.5 * s, cy - 6.0 * s);
    pb.move_to(cx, cy - 6.0 * s);
    pb.line_to(cx, cy + 6.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 7: 圆圈数字 1 步骤序号徽标
pub fn render_icon_badge(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    // 外部规整正圆
    pb.push_circle(cx, cy, 7.5 * s);
    // 内部数字 1：顶部左小折撇，垂直主干线，底部横托
    pb.move_to(cx - 1.8 * s, cy - 1.8 * s);
    pb.line_to(cx + 0.3 * s, cy - 3.8 * s);
    pb.line_to(cx + 0.3 * s, cy + 3.8 * s);
    pb.move_to(cx - 1.8 * s, cy + 3.8 * s);
    pb.line_to(cx + 2.5 * s, cy + 3.8 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}
