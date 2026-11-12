//! 长截图交互会话状态机模型。
//!
//! 管理长截图的捕获状态、拼接引擎、视口偏移、缩略图热区与专用控制条。

use super::long_capture_scroll::{AutoScrollStopReason, LongCaptureScrollDriver};
use crate::geometry::{Point, Rect};
use crate::ports::WheelDelta;
use snapin_render::stitching::{
    FrameShiftAnalysis, LongCaptureStitcher, StitchDirection, StitchFrameOutcome, StitchOptions,
    ThumbnailRenderSpec, ViewportWindow,
};
use std::time::Instant;

/// 长截图专用控制动作，对应控制栏的核心交互功能。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongCaptureAction {
    /// 拖动长截图控制栏（只移动控制栏本身，不改变截图选区）
    DragToolbar,
    /// 切换拼接方向（纵向/横向）
    ToggleDirection,
    /// 开始/停止捕获（停止状态显示“开始”，录制状态显示“停止”）
    ToggleCapture,
    /// 启动或停止自动滚到底连续拼接
    ToggleAutoScroll,
    /// 展开或收起裁剪子菜单
    ToggleCropSubMenu,
    /// 上方裁剪（纵向时为上裁剪，横向时为左裁剪）
    CropHead,
    /// 下方裁剪（纵向时为下裁剪，横向时为右裁剪）
    CropTail,
    /// 撤销最近一次手动头/尾裁剪
    UndoCrop,
    /// 切换反向滚动自动裁剪勾选框
    ToggleAutoCrop,
    /// 直接贴图置顶
    Pin,
    /// 保存为图片文件
    Save,
    /// 关闭长截图
    Close,
    /// 复制长图到剪贴板并退出
    Copy,
    /// 完成并进入全功能滚动编辑框
    EnterEditing,
}

impl LongCaptureAction {
    /// 完整中文描述
    pub fn label(self) -> &'static str {
        match self {
            Self::DragToolbar => "拖动工具栏",
            Self::ToggleDirection => "切换截图方向",
            Self::ToggleCapture => "开始/停止截图",
            Self::ToggleAutoScroll => "自动滚动/停止自动滚动",
            Self::ToggleCropSubMenu => "裁剪功能",
            Self::CropHead => "上裁剪/左裁剪",
            Self::CropTail => "下裁剪/右裁剪",
            Self::UndoCrop => "撤销裁剪",
            Self::ToggleAutoCrop => "自动裁剪",
            Self::Pin => "贴图",
            Self::Save => "保存",
            Self::Close => "关闭",
            Self::Copy => "复制",
            Self::EnterEditing => "完成并进入编辑",
        }
    }
}

/// 方向切换交互请求结果，防止误触直接清空已拼接的多屏长图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionToggleOutcome {
    /// 画布尚无增量或已完成二次确认，已切换方向并重建画布。
    Switched,
    /// 当前画布存在已拼接增量，已进入二次确认提示态并暂保留原图。
    NeedsConfirmation,
}

/// 长截图会话可变状态（显式不派生 `Clone`，避免渲染/事件热路径意外深拷贝画布与裁剪历史）。
#[derive(Debug)]
pub struct LongCaptureSession {
    /// 核心拼接引擎
    pub stitcher: LongCaptureStitcher,
    /// 是否正处于自动捕获拼接状态（true: 正在录制; false: 停止）
    pub is_capturing: bool,
    /// 是否已进入长图全功能滚动标注编辑模式
    pub is_editing: bool,
    /// 当前拼接主方向（垂直/水平）
    pub direction: StitchDirection,
    /// 是否开启反向滚动自动裁剪
    pub auto_crop: bool,
    /// 动态锁定的拼接正方向（+1 表示向下/向右，-1 表示向上/向左，None 表示尚未确立等待初次位移）
    pub locked_direction: Option<i32>,
    /// 上次拖动或滚轮时的单轴参考坐标
    pub last_drag_coord: Option<f32>,
    /// 裁剪子工具栏是否展开
    pub crop_sub_menu_open: bool,
    /// 已有拼接增量时点按方向切换后的二次确认挂起状态
    pub pending_direction_confirm: bool,
    /// 当前帧是否成功匹配拼接（用于绿色视口指示框反馈）
    pub is_matched: bool,
    /// 长图在编辑模式视口中的垂直滚动偏移量（像素）
    pub viewport_offset_y: f32,
    /// 长图在编辑模式视口中的水平滚动偏移量（像素）
    pub viewport_offset_x: f32,
    /// 上次捕获帧的时间戳（用于帧率节流）
    pub last_feed_time: Instant,
    /// 侧边缩略图悬浮窗屏幕矩形（用于命中测试与渲染）
    pub thumbnail_rect: Option<Rect>,
    /// 长截图专用工具栏屏幕矩形
    pub toolbar_rect: Option<Rect>,
    /// 裁剪展开子栏屏幕矩形
    pub crop_submenu_rect: Option<Rect>,
    /// 专用工具栏当前悬停的动作按钮
    pub hovered_action: Option<LongCaptureAction>,
    /// 用户拖动后的控制栏左上角位置；`None` 表示继续跟随选区自动布局
    pub toolbar_position: Option<(f32, f32)>,
    /// 正在拖动控制栏时记录的鼠标起点与工具栏起点
    toolbar_drag_origin: Option<((f32, f32), (f32, f32))>,
    /// 是否由异步后台工作线程承担侧边缩略图重采样，启用后允许重绘期优先展示最近一次已完成的旧缩略图
    pub(crate) async_thumbnail_enabled: bool,
    /// 最近一次渲染请求期望的后台缩略图代际号与规格键
    pub(crate) desired_thumbnail_spec: Option<(u64, ThumbnailRenderSpec)>,
    /// 最近一次在面板上展示使用的缩略图规格，供拼接提交后提前派发后台预生成
    pub(crate) last_thumbnail_spec: Option<ThumbnailRenderSpec>,
    /// 滚轮增量蓄水池与自动滚动驱动器
    scroll_driver: LongCaptureScrollDriver,
}

