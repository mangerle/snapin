//! 工具栏矢量图标绘制模块。
//!
//! 提供经典现代浅色风格的矢量图标高精光栅化渲染。

use super::items::ToolbarItem;
use crate::snipper::selection::Rect;
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 创建开启抗锯齿的基础画笔
pub(crate) fn make_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    paint
}

/// 向 PathBuilder 添加圆角矩形
pub(crate) fn add_round_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let r = r.min(w / 2.0).min(h / 2.0);
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
}

/// 绘制各项具体矢量图标 (严格对齐经典工具栏图标造型，深炭黑灰线条)
pub fn render_item_icon(
    pixmap: &mut PixmapMut,
    item: ToolbarItem,
    rect: &Rect,
    s: f32,
    active_color: Color,
) {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let paint = make_paint(Color::from_rgba8(45, 48, 56, 255));
    let stroke = Stroke {
        width: 1.8 * s,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };

    match item {
        ToolbarItem::ToolRect => render_icon_rect(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ToolArrow => {
            render_icon_arrow(pixmap, cx, cy, s, active_color, &paint, &stroke)
        }
        ToolbarItem::ToolPen => render_icon_pen(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ToolMarker => render_icon_marker(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ToolMosaic => render_icon_mosaic(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ToolText => render_icon_text(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ToolBadge => render_icon_badge(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ActionUndo => render_icon_undo(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ActionRedo => render_icon_redo(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ActionClose => render_icon_close(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ActionPin => render_icon_pin(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ActionSave => render_icon_save(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ActionCopy => render_icon_copy(pixmap, cx, cy, s, &paint, &stroke),
        ToolbarItem::ToolEllipse => {}
    }
}

/// 图标 1: 几何图形 (矩形与扁椭圆相交组合)
fn render_icon_rect(
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
    let a = 5.5 * s;
    let b = 4.0 * s;
    let ox = a * 0.552_284_8_f32;
    let oy = b * 0.552_284_8_f32;
    let mut epb = PathBuilder::new();
    epb.move_to(ecx - a, ecy);
    epb.cubic_to(ecx - a, ecy - oy, ecx - ox, ecy - b, ecx, ecy - b);
    epb.cubic_to(ecx + ox, ecy - b, ecx + a, ecy - oy, ecx + a, ecy);
    epb.cubic_to(ecx + a, ecy + oy, ecx + ox, ecy + b, ecx, ecy + b);
    epb.cubic_to(ecx - ox, ecy + b, ecx - a, ecy + oy, ecx - a, ecy);
    epb.close();
    if let Some(path) = epb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 2: 折线上升指示箭头 + 右上当前所选颜色微圆点
fn render_icon_arrow(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    active_color: Color,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 7.0 * s, cy + 3.5 * s);
    pb.line_to(cx - 3.0 * s, cy + 6.0 * s);
    pb.line_to(cx + 1.0 * s, cy - 1.5 * s);
    pb.line_to(cx + 6.0 * s, cy - 4.5 * s);
    pb.move_to(cx + 2.0 * s, cy - 5.5 * s);
    pb.line_to(cx + 6.5 * s, cy - 5.0 * s);
    pb.line_to(cx + 5.5 * s, cy - 0.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }

    let mut dpb = PathBuilder::new();
    dpb.push_circle(cx + 6.5 * s, cy - 7.0 * s, 2.2 * s);
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

/// 图标 3: 45度经典木质铅笔
fn render_icon_pen(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 2.5 * s, cy + 2.5 * s);
    pb.line_to(cx + 3.5 * s, cy - 3.5 * s);
    pb.line_to(cx + 6.0 * s, cy - 1.0 * s);
    pb.line_to(cx, cy + 5.0 * s);
    pb.close();
    pb.move_to(cx - 2.5 * s, cy + 2.5 * s);
    pb.line_to(cx - 6.5 * s, cy + 6.5 * s);
    pb.line_to(cx, cy + 5.0 * s);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }

    let mut tip_pb = PathBuilder::new();
    tip_pb.move_to(cx - 4.5 * s, cy + 4.5 * s);
    tip_pb.line_to(cx - 6.5 * s, cy + 6.5 * s);
    tip_pb.line_to(cx - 3.5 * s, cy + 5.8 * s);
    tip_pb.close();
    if let Some(tpath) = tip_pb.finish() {
        pixmap.fill_path(
            &tpath,
            paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 图标 4: 荧光记号笔 (倒梯形宽笔头 + 笔身)
fn render_icon_marker(
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
fn render_icon_mosaic(
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
fn render_icon_text(
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

/// 图标 7: 步骤序号印章徽标
fn render_icon_badge(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx - 1.5 * s, cy + 1.5 * s, 5.5 * s);
    pb.move_to(cx + 2.5 * s, cy - 2.5 * s);
    pb.line_to(cx + 6.5 * s, cy - 6.5 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 8: 向左回转撤销
fn render_icon_undo(
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
fn render_icon_redo(
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
fn render_icon_close(
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

/// 图标 11: 45度立体图钉
fn render_icon_pin(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    paint: &Paint,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 2.0 * s, cy - 7.0 * s);
    pb.line_to(cx + 7.0 * s, cy + 2.0 * s);
    pb.move_to(cx, cy - 5.0 * s);
    pb.line_to(cx - 4.5 * s, cy - 0.5 * s);
    pb.line_to(cx - 2.0 * s, cy + 2.0 * s);
    pb.line_to(cx - 7.0 * s, cy + 7.0 * s);
    pb.line_to(cx - 2.0 * s, cy + 7.0 * s);
    pb.line_to(cx - 0.5 * s, cy + 4.5 * s);
    pb.line_to(cx + 5.0 * s, cy);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

/// 图标 12: 经典 3.5 寸软盘
fn render_icon_save(
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
fn render_icon_copy(
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
