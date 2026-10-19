//! 工具栏动作类别图标绘制实现。

use super::primitives::add_round_rect;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

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
