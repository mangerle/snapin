//! 贴图标注工具栏配置、坐标映射与工具栏点击分发。
//!
//! 贴图窗口复用截图工具栏的布局与渲染实现，主栏在绘制工具与撤销/重做之外，
//! 还提供整图 OCR、整图翻译以及复制、保存、关闭等贴图动作。

use super::jobs::pixmap_to_rgba_image;
use super::menu_actions::{copy_paster_to_clipboard, save_paster_to_disk};
use super::state::PasterWindowState;
use crate::editor::toolbar::{Tool, Toolbar, ToolbarAction, ToolbarItem};
use crate::geometry::Point;
#[cfg(test)]
use crate::geometry::Rect;
use crate::ports::PasterJobKind;
use winit::window::Window;

/// 贴图窗口主栏按钮序列：绘制工具、撤销/重做、整图识别/翻译与贴图动作。
///
/// 文字工具通过窗口 IME 会话输入，与截图侧共用同一套 `TextInputSession`；
/// 不包含截图专属的贴图（ActionPin）按钮。
/// 分隔线由 `metrics::has_separator_before` 按按钮语义自动插入，
/// 因此撤销与关闭动作之前各会有一条分隔线。
///
/// `TextInputSession`: crate::editor::text_input::TextInputSession
pub const PASTER_MAIN_ITEMS: &[ToolbarItem] = &[
    ToolbarItem::Tool(Tool::Rect),
    ToolbarItem::Tool(Tool::Badge),
    ToolbarItem::Tool(Tool::Arrow),
    ToolbarItem::Tool(Tool::Pen),
    ToolbarItem::Tool(Tool::Marker),
    ToolbarItem::Tool(Tool::Mosaic),
    ToolbarItem::Tool(Tool::Magnifier),
    ToolbarItem::Tool(Tool::Spotlight),
    ToolbarItem::Tool(Tool::Eraser),
    ToolbarItem::Tool(Tool::Text),
    ToolbarItem::Action(ToolbarAction::Undo),
    ToolbarItem::Action(ToolbarAction::Redo),
    ToolbarItem::Action(ToolbarAction::Ocr),
    ToolbarItem::Action(ToolbarAction::Translate),
    ToolbarItem::Action(ToolbarAction::MouseThrough),
    ToolbarItem::Action(ToolbarAction::Close),
    ToolbarItem::Action(ToolbarAction::Save),
    ToolbarItem::Action(ToolbarAction::Copy),
];

/// 几何工具属性栏的保守预留宽度（界面像素）。
///
/// 窗口扩容时按此宽度为二级属性栏留出横向空间；主栏按钮更多时实际窗口宽度
/// 仍由主栏决定。抽成常量是为了以后增删属性控件时只需调整一处。
const SUB_BAR_MAX_WIDTH: f32 = 1260.0;

/// 二级属性栏在工具条上方占用的高度（面板 36 + 与主栏间距 4，界面像素）。
///
/// 主栏必须下移这段距离，属性栏才能从主栏上方展开而不遮挡缩放后的图像。
const SUB_BAR_HEIGHT: f32 = 36.0;

/// 二级属性栏与主栏之间的间距（界面像素）
const SUB_BAR_GAP: f32 = 4.0;

/// 创建贴图专用工具栏。
pub(crate) fn create_paster_toolbar() -> Toolbar {
    let mut toolbar = Toolbar::default();
    toolbar.main_items = PASTER_MAIN_ITEMS;
    // 贴图窗口没有截图侧的 Ctrl+Z/Ctrl+S 等键位，悬浮提示只展示按钮名称。
    toolbar.show_shortcut_hints = false;
    toolbar
}

