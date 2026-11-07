//! 录制工具栏的专用矢量图标。
//!
//! 标注类图标继续复用编辑工具栏，录制、音频、鼠标、设置和窗口控制图标在本模块
//! 按同一线宽与圆角风格补充，确保新增控件不会引入另一套视觉语言。

use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

fn ink() -> Color {
    Color::from_rgba8(45, 48, 56, 255)
}

fn muted() -> Color {
    Color::from_rgba8(148, 163, 184, 255)
}

fn danger() -> Color {
    Color::from_rgba8(239, 68, 68, 255)
}

fn active() -> Color {
    Color::from_rgba8(37, 99, 235, 255)
}

fn stroke(scale: f32) -> Stroke {
    Stroke {
        width: 1.7 * scale,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    }
}

fn paint(color: Color) -> Paint<'static> {
    make_paint(color)
}

/// 录制工具栏图标语义与状态。
#[derive(Debug, Clone, Copy)]
pub(super) enum IconVariant {
    /// 录制开始/停止切换。
    Record { recording: bool },
    /// 暂停或继续。
    Pause { paused: bool, enabled: bool },
    /// 系统声音开关。
    Audio { enabled: bool },
    /// 鼠标指针开关。
    Cursor { enabled: bool },
    /// 设置菜单。
    Settings,
    /// 下拉箭头。
    Chevron,
    /// 四向移动。
    Move,
    /// 关闭。
    Close,
    /// 重新开始。
    Restart,
}

/// 单个图标绘制参数。
#[derive(Debug, Clone, Copy)]
pub(super) struct IconSpec<'a> {
    /// 图标按钮矩形。
    pub(super) rect: &'a Rect,
    /// 界面缩放。
    pub(super) scale: f32,
    /// 是否悬停。
    pub(super) hovered: bool,
    /// 图标语义与业务开关。
    pub(super) variant: IconVariant,
}

struct Visual<'a> {
    rect: &'a Rect,
    scale: f32,
    hovered: bool,
}

/// 按语义绘制录制工具栏专用图标。
pub(super) fn draw_icon(pixmap: &mut PixmapMut, spec: IconSpec<'_>) {
    let visual = Visual {
        rect: spec.rect,
        scale: spec.scale,
        hovered: spec.hovered,
    };
    match spec.variant {
        IconVariant::Record { recording } => draw_record(pixmap, &visual, recording),
        IconVariant::Pause { paused, enabled } => draw_pause(pixmap, &visual, paused, enabled),
        IconVariant::Audio { enabled } => draw_audio(pixmap, &visual, enabled),
        IconVariant::Cursor { enabled } => draw_cursor(pixmap, &visual, enabled),
        IconVariant::Settings => draw_settings(pixmap, &visual),
        IconVariant::Chevron => draw_chevron(pixmap, &visual),
        IconVariant::Move => draw_move(pixmap, &visual),
        IconVariant::Close => draw_close(pixmap, &visual),
        IconVariant::Restart => draw_restart(pixmap, &visual),
    }
}