impl LongCaptureSession {
    /// 基于初始捕获帧创建长截图会话，并在进入长截图后立即开始录制。
    pub fn new(first_frame: image::RgbaImage) -> Self {
        let direction = StitchDirection::Vertical;
        let frame_main = first_frame.height();
        let stitcher = LongCaptureStitcher::new(first_frame, direction, StitchOptions::default());
        let mut scroll_driver = LongCaptureScrollDriver::default();
        scroll_driver.set_frame_main_span(frame_main);
        Self {
            stitcher,
            is_capturing: true,
            is_editing: false,
            direction,
            auto_crop: true,
            locked_direction: None,
            last_drag_coord: None,
            crop_sub_menu_open: false,
            pending_direction_confirm: false,
            is_matched: true,
            viewport_offset_y: 0.0,
            viewport_offset_x: 0.0,
            last_feed_time: Instant::now(),
            thumbnail_rect: None,
            toolbar_rect: None,
            crop_submenu_rect: None,
            hovered_action: None,
            toolbar_position: None,
            toolbar_drag_origin: None,
            async_thumbnail_enabled: false,
            desired_thumbnail_spec: None,
            last_thumbnail_spec: None,
            scroll_driver,
        }
    }

    /// 显式复制长截图会话快照；会克隆分块画布引用、缩略图缓存及裁剪撤销历史，仅限测试按需显式调用。
    #[cfg(test)]
    #[must_use]
    pub(crate) fn fork(&self) -> Self {
        Self {
            stitcher: self.stitcher.clone(),
            is_capturing: self.is_capturing,
            is_editing: self.is_editing,
            direction: self.direction,
            auto_crop: self.auto_crop,
            locked_direction: self.locked_direction,
            last_drag_coord: self.last_drag_coord,
            crop_sub_menu_open: self.crop_sub_menu_open,
            pending_direction_confirm: self.pending_direction_confirm,
            is_matched: self.is_matched,
            viewport_offset_y: self.viewport_offset_y,
            viewport_offset_x: self.viewport_offset_x,
            last_feed_time: self.last_feed_time,
            thumbnail_rect: self.thumbnail_rect,
            toolbar_rect: self.toolbar_rect,
            crop_submenu_rect: self.crop_submenu_rect,
            hovered_action: self.hovered_action,
            toolbar_position: self.toolbar_position,
            toolbar_drag_origin: self.toolbar_drag_origin,
            async_thumbnail_enabled: self.async_thumbnail_enabled,
            desired_thumbnail_spec: self.desired_thumbnail_spec,
            last_thumbnail_spec: self.last_thumbnail_spec,
            scroll_driver: self.scroll_driver.clone(),
        }
    }

    /// 是否正在拖动长截图控制栏。
    pub fn is_dragging_toolbar(&self) -> bool {
        self.toolbar_drag_origin.is_some()
    }

    /// 记录控制栏拖动起点。
    ///
    /// 返回 `false` 表示当前尚未完成首帧布局，无法获得控制栏矩形；调用方应继续等待下一次渲染。
    pub fn begin_toolbar_drag(&mut self, pointer_x: f32, pointer_y: f32) -> bool {
        let Some(rect) = self.toolbar_rect else {
            return false;
        };
        self.toolbar_position = Some((rect.x, rect.y));
        self.toolbar_drag_origin = Some(((pointer_x, pointer_y), (rect.x, rect.y)));
        true
    }

    /// 根据鼠标位移更新控制栏位置，并把控制栏完整限制在屏幕可视区域内。
    pub fn drag_toolbar_to(
        &mut self,
        pointer_x: f32,
        pointer_y: f32,
        screen_width: f32,
        screen_height: f32,
        ui_scale: f32,
    ) -> bool {
        let Some(((start_x, start_y), (origin_x, origin_y))) = self.toolbar_drag_origin else {
            return false;
        };
        let Some(rect) = self.toolbar_rect else {
            return false;
        };

        let margin = 8.0 * ui_scale;
        let max_x = (screen_width - rect.width - margin).max(margin);
        let max_y = (screen_height - rect.height - margin).max(margin);
        let next_x = (origin_x + pointer_x - start_x).clamp(margin, max_x);
        let next_y = (origin_y + pointer_y - start_y).clamp(margin, max_y);
        let next_position = (next_x, next_y);
        if self.toolbar_position == Some(next_position) {
            return false;
        }
        self.toolbar_position = Some(next_position);
        true
    }

