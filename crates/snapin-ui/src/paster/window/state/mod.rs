//! 贴图窗口运行时状态封装与窗口尺寸缩放管理。

use crate::model::PasterItem;
use crate::paster::context_menu::ContextMenuState;
use crate::paster::window::annotation::{
    create_paster_toolbar, screen_to_image_point, toolbar_required_extent,
};
use crate::ports::NoopDesktop;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

mod annotations;
mod chrome;
pub(crate) mod clipboard;
mod document;
mod frame_cache;
pub(crate) mod interaction;
mod job_slot;
mod jobs;
mod services;
mod viewport;

use chrome::PasterChrome;
use clipboard::PendingClipboardJob;
use document::PasterDocument;
use frame_cache::FrameCache;
use interaction::PasterInteraction;
use job_slot::PasterJobSlot;
use services::PasterServices;
use viewport::PasterViewport;

use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::{TranslationAction, TranslationShapeItem};
use snapin_config::config::{Config, ZoomAnchor};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::keyboard::ModifiersState;
use winit::window::Window;

/// 贴图窗口状态提示的展示有效期
pub(crate) const STATUS_MESSAGE_TTL: Duration = Duration::from_secs(3);

/// 贴图窗口运行时状态
pub struct PasterWindowState {
    /// 贴图内容文档：图像、底图与标注历史
    pub(crate) document: PasterDocument,
    /// 工具栏、笔迹、文字会话与右键菜单
    pub(crate) interaction: PasterInteraction,
    /// 配置、媒体执行器与桌面端口
    pub(crate) services: PasterServices,
    pub should_close: bool,
    /// 窗口外观与交互开关
    pub(crate) chrome: PasterChrome,
    pub file_path: Option<PathBuf>,
    /// 视口缩放与光标位置
    pub(crate) viewport: PasterViewport,
    /// 后台媒体任务槽；窗口关闭时通过它取消在途任务
    pub(crate) job_slot: PasterJobSlot,
    /// 当前在途剪贴板任务
    pub(crate) pending_clipboard: Option<PendingClipboardJob>,
    /// 图像顶部状态提示及其产生时刻，超过 [`STATUS_MESSAGE_TTL`] 后自动失效
    pub(crate) status_message: Option<(String, Instant)>,
    /// 当前修饰键状态；文本输入据此忽略 Ctrl 组合字符
    pub(crate) modifiers: ModifiersState,
    /// 渲染帧缓冲与输入区域缓存；都属于可丢弃的表现层缓存
    pub(crate) frame_cache: FrameCache,
}

impl PasterWindowState {
    pub fn new(item: PasterItem) -> Self {
        Self {
            document: PasterDocument::new(item),
            interaction: PasterInteraction::new(create_paster_toolbar()),
            services: PasterServices::new(Arc::new(Config::default()), Arc::new(NoopDesktop)),
            should_close: false,
            chrome: PasterChrome::default(),
            file_path: None,
            viewport: PasterViewport::default(),
            job_slot: PasterJobSlot::default(),
            pending_clipboard: None,
            status_message: None,
            modifiers: ModifiersState::default(),
            frame_cache: FrameCache::default(),
        }
    }

    /// 根据用户配置初始化贴图状态（注入初始不透明度）
    pub fn from_config(mut item: PasterItem, config: Arc<Config>) -> Self {
        item.transform.set_opacity(config.paster.initial_opacity);
        let mut state = Self::new(item);
        state.services.config = config;
        state
    }

    /// 顺时针旋转 90 度并同步调整物理窗口尺寸
    pub fn rotate_cw(&mut self, window: &Window) -> bool {
        if let Some(rotated) = crate::model::transform::rotate_pixmap_cw(&self.document.item.pixmap)
        {
            self.document.item.pixmap = rotated;
            self.reset_annotation_baseline();
            apply_window_scale_resize(self, window);
            true
        } else {
            false
        }
    }

    /// 逆时针旋转 90 度并同步调整物理窗口尺寸
    pub fn rotate_ccw(&mut self, window: &Window) -> bool {
        if let Some(rotated) =
            crate::model::transform::rotate_pixmap_ccw(&self.document.item.pixmap)
        {
            self.document.item.pixmap = rotated;
            self.reset_annotation_baseline();
            apply_window_scale_resize(self, window);
            true
        } else {
            false
        }
    }

    /// 水平翻转图像
    pub fn flip_horizontal(&mut self) -> bool {
        if let Some(flipped) =
            crate::model::transform::flip_pixmap_horizontal(&self.document.item.pixmap)
        {
            self.document.item.pixmap = flipped;
            self.reset_annotation_baseline();
            true
        } else {
            false
        }
    }

