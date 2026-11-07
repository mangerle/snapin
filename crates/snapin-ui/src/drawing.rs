//! 共享 tiny-skia 绘制原语。
//!
//! 工具栏图标、贴图窗口边框与右键菜单都使用同一套抗锯齿画笔与路径构造，
//! 避免各渲染模块各自复制圆角矩形/椭圆路径而产生细微视觉漂移。

use tiny_skia::{Color, Paint, PathBuilder};

/// 圆角矩形几何参数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RoundRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

/// 椭圆几何参数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ellipse {
    pub center_x: f32,
    pub center_y: f32,
    pub radius_x: f32,
    pub radius_y: f32,
}

/// 创建开启抗锯齿的基础画笔。
pub(crate) fn make_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    paint
}

/// 向 PathBuilder 添加直角矩形路径。
pub(crate) fn add_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pb.push_rect(r);
    }
}

/// 向 PathBuilder 添加圆角矩形路径。
pub(crate) fn add_round_rect(pb: &mut PathBuilder, rect: RoundRect) {
    let radius = rect.radius.min(rect.width / 2.0).min(rect.height / 2.0);
    pb.move_to(rect.x + radius, rect.y);
    pb.line_to(rect.x + rect.width - radius, rect.y);
    pb.quad_to(
        rect.x + rect.width,
        rect.y,
        rect.x + rect.width,
        rect.y + radius,
    );
    pb.line_to(rect.x + rect.width, rect.y + rect.height - radius);
    pb.quad_to(
        rect.x + rect.width,
        rect.y + rect.height,
        rect.x + rect.width - radius,
        rect.y + rect.height,
    );
    pb.line_to(rect.x + radius, rect.y + rect.height);
    pb.quad_to(
        rect.x,
        rect.y + rect.height,
        rect.x,
        rect.y + rect.height - radius,
    );
    pb.line_to(rect.x, rect.y + radius);
    pb.quad_to(rect.x, rect.y, rect.x + radius, rect.y);
    pb.close();
}

/// 向 PathBuilder 添加椭圆路径。
pub(crate) fn add_ellipse(pb: &mut PathBuilder, ellipse: Ellipse) {
    let offset_x = ellipse.radius_x * 0.552_284_8_f32;
    let offset_y = ellipse.radius_y * 0.552_284_8_f32;
    let (cx, cy) = (ellipse.center_x, ellipse.center_y);
    let (rx, ry) = (ellipse.radius_x, ellipse.radius_y);
    pb.move_to(cx - rx, cy);
    pb.cubic_to(cx - rx, cy - offset_y, cx - offset_x, cy - ry, cx, cy - ry);
    pb.cubic_to(cx + offset_x, cy - ry, cx + rx, cy - offset_y, cx + rx, cy);
    pb.cubic_to(cx + rx, cy + offset_y, cx + offset_x, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - offset_x, cy + ry, cx - rx, cy + offset_y, cx - rx, cy);
    pb.close();
}
