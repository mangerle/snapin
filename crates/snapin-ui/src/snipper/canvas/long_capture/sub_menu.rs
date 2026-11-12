//! 长截图裁剪展开子菜单单源几何布局、光栅化渲染与命中测试。
//!
//! # 设计原理
//!
//! 将 `[上/左剪] [下/右剪] [撤销] | [✓ 自动裁剪]` 的几何排布收敛为统一的
//! `CropSubMenuLayout`，供渲染光栅化与鼠标命中测试共同消费，消除坐标硬编码重复与偏移；
//! 同时引入完整视口夹取和上下自动翻折，确保即使控制栏紧贴屏幕底端或右边缘，
//! 展开的子菜单也能完整留在可视区内部。

use super::icons::{
    draw_checkbox, draw_crop_icon, draw_undo_crop_icon, CropIconStyle, UndoIconStyle,
};
use super::CanvasSize;
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use crate::snipper::window::state::long_capture::{LongCaptureAction, LongCaptureSession};
use snapin_foundation::i18n::tr;
use snapin_render::stitching::StitchDirection;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 子菜单布局请求上下文（对齐主工具栏与屏幕可视边界）。
#[derive(Clone, Copy)]
pub(crate) struct CropSubMenuPlacement {
    pub(crate) scissors_x: f32,
    pub(crate) bar_rect: Rect,
    pub(crate) screen_size: CanvasSize,
    pub(crate) ui_scale: f32,
}

/// 裁剪展开子菜单的统一控件矩形集合。
#[derive(Debug, Clone, Copy)]
pub(crate) struct CropSubMenuLayout {
    pub(crate) sub_rect: Rect,
    pub(crate) head_rect: Rect,
    pub(crate) tail_rect: Rect,
    pub(crate) undo_rect: Rect,
    pub(crate) div_x: f32,
    pub(crate) auto_rect: Rect,
    pub(crate) auto_box_x: f32,
    pub(crate) auto_box_y: f32,
}

/// 计算子菜单及各内部按钮的屏幕矩形，贴近底边时自动翻折至主栏上方，并全向夹取到屏幕内。
pub(crate) fn resolve_crop_submenu_layout(placement: CropSubMenuPlacement) -> CropSubMenuLayout {
    let s = placement.ui_scale;
    let margin = 8.0 * s;
    let sub_w = 200.0 * s;
    let sub_h = 32.0 * s;
    let screen_w = placement.screen_size.width as f32;
    let screen_h = placement.screen_size.height as f32;

    let max_x = (screen_w - sub_w - margin).max(margin);
    let sub_x = placement.scissors_x.clamp(margin, max_x);

    let below_y = placement.bar_rect.y + placement.bar_rect.height + 4.0 * s;
    let above_y = placement.bar_rect.y - sub_h - 4.0 * s;
    let max_y = (screen_h - sub_h - margin).max(margin);
    let preferred_y = if below_y + sub_h > screen_h - margin && above_y >= margin {
        above_y
    } else {
        below_y
    };
    let sub_y = preferred_y.clamp(margin, max_y);

    let sub_rect = Rect {
        x: sub_x,
        y: sub_y,
        width: sub_w,
        height: sub_h,
    };
    compute_sub_controls(sub_rect, s)
}

fn compute_sub_controls(sub_rect: Rect, s: f32) -> CropSubMenuLayout {
    let btn_y = sub_rect.y + 3.0 * s;
    let btn_h = sub_rect.height - 6.0 * s;
    let btn_w = 26.0 * s;
    CropSubMenuLayout {
        sub_rect,
        head_rect: Rect::new(sub_rect.x + 6.0 * s, btn_y, btn_w, btn_h),
        tail_rect: Rect::new(sub_rect.x + 36.0 * s, btn_y, btn_w, btn_h),
        undo_rect: Rect::new(sub_rect.x + 66.0 * s, btn_y, btn_w, btn_h),
        div_x: sub_rect.x + 98.0 * s,
        auto_rect: Rect::new(sub_rect.x + 104.0 * s, btn_y, 90.0 * s, btn_h),
        auto_box_x: sub_rect.x + 108.0 * s,
        auto_box_y: sub_rect.y + (sub_rect.height - 13.0 * s) / 2.0,
    }
}

