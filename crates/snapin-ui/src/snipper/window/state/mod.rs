//! 截图交互会话状态机模型与配置初始化。

mod editor;
pub(crate) mod frame_sync;
mod jobs;
pub(crate) mod long_capture;
mod long_capture_scroll;
mod ocr;
mod phase;
mod recording;
mod session;
mod viewport;

use super::jobs::SnipperJobKind;
use super::types::SnipMode;
use crate::editor::toolbar::{clear_remembered_sub_options, remembered_sub_options, Toolbar};
#[cfg(test)]
use crate::ports::PortError;
use crate::ports::{DesktopPort, JobCancel, JobId, NoopDesktop};
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window_rect::WindowRect;
use editor::SnipperEditor;
#[cfg(test)]
use image::GenericImageView;
use image::RgbaImage;
use jobs::{PendingMediaJob, SnipperJobs};
pub(crate) use long_capture::LongCanvasSpace;
pub use long_capture::{DirectionToggleOutcome, LongCaptureAction, LongCaptureSession};
pub(crate) use long_capture_scroll::AutoScrollStopReason;
use ocr::SnipperOcr;
pub use phase::SnipperPhase;
pub use recording::{
    RecordingAction, RecordingItem, RecordingSession, RecordingStatus, RecordingTransition,
};
use session::SnipperSession;
use snapin_config::config::{Config, InterfaceConfig, SnipConfig};
#[cfg(test)]
use snapin_foundation::{i18n::tr, trf};
#[cfg(test)]
use snapin_render::StitchFrameOutcome;
use std::sync::Arc;
#[cfg(test)]
use tracing::warn;
use viewport::SnipperViewport;

/// 异步抓屏结果对覆盖层冻结底图造成的影响。
///
/// 之所以单独建模而非仅返回布尔值，是因为 `Stale`（过期回调）、`NoBgUpdate`（静止、
/// 失配或尺寸漂移帧仍需更新状态提示与指示灯）和 `BgUpdated`（仅在长截图参考帧推进或
/// 反向裁剪时替换 `bg_image` 并重建整屏暗化缓存）对应不同的重绘成本，避免冗余像素遍历。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapturedFrameEffect {
    /// 抓屏序号已过期或与当前在途请求不匹配，忽略本次结果。
    Stale,
    /// 本次结果已被处理（状态提示或指示灯可能更新），但参考底图未变化，无需重建整屏暗化缓存。
    NoBgUpdate,
    /// 拼接画布与参考帧已更新，`bg_image` 已被替换为最新帧，需重建暗化背景。
    BgUpdated,
}

/// 截图会话可变运行状态封装。
///
/// 字段按视口、编辑、会话、异步任务和 OCR 五个职责分组，避免所有事件处理器都
/// 直接操作一个无边界的字段集合。各子状态仍由同一条 GUI 线程独占，不引入额外锁。
pub struct SnipperState {
    /// 选区、窗口候选与光标展示状态。
    pub(crate) viewport: SnipperViewport,
    /// 工具栏、标注历史与文本输入状态。
    pub(crate) editor: SnipperEditor,
    /// 配置、宿主端口与最终输出状态。
    pub(crate) session: SnipperSession,
    /// 异步媒体与剪贴板任务状态。
    pub(crate) jobs: SnipperJobs,
    /// OCR、二维码与智能打码状态。
    pub(crate) ocr: SnipperOcr,
    /// 选区屏幕录制会话。
    pub(crate) recording: Option<RecordingSession>,
    /// 滚动拼接长截图会话状态（若激活）。
    pub(crate) long_capture: Option<LongCaptureSession>,
}

impl SnipperState {
    /// 创建无宿主副作用的截图状态。
    pub fn new() -> Self {
        Self::new_with_desktop(Arc::new(NoopDesktop))
    }

    /// 使用宿主注入的桌面端口构建空状态。
    ///
    /// UI 不直接读写系统剪贴板或选区缓存文件，因此单元测试与渲染测试可以使用
    /// `NoopDesktop` 构造完全无系统副作用的会话状态。
    pub fn new_with_desktop(desktop: Arc<dyn DesktopPort>) -> Self {
        let mut toolbar = Toolbar::default();
        toolbar.draggable = true;
        Self {
            viewport: SnipperViewport::new(),
            editor: SnipperEditor::new(toolbar),
            session: SnipperSession::new(desktop, SnipMode::default()),
            jobs: SnipperJobs::new(),
            ocr: SnipperOcr::default(),
            recording: None,
            long_capture: None,
        }
    }