    /// 结束控制栏拖动；返回本次释放是否消费了拖动状态。
    pub fn end_toolbar_drag(&mut self) -> bool {
        self.toolbar_drag_origin.take().is_some()
    }

    /// 重置锁定的拼接方向（满足 PixPin 官方规定的 4 大重置时机）
    pub fn reset_direction(&mut self) {
        self.locked_direction = None;
        self.last_drag_coord = None;
    }

    /// 判断长截图画布是否与初始选区首帧尺寸一致（即已反向裁剪回退到原点）
    pub fn is_at_initial_size(&self) -> bool {
        let (cw, ch) = self.stitcher.dimensions();
        let (iw, ih) = self.stitcher.initial_frame_size();
        match self.direction {
            StitchDirection::Vertical => ch <= ih,
            StitchDirection::Horizontal => cw <= iw,
        }
    }

    /// 切换开始/停止捕获（时机 1: 重新开始时更新拼接方向）
    pub fn toggle_capture(&mut self) {
        self.is_capturing = !self.is_capturing;
        self.reset_direction();
        self.scroll_driver.clear_all();
        self.pending_direction_confirm = false;
        if self.is_capturing {
            // 开始录制时关闭裁剪子菜单，聚焦于录制
            self.crop_sub_menu_open = false;
        }
    }

    /// 以当前选区画面重新开始一轮录制。
    ///
    /// 用户在停止状态可以调整选区大小或位置；旧拼接器的初始帧尺寸已经失效，
    /// 继续复用会将所有新帧判定为尺寸不匹配。这里保留方向、自动裁剪与工具栏位置，
    /// 但重建拼接画布，符合 PixPin“停止即取消本轮长截图”的交互语义。
    pub fn start_capture(&mut self, first_frame: image::RgbaImage) {
        let mut options = *self.stitcher.options();
        options.auto_crop_reverse = self.auto_crop;
        self.stitcher = LongCaptureStitcher::new(first_frame, self.direction, options);
        self.is_capturing = true;
        self.is_editing = false;
        self.reset_direction();
        self.scroll_driver.clear_all();
        self.sync_scroll_frame_span();
        self.crop_sub_menu_open = false;
        self.pending_direction_confirm = false;
        self.is_matched = true;
        self.viewport_offset_y = 0.0;
        self.viewport_offset_x = 0.0;
        self.last_feed_time = Instant::now();
        self.thumbnail_rect = None;
    }

    /// 直接切换拼接方向并从最近一帧重建画布。
    pub fn toggle_direction(&mut self) {
        self.direction = match self.direction {
            StitchDirection::Vertical => StitchDirection::Horizontal,
            StitchDirection::Horizontal => StitchDirection::Vertical,
        };
        // 两个方向的主增长轴不同，必须从最近一帧重建画布，不能沿用旧方向的长图。
        self.stitcher.restart_with_direction(self.direction);
        self.reset_direction();
        self.scroll_driver.clear_all();
        self.sync_scroll_frame_span();
        self.crop_sub_menu_open = false;
        self.pending_direction_confirm = false;
        self.viewport_offset_y = 0.0;
        self.viewport_offset_x = 0.0;
        self.thumbnail_rect = None;
    }

    fn sync_scroll_frame_span(&mut self) {
        let (iw, ih) = self.stitcher.initial_frame_size();
        let main = match self.direction {
            StitchDirection::Vertical => ih,
            StitchDirection::Horizontal => iw,
        };
        self.scroll_driver.set_frame_main_span(main);
    }

    /// 带防误清空保护的方向切换请求：若画布已积累拼接增量或裁剪状态，首次点击仅进入二次确认态。
    pub fn request_toggle_direction(&mut self) -> DirectionToggleOutcome {
        self.crop_sub_menu_open = false;
        let is_untouched_initial = self.stitcher.dimensions() == self.stitcher.initial_frame_size()
            && !self.stitcher.can_undo_crop();
        if is_untouched_initial || self.pending_direction_confirm {
            self.toggle_direction();
            return DirectionToggleOutcome::Switched;
        }
        self.pending_direction_confirm = true;
        DirectionToggleOutcome::NeedsConfirmation
    }

    /// 切换自动裁剪设置
    pub fn toggle_auto_crop(&mut self) {
        self.pending_direction_confirm = false;
        self.auto_crop = !self.auto_crop;
        let mut options = *self.stitcher.options();
        options.auto_crop_reverse = self.auto_crop;
        self.stitcher.set_options(options);
    }