/// 计算显示标注工具条时窗口需要的最小宽度与图像下方的额外高度。
///
/// 返回值中的宽度是窗口整体最小宽度（图像较窄时会据此扩宽窗口），
/// 高度只是「图像底部以下」所需的增量，由调用方叠加到缩放后的图像高度上。
/// 宽度需同时容纳主栏与二级属性栏，高度需覆盖属性栏预留区、主栏本身与上下边距。
///
/// 抽成纯函数既方便对多种界面倍率做单元测试，也避免布局与窗口扩容各自维护一套常量。
pub(crate) fn toolbar_required_extent(ui_scale: f32, main_w: f32, main_h: f32) -> (f32, f32) {
    let ui_scale = ui_scale.max(0.1);
    let margin = 8.0 * ui_scale;
    let width = (main_w + margin * 2.0).max(SUB_BAR_MAX_WIDTH * ui_scale + margin * 2.0);
    // 高度在工具条下方预留二级栏空间：主栏紧贴图像（间距仅一个 margin），
    // 属性栏需要时浮在主栏上方（与截图一致），不会把工具条整体推远。
    let height = margin + main_h + SUB_BAR_GAP * ui_scale + SUB_BAR_HEIGHT * ui_scale + margin;
    (width, height)
}

/// 将窗口屏幕坐标换算为图像像素坐标，并夹取到图像范围内。
///
/// 前提假设：贴图图像从窗口客户区左上角 (0, 0) 开始，按 `transform.scale` 等比缩放绘制；
/// 边框绘制在图像内部、不占用额外布局。因此屏幕坐标除以缩放倍数即可得到图像像素坐标，
/// 超出图像边界的光标位置必须夹取，保证笔迹不会落在图像之外。
pub(crate) fn screen_to_image_point(
    pos: (f32, f32),
    scale: f32,
    width: u32,
    height: u32,
) -> (f32, f32) {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    (
        (pos.0 / scale).clamp(0.0, width as f32),
        (pos.1 / scale).clamp(0.0, height as f32),
    )
}

/// 将图像像素坐标换算为窗口屏幕坐标。
///
/// 与 `screen_to_image_point` 使用同一缩放假设，供标注预览按屏幕坐标绘制。
pub(crate) fn image_to_screen_point(pos: Point, scale: f32) -> Point {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    Point::new(pos.x * scale, pos.y * scale)
}

/// 判断屏幕坐标是否落在缩放后的贴图图像内部。
pub(crate) fn is_point_inside_image(pos: (f32, f32), scale: f32, width: u32, height: u32) -> bool {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    let scaled_w = width as f32 * scale;
    let scaled_h = height as f32 * scale;
    pos.0 >= 0.0 && pos.1 >= 0.0 && pos.0 <= scaled_w && pos.1 <= scaled_h
}

/// 按当前图像尺寸、缩放与界面倍率刷新工具栏布局。
///
/// 工具条锚定在「缩放后的图像」正下方：`x` 固定为左边距，`y` 为图像底边加上
/// 二级属性栏预留高度。贴图窗口会按 [`toolbar_required_extent`] 扩宽并加高，
/// 因此小贴图上的工具条不会再被窗口边缘裁掉；透明扩容区域不会渲染任何像素。
///
/// 始终左对齐：二级属性栏从主栏左端向右展开，靠左摆放能为属性栏与下拉菜单
/// 留出最大可用空间；窗口宽度不足时由窗口扩容兜底，而不是把工具条压缩或右移。
pub(crate) fn refresh_toolbar_layout(
    state: &mut PasterWindowState,
    window_width: f32,
    window_height: f32,
) {
    let ui_scale = state.viewport.ui_scale.max(0.1);
    state.interaction.annotation.toolbar.ui_scale = ui_scale;

    let scale = state.document.item.transform.scale().max(f32::EPSILON);
    let scaled_h = (state.document.item.pixmap.height() as f32 * scale).max(1.0);
    let margin = 8.0 * ui_scale;
    let bar_x = margin;
    // 工具条紧贴图像下边缘，只留一个 margin；二级栏需要时浮在主栏上方
    let bar_y = scaled_h + margin;

    // 先更新窗口边界，place_main_bar 内部的属性栏布局才能按正确的屏幕尺寸避让
    state
        .interaction
        .annotation
        .toolbar
        .set_screen_bounds(window_width.max(1.0), window_height.max(1.0));
    state
        .interaction
        .annotation
        .toolbar
        .place_main_bar(bar_x, bar_y);
}