    /// 按用户配置决定是否持久化本次选区。
    pub fn remember_selection(&self, rect: Rect) {
        if self.session.config.snip.remember_last_selection {
            self.session.desktop.save_last_selection(rect);
        }
    }

    /// 取消当前媒体任务与在途长截图抓屏请求（若存在）。
    pub fn cancel_pending_job(&mut self) {
        if let Some(pending) = self.jobs.pending_media.take() {
            (pending.cancel)();
        }
        self.jobs.cancel_screen_capture();
    }

    /// 中止并彻底清理当前长截图会话及其关联编辑与在途抓屏状态。
    ///
    /// 为什么集中收口而非各处散落写入 `long_capture = None`：
    /// 长截图同时跨越在途抓屏请求、编辑期工具/草稿笔迹以及长图坐标系内的标注历史，
    /// 仅置空会话或仅重置选区都会残留过期回调、形成幽灵长截图阶段或污染退回后的选区；
    /// 集中清理可确保 Esc、右键、关闭按钮与窗口关闭维持完全一致的状态不变量。
    pub(crate) fn abort_long_capture(&mut self) -> bool {
        if self.long_capture.take().is_none() {
            return false;
        }
        self.cancel_pending_job();
        self.editor.annotation.active_text_input = None;
        self.editor.annotation.is_selecting_text = false;
        self.editor.annotation.stroke.clear();
        self.editor.annotation.toolbar.active_tool = None;
        self.editor.annotation.toolbar.sub_rect = None;
        self.editor.annotation.toolbar.restore_default_items();
        self.editor.selected_annotation = None;
        self.editor.annotation_drag = None;
        self.editor.annotations.clear();
        self.clear_status_message();
        true
    }

    /// 是否存在尚未完成的媒体任务。
    pub fn has_pending_job(&self) -> bool {
        self.jobs.pending_media.is_some()
    }

    /// 登记一个已成功提交的媒体任务。
    pub(crate) fn begin_pending_job(&mut self, kind: SnipperJobKind, id: JobId, cancel: JobCancel) {
        self.begin_pending_job_at(kind, id, cancel, None);
    }

    /// 登记一个美化贴图任务，并记录结果渲染完成后的放置位置。
    ///
    /// 贴图落点在提交时固化，避免任务执行期间用户移动选区导致图像与窗口位置错位。
    pub(crate) fn begin_pin_job(&mut self, id: JobId, cancel: JobCancel, position: (i32, i32)) {
        self.begin_pending_job_at(SnipperJobKind::Pin, id, cancel, Some(position));
    }

    /// 登记在途任务的统一入口。
    fn begin_pending_job_at(
        &mut self,
        kind: SnipperJobKind,
        id: JobId,
        cancel: JobCancel,
        pin_position: Option<(i32, i32)>,
    ) {
        debug_assert!(
            self.jobs.pending_media.is_none(),
            "同一截图会话只能有一个在途任务"
        );
        debug_assert!(
            (kind == SnipperJobKind::Pin) == pin_position.is_some(),
            "只有贴图任务可以携带贴图落点"
        );
        self.jobs.pending_media = Some(PendingMediaJob {
            kind,
            id,
            cancel,
            pin_position,
        });
    }

    /// 仅当任务类型与 ID 都匹配时取出在途任务；调用方据此读取任务附加信息。
    pub(crate) fn take_matching_job(
        &mut self,
        kind: SnipperJobKind,
        id: JobId,
    ) -> Option<PendingMediaJob> {
        let matches = self
            .jobs
            .pending_media
            .as_ref()
            .is_some_and(|pending| pending.kind == kind && pending.id == id);
        if matches {
            self.jobs.pending_media.take()
        } else {
            None
        }
    }

    /// 当前任务 ID；仅供同 crate 的任务状态校验与测试使用。
    #[cfg(test)]
    pub(crate) fn pending_job_id(&self) -> Option<JobId> {
        self.jobs.pending_media.as_ref().map(|pending| pending.id)
    }