    /// 切换展开/收起裁剪子菜单
    pub fn toggle_crop_sub_menu(&mut self) {
        self.pending_direction_confirm = false;
        self.crop_sub_menu_open = !self.crop_sub_menu_open;
    }

    /// 投喂最新捕获的屏幕帧并返回精细状态结果，精确区分推进、静止、失配、达限与忽略。
    #[cfg(test)]
    pub fn feed_frame_outcome(&mut self, frame: &image::RgbaImage) -> StitchFrameOutcome {
        if !self.is_capturing || self.is_editing {
            return StitchFrameOutcome::Ignored;
        }
        self.last_feed_time = Instant::now();
        let outcome = self.stitcher.feed_frame_outcome(frame);
        self.record_feed_outcome(outcome)
    }

    /// 提交后台工作线程已完成匹配的位移结果，避免在 GUI 主线程重复运算匹配算法。
    pub fn apply_analyzed_shift(
        &mut self,
        selection_frame: image::RgbaImage,
        expected_generation: u64,
        analysis: FrameShiftAnalysis,
    ) -> StitchFrameOutcome {
        if !self.is_capturing || self.is_editing {
            return StitchFrameOutcome::Ignored;
        }
        self.last_feed_time = Instant::now();
        let outcome =
            self.stitcher
                .apply_analyzed_shift(selection_frame, expected_generation, analysis);
        self.record_feed_outcome(outcome)
    }

    fn record_feed_outcome(&mut self, outcome: StitchFrameOutcome) -> StitchFrameOutcome {
        match outcome {
            StitchFrameOutcome::Stitched { .. } => {
                self.is_matched = true;
                self.pending_direction_confirm = false;
                self.locked_direction = self.stitcher.growth_polarity();
            }
            StitchFrameOutcome::Unchanged => {
                self.is_matched = true;
            }
            StitchFrameOutcome::Unmatched { .. } => {
                self.is_matched = false;
            }
            StitchFrameOutcome::LimitReached | StitchFrameOutcome::Ignored => {}
        }
        self.scroll_driver.record_outcome(&outcome, Instant::now());
        outcome
    }

    /// 是否正在执行自动连续滚动抓屏。
    #[must_use]
    pub fn is_auto_scrolling(&self) -> bool {
        self.scroll_driver.auto_scrolling
    }

    /// 将捕获态收到的高频手动滚轮增量并入待消费队列。
    pub(crate) fn enqueue_wheel_delta(&mut self, delta: WheelDelta) {
        self.scroll_driver.enqueue_manual_wheel(delta);
    }

    /// 启动自动连续滚动并立即触发首步抓帧。
    pub fn start_auto_scroll(&mut self, now: Instant) {
        self.pending_direction_confirm = false;
        self.crop_sub_menu_open = false;
        self.scroll_driver.start_auto_scroll(now);
    }

    /// 停止自动连续滚动。
    pub fn stop_auto_scroll(&mut self) {
        self.scroll_driver.stop_auto_scroll();
    }

    /// 取出下一批准备提交给底层滚动注入的增量。
    pub(crate) fn take_ready_dispatch_delta(&mut self, now: Instant) -> Option<WheelDelta> {
        self.scroll_driver
            .take_ready_dispatch_delta(self.direction, now)
    }

    /// 返回自动滚动模式下次必须唤醒检查的时刻。
    pub(crate) fn auto_scroll_deadline(&self) -> Option<Instant> {
        self.scroll_driver.auto_scroll_deadline()
    }

    /// 消费最近一次自动滚动被动停止原因，以便界面展示精准中文状态文案。
    pub(crate) fn take_last_auto_stop_reason(&mut self) -> Option<AutoScrollStopReason> {
        self.scroll_driver.take_stop_reason()
    }

    /// 上裁剪（裁剪长图顶部/左侧），返回实际裁去的像素量。
    ///
    /// 若已裁回初始单帧尺寸以内或裁剪了激活前插锚点导致剩余不足单帧，同步解锁方向或自动暂停录制。
    pub fn crop_head(&mut self, pixels: u32) -> u32 {
        self.pending_direction_confirm = false;
        let cropped = self.stitcher.crop_head(pixels);
        if self.is_at_initial_size() {
            self.stitcher.reset_growth_polarity();
            self.reset_direction();
        } else {
            self.locked_direction = self.stitcher.growth_polarity();
        }
        if self.stitcher.is_anchor_truncated_below_frame() && self.is_capturing {
            self.is_capturing = false;
            self.scroll_driver.clear_all();
        }
        cropped
    }

    /// 下裁剪（裁剪长图底部/右侧），返回实际裁去的像素量。
    ///
    /// 回退至初始单帧尺寸以内时同步重置主方向；若进一步裁入单帧内部（失去完整锚点帧），
    /// 则自动暂停录制以避免后续滚动产生断层拼接。
    pub fn crop_tail(&mut self, pixels: u32) -> u32 {
        self.pending_direction_confirm = false;
        let cropped = self.stitcher.crop_tail(pixels);
        if self.is_at_initial_size() {
            self.stitcher.reset_growth_polarity();
            self.reset_direction();
        } else {
            self.locked_direction = self.stitcher.growth_polarity();
        }
        if self.stitcher.is_anchor_truncated_below_frame() && self.is_capturing {
            self.is_capturing = false;
            self.scroll_driver.clear_all();
        }
        cropped
    }