/// 绘制裁剪展开子浮条，并返回子菜单内部正在悬停的目标矩形与提示键（若有）。
pub(crate) fn render_crop_sub_menu(
    pixmap: &mut PixmapMut,
    session: &mut LongCaptureSession,
    placement: CropSubMenuPlacement,
) -> Option<(Rect, &'static str)> {
    let s = placement.ui_scale;
    let layout = resolve_crop_submenu_layout(placement);
    session.crop_submenu_rect = Some(layout.sub_rect);

    draw_submenu_panel(pixmap, &layout.sub_rect, s);
    draw_submenu_divider(pixmap, &layout, s);

    let crop_enabled = session.can_crop();
    let head_hovered = session.hovered_action == Some(LongCaptureAction::CropHead);
    draw_sub_icon_button(pixmap, &layout.head_rect, head_hovered && crop_enabled, s);
    draw_crop_icon(
        pixmap,
        layout.head_rect.x + layout.head_rect.width / 2.0,
        layout.head_rect.y + layout.head_rect.height / 2.0,
        CropIconStyle {
            direction: session.direction,
            is_head: true,
            scale: s,
            hovered: head_hovered,
            enabled: crop_enabled,
        },
    );

    let tail_hovered = session.hovered_action == Some(LongCaptureAction::CropTail);
    draw_sub_icon_button(pixmap, &layout.tail_rect, tail_hovered && crop_enabled, s);
    draw_crop_icon(
        pixmap,
        layout.tail_rect.x + layout.tail_rect.width / 2.0,
        layout.tail_rect.y + layout.tail_rect.height / 2.0,
        CropIconStyle {
            direction: session.direction,
            is_head: false,
            scale: s,
            hovered: tail_hovered,
            enabled: crop_enabled,
        },
    );

    let undo_enabled = session.can_undo_crop();
    let undo_hovered = session.hovered_action == Some(LongCaptureAction::UndoCrop);
    draw_sub_icon_button(pixmap, &layout.undo_rect, undo_hovered && undo_enabled, s);
    draw_undo_crop_icon(
        pixmap,
        layout.undo_rect.x + layout.undo_rect.width / 2.0,
        layout.undo_rect.y + layout.undo_rect.height / 2.0,
        UndoIconStyle {
            scale: s,
            hovered: undo_hovered,
            enabled: undo_enabled,
        },
    );

    let auto_hovered = session.hovered_action == Some(LongCaptureAction::ToggleAutoCrop);
    draw_auto_crop_control(pixmap, &layout, session.auto_crop, auto_hovered, s);

    submenu_hover_tooltip(session, &layout)
}

fn submenu_hover_tooltip(
    session: &LongCaptureSession,
    layout: &CropSubMenuLayout,
) -> Option<(Rect, &'static str)> {
    match session.hovered_action? {
        LongCaptureAction::CropHead => Some((
            layout.head_rect,
            if !session.can_crop() {
                "已达最小尺寸，无法继续裁剪"
            } else {
                match session.direction {
                    StitchDirection::Vertical => "上裁剪 (30px)",
                    StitchDirection::Horizontal => "左裁剪 (30px)",
                }
            },
        )),
        LongCaptureAction::CropTail => Some((
            layout.tail_rect,
            if !session.can_crop() {
                "已达最小尺寸，无法继续裁剪"
            } else {
                match session.direction {
                    StitchDirection::Vertical => "下裁剪 (30px)",
                    StitchDirection::Horizontal => "右裁剪 (30px)",
                }
            },
        )),
        LongCaptureAction::UndoCrop => Some((
            layout.undo_rect,
            if session.can_undo_crop() {
                "撤销裁剪 (Ctrl+Z)"
            } else {
                "暂无可撤销的裁剪记录"
            },
        )),
        LongCaptureAction::ToggleAutoCrop => Some((layout.auto_rect, "反向滚动时自动裁剪多余画面")),
        _ => None,
    }
}

fn draw_submenu_panel(pixmap: &mut PixmapMut, sub_rect: &Rect, s: f32) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: sub_rect.x,
            y: sub_rect.y,
            width: sub_rect.width,
            height: sub_rect.height,
            radius: 5.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 252);
        pixmap.fill_path(
            &path,
            &bg_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut stroke_paint = Paint::default();
        stroke_paint.set_color_rgba8(0, 0, 0, 26);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }
}

fn draw_submenu_divider(pixmap: &mut PixmapMut, layout: &CropSubMenuLayout, s: f32) {
    let mut div_pb = PathBuilder::new();
    div_pb.move_to(layout.div_x, layout.sub_rect.y + 6.0 * s);
    div_pb.line_to(
        layout.div_x,
        layout.sub_rect.y + layout.sub_rect.height - 6.0 * s,
    );
    if let Some(div_path) = div_pb.finish() {
        let mut div_paint = Paint::default();
        div_paint.set_color_rgba8(0, 0, 0, 22);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&div_path, &div_paint, &stroke, Transform::identity(), None);
    }
}

fn draw_auto_crop_control(
    pixmap: &mut PixmapMut,
    layout: &CropSubMenuLayout,
    checked: bool,
    hovered: bool,
    s: f32,
) {
    if hovered {
        draw_button_hover_pill(pixmap, &layout.auto_rect, s, 10);
    }
    draw_checkbox(
        pixmap,
        layout.auto_box_x,
        layout.auto_box_y,
        checked,
        s,
        hovered,
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            tr("自动裁剪"),
            TextDrawOptions::new(
                layout.auto_box_x + 18.0 * s,
                layout.sub_rect.y + 7.5 * s,
                11.0 * s,
                (45, 52, 65, 255),
            ),
        );
    });
}

