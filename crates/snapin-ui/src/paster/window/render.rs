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
use crate::paster::PresentOutcome;
use crate::status_badge;
use snapin_render::with_thread_local_text_renderer;
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
///
/// # Errors
///
/// 表面尺寸调整、缓冲分配或像素呈现失败时返回 [`UiError`]。
pub fn render_paster_to_surface(
    state: &mut PasterWindowState,
    surface: &mut PasterSurface,
    width: u32,
    height: u32,
) -> Result<PresentOutcome, UiError> {
    surface.resize(width, height)?;
    let mut target_pixmap = state
        .take_frame_pixmap(width, height)
        .ok_or(UiError::PixmapAlloc { width, height })?;
    target_pixmap.fill(Color::TRANSPARENT);
    let metrics = PasterFrameMetrics::new(state, width, height);
    prepare_frame_backdrop(state, surface, &mut target_pixmap, metrics);
    render_frame_content(state, &mut target_pixmap, metrics);
    update_input_region(state, surface, metrics);

    // 提交渲染至共享内存表面呈现（ARGB 通路下未被内容覆盖的像素保持透明）
    let result = surface.present(&target_pixmap);
    state.restore_frame_pixmap(target_pixmap);
    result
}

/// 当前帧的窗口尺寸、缩放倍率与缩放后图像尺寸。
#[derive(Debug, Clone, Copy)]
struct PasterFrameMetrics {
    width: u32,
    height: u32,
    scale: f32,
    image_width: f32,
    image_height: f32,
}

impl PasterFrameMetrics {
    /// 从窗口状态计算本帧几何。
    fn new(state: &PasterWindowState, width: u32, height: u32) -> Self {
        let scale = state.document.item.transform.scale();
        Self {
            width,
            height,
            scale,
            image_width: (state.document.item.pixmap.width() as f32 * scale).max(1.0),
            image_height: (state.document.item.pixmap.height() as f32 * scale).max(1.0),
        }
    }

    /// 窗口是否因菜单避让而超出缩放后的贴图图像。
    fn window_exceeds_image(self) -> bool {
        (self.width as f32) > self.image_width + 0.5
            || (self.height as f32) > self.image_height + 0.5
    }
}

/// 仅在无透明合成且菜单撑大窗口时铺设浅色底，避免回退通路露出黑底。
fn prepare_frame_backdrop(
    state: &PasterWindowState,
    surface: &PasterSurface,
    target_pixmap: &mut Pixmap,
    metrics: PasterFrameMetrics,
) {
    // 右键菜单放不下时窗口会被临时撑大：ARGB8888 通路下扩容区域本就是全透明像素，无需涂抹
    // （涂抹反而会遮掉透明效果）；只有回退到无 alpha 的 softbuffer 通路（Xrgb8888）时，
    // 未被内容覆盖的区域若不涂抹就会呈现纯黑，因此仅在回退通路上铺一层与菜单卡片一致的浅色底。
    if needs_opaque_backdrop(
        surface.supports_transparency(),
        state.interaction.context_menu.is_open,
        metrics.window_exceeds_image(),
    ) {
        target_pixmap.fill(Color::from_rgba8(241, 245, 249, 255));
    }
}

/// 按图像、标注预览、边框、状态提示、工具栏与菜单的顺序合成一帧。
fn render_frame_content(
    state: &mut PasterWindowState,
    target_pixmap: &mut Pixmap,
    metrics: PasterFrameMetrics,
) {
    render_image_layer(state, target_pixmap, metrics.scale);
    render_pending_annotation_preview(state, target_pixmap);
    render_text_session_preview(state, target_pixmap);
    render_paster_border_and_shadow(
        target_pixmap,
        metrics.image_width,
        metrics.image_height,
        state.chrome.has_shadow,
    );
    render_status_message(state, target_pixmap, metrics.image_width);
    render_toolbar_and_menu(state, target_pixmap, metrics);
}

