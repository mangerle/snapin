//! 长截图白色悬浮控制栏、侧边缩略预览与视口指示器模块。
//!
//! # 设计原理
//!
//! 深度复刻 PixPin 长截图交互标准与 snapin 原生工具栏设计规范：
//! 1. 采用白色悬浮圆角卡片底板，与编辑器主工具栏风格保持高度统一；
//! 2. 位于选区上方左侧对齐（上方空间不足时自适应放置在选区下方），避免遮挡选区内容；
//! 3. 完整采用纯矢量图标呈现所有控制动作，杜绝低效文字堆砌：
//!    - 左侧实时像素尺寸与双向指示箭头，右端六点抓手仅用于移动控制栏
//!    - 垂直 / 水平截图方向下拉胶囊按钮
//!    - 剪刀造型裁剪按钮与下方弹出式子菜单（上/下/左/右剪与自动裁剪开关）
//!    - 鲜明红色边框配实心播放/停止键的录制切换胶囊按钮
//!    - 二次编辑；贴图、保存、复制、关闭复用编辑器现有图标
//!    - 悬停提示气泡（如悬停剪刀显示“裁剪”）
//!    - 侧边缩略预览面板（仅在录制态展示）与绿色半透明视口指示器

mod bar;
mod dock;
mod icons;
mod sub_menu;
pub(crate) mod types;

use super::CanvasSize;
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use crate::snipper::window::state::long_capture::{LongCaptureAction, LongCaptureSession};
use bar::*;
pub(crate) use dock::render_long_capture_dock;
use icons::*;
use snapin_editor::toolbar::items::ToolbarAction as EditorToolbarAction;
use snapin_render::{with_thread_local_text_renderer, StitchDirection, TextDrawOptions};
pub(crate) use sub_menu::hit_test_crop_sub_menu;
use sub_menu::render_crop_sub_menu;
use tiny_skia::{Paint, PathBuilder, PixmapMut, PixmapRef, Stroke, Transform};
pub(crate) use types::{long_capture_main_button_defs, long_capture_size_readout_width};