    /// 垂直翻转图像
    pub fn flip_vertical(&mut self) -> bool {
        if let Some(flipped) =
            crate::model::transform::flip_pixmap_vertical(&self.document.item.pixmap)
        {
            self.document.item.pixmap = flipped;
            self.reset_annotation_baseline();
            true
        } else {
            false
        }
    }

    /// 设置缩放比例并同步调整物理窗口尺寸
    pub fn set_scale(&mut self, scale: f32, window: &Window) {
        self.document.item.transform.set_scale(scale);
        apply_window_scale_resize(self, window);
    }

    /// 滚轮平滑缩放，并按配置的光标/中心锚点重新定位窗口。
    pub fn zoom(&mut self, delta: f32, window: &Window) {
        let old_size = window.inner_size();
        let old_scale = self.document.item.transform.scale();
        self.document.item.transform.zoom(delta);
        let new_scale = self.document.item.transform.scale();
        if (new_scale - old_scale).abs() < f32::EPSILON {
            return;
        }

        let base_width = self.document.item.pixmap.width() as f32;
        let base_height = self.document.item.pixmap.height() as f32;
        let new_width = (base_width * new_scale).max(20.0);
        let new_height = (base_height * new_scale).max(20.0);
        let (ratio_x, ratio_y) = zoom_anchor_ratio(
            self.services.config.paster.zoom_anchor,
            self.viewport.cursor_pos,
            old_size.width as f32,
            old_size.height as f32,
        );

        // Wayland 的 xdg-shell 不允许客户端查询/设置顶层窗口位置，winit 会返回
        // NotSupported 并忽略 set_outer_position；此时不能假装锚点缩放生效，
        // 只保留窗口尺寸变化，由合成器决定落点，避免把错误静默吞掉。
        if supports_window_positioning(window) {
            match window.outer_position() {
                Ok(position) => {
                    let delta_x = ((new_width - old_size.width as f32) * ratio_x).round() as i32;
                    let delta_y = ((new_height - old_size.height as f32) * ratio_y).round() as i32;
                    window.set_outer_position(PhysicalPosition::new(
                        position.x - delta_x,
                        position.y - delta_y,
                    ));
                }
                Err(error) => {
                    tracing::debug!("读取贴图窗口位置失败，缩放锚点退回合成器默认落点: {error}");
                }
            }
        }

        apply_window_scale_resize(self, window);
    }

    /// 微调不透明度
    pub fn adjust_opacity(&mut self, delta: f32) {
        self.document.item.transform.adjust_opacity(delta);
    }