    /// 撤销最近一次手动头/尾裁剪并无损恢复画布与尾端参考帧。
    pub fn undo_crop(&mut self) -> bool {
        self.pending_direction_confirm = false;
        let ok = self.stitcher.undo_crop();
        if ok {
            self.locked_direction = self.stitcher.growth_polarity();
        }
        ok
    }

    /// 当前是否存在可撤销的手动头/尾裁剪记录。
    pub fn can_undo_crop(&self) -> bool {
        self.stitcher.can_undo_crop()
    }

    /// 当前画布是否仍有继续手动头/尾裁剪的像素余量。
    pub fn can_crop(&self) -> bool {
        self.stitcher.can_crop()
    }

    /// 获取当前视窗在长图中的映射区间
    pub fn viewport_window(&self) -> ViewportWindow {
        self.stitcher.current_viewport_window()
    }

    /// 进入长图滚动编辑模式
    pub fn enter_editing(&mut self) {
        self.is_capturing = false;
        self.is_editing = true;
        self.crop_sub_menu_open = false;
        self.pending_direction_confirm = false;
        self.scroll_driver.clear_all();
    }

    /// 在长图编辑模式下沿当前拼接主轴（垂直/水平）根据滚轮增量调整视口偏移。
    pub fn scroll_viewport(&mut self, delta: f32, viewport_span: f32) {
        let (cw, ch) = self.stitcher.dimensions();
        match self.direction {
            StitchDirection::Vertical => {
                let canvas_height = ch as f32;
                let max_offset = (canvas_height - viewport_span).max(0.0);
                self.viewport_offset_y = (self.viewport_offset_y - delta).clamp(0.0, max_offset);
            }
            StitchDirection::Horizontal => {
                let canvas_width = cw as f32;
                let max_offset = (canvas_width - viewport_span).max(0.0);
                self.viewport_offset_x = (self.viewport_offset_x - delta).clamp(0.0, max_offset);
            }
        }
    }

    /// 计算长图画布相对于视口尺寸 `(view_width, view_height)` 的整型偏移 `(offset_x, offset_y)`。
    pub(crate) fn effective_viewport_offset(
        &self,
        view_width: u32,
        view_height: u32,
    ) -> (u32, u32) {
        let (cw, ch) = self.stitcher.dimensions();
        match self.direction {
            StitchDirection::Vertical => (
                0,
                (self.viewport_offset_y.round().max(0.0) as u32)
                    .min(ch.saturating_sub(view_height)),
            ),
            StitchDirection::Horizontal => (
                (self.viewport_offset_x.round().max(0.0) as u32).min(cw.saturating_sub(view_width)),
                0,
            ),
        }
    }
}

/// 长截图编辑态下的屏幕坐标与长画布坐标双向映射器。
///
/// 实现初衷：编辑态下长画布随 `viewport_offset_x/y` 在选区视窗内滚动，若标注直接记录屏幕坐标，
/// 滚动时标注会停在原地且导出无法对应长图位置。通过将标注统一存储于 `(0..lw, 0..lh)`
/// 长画布坐标空间并在渲染/交互边界完成往返映射，彻底消除双坐标系错位与累积舍入误差。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LongCanvasSpace {
    /// 屏幕选区对应的像素边界（左上角为长画布可视切片在屏幕上的绘制原点）
    pub viewport_screen: Rect,
    /// 当前可视区域实际占用的宽高（若长图被裁剪得小于选区，仅包含有效图像区域）
    pub visible_size: (f32, f32),
    /// 长画布视口左上角偏移 `(offset_x, offset_y)`
    pub canvas_offset: (f32, f32),
}

impl LongCanvasSpace {
    /// 根据长截图会话、锁定选区与屏幕尺寸构造映射器；仅编辑态且有效面积非零时返回 `Some`。
    pub(crate) fn from_session(
        session: &LongCaptureSession,
        selection_rect: &Rect,
        screen_size: (f32, f32),
    ) -> Option<Self> {
        if !session.is_editing {
            return None;
        }
        let (max_w, max_h) = if screen_size.0 > 0.0 && screen_size.1 > 0.0 {
            (screen_size.0.round() as u32, screen_size.1.round() as u32)
        } else {
            (u32::MAX, u32::MAX)
        };
        let (px, py, pw, ph) = selection_rect.to_pixel_bounds(max_w, max_h);
        if pw == 0 || ph == 0 {
            return None;
        }
        let (cw, ch) = session.stitcher.dimensions();
        let (ox, oy) = session.effective_viewport_offset(pw, ph);
        let copy_w = pw.min(cw.saturating_sub(ox));
        let copy_h = ph.min(ch.saturating_sub(oy));
        if copy_w == 0 || copy_h == 0 {
            return None;
        }
        Some(Self {
            viewport_screen: Rect {
                x: px as f32,
                y: py as f32,
                width: pw as f32,
                height: ph as f32,
            },
            visible_size: (copy_w as f32, copy_h as f32),
            canvas_offset: (ox as f32, oy as f32),
        })
    }