/// 绘制应用透明度与缩放设置后的贴图图像。
fn render_image_layer(state: &PasterWindowState, target_pixmap: &mut Pixmap, scale: f32) {
    let tf = Transform::from_scale(scale, scale);
    let quality = if state.chrome.smooth_scaling {
        FilterQuality::Bilinear
    } else {
        FilterQuality::Nearest
    };

    let paint = PixmapPaint {
        opacity: state.document.item.transform.opacity(),
        quality,
        ..Default::default()
    };
    target_pixmap.draw_pixmap(0, 0, state.document.item.pixmap.as_ref(), &paint, tf, None);
}

/// 绘制标注工具栏与右键菜单。
fn render_toolbar_and_menu(
    state: &mut PasterWindowState,
    target_pixmap: &mut Pixmap,
    metrics: PasterFrameMetrics,
) {
    if state.chrome.show_toolbar || state.interaction.context_menu.is_open {
        if state.chrome.show_toolbar {
            // 与命中测试同源：工具条锚定在缩放后的图像正下方，窗口扩容区域保持透明
            refresh_toolbar_layout(state, metrics.width as f32, metrics.height as f32);
        }
        with_thread_local_text_renderer(|tr| {
            if state.chrome.show_toolbar {
                state
                    .interaction
                    .annotation
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
}

/// 透明扩窗后按当前布局声明可交互区域，避免空白区域吞掉桌面点击。
fn update_input_region(
    state: &mut PasterWindowState,
    surface: &mut PasterSurface,
    metrics: PasterFrameMetrics,
) {
    let input_layout = input_layout_from_state(state, metrics.width as f32, metrics.height as f32);
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
}

/// 将当前拖拽中的笔迹按屏幕坐标绘制到目标画布上。
///
/// 待提交点集保存的是图像像素坐标，预览必须乘以当前缩放才能与图像对齐；
/// 线宽同样随缩放放大。序号半径在工厂内部以 `stroke_width * ui_scale` 推导，
/// 因此这里改为放大 `ui_scale` 而不是先放大线宽，避免半径夹取区间随预览偏移。
fn render_pending_annotation_preview(state: &PasterWindowState, target: &mut Pixmap) {
    if !state.interaction.annotation.stroke.is_drawing {
        return;
    }
    let Some(tool) = state.interaction.annotation.toolbar.active_tool else {
        return;
    };
    if state.interaction.annotation.stroke.is_empty() {
        return;
    }

    let scale = state.document.item.transform.scale().max(f32::EPSILON);
    let mut scaled_points = Vec::with_capacity(state.interaction.annotation.stroke.points.len());
    for &point in &state.interaction.annotation.stroke.points {
        scaled_points.push(image_to_screen_point(point, scale));
    }

    let Some(mut params) = AnnotationDraftParams::from_toolbar_and_points(
        &state.interaction.annotation.toolbar,
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
    if has_shadow {
        draw_paster_shadow(pixmap, w, h);
    }
    draw_paster_borders(pixmap, w, h);
}

/// 绘制多层内缩阴影。
fn draw_paster_shadow(pixmap: &mut Pixmap, width: f32, height: f32) {
    let shadow_steps = [
        (1.0, 90u8),
        (2.0, 65u8),
        (3.0, 45u8),
        (4.0, 30u8),
        (5.0, 18u8),
        (6.0, 8u8),
    ];
    for (inset, alpha) in shadow_steps {
        let mut builder = PathBuilder::new();
        builder.move_to(inset, inset);
        builder.line_to(width - inset, inset);
        builder.line_to(width - inset, height - inset);
        builder.line_to(inset, height - inset);
        builder.close();
        if let Some(path) = builder.finish() {
            let paint = make_paint(Color::from_rgba8(0, 0, 0, alpha));
            let stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

/// 绘制外亮内暗双层微边框。
fn draw_paster_borders(pixmap: &mut Pixmap, width: f32, height: f32) {
    let mut pb_border = PathBuilder::new();
    pb_border.move_to(0.5, 0.5);
    pb_border.line_to(width - 0.5, 0.5);
    pb_border.line_to(width - 0.5, height - 0.5);
    pb_border.line_to(0.5, height - 0.5);
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
        pb_inner.line_to(width - 1.5, 1.5);
        pb_inner.line_to(width - 1.5, height - 1.5);
        pb_inner.line_to(1.5, height - 1.5);
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
