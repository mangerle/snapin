//! 贴图变换光栅化合成、精致边框与右键上下文菜单渲染。

use super::events::PasterWindowState;
use crate::drawing::make_paint;
use crate::editor::annotation_builder::{render_draft_preview, AnnotationDraftParams};
use crate::editor::toolbar::Tool;
use crate::error::UiError;
use crate::paster::context_menu::render::render_context_menu;
use crate::paster::surface::PasterSurface;
use crate::paster::window::annotation::{image_to_screen_point, refresh_toolbar_layout};
use crate::paster::window::input_region::{
    compute_input_rects, input_layout_from_state, physical_rects_to_surface, InputRegionCache,
};
use crate::paster::window::text_session::render_text_session_preview;
use crate::status_badge;
use snapin_core::render::with_thread_local_text_renderer;
use std::time::Instant;
use tiny_skia::{Color, FilterQuality, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

/// 判定当前帧是否需要为被撑大的窗口空白区域铺设不透明底色。
///
/// 输入依次为：呈现通路是否支持透明合成、右键上下文菜单是否展开、窗口是否大于缩放后的贴图尺寸。
/// 仅当三者同时成立（回退到无 alpha 的通路 + 菜单把窗口撑大）时返回 `true`：
/// ARGB8888 通路下扩容区域本就是全透明像素，涂抹反而会遮掉透明效果；
/// 无 alpha 的 softbuffer 通路下不涂抹则会让空白区域露出黑色。
/// 抽成纯函数是为了脱离真实 Wayland 连接对四种组合做完整单元测试。
fn needs_opaque_backdrop(transparent: bool, menu_open: bool, window_exceeds_image: bool) -> bool {
    !transparent && menu_open && window_exceeds_image
}

/// 根据几何变换矩阵将贴图图元、精致边框、阴影及右键菜单光栅化到目标表面
pub fn render_paster_to_surface(
    state: &mut PasterWindowState,
    surface: &mut PasterSurface,
    width: u32,
    height: u32,
) -> Result<(), UiError> {
    surface.resize(width, height)?;

    let mut target_pixmap = state
        .take_frame_pixmap(width, height)
        .ok_or(UiError::PixmapAlloc { width, height })?;
    target_pixmap.fill(Color::TRANSPARENT);

    // 右键菜单放不下时窗口会被临时撑大：ARGB8888 通路下扩容区域本就是全透明像素，无需涂抹
    // （涂抹反而会遮掉透明效果）；只有回退到无 alpha 的 softbuffer 通路（Xrgb8888）时，
    // 未被内容覆盖的区域若不涂抹就会呈现纯黑，因此仅在回退通路上铺一层与菜单卡片一致的浅色底。
    let scale = state.document.item.transform.scale;
    let scaled_w = (state.document.item.pixmap.width() as f32 * scale).max(1.0);
    let scaled_h = (state.document.item.pixmap.height() as f32 * scale).max(1.0);
    let window_exceeds_image = (width as f32) > scaled_w + 0.5 || (height as f32) > scaled_h + 0.5;
    if needs_opaque_backdrop(
        surface.supports_transparency(),
        state.interaction.context_menu.is_open,
        window_exceeds_image,
    ) {
        target_pixmap.fill(Color::from_rgba8(241, 245, 249, 255));
    }

    // 1. 构建几何变换矩阵 (缩放)
    let tf = Transform::from_scale(scale, scale);

    let quality = if state.chrome.smooth_scaling {
        FilterQuality::Bilinear
    } else {
        FilterQuality::Nearest
    };

    let paint = PixmapPaint {
        opacity: state.document.item.transform.opacity,
        quality,
        ..Default::default()
    };

    // 2. 光栅化贴图图像本体
    target_pixmap.draw_pixmap(0, 0, state.document.item.pixmap.as_ref(), &paint, tf, None);

    // 3. 绘制拖拽中的标注预览与文字输入框：位于图像之上、边框与工具栏之下
    render_pending_annotation_preview(state, &mut target_pixmap);
    render_text_session_preview(state, &mut target_pixmap);

    // 4. 绘制贴图精致外边框与窗口阴影。
    //    必须按「缩放后的贴图尺寸」描边而不是窗口尺寸：右键菜单放不下时窗口会被临时撑大，
    //    若跟着窗口描边，边框会一并扩大，用户一眼就能看出贴图被撑过——透明像素挡不住这个观感。
    render_paster_border_and_shadow(
        &mut target_pixmap,
        scaled_w,
        scaled_h,
        state.chrome.has_shadow,
    );

    // 5. 在图像顶部居中绘制状态提示（OCR / 翻译进度与结果），随后再叠加工具栏与菜单
    render_status_message(state, &mut target_pixmap, scaled_w);

    // 6. 绘制标注工具栏与右键上下文菜单 (统一复用线程局部高精度文本排版引擎)
    if state.chrome.show_toolbar || state.interaction.context_menu.is_open {
        if state.chrome.show_toolbar {
            // 与命中测试同源：工具条锚定在缩放后的图像正下方，窗口扩容区域保持透明
            refresh_toolbar_layout(state, width as f32, height as f32);
        }
        with_thread_local_text_renderer(|tr| {
            if state.chrome.show_toolbar {
                state
                    .interaction
                    .toolbar
                    .render(&mut target_pixmap.as_mut());
            }
            if state.interaction.context_menu.is_open {
                render_context_menu(
                    &state.interaction.context_menu,
                    &mut target_pixmap.as_mut(),
                    tr,
                    state.chrome.show_toolbar,
                    state.chrome.has_shadow,
                    state.chrome.smooth_scaling,
                );
            }
        });
    }

    // 7. 透明扩窗后按当前布局声明可交互区域，避免空白区域吞掉本应落到桌面的点击。
    //    布局与光标坐标均为窗口物理像素，提交前统一换算为 Wayland surface-local 逻辑坐标。
    let input_layout = input_layout_from_state(state, width as f32, height as f32);
    let cache_hit = state
        .frame_cache
        .input_region
        .as_ref()
        .is_some_and(|cached| cached.layout == input_layout);
    if !cache_hit {
        let physical_rects = compute_input_rects(&input_layout);
        let surface_rects = physical_rects_to_surface(&physical_rects, state.viewport.ui_scale);
        if let Err(e) = surface.set_input_rects(&surface_rects) {
            tracing::warn!("更新贴图窗口输入区域失败，透明空白可能拦截桌面点击: {e}");
        } else {
            state.frame_cache.input_region = Some(InputRegionCache {
                layout: input_layout,
                rects: surface_rects,
            });
        }
    }

    // 8. 提交渲染至共享内存表面呈现（ARGB 通路下未被内容覆盖的像素保持透明）
    let result = surface.present(&target_pixmap);
    state.restore_frame_pixmap(target_pixmap);
    result
}

/// 将当前拖拽中的笔迹按屏幕坐标绘制到目标画布上。
///
/// 待提交点集保存的是图像像素坐标，预览必须乘以当前缩放才能与图像对齐；
/// 线宽同样随缩放放大。序号半径在工厂内部以 `stroke_width * ui_scale` 推导，
/// 因此这里改为放大 `ui_scale` 而不是先放大线宽，避免半径夹取区间随预览偏移。
fn render_pending_annotation_preview(state: &PasterWindowState, target: &mut Pixmap) {
    if !state.interaction.stroke.is_drawing {
        return;
    }
    let Some(tool) = state.interaction.toolbar.active_tool else {
        return;
    };
    if state.interaction.stroke.points.is_empty() {
        return;
    }

    let scale = state.document.item.transform.scale.max(f32::EPSILON);
    let mut scaled_points = Vec::with_capacity(state.interaction.stroke.points.len());
    for &point in &state.interaction.stroke.points {
        scaled_points.push(image_to_screen_point(point, scale));
    }

    let Some(mut params) = AnnotationDraftParams::from_toolbar_and_points(
        &state.interaction.toolbar,
        tool,
        &scaled_points,
        state.document.annotations.next_badge_number(),
    ) else {
        return;
    };
    if tool == Tool::Badge {
        params.style.ui_scale *= scale;
    } else {
        params.style.stroke_width *= scale;
    }
    render_draft_preview(&params, &mut target.as_mut());
}

/// 在图像顶部居中绘制半透明深色胶囊与白色状态文字。
///
/// 布局与截图侧共用 [`crate::status_badge`]，贴图只覆盖宽度来源、顶部距离与配色。
fn render_status_message(state: &PasterWindowState, target: &mut Pixmap, image_w: f32) {
    let Some((message, _)) = state.status_message.as_ref() else {
        return;
    };
    if message.is_empty() || state.is_status_expired(Instant::now()) {
        return;
    }
    let layout = status_badge::layout(
        message,
        image_w,
        state.viewport.ui_scale,
        status_badge::PASTER_STATUS_STYLE,
    );
    status_badge::render(
        &mut target.as_mut(),
        message,
        &layout,
        status_badge::PASTER_STATUS_STYLE,
    );
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

#[cfg(test)]
mod tests {
    use super::needs_opaque_backdrop;

    /// 底色铺设判定必须在“无透明通路 + 菜单展开 + 窗口超出缩放后贴图”三者同时成立时为真
    #[test]
    fn 底色铺设判定四种组合() {
        // 组合一：ARGB 通路支持透明合成，扩容区域保持全透明，不得涂抹
        assert!(
            !needs_opaque_backdrop(true, true, true),
            "支持透明的通路下窗口被撑大时无需铺设底色"
        );
        // 组合二：softbuffer 回退通路且菜单把窗口撑大，不铺底会露黑，必须涂抹
        assert!(
            needs_opaque_backdrop(false, true, true),
            "无 alpha 的回退通路下窗口被撑大时必须铺设不透明底色"
        );
        // 组合三：回退通路但菜单未展开，窗口与贴图尺寸一致，不存在扩容出来的空白区域
        assert!(
            !needs_opaque_backdrop(false, false, true),
            "菜单未展开时不存在扩容区域，无需铺设底色"
        );
        // 组合四：回退通路且菜单已展开，但窗口未超出缩放后的贴图，没有空白区域
        assert!(
            !needs_opaque_backdrop(false, true, false),
            "窗口未超出缩放后贴图尺寸时无需铺设底色"
        );
    }
}