fn draw_sub_icon_button(pixmap: &mut PixmapMut, rect: &Rect, is_hovered: bool, s: f32) {
    if is_hovered {
        draw_button_hover_pill(pixmap, rect, s, 16);
    }
}

fn draw_button_hover_pill(pixmap: &mut PixmapMut, rect: &Rect, s: f32, alpha: u8) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 3.5 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, alpha);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 命中测试裁剪子菜单（完全复用同一 `compute_sub_controls` 布局基准）。
pub(crate) fn hit_test_crop_sub_menu(
    session: &LongCaptureSession,
    cursor_x: f32,
    cursor_y: f32,
    s: f32,
) -> Option<LongCaptureAction> {
    let sub_rect = session.crop_submenu_rect?;
    if !sub_rect.contains(cursor_x, cursor_y) {
        return None;
    }

    let layout = compute_sub_controls(sub_rect, s);
    if layout.head_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::CropHead);
    }
    if layout.tail_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::CropTail);
    }
    if layout.undo_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::UndoCrop);
    }
    if layout.auto_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::ToggleAutoCrop);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn test_crop_submenu_flips_above_at_bottom_and_clamps_right() {
        let screen = CanvasSize {
            width: 800,
            height: 600,
        };
        let bar_rect = Rect::new(450.0, 556.0, 342.0, 36.0);
        let layout = resolve_crop_submenu_layout(CropSubMenuPlacement {
            scissors_x: 720.0,
            bar_rect,
            screen_size: screen,
            ui_scale: 1.0,
        });

        assert!(
            layout.sub_rect.y + layout.sub_rect.height <= bar_rect.y,
            "工具栏贴底时子菜单应自动向上翻折"
        );
        assert!(layout.sub_rect.y >= 8.0);
        assert!(
            layout.sub_rect.x + layout.sub_rect.width <= (screen.width as f32) - 8.0,
            "靠右剪刀位置展开子菜单不得溢出右边界"
        );
    }

    #[test]
    fn test_crop_submenu_hit_test_covers_undo_crop_button() {
        let mut session =
            LongCaptureSession::new(RgbaImage::from_pixel(100, 120, Rgba([10, 20, 30, 255])));
        let bar_rect = Rect::new(100.0, 100.0, 320.0, 36.0);
        let layout = resolve_crop_submenu_layout(CropSubMenuPlacement {
            scissors_x: 180.0,
            bar_rect,
            screen_size: CanvasSize {
                width: 800,
                height: 600,
            },
            ui_scale: 1.0,
        });
        session.crop_submenu_rect = Some(layout.sub_rect);

        assert_eq!(
            hit_test_crop_sub_menu(
                &session,
                layout.head_rect.x + 4.0,
                layout.head_rect.y + 4.0,
                1.0
            ),
            Some(LongCaptureAction::CropHead)
        );
        assert_eq!(
            hit_test_crop_sub_menu(
                &session,
                layout.tail_rect.x + 4.0,
                layout.tail_rect.y + 4.0,
                1.0
            ),
            Some(LongCaptureAction::CropTail)
        );
        assert_eq!(
            hit_test_crop_sub_menu(
                &session,
                layout.undo_rect.x + 4.0,
                layout.undo_rect.y + 4.0,
                1.0
            ),
            Some(LongCaptureAction::UndoCrop)
        );
        assert_eq!(
            hit_test_crop_sub_menu(
                &session,
                layout.auto_rect.x + 4.0,
                layout.auto_rect.y + 4.0,
                1.0
            ),
            Some(LongCaptureAction::ToggleAutoCrop)
        );
    }

    #[test]
    fn test_crop_submenu_shows_min_size_tooltip_when_tiny_selection_cannot_crop() {
        let mut session =
            LongCaptureSession::new(RgbaImage::from_pixel(60, 24, Rgba([10, 20, 30, 255])));
        assert!(!session.can_crop());
        let bar_rect = Rect::new(100.0, 100.0, 320.0, 36.0);
        let layout = resolve_crop_submenu_layout(CropSubMenuPlacement {
            scissors_x: 180.0,
            bar_rect,
            screen_size: CanvasSize {
                width: 800,
                height: 600,
            },
            ui_scale: 1.0,
        });
        session.hovered_action = Some(LongCaptureAction::CropHead);
        let tip = submenu_hover_tooltip(&session, &layout);
        assert_eq!(tip, Some((layout.head_rect, "已达最小尺寸，无法继续裁剪")));
    }
}