    /// 设置画布顶部提示信息。
    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.session.status_message = Some(message.into());
    }

    /// 清除画布顶部提示信息。
    pub fn clear_status_message(&mut self) {
        self.session.status_message = None;
    }

    /// 将异步抓取的完整屏幕帧裁剪到当前选区，并投喂给长截图拼接器。
    ///
    /// 返回 [`StitchFrameOutcome`] 以明确告知上层是否真实修改了画布与参考帧。抓屏回包
    /// 可能晚于用户停止或关闭长截图，或在窗口/缩放波动时返回不同尺寸，因此任何无效帧均安全
    /// 返回 [`StitchFrameOutcome::Ignored`] 而不污染现存会话与冻结底图。
    #[cfg(test)]
    pub(crate) fn apply_long_capture_frame(
        &mut self,
        full_frame: &RgbaImage,
    ) -> StitchFrameOutcome {
        let SelectionState::Locked { rect, .. } = self.viewport.selection else {
            return StitchFrameOutcome::Ignored;
        };
        let x = (rect.x.max(0.0) as u32).min(full_frame.width());
        let y = (rect.y.max(0.0) as u32).min(full_frame.height());
        let width = (rect.width as u32).min(full_frame.width().saturating_sub(x));
        let height = (rect.height as u32).min(full_frame.height().saturating_sub(y));
        if width == 0 || height == 0 {
            return StitchFrameOutcome::Ignored;
        }

        let Some(session) = self.long_capture.as_mut() else {
            return StitchFrameOutcome::Ignored;
        };
        if !session.is_capturing || (width, height) != session.stitcher.initial_frame_size() {
            return StitchFrameOutcome::Ignored;
        }
        let selection_frame = full_frame.view(x, y, width, height).to_image();
        session.feed_frame_outcome(&selection_frame)
    }

    /// 校验异步抓屏序号与全屏分辨率，并依据真实拼接结果决定是否更新覆盖层冻结底图。
    ///
    /// 按真实状态转移返回 [`CapturedFrameEffect`]：仅在画布末端参考帧变化时替换 `bg_image`，
    /// 避免静止、失配、暂停后滞留回包或分辨率突变误覆底图或反复重建整屏暗化纹理。
    #[cfg(test)]
    pub(crate) fn apply_captured_long_frame(
        &mut self,
        bg_image: &mut RgbaImage,
        request_id: u64,
        result: Result<RgbaImage, PortError>,
    ) -> CapturedFrameEffect {
        if !self.jobs.finish_screen_capture(request_id) {
            return CapturedFrameEffect::Stale;
        }
        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                warn!("长截图抓屏失败，请求序号: {request_id}，原因: {error}");
                self.set_status_message(trf!(
                    "长截图抓屏失败: {}",
                    "Long capture failed: {}",
                    error
                ));
                return CapturedFrameEffect::NoBgUpdate;
            }
        };
        if !self.is_valid_long_capture_dimensions(bg_image, &frame) {
            return CapturedFrameEffect::NoBgUpdate;
        }
        let outcome = self.apply_long_capture_frame(&frame);
        self.finish_stitch_outcome(bg_image, frame, outcome)
    }

    /// 根据用户配置、触发模式与宿主端口构建初始状态。
    pub fn from_config(cfg: Config, mode: SnipMode, desktop: Arc<dyn DesktopPort>) -> Self {
        let mut state = Self::new_with_desktop(desktop);
        state.session.mode = mode;
        apply_interface_config(&mut state, &cfg.interface);
        apply_snip_config(&mut state, &cfg.snip, mode);
        state.session.config = cfg;
        state
    }
}

impl Default for SnipperState {
    fn default() -> Self {
        Self::new()
    }
}

/// 注入界面视觉与放大镜设置
fn apply_interface_config(state: &mut SnipperState, iface: &InterfaceConfig) {
    state.viewport.magnifier.enabled = iface.enable_magnifier;
    state.viewport.magnifier.scale = iface.magnifier_scale;
    state.viewport.magnifier.show_grid = iface.show_grid_lines;
    state.viewport.magnifier.format = iface.color_format;
    state.editor.annotation.toolbar.ui_scale = iface.toolbar_scale;
}

