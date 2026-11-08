//! 长截图交互会话状态机模型。
//!
//! 管理长截图的捕获状态、拼接引擎、视口偏移、缩略图热区与专用控制条。

use crate::geometry::Rect;
use snapin_render::stitching::{
    LongCaptureStitcher, StitchDirection, StitchOptions, ViewportWindow,
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
    /// 展开或收起裁剪子菜单
    ToggleCropSubMenu,
    /// 上方裁剪（纵向时为上裁剪，横向时为左裁剪）
    CropHead,
    /// 下方裁剪（纵向时为下裁剪，横向时为右裁剪）
    CropTail,
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
            Self::ToggleCropSubMenu => "裁剪功能",
            Self::CropHead => "上裁剪/左裁剪",
            Self::CropTail => "下裁剪/右裁剪",
            Self::ToggleAutoCrop => "自动裁剪",
            Self::Pin => "贴图",
            Self::Save => "保存",
            Self::Close => "关闭",
            Self::Copy => "复制",
            Self::EnterEditing => "完成并进入编辑",
        }
    }
}

/// 长截图会话可变状态。
#[derive(Debug, Clone)]
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
}

impl LongCaptureSession {
    /// 基于初始捕获帧创建长截图会话，并在进入长截图后立即开始录制。
    pub fn new(first_frame: image::RgbaImage) -> Self {
        let direction = StitchDirection::Vertical;
        let stitcher = LongCaptureStitcher::new(first_frame, direction, StitchOptions::default());
        Self {
            stitcher,
            is_capturing: true,
            is_editing: false,
            direction,
            auto_crop: true,
            locked_direction: None,
            last_drag_coord: None,
            crop_sub_menu_open: false,
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
        let (cw, ch) = self.stitcher.canvas().dimensions();
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
        self.crop_sub_menu_open = false;
        self.is_matched = true;
        self.viewport_offset_y = 0.0;
        self.viewport_offset_x = 0.0;
        self.last_feed_time = Instant::now();
        self.thumbnail_rect = None;
    }

    /// 切换拼接方向（时机 2: 切换截图方向后再切换回来重新确定）
    pub fn toggle_direction(&mut self) {
        self.direction = match self.direction {
            StitchDirection::Vertical => StitchDirection::Horizontal,
            StitchDirection::Horizontal => StitchDirection::Vertical,
        };
        // 两个方向的主增长轴不同，必须从最近一帧重建画布，不能沿用旧方向的长图。
        self.stitcher.restart_with_direction(self.direction);
        self.reset_direction();
        self.viewport_offset_y = 0.0;
        self.viewport_offset_x = 0.0;
        self.thumbnail_rect = None;
    }

    /// 切换自动裁剪设置
    pub fn toggle_auto_crop(&mut self) {
        self.auto_crop = !self.auto_crop;
        let mut options = *self.stitcher.options();
        options.auto_crop_reverse = self.auto_crop;
        self.stitcher.set_options(options);
    }

    /// 切换展开/收起裁剪子菜单
    pub fn toggle_crop_sub_menu(&mut self) {
        self.crop_sub_menu_open = !self.crop_sub_menu_open;
    }

    /// 投喂最新捕获的屏幕帧
    pub fn feed_frame(&mut self, frame: &image::RgbaImage) -> bool {
        if !self.is_capturing || self.is_editing {
            return false;
        }
        self.last_feed_time = Instant::now();
        let res = self.stitcher.feed_frame(frame);
        let matched = res.is_matched(self.stitcher.options().confidence_threshold);
        self.is_matched = matched;
        matched
    }

    /// 上次投喂帧以来的耗时
    pub fn last_feed_elapsed(&self) -> std::time::Duration {
        self.last_feed_time.elapsed()
    }

    /// 上裁剪（裁剪长图顶部/左侧）
    pub fn crop_head(&mut self, pixels: u32) {
        self.stitcher.crop_head(pixels);
    }

    /// 下裁剪（裁剪长图底部/右侧）
    pub fn crop_tail(&mut self, pixels: u32) {
        self.stitcher.crop_tail(pixels);
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
    }

    /// 在长图编辑模式下根据滚轮滚动调整视口偏移
    pub fn scroll_viewport(&mut self, delta_y: f32, viewport_span: f32) {
        match self.direction {
            StitchDirection::Vertical => {
                let canvas_height = self.stitcher.canvas().height() as f32;
                let max_offset = (canvas_height - viewport_span).max(0.0);
                self.viewport_offset_y = (self.viewport_offset_y - delta_y).clamp(0.0, max_offset);
            }
            StitchDirection::Horizontal => {
                let canvas_width = self.stitcher.canvas().width() as f32;
                let max_offset = (canvas_width - viewport_span).max(0.0);
                self.viewport_offset_x = (self.viewport_offset_x - delta_y).clamp(0.0, max_offset);
            }
        }
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
        assert!(state.apply_long_capture_frame(&scrolled_full));
        assert_eq!(
            state
                .long_capture
                .as_ref()
                .map(|session| session.stitcher.canvas().dimensions()),
            Some((200, 240))
        );
    }

    #[test]
    fn test_crop_head_and_tail() {
        let frame1 = create_pattern_frame(200, 200, 0);
        let mut session = LongCaptureSession::new(frame1);

        // 投喂位移 40 像素的第 2 帧，长图画布扩展至 240
        let frame2 = create_pattern_frame(200, 200, 40);
        let matched = session.feed_frame(&frame2);
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
        session.feed_frame(&frame2);
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
        assert!(session.feed_frame(&second));
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
}