/// 绘制长截图 PixPin 风格白色悬浮控制栏（位于选区上方）
pub(crate) fn render_long_capture_control_bar(
    pixmap: &mut PixmapMut,
    session: &mut LongCaptureSession,
    selection_rect: &Rect,
    screen_size: CanvasSize,
    ui_scale: f32,
) {
    let s = ui_scale;
    let (sel_w, sel_h) = (
        selection_rect.width.round() as u32,
        selection_rect.height.round() as u32,
    );
    let defs = long_capture_main_button_defs(session, sel_w, sel_h);
    let readout_w = long_capture_size_readout_width(sel_w, sel_h) * s;
    let actions_w = action_buttons_width(&defs, s);
    let total_buttons_w = 12.0 * s + readout_w + 8.0 * s + actions_w;
    let bar_h = 36.0 * s;

    // 默认放置在选区上方，左边缘对齐
    let mut bar_x = selection_rect.x;
    if bar_x + total_buttons_w > (screen_size.width as f32) - 8.0 * s {
        bar_x = (screen_size.width as f32) - total_buttons_w - 8.0 * s;
    }
    if bar_x < 8.0 * s {
        bar_x = 8.0 * s;
    }

    let mut bar_y = selection_rect.y - bar_h - 8.0 * s;
    // 若上方空间不足，则自适应放置在选区下方
    if bar_y < 8.0 * s {
        bar_y = selection_rect.y + selection_rect.height + 8.0 * s;
    }

    let default_position = (bar_x, bar_y);
    let requested_position = session.toolbar_position.unwrap_or(default_position);
    let max_x = ((screen_size.width as f32) - total_buttons_w - 8.0 * s).max(8.0 * s);
    let max_y = ((screen_size.height as f32) - bar_h - 8.0 * s).max(8.0 * s);
    bar_x = requested_position.0.clamp(8.0 * s, max_x);
    bar_y = requested_position.1.clamp(8.0 * s, max_y);
    if session.toolbar_position.is_some() {
        session.toolbar_position = Some((bar_x, bar_y));
    }

    session.toolbar_rect = Some(Rect {
        x: bar_x,
        y: bar_y,
        width: total_buttons_w,
        height: bar_h,
    });

    // 1. 绘制白色圆角卡片底板与轻量微阴影
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: bar_x,
            y: bar_y,
            width: total_buttons_w,
            height: bar_h,
            radius: 6.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 252);
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
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

    // 2. 先绘制只读尺寸信息，再绘制可交互按钮；拖动点固定在最右端
    let mut cur_x = bar_x + 6.0 * s;
    let btn_y = bar_y + 4.0 * s;
    let btn_h = bar_h - 8.0 * s;

    draw_size_readout(
        pixmap,
        SizeReadoutSpec {
            direction: session.direction,
            x: cur_x,
            y: btn_y,
            width: readout_w,
            height: btn_h,
            selection_w: sel_w,
            selection_h: sel_h,
            scale: s,
        },
    );
    cur_x += readout_w;
    draw_vertical_divider(pixmap, cur_x + 4.0 * s, btn_y, btn_h, s);
    cur_x += 8.0 * s;

    let mut hovered_btn_rect: Option<(Rect, &'static str)> = None;
    let mut scissors_btn_bottom: Option<(f32, f32)> = None;

    for (index, def) in defs.iter().enumerate() {
        if index > 0 {
            cur_x += 3.0 * s;
        }
        let btn_w = def.width * s;
        let is_hovered = session.hovered_action == Some(def.action);
        let btn_rect = Rect {
            x: cur_x,
            y: btn_y,
            width: btn_w,
            height: btn_h,
        };

        if is_hovered {
            hovered_btn_rect = Some((btn_rect, def.tooltip));
        }

        match def.action {
            LongCaptureAction::ToggleDirection => {
                // 切换截图方向（圆角胶囊背景 + 文字 + 下拉倒三角）
                let cap_radius = 10.0 * s;
                let mut cap_pb = PathBuilder::new();
                add_round_rect(
                    &mut cap_pb,
                    RoundRect {
                        x: cur_x,
                        y: btn_y + 2.0 * s,
                        width: btn_w,
                        height: btn_h - 4.0 * s,
                        radius: cap_radius,
                    },
                );
                if let Some(cap_path) = cap_pb.finish() {
                    let mut cap_paint = Paint::default();
                    if is_hovered {
                        cap_paint.set_color_rgba8(0, 0, 0, 18);
                    } else {
                        cap_paint.set_color_rgba8(0, 0, 0, 10);
                    }
                    pixmap.fill_path(
                        &cap_path,
                        &cap_paint,
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }

                let dir_text = match session.direction {
                    StitchDirection::Vertical => "垂直",
                    StitchDirection::Horizontal => "水平",
                };
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        dir_text,
                        TextDrawOptions::new(
                            cur_x + 6.0 * s,
                            btn_y + 5.0 * s,
                            11.0 * s,
                            (45, 52, 65, 255),
                        ),
                    );
                });
                draw_dropdown_arrow(pixmap, cur_x + btn_w - 9.0 * s, btn_y + btn_h / 2.0, s);
            }
            LongCaptureAction::ToggleCropSubMenu => {
                // 剪刀裁剪图标（记录下方坐标供子菜单对齐）
                scissors_btn_bottom = Some((cur_x, bar_y + bar_h + 4.0 * s));
                if session.crop_sub_menu_open {
                    draw_button_active_bg(pixmap, &btn_rect, s);
                } else if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_scissors_icon(
                    pixmap,
                    btn_rect.x + btn_rect.width / 2.0,
                    btn_rect.y + btn_rect.height / 2.0,
                    s,
                    is_hovered,
                );
            }
            LongCaptureAction::ToggleCapture => {
                // 开始/停止录制红框胶囊
                draw_capture_toggle_button(pixmap, &btn_rect, session.is_capturing, is_hovered, s);
            }
            LongCaptureAction::EnterEditing => {
                // 二次编辑图标（带铅笔方框）
                if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_edit_icon(
                    pixmap,
                    btn_rect.x + btn_rect.width / 2.0,
                    btn_rect.y + btn_rect.height / 2.0,
                    s,
                    is_hovered,
                );
            }
            LongCaptureAction::Save => {
                if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_editor_action_icon(pixmap, EditorToolbarAction::Save, &btn_rect, s);
            }
            LongCaptureAction::Pin => {
                if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_editor_action_icon(pixmap, EditorToolbarAction::Pin, &btn_rect, s);
            }
            LongCaptureAction::Close => {
                if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_editor_action_icon(pixmap, EditorToolbarAction::Close, &btn_rect, s);
            }
            LongCaptureAction::Copy => {
                if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_editor_action_icon(pixmap, EditorToolbarAction::Copy, &btn_rect, s);
            }
            LongCaptureAction::DragToolbar => {
                if is_hovered {
                    draw_button_hover_bg(pixmap, &btn_rect, s);
                }
                draw_drag_handle(pixmap, &btn_rect, s);
            }
            _ => {}
        }

        cur_x += btn_w;
    }

    // 3. 若裁剪子菜单处于展开状态，绘制裁剪子浮栏（左边缘与剪刀按钮对齐）
    if session.crop_sub_menu_open {
        let (sub_x, sub_y) =
            scissors_btn_bottom.unwrap_or((bar_x + 120.0 * s, bar_y + bar_h + 4.0 * s));
        render_crop_sub_menu(pixmap, session, sub_x, sub_y, s);
    } else {
        session.crop_submenu_rect = None;

        // 4. 若子菜单未展开且有按钮处于悬停态，在下方显示白色圆角气泡提示
        if let Some((target_rect, tooltip)) = hovered_btn_rect {
            if session.hovered_action != Some(LongCaptureAction::DragToolbar) {
                draw_tooltip_bubble(pixmap, &target_rect, tooltip, s, screen_size.width as f32);
            }
        }
    }
}