/// 注入截图流程、默认标注工具与选区记忆设置
fn apply_snip_config(state: &mut SnipperState, snip: &SnipConfig, mode: SnipMode) {
    state.editor.annotation.toolbar.sub_options = crate::editor::toolbar::SubToolOptions::default();
    state
        .editor
        .annotation
        .toolbar
        .sub_options
        .custom_stroke_width = snip.default_stroke_width;
    state.editor.annotation.toolbar.sub_options.font_size = snip.default_font_size;
    state.editor.annotation.toolbar.sub_options.color = tiny_skia::Color::from_rgba8(
        snip.default_color.r,
        snip.default_color.g,
        snip.default_color.b,
        snip.default_color.a,
    );
    state.editor.annotation.toolbar.remember_sub_options = snip.remember_toolbar_settings;
    if snip.remember_toolbar_settings {
        if let Some(options) = remembered_sub_options() {
            state.editor.annotation.toolbar.sub_options = options;
        }
    } else {
        clear_remembered_sub_options();
    }
    // 初始进入截图时，未完成选区前不激活标注工具，确保左键优先用于选区拉框
    state.editor.annotation.toolbar.active_tool = None;

    // 仅“自定义截屏”复用上次选区；常规截图始终从空白选区开始，
    // 避免启动时自动锁定旧尺寸、妨碍用户重新拉框。
    if snip.remember_last_selection && should_restore_last_selection(mode) {
        if let Some(rect) = state.session.desktop.load_last_selection() {
            state.viewport.selection = SelectionState::Locked {
                rect,
                active_drag: None,
            };
        }
    }
}

/// 判断当前触发模式是否允许恢复历史选区。
fn should_restore_last_selection(mode: SnipMode) -> bool {
    mode == SnipMode::Custom
}

/// 将扩展返回的窗口矩形夹取到截图图像范围内。
pub(crate) fn clamp_window_rect(win: WindowRect, screen_w: f32, screen_h: f32) -> Option<Rect> {
    let left = (win.x as f32).max(0.0).min(screen_w);
    let top = (win.y as f32).max(0.0).min(screen_h);
    let right = (win.x as f32 + win.width as f32).max(0.0).min(screen_w);
    let bottom = (win.y as f32 + win.height as f32).max(0.0).min(screen_h);
    let width = (right - left).max(0.0);
    let height = (bottom - top).max(0.0);
    if width > 1.0 && height > 1.0 {
        Some(Rect {
            x: left,
            y: top,
            width,
            height,
        })
    } else {
        None
    }
}

/// 查找光标位置命中的顶层窗口；窗口列表按从底到顶排列。
pub(crate) fn window_at(pos: (f32, f32), windows: &[Rect]) -> Option<Rect> {
    windows
        .iter()
        .rev()
        .find(|rect| rect.contains(pos.0, pos.1))
        .copied()
}

impl SnipperState {
    /// 把当前标注历史栅格化为录制透明层并同步给后台采集器。
    ///
    /// 只允许在 GUI 线程调用。没有录制会话、未开始录制或没有标注时分别安全跳过，
    /// 这样现有截图路径无需感知录屏状态。
    pub(crate) fn sync_recording_overlay(&mut self) {
        let Some(region) = self.recording.as_ref().and_then(|session| session.region()) else {
            return;
        };
        let overlay = if self.editor.annotations.can_undo() {
            let Some(mut pixmap) = tiny_skia::Pixmap::new(region.width, region.height) else {
                return;
            };
            self.editor.annotations.replay_with_offset(
                &mut pixmap.as_mut(),
                -(region.x as f32),
                -(region.y as f32),
            );
            RgbaImage::from_raw(region.width, region.height, pixmap.take())
        } else {
            None
        };
        if let Some(session) = self.recording.as_ref() {
            session.update_overlay(overlay);
        }
    }

    /// 当前若处于长截图编辑态且选区已锁定，返回屏幕与长画布的双向坐标映射器。
    pub(crate) fn long_canvas_space(&self) -> Option<LongCanvasSpace> {
        let session = self.long_capture.as_ref()?;
        let SelectionState::Locked { rect, .. } = &self.viewport.selection else {
            return None;
        };
        LongCanvasSpace::from_session(session, rect, self.viewport.screen_size)
    }