/// 检测并处理贴图标注工具栏的点击，返回是否已消费本次点击。
///
/// 命中范围包含主栏、二级属性栏与展开的下拉菜单；绘制工具由工具栏内部切换激活状态，
/// 撤销/重做、整图识别/翻译、复制、保存、关闭等动作在此分发。
/// 识别与翻译任务异步执行，提交成功后立即返回，结果由工作区事件循环回填。
pub(crate) fn handle_toolbar_click(
    state: &mut PasterWindowState,
    window_size: (f32, f32),
    window: &Window,
) -> bool {
    if !state.chrome.show_toolbar {
        return false;
    }
    refresh_toolbar_layout(state, window_size.0, window_size.1);

    let (cx, cy) = state.viewport.cursor_pos;
    if !state.interaction.annotation.toolbar.contains_point(cx, cy) {
        return false;
    }

    if let Some(item) = state.interaction.annotation.toolbar.handle_click(cx, cy) {
        match item {
            ToolbarAction::Undo => {
                state.undo_annotation();
            }
            ToolbarAction::Redo => {
                state.redo_annotation();
            }
            ToolbarAction::Ocr => submit_paster_ocr(state),
            ToolbarAction::SmartRedact | ToolbarAction::Qr | ToolbarAction::ScreenRecording => {}
            ToolbarAction::Translate => submit_paster_translate(state),
            ToolbarAction::MouseThrough => {
                state.set_mouse_through(!state.chrome.mouse_through, window);
            }
            ToolbarAction::Copy => {
                copy_paster_to_clipboard(state);
            }
            ToolbarAction::Save => save_paster_to_disk(state),
            ToolbarAction::Close => state.should_close = true,
            ToolbarAction::ClearAnnotations => {
                state.clear_annotations();
            }
            ToolbarAction::Pin | ToolbarAction::LongCapture => {}
        }
    }
    true
}

/// 提交整图文字识别任务；已有任务或执行器不可用时只更新顶部提示。
fn submit_paster_ocr(state: &mut PasterWindowState) {
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }
    let Some(runtime) = state.services.job_runtime.clone() else {
        state.set_status_message(snapin_foundation::i18n::tr("后台任务媒体执行器不可用"));
        return;
    };
    let image = match pixmap_to_rgba_image(&state.document.item.pixmap) {
        Ok(image) => image,
        Err(message) => {
            state.set_status_message(format!("读取贴图像素失败: {message}"));
            return;
        }
    };

    match runtime.submit_ocr(state.document.item.id, image) {
        Ok((job_id, cancel)) => {
            state.begin_job(PasterJobKind::Ocr, job_id, cancel);
            state.set_status_message(snapin_foundation::i18n::tr("正在识别文字..."));
        }
        Err(message) => state.set_status_message(format!("提交文字识别任务失败: {message}")),
    }
}

/// 提交整图翻译任务；目标语言与背景覆盖模式取自当前配置。
fn submit_paster_translate(state: &mut PasterWindowState) {
    if state.has_pending_job() {
        state.set_status_message(snapin_foundation::i18n::tr("请等待当前任务完成"));
        return;
    }
    let Some(runtime) = state.services.job_runtime.clone() else {
        state.set_status_message(snapin_foundation::i18n::tr("后台任务媒体执行器不可用"));
        return;
    };
    let image = match pixmap_to_rgba_image(&state.document.item.pixmap) {
        Ok(image) => image,
        Err(message) => {
            state.set_status_message(format!("读取贴图像素失败: {message}"));
            return;
        }
    };

    match runtime.submit_translate(
        state.document.item.id,
        image,
        state.services.config.translate.cover_mode,
    ) {
        Ok((job_id, cancel)) => {
            state.begin_job(PasterJobKind::Translate, job_id, cancel);
            state.set_status_message(snapin_foundation::i18n::tr("正在翻译文字..."));
        }
        Err(message) => state.set_status_message(format!("提交翻译任务失败: {message}")),
    }
}

#[cfg(test)]
mod tests;