/// 命中测试长截图控制栏按钮（包含主栏与展开的裁剪子栏）
pub(crate) fn hit_test_long_capture_bar(
    session: &LongCaptureSession,
    cursor_x: f32,
    cursor_y: f32,
    ui_scale: f32,
    selection_w: u32,
    selection_h: u32,
) -> Option<LongCaptureAction> {
    let s = ui_scale;

    // 1. 优先检测裁剪展开子栏
    if let Some(action) = hit_test_crop_sub_menu(session, cursor_x, cursor_y, s) {
        return Some(action);
    }

    // 2. 检测主控制栏
    let bar_rect = session.toolbar_rect?;
    if !bar_rect.contains(cursor_x, cursor_y) {
        return None;
    }

    let defs = long_capture_main_button_defs(session, selection_w, selection_h);
    let readout_w = long_capture_size_readout_width(selection_w, selection_h) * s;
    let mut cur_x = bar_rect.x + 6.0 * s + readout_w + 8.0 * s;
    let btn_y = bar_rect.y + 4.0 * s;
    let btn_h = bar_rect.height - 8.0 * s;

    for (index, def) in defs.iter().enumerate() {
        if index > 0 {
            cur_x += 3.0 * s;
        }
        let btn_w = def.width * s;
        let btn_rect = Rect {
            x: cur_x,
            y: btn_y,
            width: btn_w,
            height: btn_h,
        };
        if btn_rect.contains(cursor_x, cursor_y) {
            return Some(def.action);
        }
        cur_x += btn_w;
    }

    None
}

/// 将已处于预乘 Alpha 格式的局部离屏画布切片精确拷贝到目标 Pixmap 指定坐标。
pub(crate) fn copy_premultiplied_pixmap_region(
    dst: &mut PixmapMut,
    src: PixmapRef<'_>,
    dest_x: u32,
    dest_y: u32,
) {
    let (sw, sh) = (src.width(), src.height());
    let (dw, dh) = (dst.width(), dst.height());
    let copy_w = sw.min(dw.saturating_sub(dest_x));
    let copy_h = sh.min(dh.saturating_sub(dest_y));
    if copy_w == 0 || copy_h == 0 {
        return;
    }

    let src_raw = src.data();
    let dst_raw = dst.data_mut();
    let src_stride = sw as usize * 4;
    let dst_stride = dw as usize * 4;
    let row_bytes = copy_w as usize * 4;

    for row in 0..copy_h as usize {
        let s_offset = row * src_stride;
        let d_offset = (dest_y as usize + row) * dst_stride + dest_x as usize * 4;
        if let (Some(dst_slice), Some(src_slice)) = (
            dst_raw.get_mut(d_offset..d_offset + row_bytes),
            src_raw.get(s_offset..s_offset + row_bytes),
        ) {
            dst_slice.copy_from_slice(src_slice);
        }
    }
}