    /// 将屏幕坐标转换为当前激活标注文档所属坐标系（长图编辑态为长画布坐标，普通态为屏幕坐标）。
    pub(crate) fn annotation_point(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        self.long_canvas_space()
            .map_or((screen_x, screen_y), |space| {
                space.screen_to_canvas(screen_x, screen_y)
            })
    }

    /// 判断屏幕点位是否允许与已有标注交互（长图编辑态下禁止点击可视视口外的被裁剪图元）。
    pub(crate) fn can_interact_annotation_at(&self, screen_x: f32, screen_y: f32) -> bool {
        self.long_canvas_space()
            .is_none_or(|space| space.contains_visible_screen_point(screen_x, screen_y))
    }
}

/// 提交并关闭当前文本输入会话。
///
/// 文本图元同样遵守"最新一条可编辑"的约定：提交后立即把选中切到它，
/// 使刚输入的文字可以马上拖动或用滚轮调字号。
pub fn commit_active_text_input(state: &mut SnipperState) {
    if let Some(session) = state.editor.annotation.active_text_input.take() {
        if let Some(action) = session.commit() {
            state.editor.annotations.push(action);
            super::annotation_edit::select_latest(state);
            state.sync_recording_overlay();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    static TOOLBAR_MEMORY_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// 仅自定义截图恢复历史选区，常规截图始终从空白起步
    #[test]
    fn test_should_restore_last_selection_by_mode() {
        assert!(!should_restore_last_selection(SnipMode::Normal));
        assert!(should_restore_last_selection(SnipMode::Custom));
        assert!(!should_restore_last_selection(SnipMode::AutoCopy));
        assert!(!should_restore_last_selection(SnipMode::Translate));
        assert!(!should_restore_last_selection(SnipMode::Fullscreen));
        assert!(!should_restore_last_selection(SnipMode::PickColor));
    }

    /// 初始状态不得把 (0,0) 当成真实光标位置
    #[test]
    fn test_cursor_not_ready_until_first_move() {
        assert!(!SnipperState::new().viewport.cursor_ready);
    }

    /// 在途任务必须由类型、ID 和取消闭包共同组成，任一项不匹配都不得清除当前任务。
    #[test]
    fn test_pending_job_requires_matching_kind_and_id_and_cancel_runs() {
        let mut state = SnipperState::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_flag = cancelled.clone();
        state.begin_pending_job(
            SnipperJobKind::Ocr,
            JobId::new(7),
            Box::new(move || cancel_flag.store(true, Ordering::Release)),
        );

        assert!(state
            .take_matching_job(SnipperJobKind::Save, JobId::new(7))
            .is_none());
        assert!(state
            .take_matching_job(SnipperJobKind::Ocr, JobId::new(8))
            .is_none());
        assert!(state.has_pending_job(), "错误回调不得清除当前任务");

        state.cancel_pending_job();
        assert!(cancelled.load(Ordering::Acquire));
        assert!(!state.has_pending_job());
    }

    /// 默认关闭记忆时，新建截图应恢复配置默认值而不是沿用上次改动。
    #[test]
    fn test_toolbar_options_reset_between_captures_by_default() {
        let _guard = TOOLBAR_MEMORY_TEST_LOCK
            .lock()
            .expect("工具栏记忆测试锁不得中毒");
        clear_remembered_sub_options();
        let config = Config::default();
        let desktop: Arc<dyn DesktopPort> = Arc::new(NoopDesktop);
        let mut first =
            SnipperState::from_config(config.clone(), SnipMode::Normal, desktop.clone());
        first.editor.annotation.toolbar.sub_options.color =
            tiny_skia::Color::from_rgba8(0, 0, 255, 255);
        first.editor.annotation.toolbar.update_layout(
            &Rect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 100.0,
            },
            800.0,
            600.0,
        );

        let second = SnipperState::from_config(config, SnipMode::Normal, desktop);
        assert_eq!(
            second.editor.annotation.toolbar.sub_options.color,
            tiny_skia::Color::from_rgba8(239, 68, 68, 255)
        );
    }

    /// 开启记忆后，下一次截图应继承上一张截图最后调整的工具栏选项。
    #[test]
    fn test_toolbar_options_can_be_remembered_between_captures() {
        let _guard = TOOLBAR_MEMORY_TEST_LOCK
            .lock()
            .expect("工具栏记忆测试锁不得中毒");
        clear_remembered_sub_options();
        let mut config = Config::default();
        config.snip.remember_toolbar_settings = true;
        let desktop: Arc<dyn DesktopPort> = Arc::new(NoopDesktop);
        let mut first =
            SnipperState::from_config(config.clone(), SnipMode::Normal, desktop.clone());
        first.editor.annotation.toolbar.sub_options.color =
            tiny_skia::Color::from_rgba8(0, 0, 255, 255);
        first.editor.annotation.toolbar.update_layout(
            &Rect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 100.0,
            },
            800.0,
            600.0,
        );

        let second = SnipperState::from_config(config, SnipMode::Normal, desktop);
        assert_eq!(
            second.editor.annotation.toolbar.sub_options.color,
            tiny_skia::Color::from_rgba8(0, 0, 255, 255)
        );
        clear_remembered_sub_options();
    }

    /// 窗口矩形应夹取到截图范围，完全越界的窗口被丢弃
    #[test]
    fn test_clamp_window_rect_clips_to_screen() {
        let rect = clamp_window_rect(
            WindowRect {
                x: -10,
                y: 20,
                width: 200,
                height: 100,
            },
            100.0,
            80.0,
        )
        .expect("部分可见窗口应保留");
        assert_eq!(
            rect,
            Rect {
                x: 0.0,
                y: 20.0,
                width: 100.0,
                height: 60.0,
            }
        );

        assert!(
            clamp_window_rect(
                WindowRect {
                    x: 200,
                    y: 200,
                    width: 10,
                    height: 10,
                },
                100.0,
                80.0,
            )
            .is_none(),
            "完全越界窗口必须丢弃"
        );
    }

    /// 悬停命中应优先选择堆叠列表中更靠上的窗口
    #[test]
    fn test_window_at_prefers_topmost() {
        let bottom = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let top = Rect {
            x: 20.0,
            y: 20.0,
            width: 50.0,
            height: 50.0,
        };
        assert_eq!(window_at((30.0, 30.0), &[bottom, top]), Some(top));
        assert_eq!(window_at((5.0, 5.0), &[bottom, top]), Some(bottom));
        assert_eq!(window_at((200.0, 200.0), &[bottom, top]), None);
    }

    fn make_textured_screen(scroll_y: u32) -> (RgbaImage, RgbaImage) {
        use image::Rgba;

        let mut screen = RgbaImage::from_pixel(120, 140, Rgba([10, 10, 10, 255]));
        let mut crop = RgbaImage::new(80, 100);
        for y in 0..100u32 {
            let wy = y.saturating_add(scroll_y);
            for x in 0..80u32 {
                let r = ((wy.wrapping_mul(37).wrapping_add(x * 13)) % 251) as u8;
                let g = ((wy.wrapping_mul(73).wrapping_add(x * 29)) % 241) as u8;
                let b = ((((wy * wy) / 17).wrapping_add(x * 41)) % 239) as u8;
                let px = Rgba([r, g, b, 255]);
                screen.put_pixel(20 + x, 20 + y, px);
                crop.put_pixel(x, y, px);
            }
        }
        (screen, crop)
    }

    fn setup_long_capture_state() -> (SnipperState, RgbaImage) {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 80.0, 100.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let (screen, crop) = make_textured_screen(0);
        state.long_capture = Some(LongCaptureSession::new(crop));
        (state, screen)
    }

    #[test]
    fn test_apply_captured_long_frame_syncs_bg_on_stitch_and_preserves_on_unchanged_or_unmatched() {
        use image::Rgba;

        let (mut state, mut bg_image) = setup_long_capture_state();

        let (shifted_screen, _) = make_textured_screen(30);
        let req1 = state.jobs.begin_screen_capture().expect("应成功注册抓屏");
        assert_eq!(
            state.apply_captured_long_frame(&mut bg_image, req1, Ok(shifted_screen.clone())),
            CapturedFrameEffect::BgUpdated
        );
        assert_eq!(bg_image, shifted_screen);
        let session = state.long_capture.as_ref().expect("会话应存在");
        assert!(session.is_matched);
        assert_eq!(session.stitcher.canvas().dimensions(), (80, 130));

        let mut unrelated_screen = RgbaImage::from_pixel(120, 140, Rgba([10, 10, 10, 255]));
        for y in 0..100u32 {
            for x in 0..80u32 {
                let v = if ((x / 7) + (y / 7)) % 2 == 0 { 0 } else { 255 };
                unrelated_screen.put_pixel(20 + x, 20 + y, Rgba([v, v, v, 255]));
            }
        }
        let req2 = state.jobs.begin_screen_capture().expect("应成功注册抓屏");
        assert_eq!(
            state.apply_captured_long_frame(&mut bg_image, req2, Ok(unrelated_screen)),
            CapturedFrameEffect::NoBgUpdate
        );
        assert_eq!(bg_image, shifted_screen, "失配帧绝不能覆盖当前参考底图");
        assert!(!state.long_capture.as_ref().expect("会话应存在").is_matched);
        assert!(state
            .session
            .status_message
            .as_deref()
            .is_some_and(|msg| msg.contains("未能匹配新画面")));

        let req3 = state.jobs.begin_screen_capture().expect("应成功注册抓屏");
        assert_eq!(
            state.apply_captured_long_frame(&mut bg_image, req3, Ok(shifted_screen.clone())),
            CapturedFrameEffect::NoBgUpdate
        );
        assert_eq!(bg_image, shifted_screen);
        assert!(state.long_capture.as_ref().expect("会话应存在").is_matched);
        assert_eq!(
            state.session.status_message.as_deref(),
            Some(tr("长截图录制中：在选区内滚动鼠标自动拼接"))
        );
    }

    #[test]
    fn test_apply_captured_long_frame_rejects_mismatch_stale_error_and_paused() {
        use image::Rgba;
        use snapin_ports::PortErrorKind;

        let (mut state, mut bg_image) = setup_long_capture_state();
        let initial_bg = bg_image.clone();

        let req = state.jobs.begin_screen_capture().expect("应成功注册抓屏");
        let (shifted_screen, _) = make_textured_screen(25);
        assert_eq!(
            state.apply_captured_long_frame(
                &mut bg_image,
                req.saturating_add(1),
                Ok(shifted_screen),
            ),
            CapturedFrameEffect::Stale
        );
        assert_eq!(bg_image, initial_bg);
        assert!(state.jobs.has_pending_screen_capture());

        assert_eq!(
            state.apply_captured_long_frame(
                &mut bg_image,
                req,
                Err(PortError::with_kind(
                    PortErrorKind::Timeout,
                    "capture",
                    "超时",
                )),
            ),
            CapturedFrameEffect::NoBgUpdate
        );
        assert_eq!(bg_image, initial_bg);
        assert!(state
            .session
            .status_message
            .as_deref()
            .is_some_and(|msg| msg.contains("长截图抓屏失败")));

        let wrong_size = RgbaImage::from_pixel(240, 280, Rgba([50, 60, 70, 255]));
        let req2 = state.jobs.begin_screen_capture().expect("应成功注册抓屏");
        assert_eq!(
            state.apply_captured_long_frame(&mut bg_image, req2, Ok(wrong_size)),
            CapturedFrameEffect::NoBgUpdate
        );
        assert_eq!(bg_image, initial_bg);
        assert_eq!(
            state.session.status_message.as_deref(),
            Some(tr("抓屏分辨率与当前画布不一致，已保留原底图"))
        );

        state
            .long_capture
            .as_mut()
            .expect("会话应存在")
            .toggle_capture();
        let (shifted_screen, _) = make_textured_screen(25);
        let req3 = state.jobs.begin_screen_capture().expect("应成功注册抓屏");
        assert_eq!(
            state.apply_captured_long_frame(&mut bg_image, req3, Ok(shifted_screen)),
            CapturedFrameEffect::NoBgUpdate
        );
        assert_eq!(bg_image, initial_bg);
    }
}