/// 绘制六点拖动手柄。
pub(super) fn draw_drag_handle(pixmap: &mut PixmapMut, rect: &Rect, scale: f32) {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    for column in [-1.8_f32, 1.8] {
        for row in [-4.0_f32, 0.0, 4.0] {
            path.push_circle((cx + column * scale).round(), cy + row * scale, 1.2 * scale);
        }
    }
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &paint(Color::from_rgba8(148, 163, 184, 210)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制录制切换按钮；开始显示红色圆点，录制中显示红色停止方块。
fn draw_record(pixmap: &mut PixmapMut, visual: &Visual<'_>, recording: bool) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut ring = PathBuilder::new();
    ring.push_circle(cx, cy, 6.0 * scale);
    if let Some(path) = ring.finish() {
        pixmap.stroke_path(
            &path,
            &paint(danger()),
            &Stroke {
                width: 1.4 * scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    let mut shape = PathBuilder::new();
    if recording {
        add_round_rect(
            &mut shape,
            RoundRect {
                x: cx - 3.0 * scale,
                y: cy - 3.0 * scale,
                width: 6.0 * scale,
                height: 6.0 * scale,
                radius: 1.0 * scale,
            },
        );
    } else {
        shape.push_circle(cx, cy, 3.1 * scale);
    }
    if let Some(path) = shape.finish() {
        pixmap.fill_path(
            &path,
            &paint(danger()),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制暂停或继续按钮。
fn draw_pause(pixmap: &mut PixmapMut, visual: &Visual<'_>, paused: bool, enabled: bool) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered && enabled {
        draw_hover(pixmap, rect, scale);
    }
    let color = if enabled { ink() } else { muted() };
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    if paused {
        path.move_to(cx - 3.2 * scale, cy - 5.0 * scale);
        path.line_to(cx + 4.6 * scale, cy);
        path.line_to(cx - 3.2 * scale, cy + 5.0 * scale);
        path.close();
        if let Some(path) = path.finish() {
            pixmap.fill_path(
                &path,
                &paint(color),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    } else {
        path.move_to(cx - 3.4 * scale, cy - 4.8 * scale);
        path.line_to(cx - 3.4 * scale, cy + 4.8 * scale);
        path.move_to(cx + 3.4 * scale, cy - 4.8 * scale);
        path.line_to(cx + 3.4 * scale, cy + 4.8 * scale);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(
                &path,
                &paint(color),
                &Stroke {
                    width: 2.4 * scale,
                    line_cap: tiny_skia::LineCap::Round,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }
}

/// 绘制系统声音图标。
fn draw_audio(pixmap: &mut PixmapMut, visual: &Visual<'_>, enabled: bool) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let color = if enabled { active() } else { ink() };
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - 5.5 * scale, cy - 2.2 * scale);
    path.line_to(cx - 3.0 * scale, cy - 2.2 * scale);
    path.line_to(cx + 0.5 * scale, cy - 5.0 * scale);
    path.line_to(cx + 0.5 * scale, cy + 5.0 * scale);
    path.line_to(cx - 3.0 * scale, cy + 2.2 * scale);
    path.line_to(cx - 5.5 * scale, cy + 2.2 * scale);
    path.close();
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let mut waves = PathBuilder::new();
    waves.move_to(cx + 2.0 * scale, cy - 2.8 * scale);
    waves.quad_to(cx + 4.2 * scale, cy, cx + 2.0 * scale, cy + 2.8 * scale);
    if enabled {
        waves.move_to(cx + 3.5 * scale, cy - 4.5 * scale);
        waves.quad_to(cx + 6.5 * scale, cy, cx + 3.5 * scale, cy + 4.5 * scale);
    } else {
        waves.move_to(cx + 2.0 * scale, cy + 5.5 * scale);
        waves.line_to(cx + 6.0 * scale, cy - 5.5 * scale);
    }
    if let Some(path) = waves.finish() {
        pixmap.stroke_path(
            &path,
            &paint(color),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

/// 绘制鼠标指针图标。
fn draw_cursor(pixmap: &mut PixmapMut, visual: &Visual<'_>, enabled: bool) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let color = if enabled { active() } else { ink() };
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - 3.4 * scale, cy - 5.8 * scale);
    path.line_to(cx + 4.3 * scale, cy + 1.0 * scale);
    path.line_to(cx + 0.7 * scale, cy + 1.3 * scale);
    path.line_to(cx + 2.3 * scale, cy + 5.4 * scale);
    path.line_to(cx + 0.2 * scale, cy + 6.2 * scale);
    path.line_to(cx - 1.4 * scale, cy + 2.0 * scale);
    path.line_to(cx - 3.8 * scale, cy + 3.8 * scale);
    path.close();
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &paint(Color::WHITE),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        pixmap.stroke_path(
            &path,
            &paint(color),
            &Stroke {
                width: 1.3 * scale,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制设置齿轮。
fn draw_settings(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.push_circle(cx, cy, 5.6 * scale);
    for index in 0..8 {
        let angle = std::f32::consts::TAU * index as f32 / 8.0;
        path.move_to(
            cx + angle.cos() * 3.6 * scale,
            cy + angle.sin() * 3.6 * scale,
        );
        path.line_to(
            cx + angle.cos() * 7.0 * scale,
            cy + angle.sin() * 7.0 * scale,
        );
    }
    path.push_circle(cx, cy, 1.8 * scale);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

/// 绘制下拉箭头。
fn draw_chevron(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale) = (visual.rect, visual.scale);
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - 2.4 * scale, cy - 1.2 * scale);
    path.line_to(cx, cy + 1.4 * scale);
    path.line_to(cx + 2.4 * scale, cy - 1.2 * scale);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

/// 绘制四向移动图标。
fn draw_move(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - 5.2 * scale, cy);
    path.line_to(cx + 5.2 * scale, cy);
    path.move_to(cx, cy - 5.2 * scale);
    path.line_to(cx, cy + 5.2 * scale);
    for (dx, dy) in [(-1.0_f32, 0.0_f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        let tip_x = cx + dx * 6.6 * scale;
        let tip_y = cy + dy * 6.6 * scale;
        let base_x = cx + dx * 3.8 * scale;
        let base_y = cy + dy * 3.8 * scale;
        let side_x = -dy * 1.8 * scale;
        let side_y = dx * 1.8 * scale;
        path.move_to(tip_x, tip_y);
        path.line_to(base_x + side_x, base_y + side_y);
        path.line_to(base_x - side_x, base_y - side_y);
        path.close();
    }
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

/// 绘制关闭叉号。
fn draw_close(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - 5.0 * scale, cy - 5.0 * scale);
    path.line_to(cx + 5.0 * scale, cy + 5.0 * scale);
    path.move_to(cx + 5.0 * scale, cy - 5.0 * scale);
    path.line_to(cx - 5.0 * scale, cy + 5.0 * scale);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

/// 绘制重新开始圆弧箭头。
fn draw_restart(pixmap: &mut PixmapMut, visual: &Visual<'_>) {
    let (rect, scale, hovered) = (visual.rect, visual.scale, visual.hovered);
    if hovered {
        draw_hover(pixmap, rect, scale);
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx + 4.8 * scale, cy - 1.4 * scale);
    path.cubic_to(
        cx + 4.1 * scale,
        cy - 5.0 * scale,
        cx - 0.8 * scale,
        cy - 6.1 * scale,
        cx - 3.9 * scale,
        cy - 3.4 * scale,
    );
    path.cubic_to(
        cx - 7.6 * scale,
        cy - 0.2 * scale,
        cx - 5.7 * scale,
        cy + 5.2 * scale,
        cx - 1.4 * scale,
        cy + 5.4 * scale,
    );
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
    let mut head = PathBuilder::new();
    head.move_to(cx + 2.0 * scale, cy - 4.8 * scale);
    head.line_to(cx + 6.3 * scale, cy - 2.2 * scale);
    head.line_to(cx + 2.8 * scale, cy + 0.6 * scale);
    if let Some(path) = head.finish() {
        pixmap.stroke_path(
            &path,
            &paint(ink()),
            &stroke(scale),
            Transform::identity(),
            None,
        );
    }
}

fn draw_hover(pixmap: &mut PixmapMut, rect: &Rect, scale: f32) {
    let mut path = PathBuilder::new();
    add_round_rect(
        &mut path,
        RoundRect {
            x: rect.x + 2.0 * scale,
            y: rect.y + 4.0 * scale,
            width: rect.width - 4.0 * scale,
            height: rect.height - 8.0 * scale,
            radius: 5.0 * scale,
        },
    );
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &paint(Color::from_rgba8(15, 23, 42, 18)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
