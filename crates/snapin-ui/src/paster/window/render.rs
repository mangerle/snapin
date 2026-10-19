//! 贴图变换光栅化合成、精致边框与右键上下文菜单渲染。

use super::events::PasterWindowState;
use crate::paster::context_menu::render::render_context_menu;
use crate::surface::blit_pixmap_to_surface;
use snapin_core::error::SnapinError;
use std::num::NonZeroU32;
use std::sync::Arc;
use tiny_skia::{Color, FilterQuality, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};
use winit::window::Window;

/// 创建开启抗锯齿的基础画笔
fn make_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    paint
}

/// 根据几何变换矩阵将贴图图元、精致边框、阴影及右键菜单光栅化到目标表面
pub fn render_paster_to_surface(
    state: &mut PasterWindowState,
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    width: u32,
    height: u32,
) -> Result<(), SnapinError> {
    let w = NonZeroU32::new(width.max(1)).ok_or_else(|| SnapinError::SoftbufferError {
        message: "无效的贴图表面宽度".to_string(),
    })?;
    let h = NonZeroU32::new(height.max(1)).ok_or_else(|| SnapinError::SoftbufferError {
        message: "无效的贴图表面高度".to_string(),
    })?;

    surface
        .resize(w, h)
        .map_err(|e| SnapinError::SoftbufferError {
            message: format!("调整贴图帧缓冲尺寸失败: {e}"),
        })?;

    let mut target_pixmap =
        Pixmap::new(width, height).ok_or_else(|| SnapinError::SoftbufferError {
            message: "分配贴图渲染双缓冲区失败".to_string(),
        })?;

    // 1. 构建几何变换矩阵 (缩放)
    let scale = state.item.transform.scale;
    let tf = Transform::from_scale(scale, scale);

    let quality = if state.smooth_scaling {
        FilterQuality::Bilinear
    } else {
        FilterQuality::Nearest
    };

    let paint = PixmapPaint {
        opacity: state.item.transform.opacity,
        quality,
        ..Default::default()
    };

    // 2. 光栅化贴图图像本体
    target_pixmap.draw_pixmap(0, 0, state.item.pixmap.as_ref(), &paint, tf, None);

    // 3. 绘制贴图精致外边框与窗口阴影
    render_paster_border_and_shadow(
        &mut target_pixmap,
        width as f32,
        height as f32,
        state.has_shadow,
    );

    // 4. 绘制现代化胶囊快捷工具条 (若用户通过右键菜单勾选开启)
    if state.show_toolbar {
        render_paster_toolbar(
            &mut target_pixmap,
            &mut state.text_renderer,
            width as f32,
            height as f32,
            state.ui_scale,
        );
    }

    // 5. 叠加渲染右键上下文菜单与二级子菜单 (若当前处于激活展开状态)
    if state.context_menu.is_open {
        render_context_menu(
            &state.context_menu,
            &mut target_pixmap.as_mut(),
            &mut state.text_renderer,
            state.show_toolbar,
            state.has_shadow,
            state.smooth_scaling,
        );
    }

    // 5. 提交渲染至 softbuffer 显存呈现
    blit_pixmap_to_surface(surface, &target_pixmap)
}

/// 绘制贴图边缘微反光边框与立体阴影
fn render_paster_border_and_shadow(pixmap: &mut Pixmap, w: f32, h: f32, has_shadow: bool) {
    if w <= 4.0 || h <= 4.0 {
        return;
    }

    // 1. 窗口立体阴影层 (开启时呈现明显且自然的立体层次投影)
    if has_shadow {
        let shadow_steps = [
            (1.0, 90u8),
            (2.0, 65u8),
            (3.0, 45u8),
            (4.0, 30u8),
            (5.0, 18u8),
            (6.0, 8u8),
        ];
        for (inset, alpha) in shadow_steps {
            let mut pb_shadow = PathBuilder::new();
            pb_shadow.move_to(inset, inset);
            pb_shadow.line_to(w - inset, inset);
            pb_shadow.line_to(w - inset, h - inset);
            pb_shadow.line_to(inset, h - inset);
            pb_shadow.close();
            if let Some(path) = pb_shadow.finish() {
                let sp = make_paint(Color::from_rgba8(0, 0, 0, alpha));
                let stroke = Stroke {
                    width: 1.0,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &sp, &stroke, Transform::identity(), None);
            }
        }
    }

    // 2. 贴图精致双层微边框
    let mut pb_border = PathBuilder::new();
    pb_border.move_to(0.5, 0.5);
    pb_border.line_to(w - 0.5, 0.5);
    pb_border.line_to(w - 0.5, h - 0.5);
    pb_border.line_to(0.5, h - 0.5);
    pb_border.close();

    if let Some(path) = pb_border.finish() {
        let bp = make_paint(Color::from_rgba8(220, 225, 235, 200));
        let stroke = Stroke {
            width: 1.5,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &bp, &stroke, Transform::identity(), None);

        let mut pb_inner = PathBuilder::new();
        pb_inner.move_to(1.5, 1.5);
        pb_inner.line_to(w - 1.5, 1.5);
        pb_inner.line_to(w - 1.5, h - 1.5);
        pb_inner.line_to(1.5, h - 1.5);
        pb_inner.close();
        if let Some(in_path) = pb_inner.finish() {
            let ip = make_paint(Color::from_rgba8(0, 0, 0, 80));
            let in_stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };
            pixmap.stroke_path(&in_path, &ip, &in_stroke, Transform::identity(), None);
        }
    }
}

/// 绘制贴图底部现代胶囊快捷工具条
fn render_paster_toolbar(
    pixmap: &mut Pixmap,
    text_renderer: &mut crate::paster::context_menu::render::TextRenderer,
    w: f32,
    h: f32,
    ui_scale: f32,
) {
    let s = ui_scale;
    let bar_w = (230.0 * s).min(w - 16.0);
    let bar_h = 28.0 * s;
    if bar_w < 100.0 || h < bar_h + 16.0 {
        return;
    }

    let bar_x = (w - bar_w) / 2.0;
    let bar_y = h - bar_h - 8.0 * s;

    // 绘制现代胶囊底板
    let mut pb = PathBuilder::new();
    let r = bar_h / 2.0;
    pb.move_to(bar_x + r, bar_y);
    pb.line_to(bar_x + bar_w - r, bar_y);
    pb.quad_to(bar_x + bar_w, bar_y, bar_x + bar_w, bar_y + r);
    pb.quad_to(
        bar_x + bar_w,
        bar_y + bar_h,
        bar_x + bar_w - r,
        bar_y + bar_h,
    );
    pb.line_to(bar_x + r, bar_y + bar_h);
    pb.quad_to(bar_x, bar_y + bar_h, bar_x, bar_y + r);
    pb.quad_to(bar_x, bar_y, bar_x + r, bar_y);
    pb.close();

    if let Some(path) = pb.finish() {
        let bg = make_paint(Color::from_rgba8(18, 20, 28, 235));
        pixmap.fill_path(
            &path,
            &bg,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        let border = make_paint(Color::from_rgba8(255, 255, 255, 40));
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }

    // 绘制工具快捷文字标签
    let label = "复制(C) | 保存(S) | 旋转(3) | 翻转(H)";
    let font_size = 11.5 * s;
    let text_x = bar_x + 12.0 * s;
    let text_y = bar_y + (bar_h - font_size * 1.25) / 2.0;
    text_renderer.draw_text(
        &mut pixmap.as_mut(),
        label,
        text_x,
        text_y,
        font_size,
        Color::from_rgba8(240, 243, 250, 255),
    );
}