    /// 设置图像顶部状态提示，有效期从当前时刻开始计算。
    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.set_status_message_at(message, Instant::now());
    }

    /// 以指定时刻设置状态提示，便于对过期判定做确定性单元测试。
    pub(crate) fn set_status_message_at(&mut self, message: impl Into<String>, now: Instant) {
        self.status_message = Some((message.into(), now));
    }

    /// 判断状态提示在给定时刻是否已过期；没有提示时视为已过期。
    pub(crate) fn is_status_expired(&self, now: Instant) -> bool {
        match &self.status_message {
            Some((_, created)) => now.saturating_duration_since(*created) >= STATUS_MESSAGE_TTL,
            None => true,
        }
    }

    /// 清除已过期的状态提示，返回是否发生了清除。
    pub(crate) fn clear_expired_status(&mut self, now: Instant) -> bool {
        if self.status_message.is_some() && self.is_status_expired(now) {
            self.status_message = None;
            true
        } else {
            false
        }
    }

    /// 当前状态提示的失效时刻，供事件循环设置定时唤醒。
    pub(crate) fn status_deadline(&self) -> Option<Instant> {
        self.status_message
            .as_ref()
            .and_then(|(_, created)| created.checked_add(STATUS_MESSAGE_TTL))
    }

    /// 将整图翻译结果作为一个复合标注压入历史，并从底图重建展示图像。
    ///
    /// 返回实际注入的翻译段数。翻译图元坐标与底图处于同一图像像素坐标系，
    /// 因此必须走撤销/重做同源的重建路径，保证后续撤销可以精确恢复底图。
    pub fn apply_translation_items(&mut self, items: Vec<TranslationShapeItem>) -> usize {
        if items.is_empty() {
            return 0;
        }
        let count = items.len();
        self.ensure_base_pixmap();
        self.document
            .annotations
            .push(AnnotationShape::Translation(TranslationAction::new(items)));
        self.rebuild_item_pixmap();
        count
    }

    /// 更新贴图像素内容并同步调整物理窗口尺寸
    pub fn replace_pixmap(&mut self, pixmap: tiny_skia::Pixmap, window: &Window) {
        self.document.item.pixmap = pixmap;
        // 新图像意味着旧标注的坐标体系已完全失效，必须连同历史一起重置
        self.reset_annotation_baseline();
        apply_window_scale_resize(self, window);
    }

    /// 显示/隐藏标注工具栏，并让窗口尺寸跟随工具条扩缩。
    ///
    /// 显示时窗口需要在图像下方额外容纳属性栏与主栏；隐藏时恢复为贴近图像的尺寸。
    /// 隐藏同时提交进行中的文字会话并退出绘制模式，避免已输入内容丢失或 IME 状态残留。
    ///
    /// 鼠标穿透开启时图像区域不参与输入命中，工具条是唯一可恢复交互的手柄；
    /// 此时禁止隐藏工具条，否则窗口将完全点击穿透且无法从窗口内部恢复。
    pub fn set_toolbar_visible(&mut self, visible: bool, window: &Window) {
        if !self.can_set_toolbar_visible(visible) {
            self.set_status_message(snapin_foundation::i18n::tr(
                "鼠标穿透开启时不能隐藏工具条，请先关闭穿透",
            ));
            return;
        }

        self.chrome.show_toolbar = visible;
        if !visible {
            self.commit_text_session();
            self.interaction.annotation.toolbar.active_tool = None;
            self.interaction.annotation.stroke.points.clear();
            self.interaction.annotation.stroke.is_drawing = false;
        }
        apply_window_scale_resize(self, window);
    }

    /// 判断当前状态是否允许把工具条切换到目标可见性。
    ///
    /// 只有“隐藏工具条”在鼠标穿透开启时被拒绝；显示工具条始终允许，
    /// 这样 `set_mouse_through(true)` 的强制显示路径不会被守卫误伤。
    pub(crate) fn can_set_toolbar_visible(&self, visible: bool) -> bool {
        visible || !self.chrome.mouse_through
    }

    /// 切换贴图鼠标穿透。
    ///
    /// 开启时强制显示工具条并关闭右键菜单：图像区域将不再参与输入命中，
    /// 用户只能通过工具条上的穿透按钮恢复交互，因此工具条必须始终可见。
    pub fn set_mouse_through(&mut self, enabled: bool, window: &Window) {
        self.chrome.mouse_through = enabled;
        if enabled {
            if !self.chrome.show_toolbar {
                self.set_toolbar_visible(true, window);
            }
            self.cancel_text_session();
            window.set_ime_allowed(false);
            self.interaction.annotation.stroke.points.clear();
            self.interaction.annotation.stroke.is_drawing = false;
            self.interaction.context_menu.close();
            self.set_status_message(snapin_foundation::i18n::tr(
                "鼠标穿透已开启，点击工具条上的穿透按钮恢复",
            ));
        } else {
            self.set_status_message(snapin_foundation::i18n::tr("鼠标穿透已关闭"));
        }

        apply_window_scale_resize(self, window);
        window.request_redraw();
    }

    /// 判断屏幕坐标是否落在缩放后的图像内部（边框画在图像内部，不占布局，故直接用图像尺寸换算）。
    pub(crate) fn screen_point_inside_image(&self, pos: (f32, f32)) -> bool {
        crate::paster::window::annotation::is_point_inside_image(
            pos,
            self.document.item.transform.scale(),
            self.document.item.pixmap.width(),
            self.document.item.pixmap.height(),
        )
    }

    /// 屏幕坐标转图像坐标（使用当前缩放并夹取边界）
    pub(crate) fn image_point(&self, screen_pos: (f32, f32)) -> (f32, f32) {
        screen_to_image_point(
            screen_pos,
            self.document.item.transform.scale(),
            self.document.item.pixmap.width(),
            self.document.item.pixmap.height(),
        )
    }

    /// 从原始底图重建展示图像，并按顺序重放当前生效的标注。
    fn rebuild_item_pixmap(&mut self) {
        let Some(base) = self.document.base_pixmap.as_ref() else {
            return;
        };
        let mut pixmap = base.clone();
        self.document.annotations.replay_all(&mut pixmap.as_mut());
        self.document.item.pixmap = pixmap;
        if !self.document.annotations.can_undo() && !self.document.annotations.can_redo() {
            // 历史已空，底图与当前图完全一致，释放副本回到单图状态。
            self.document.base_pixmap = None;
        }
    }

    /// 首次产生标注前捕获底图，避免所有贴图窗口无条件持有第二份完整像素。
    pub(crate) fn ensure_base_pixmap(&mut self) {
        if self.document.base_pixmap.is_none() {
            self.document.base_pixmap = Some(self.document.item.pixmap.clone());
        }
    }

    /// 清空全部标注并从底图恢复图像。
    pub(crate) fn clear_annotations(&mut self) {
        self.ensure_base_pixmap();
        self.document.annotations.clear();
        self.interaction.annotation.stroke.clear();
        self.interaction.annotation.active_text_input = None;
        self.rebuild_item_pixmap();
    }

    /// 将当前图像固化为新的标注底图并清空历史。
    ///
    /// 旋转/翻转/替换图像会改变像素坐标体系，历史图元在新坐标系中已失去意义
    /// （特别是旋转会交换宽高），因此必须整体重置而不能保留撤销记录。
    fn reset_annotation_baseline(&mut self) {
        self.document.base_pixmap = None;
        self.document.annotations.clear();
        self.interaction.annotation.stroke.points.clear();
        self.interaction.annotation.stroke.is_drawing = false;
        // 坐标系已失效，进行中的文字会话不能再落到新底图上
        self.interaction.annotation.active_text_input = None;
    }
}

