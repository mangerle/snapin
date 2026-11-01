//! 共享 tiny-skia 绘制原语。
//!
//! 工具栏图标、贴图窗口边框与右键菜单都使用同一套抗锯齿画笔与路径构造，
//! 避免各渲染模块各自复制圆角矩形/椭圆路径而产生细微视觉漂移。

use tiny_skia::{Color, Paint, PathBuilder};

/// 创建开启抗锯齿的基础画笔。
pub(crate) fn make_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    paint
}

/// 向 PathBuilder 添加圆角矩形路径。
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

/// 向 PathBuilder 添加椭圆路径。
pub(crate) fn add_ellipse(pb: &mut PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32) {
    let ox = rx * 0.552_284_8_f32;
    let oy = ry * 0.552_284_8_f32;
    pb.move_to(cx - rx, cy);
    pb.cubic_to(cx - rx, cy - oy, cx - ox, cy - ry, cx, cy - ry);
    pb.cubic_to(cx + ox, cy - ry, cx + rx, cy - oy, cx + rx, cy);
    pb.cubic_to(cx + rx, cy + oy, cx + ox, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - ox, cy + ry, cx - rx, cy + oy, cx - rx, cy);
    pb.close();
}