    /// 将屏幕坐标映射到长画布绝对坐标。
    pub(crate) fn screen_to_canvas(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            sx - self.viewport_screen.x + self.canvas_offset.0,
            sy - self.viewport_screen.y + self.canvas_offset.1,
        )
    }

    /// 将屏幕坐标先夹取到当前可视长图区域，再映射到长画布绝对坐标。
    pub(crate) fn clamp_screen_to_canvas(&self, sx: f32, sy: f32) -> Point {
        let max_sx = self.viewport_screen.x + self.visible_size.0;
        let max_sy = self.viewport_screen.y + self.visible_size.1;
        let clamped_x = sx.clamp(self.viewport_screen.x, max_sx);
        let clamped_y = sy.clamp(self.viewport_screen.y, max_sy);
        let (cx, cy) = self.screen_to_canvas(clamped_x, clamped_y);
        Point::new(cx, cy)
    }

    /// 判断屏幕坐标是否处于当前可视长图视口范围内。
    pub(crate) fn contains_visible_screen_point(&self, sx: f32, sy: f32) -> bool {
        sx >= self.viewport_screen.x
            && sx <= self.viewport_screen.x + self.visible_size.0
            && sy >= self.viewport_screen.y
            && sy <= self.viewport_screen.y + self.visible_size.1
    }

    /// 在视口大小局部离屏位图上回放画布坐标标注时所需的偏移量 `(-offset_x, -offset_y)`。
    pub(crate) fn viewport_draw_offset(&self) -> (f32, f32) {
        (-self.canvas_offset.0, -self.canvas_offset.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(width: u32, height: u32) -> image::RgbaImage {
        image::RgbaImage::from_pixel(width, height, image::Rgba([128, 128, 128, 255]))
    }

    #[test]
    fn test_long_capture_session_lifecycle() {
        let frame = create_test_frame(200, 300);
        let mut session = LongCaptureSession::new(frame);

        // 进入长截图后立即开始录制，无需再次点击开始
        assert!(session.is_capturing);
        assert!(!session.is_editing);
        assert_eq!(session.viewport_offset_y, 0.0);

        // 点击停止后回到待命状态
        session.toggle_capture();
        assert!(!session.is_capturing);

        // 进入全功能滚动编辑模式
        session.enter_editing();
        assert!(!session.is_capturing);
        assert!(session.is_editing);
    }

    #[test]
    fn test_toolbar_drag_updates_only_toolbar_position() {
        let frame = create_test_frame(200, 300);
        let mut session = LongCaptureSession::new(frame);
        session.toolbar_rect = Some(Rect {
            x: 100.0,
            y: 50.0,
            width: 420.0,
            height: 36.0,
        });

        assert!(session.begin_toolbar_drag(120.0, 70.0));
        assert!(session.drag_toolbar_to(150.0, 90.0, 800.0, 600.0, 1.0));
        assert_eq!(session.toolbar_position, Some((130.0, 70.0)));
        assert!(session.end_toolbar_drag());
        assert!(!session.is_dragging_toolbar());

        assert!(session.begin_toolbar_drag(130.0, 70.0));
        assert!(session.drag_toolbar_to(-100.0, -100.0, 800.0, 600.0, 1.0));
        assert_eq!(session.toolbar_position, Some((8.0, 8.0)));
        assert!(session.end_toolbar_drag());
    }

    #[test]
    fn test_restart_capture_rebuilds_canvas_for_resized_selection() {
        let frame = create_test_frame(200, 300);
        let mut session = LongCaptureSession::new(frame);
        session.toolbar_position = Some((120.0, 40.0));

        session.start_capture(create_test_frame(160, 90));

        assert!(session.is_capturing);
        assert_eq!(session.stitcher.canvas().dimensions(), (160, 90));
        assert_eq!(session.toolbar_position, Some((120.0, 40.0)));
    }

    #[test]
    fn test_long_capture_actions_labels() {
        assert_eq!(LongCaptureAction::DragToolbar.label(), "拖动工具栏");
        assert_eq!(LongCaptureAction::ToggleDirection.label(), "切换截图方向");
        assert_eq!(LongCaptureAction::ToggleCapture.label(), "开始/停止截图");
        assert_eq!(LongCaptureAction::ToggleCropSubMenu.label(), "裁剪功能");
        assert_eq!(LongCaptureAction::CropHead.label(), "上裁剪/左裁剪");
        assert_eq!(LongCaptureAction::CropTail.label(), "下裁剪/右裁剪");
        assert_eq!(LongCaptureAction::UndoCrop.label(), "撤销裁剪");
        assert_eq!(LongCaptureAction::ToggleAutoCrop.label(), "自动裁剪");
        assert_eq!(LongCaptureAction::Pin.label(), "贴图");
        assert_eq!(LongCaptureAction::Save.label(), "保存");
        assert_eq!(LongCaptureAction::Close.label(), "关闭");
        assert_eq!(LongCaptureAction::Copy.label(), "复制");
        assert_eq!(LongCaptureAction::EnterEditing.label(), "完成并进入编辑");
    }

    #[test]
    fn test_scroll_viewport_clamping() {
        let frame = create_test_frame(200, 500);
        let mut session = LongCaptureSession::new(frame);
        session.enter_editing();

        // 视口高 200，画布高 500，最大偏移为 300
        session.scroll_viewport(100.0, 200.0);
        assert_eq!(session.viewport_offset_y, 0.0); // 向上不能滚出界

        session.scroll_viewport(-150.0, 200.0);
        assert_eq!(session.viewport_offset_y, 150.0);

        session.scroll_viewport(-250.0, 200.0);
        assert_eq!(session.viewport_offset_y, 300.0); // 夹取在最大偏移 300
    }

    fn create_pattern_frame(width: u32, height: u32, offset_y: u32) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(width, height);
        for y in 0..height {
            let eff_y = y + offset_y;
            for x in 0..width {
                let r = ((eff_y * 13 + x * 7) % 256) as u8;
                let g = ((eff_y * 23 + x * 11) % 256) as u8;
                let b = ((eff_y * 37 + x * 17) % 256) as u8;
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
        img
    }

    fn create_horizontal_pattern_frame(width: u32, height: u32, offset_x: u32) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let effective_x = x + offset_x;
                let r = ((effective_x * 17 + y * 5) % 256) as u8;
                let g = ((effective_x * 29 + y * 13) % 256) as u8;
                let b = ((effective_x * 41 + y * 19) % 256) as u8;
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
        img
    }

    #[test]
    fn test_state_crops_async_screen_frame_to_selection_before_stitching() {
        use crate::snipper::selection::SelectionState;
        use crate::snipper::window::state::SnipperState;
        use image::GenericImageView;

        let mut state = SnipperState::new();
        let selection = Rect {
            x: 20.0,
            y: 30.0,
            width: 200.0,
            height: 200.0,
        };
        state.viewport.selection = SelectionState::Locked {
            rect: selection,
            active_drag: None,
        };
        let initial_full = create_pattern_frame(300, 400, 0);
        let initial_selection = initial_full.view(20, 30, 200, 200).to_image();
        let mut session = LongCaptureSession::new(initial_selection);
        session.start_capture(initial_full.view(20, 30, 200, 200).to_image());
        state.long_capture = Some(session);

        let scrolled_full = create_pattern_frame(300, 400, 40);
        assert!(state.apply_long_capture_frame(&scrolled_full).is_stitched());
        assert_eq!(
            state
                .long_capture
                .as_ref()
                .map(|session| session.stitcher.canvas().dimensions()),
            Some((200, 240))
        );
    }

    #[test]
    fn test_feed_frame_outcome_updates_match_indicator() {
        let frame1 = create_pattern_frame(160, 160, 0);
        let mut session = LongCaptureSession::new(frame1.clone());

        // 静止帧返回 Unchanged，但指示器保持绿色对齐（is_matched = true）
        assert_eq!(
            session.feed_frame_outcome(&frame1),
            StitchFrameOutcome::Unchanged
        );
        assert!(session.is_matched);

        // 失配帧将指示器切换为黄色预警，不改变画布与基准帧
        let unmatched = image::RgbaImage::from_fn(160, 160, |x, y| {
            let v = if ((x / 7) + (y / 7)) % 2 == 0 { 0 } else { 255 };
            image::Rgba([v, v, v, 255])
        });
        assert!(matches!(
            session.feed_frame_outcome(&unmatched),
            StitchFrameOutcome::Unmatched { .. }
        ));
        assert!(!session.is_matched);
        assert_eq!(session.stitcher.canvas().dimensions(), (160, 160));
        assert_eq!(session.stitcher.last_frame(), &frame1);

        // 随后传入与 frame1 连续的滚动帧仍可恢复拼接与绿色状态
        let frame2 = create_pattern_frame(160, 160, 30);
        assert!(session.feed_frame_outcome(&frame2).is_stitched());
        assert!(session.is_matched);
        assert_eq!(session.stitcher.canvas().dimensions(), (160, 190));
    }

    #[test]
    fn test_crop_head_and_tail() {
        let frame1 = create_pattern_frame(200, 200, 0);
        let mut session = LongCaptureSession::new(frame1);

        // 投喂位移 40 像素的第 2 帧，长图画布扩展至 240
        let frame2 = create_pattern_frame(200, 200, 40);
        let matched = session.feed_frame_outcome(&frame2).is_stitched();
        assert!(matched, "应成功完成位移匹配");
        assert_eq!(session.stitcher.canvas().height(), 240);

        // 首帧高度为 200，超过部分为 40。下裁剪 15 像素
        session.crop_tail(15);
        assert_eq!(session.stitcher.canvas().height(), 225);

        // 上裁剪 10 像素
        session.crop_head(10);
        assert_eq!(session.stitcher.canvas().height(), 215);
    }

    #[test]
    fn test_auto_crop_direction_and_reset_invariants() {
        let frame1 = create_pattern_frame(200, 200, 0);
        let mut session = LongCaptureSession::new(frame1);

        // 初始状态下尚未确立主拼接方向
        assert!(session.locked_direction.is_none());
        assert!(session.is_at_initial_size());

        // 模拟正向位移 50px：画布延伸
        let frame2 = create_pattern_frame(200, 200, 50);
        assert!(session.feed_frame_outcome(&frame2).is_stitched());
        assert_eq!(session.stitcher.canvas().height(), 250);
        assert!(!session.is_at_initial_size());

        // 确立正向（向下 +1）
        session.locked_direction = Some(1);

        // 检测到反向位移 30px，触发自动裁剪
        session.crop_tail(30);
        assert_eq!(session.stitcher.canvas().height(), 220);

        // 继续反向裁剪 20px，完全退回原点
        session.crop_tail(20);
        assert_eq!(session.stitcher.canvas().height(), 200);
        assert!(session.is_at_initial_size());

        // 时机 4: 回退到初始尺寸后重置主方向
        session.reset_direction();
        assert!(session.locked_direction.is_none());

        // 时机 1 与 2: 重新开始截图或切换方向也重置主方向
        session.locked_direction = Some(-1);
        session.toggle_direction();
        assert!(session.locked_direction.is_none());

        session.locked_direction = Some(1);
        session.toggle_capture();
        assert!(session.locked_direction.is_none());
    }

    #[test]
    fn test_toggle_direction_rebuilds_canvas_and_scrolls_horizontally() {
        let first = create_horizontal_pattern_frame(240, 120, 0);
        let mut session = LongCaptureSession::new(first);
        session.toggle_direction();

        assert_eq!(session.direction, StitchDirection::Horizontal);
        assert_eq!(session.stitcher.direction(), StitchDirection::Horizontal);
        assert_eq!(session.stitcher.canvas().dimensions(), (240, 120));

        let second = create_horizontal_pattern_frame(240, 120, 50);
        assert!(session.feed_frame_outcome(&second).is_stitched());
        assert_eq!(session.stitcher.canvas().dimensions(), (290, 120));

        session.enter_editing();
        session.scroll_viewport(-60.0, 240.0);
        assert_eq!(session.viewport_offset_x, 50.0);
        assert_eq!(session.viewport_offset_y, 0.0);
    }

    #[test]
    fn test_auto_crop_setting_propagates_to_stitcher_options() {
        let mut session = LongCaptureSession::new(create_test_frame(120, 160));

        session.toggle_auto_crop();
        assert!(!session.auto_crop);
        assert!(!session.stitcher.options().auto_crop_reverse);

        session.start_capture(create_test_frame(100, 140));
        assert!(!session.stitcher.options().auto_crop_reverse);
    }

    #[test]
    fn test_long_canvas_space_bidirectional_mapping_and_cropped_margin() {
        let mut session = LongCaptureSession::new(create_test_frame(200, 500));
        session.enter_editing();
        session.viewport_offset_y = 180.0;
        let selection = Rect::new(100.0, 50.0, 200.0, 300.0);

        let v_space = LongCanvasSpace::from_session(&session, &selection, (1280.0, 800.0))
            .expect("编辑态应构建垂直空间映射");
        assert_eq!(v_space.screen_to_canvas(125.0, 90.0), (25.0, 220.0));
        assert_eq!(v_space.viewport_draw_offset(), (0.0, -180.0));
        assert!(v_space.contains_visible_screen_point(299.0, 349.0));
        assert_eq!(
            v_space.clamp_screen_to_canvas(50.0, 500.0),
            Point::new(0.0, 480.0)
        );

        // 当长图经过裁剪矮于选区视口高度时，超出的选区底部空白处必须被判定为不可交互。
        let mut cropped = LongCaptureSession::new(create_test_frame(200, 120));
        cropped.enter_editing();
        let cropped_space = LongCanvasSpace::from_session(&cropped, &selection, (1280.0, 800.0))
            .expect("裁剪后短图应保留有效可视区域");
        assert!(cropped_space.contains_visible_screen_point(150.0, 160.0));
        assert!(!cropped_space.contains_visible_screen_point(150.0, 250.0));

        // 水平方向偏移映射同样保持精确对称。
        let mut horiz = LongCaptureSession::new(create_test_frame(600, 150));
        horiz.toggle_direction();
        horiz.enter_editing();
        horiz.viewport_offset_x = 140.0;
        let h_selection = Rect::new(80.0, 40.0, 250.0, 150.0);
        let h_space = LongCanvasSpace::from_session(&horiz, &h_selection, (1280.0, 800.0))
            .expect("横向编辑态应构建水平空间映射");
        assert_eq!(h_space.screen_to_canvas(110.0, 75.0), (170.0, 35.0));
        assert_eq!(h_space.viewport_draw_offset(), (-140.0, 0.0));
    }
}