/// 重新计算并应用根据缩放比例、工具条与右键菜单展开状态调整后的窗口尺寸。
///
/// 工具条显示时窗口会在图像下方扩容：宽度需同时容纳主栏与属性栏，
/// 高度叠加属性栏预留、主栏高度与上下边距。扩容区域在 ARGB8888 通路上是透明的，
/// 因此不会影响贴图观感；右键菜单所需尺寸与之取最大值，两者互不覆盖。
pub(crate) fn apply_window_scale_resize(state: &mut PasterWindowState, window: &Window) {
    let base_w = state.document.item.pixmap.width() as f32;
    let base_h = state.document.item.pixmap.height() as f32;
    let scale = state.document.item.transform.scale();
    let mut new_w = (base_w * scale).max(20.0);
    let mut new_h = (base_h * scale).max(20.0);

    if state.chrome.show_toolbar {
        // 工具条度量以 state.viewport.ui_scale 为准：首次在 HiDPI 屏上打开工具条时，
        // toolbar.ui_scale 可能仍是初始的 1.0，先同步避免按过期倍率扩容不足。
        state.interaction.annotation.toolbar.ui_scale = state.viewport.ui_scale.max(0.1);
        let (req_w, req_h) = toolbar_required_extent(
            state.viewport.ui_scale,
            state.interaction.annotation.toolbar.main_width(),
            state.interaction.annotation.toolbar.main_height(),
        );
        new_w = new_w.max(req_w);
        new_h += req_h;
    }

    // 若当前正在展示右键菜单，确保窗口物理尺寸足以容纳菜单而不被截断
    if state.interaction.context_menu.is_open {
        let main_items =
            ContextMenuState::build_main_items(state.chrome.show_toolbar, state.chrome.has_shadow);
        let (req_w, req_h) = state
            .interaction
            .context_menu
            .required_menu_extent(&main_items);
        new_w = new_w.max(req_w);
        new_h = new_h.max(req_h);
    }

    let _ = window.request_inner_size(PhysicalSize::new(new_w as u32, new_h as u32));
}

/// 当前窗口后端是否允许客户端查询和设置顶层窗口位置。
///
/// Wayland 下该能力由合成器垄断，返回 `false`；X11、Windows 与 macOS 返回 `true`。
fn supports_window_positioning(window: &Window) -> bool {
    window
        .display_handle()
        .map(|handle| !matches!(handle.as_raw(), RawDisplayHandle::Wayland(_)))
        .unwrap_or(false)
}

/// 计算缩放时用于保持锚点稳定的相对位置比例。
///
/// - 光标锚点：返回光标在旧窗口尺寸中的相对比例；
/// - 中心锚点：返回 `(0.5, 0.5)`。
pub(crate) fn zoom_anchor_ratio(
    anchor: ZoomAnchor,
    cursor: (f32, f32),
    old_width: f32,
    old_height: f32,
) -> (f32, f32) {
    match anchor {
        ZoomAnchor::Cursor => (
            (cursor.0 / old_width.max(1.0)).clamp(0.0, 1.0),
            (cursor.1 / old_height.max(1.0)).clamp(0.0, 1.0),
        ),
        ZoomAnchor::Center => (0.5, 0.5),
    }
}

#[cfg(test)]
mod tests;
