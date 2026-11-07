//! 截图选区内的屏幕录制会话状态。

use crate::editor::toolbar::Tool;
use crate::ports::{
    PortError, RecordingEvent, RecordingEventSink, RecordingFormat, RecordingHandle,
    RecordingOptions, RecordingRegion, ScreenRecorder,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// 录制工具栏中的一次性动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingAction {
    /// 开始、停止或进入编码收尾
    ToggleCapture,
    /// 暂停或恢复
    TogglePause,
    /// 切换系统声音请求
    ToggleAudio,
    /// 切换鼠标指针采集
    ToggleCursor,
    /// 打开或关闭格式菜单
    ToggleSettings,
    /// 切换帧率
    CycleFps,
    /// 撤销标注
    Undo,
    /// 拖动整条录制工具栏
    DragHandle,
    /// 移动工具栏或选区
    Move,
    /// 关闭并取消
    Close,
    /// 放弃当前录制并立即重新开始
    Restart,
}

impl RecordingAction {
    /// 工具栏悬浮提示名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::ToggleCapture => "开始或停止录制",
            Self::TogglePause => "暂停或继续",
            Self::ToggleAudio => "系统声音",
            Self::ToggleCursor => "鼠标指针",
            Self::ToggleSettings => "录制设置",
            Self::CycleFps => "切换帧率",
            Self::Undo => "撤销标注",
            Self::DragHandle => "拖动工具栏",
            Self::Move => "移动",
            Self::Close => "关闭",
            Self::Restart => "重新开始",
        }
    }
}

/// 录制工具栏按钮：录制动作与现有标注工具共用同一布局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingItem {
    /// 录制控制动作
    Action(RecordingAction),
    /// 复用主工具栏的标注工具
    Tool(Tool),
}

/// 录制会话所处的明确阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingStatus {
    /// 尚未按下录制
    Idle,
    /// 已提交后台任务，等待原生采集器就绪
    Starting,
    /// 正在采集
    Active,
    /// 已暂停采集
    Paused,
    /// 已停止采集，正在编码收尾
    Stopping,
}

impl RecordingStatus {
    /// 是否处于已经开始且尚未进入编码收尾的状态。
    pub const fn is_recording(self) -> bool {
        matches!(self, Self::Active | Self::Paused)
    }

    /// 是否仍会接收录制控制。
    pub const fn is_busy(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// 录制事件对 UI 的后续影响。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingTransition {
    /// 仅刷新界面
    Updated,
    /// 录制成功完成
    Finished(PathBuf),
    /// 录制取消完成
    Cancelled,
    /// 取消完成后应立即重新开始
    Restart,
}

/// 一次选区录制的完整 UI 状态。
pub struct RecordingSession {
    recorder: Arc<dyn ScreenRecorder>,
    sink: RecordingEventSink,
    handle: Option<Box<dyn RecordingHandle>>,
    status: RecordingStatus,
    format: RecordingFormat,
    fps: u32,
    include_system_audio: bool,
    include_cursor: bool,
    elapsed: Duration,
    region: Option<RecordingRegion>,
    settings_open: bool,
    hovered: Option<RecordingItem>,
    restart_pending: bool,
    toolbar_offset: (f32, f32),
    toolbar_drag: Option<ToolbarDrag>,
    mouse_press_consumed: bool,
}

/// 一次工具栏拖动会话的固定起点。
#[derive(Debug, Clone, Copy)]
struct ToolbarDrag {
    pointer: (f32, f32),
    offset: (f32, f32),
}

impl RecordingSession {
    /// 使用宿主录屏端口与事件投递器创建尚未开始的会话。
    pub fn new(recorder: Arc<dyn ScreenRecorder>, sink: RecordingEventSink) -> Self {
        Self {
            recorder,
            sink,
            handle: None,
            status: RecordingStatus::Idle,
            format: RecordingFormat::Mp4,
            fps: 60,
            include_system_audio: false,
            include_cursor: false,
            elapsed: Duration::ZERO,
            region: None,
            settings_open: false,
            hovered: None,
            restart_pending: false,
            toolbar_offset: (0.0, 0.0),
            toolbar_drag: None,
            mouse_press_consumed: false,
        }
    }

    /// 当前会话阶段。
    pub const fn status(&self) -> RecordingStatus {
        self.status
    }

    /// 当前输出格式。
    pub const fn format(&self) -> RecordingFormat {
        self.format
    }

    /// 当前目标帧率。
    pub const fn fps(&self) -> u32 {
        self.fps
    }

    /// 当前已录制时长。
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// 系统声音开关是否开启。
    pub const fn include_system_audio(&self) -> bool {
        self.include_system_audio
    }

    /// 鼠标指针开关是否开启。
    pub const fn include_cursor(&self) -> bool {
        self.include_cursor
    }

    /// 设置菜单是否展开。
    pub const fn settings_open(&self) -> bool {
        self.settings_open
    }

    /// 当前悬停的工具栏按钮。
    pub const fn hovered(&self) -> Option<RecordingItem> {
        self.hovered
    }

    /// 工具栏相对默认位置的拖动偏移。
    pub const fn toolbar_offset(&self) -> (f32, f32) {
        self.toolbar_offset
    }

    /// 是否正在拖动整条工具栏。
    pub const fn is_dragging_toolbar(&self) -> bool {
        self.toolbar_drag.is_some()
    }

    /// 开始拖动整条工具栏。
    pub fn begin_toolbar_drag(&mut self, pointer: (f32, f32)) -> bool {
        if self.toolbar_drag.is_some() {
            return false;
        }
        self.toolbar_drag = Some(ToolbarDrag {
            pointer,
            offset: self.toolbar_offset,
        });
        true
    }

    /// 根据当前鼠标位置更新工具栏偏移。
    pub fn drag_toolbar_to(&mut self, pointer: (f32, f32)) -> bool {
        let Some(drag) = self.toolbar_drag else {
            return false;
        };
        let next = (
            drag.offset.0 + pointer.0 - drag.pointer.0,
            drag.offset.1 + pointer.1 - drag.pointer.1,
        );
        if next == self.toolbar_offset {
            return false;
        }
        self.toolbar_offset = next;
        true
    }

    /// 结束工具栏拖动。
    pub fn end_toolbar_drag(&mut self) -> bool {
        self.toolbar_drag.take().is_some()
    }

    /// 标记当前鼠标按下事件已由录制工具栏消费。
    pub fn consume_mouse_press(&mut self) {
        self.mouse_press_consumed = true;
    }

    /// 读取并清除鼠标按下事件的消费标记。
    pub fn take_mouse_press_consumed(&mut self) -> bool {
        std::mem::take(&mut self.mouse_press_consumed)
    }

    /// 更新悬停状态，返回是否需要重绘。
    pub fn set_hovered(&mut self, item: Option<RecordingItem>) -> bool {
        if self.hovered == item {
            return false;
        }
        self.hovered = item;
        true
    }

    /// 启动录制。
    ///
    /// # Errors
    ///
    /// 录制端口拒绝启动时返回可直接展示的端口错误。
    pub fn start(&mut self, region: RecordingRegion) -> Result<(), PortError> {
        if self.status != RecordingStatus::Idle {
            return Err(PortError::new("启动屏幕录制", "录制会话已经开始"));
        }
        let options = RecordingOptions {
            region,
            fps: self.fps,
            format: self.format,
            include_cursor: self.include_cursor,
            include_system_audio: self.include_system_audio,
            highlight_cursor: self.include_cursor,
        };
        self.handle = Some(self.recorder.start(options, self.sink.clone())?);
        self.region = Some(region);
        self.status = RecordingStatus::Starting;
        self.elapsed = Duration::ZERO;
        Ok(())
    }

    /// 开始或停止录制。
    ///
    /// # Errors
    ///
    /// 启动或停止控制失败时返回端口错误。
    pub fn toggle_capture(&mut self, region: RecordingRegion) -> Result<(), PortError> {
        match self.status {
            RecordingStatus::Idle => self.start(region),
            RecordingStatus::Active | RecordingStatus::Paused => {
                let handle = self
                    .handle
                    .as_ref()
                    .ok_or_else(|| PortError::new("停止屏幕录制", "录制控制器不存在"))?;
                handle.stop()?;
                self.status = RecordingStatus::Stopping;
                Ok(())
            }
            RecordingStatus::Starting | RecordingStatus::Stopping => Ok(()),
        }
    }

    /// 暂停或恢复录制。
    ///
    /// # Errors
    ///
    /// 控制端口不可用时返回端口错误。
    pub fn toggle_pause(&mut self) -> Result<(), PortError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| PortError::new("控制屏幕录制", "录制控制器不存在"))?;
        match self.status {
            RecordingStatus::Active => {
                handle.pause()?;
                self.status = RecordingStatus::Paused;
            }
            RecordingStatus::Paused => {
                handle.resume()?;
                self.status = RecordingStatus::Active;
            }
            RecordingStatus::Idle | RecordingStatus::Starting | RecordingStatus::Stopping => {}
        }
        Ok(())
    }

    /// 切换系统声音请求。
    pub fn toggle_audio(&mut self) {
        if self.status == RecordingStatus::Idle {
            self.include_system_audio = !self.include_system_audio;
        }
    }

    /// 切换鼠标指针请求。
    pub fn toggle_cursor(&mut self) {
        if self.status == RecordingStatus::Idle {
            self.include_cursor = !self.include_cursor;
        }
    }

    /// 在常用帧率之间循环。
    pub fn cycle_fps(&mut self) {
        if self.status != RecordingStatus::Idle {
            return;
        }
        self.fps = match self.fps {
            24 => 30,
            30 => 60,
            _ => 24,
        };
    }

    /// 设置输出格式。
    pub fn set_format(&mut self, format: RecordingFormat) {
        if self.status == RecordingStatus::Idle {
            self.format = format;
        }
        self.settings_open = false;
    }

    /// 切换设置菜单。
    pub fn toggle_settings(&mut self) {
        if self.status == RecordingStatus::Idle {
            self.settings_open = !self.settings_open;
        }
    }

    /// 请求关闭前取消仍在进行的录制。
    pub fn cancel(&mut self) {
        if let Some(handle) = &self.handle {
            handle.cancel();
        }
        self.handle = None;
    }

    /// 取消当前录制并标记完成后立即重启。
    pub fn request_restart(&mut self) {
        if self.status == RecordingStatus::Idle {
            return;
        }
        self.restart_pending = true;
        self.cancel();
    }

    /// 把最新透明标注层交给后台采集器。
    pub fn update_overlay(&self, overlay: Option<image::RgbaImage>) {
        if let Some(handle) = &self.handle {
            handle.update_overlay(overlay);
        }
    }

    /// 当前录制选区。
    pub fn region(&self) -> Option<RecordingRegion> {
        self.region
    }

    /// 消费后台录制事件并更新状态。
    pub fn handle_event(&mut self, event: RecordingEvent) -> RecordingTransition {
        match event {
            RecordingEvent::Started => {
                self.status = RecordingStatus::Active;
                RecordingTransition::Updated
            }
            RecordingEvent::Progress { elapsed, .. } => {
                self.elapsed = elapsed;
                RecordingTransition::Updated
            }
            RecordingEvent::Paused => {
                self.status = RecordingStatus::Paused;
                RecordingTransition::Updated
            }
            RecordingEvent::Resumed => {
                self.status = RecordingStatus::Active;
                RecordingTransition::Updated
            }
            RecordingEvent::Finished { path } => {
                self.status = RecordingStatus::Stopping;
                self.handle = None;
                RecordingTransition::Finished(path)
            }
            RecordingEvent::Cancelled => {
                self.status = RecordingStatus::Idle;
                self.handle = None;
                if self.restart_pending {
                    self.restart_pending = false;
                    RecordingTransition::Restart
                } else {
                    RecordingTransition::Cancelled
                }
            }
            RecordingEvent::Failed { .. } => {
                self.status = RecordingStatus::Idle;
                self.handle = None;
                RecordingTransition::Updated
            }
            RecordingEvent::Warning { .. } => RecordingTransition::Updated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapin_ports::{RecordingRegion, ScreenRecorder};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct MockControl {
        paused: AtomicBool,
        stopped: AtomicBool,
        cancelled: AtomicBool,
    }

    struct MockHandle(Arc<MockControl>);

    impl RecordingHandle for MockHandle {
        fn pause(&self) -> Result<(), PortError> {
            self.0.paused.store(true, Ordering::Release);
            Ok(())
        }

        fn resume(&self) -> Result<(), PortError> {
            self.0.paused.store(false, Ordering::Release);
            Ok(())
        }

        fn stop(&self) -> Result<(), PortError> {
            self.0.stopped.store(true, Ordering::Release);
            Ok(())
        }

        fn update_overlay(&self, _overlay: Option<image::RgbaImage>) {}

        fn cancel(&self) {
            self.0.cancelled.store(true, Ordering::Release);
        }
    }

    struct MockRecorder {
        control: Arc<MockControl>,
    }

    impl ScreenRecorder for MockRecorder {
        fn start(
            &self,
            _options: RecordingOptions,
            _sink: RecordingEventSink,
        ) -> Result<Box<dyn RecordingHandle>, PortError> {
            Ok(Box::new(MockHandle(self.control.clone())))
        }
    }

    fn region() -> RecordingRegion {
        RecordingRegion {
            x: 0,
            y: 0,
            width: 320,
            height: 200,
        }
    }

    #[test]
    fn test_recording_session_cycles_common_frame_rates() {
        let mut session = RecordingSession::new(
            Arc::new(MockRecorder {
                control: Arc::new(MockControl::default()),
            }),
            Arc::new(|_| {}),
        );
        assert_eq!(session.fps(), 60);
        session.cycle_fps();
        assert_eq!(session.fps(), 24);
        session.cycle_fps();
        assert_eq!(session.fps(), 30);
        session.cycle_fps();
        assert_eq!(session.fps(), 60);
    }

    #[test]
    fn test_recording_session_controls_and_finishes() {
        let control = Arc::new(MockControl::default());
        let recorder = Arc::new(MockRecorder {
            control: control.clone(),
        });
        let mut session = RecordingSession::new(recorder, Arc::new(|_| {}));
        session.start(region()).expect("录制应可启动");
        assert_eq!(session.status(), RecordingStatus::Starting);
        session.handle_event(RecordingEvent::Started);
        assert_eq!(session.status(), RecordingStatus::Active);

        session.toggle_pause().expect("暂停应成功");
        assert!(control.paused.load(Ordering::Acquire));
        assert_eq!(session.status(), RecordingStatus::Paused);
        session.toggle_pause().expect("恢复应成功");
        assert_eq!(session.status(), RecordingStatus::Active);

        session.toggle_capture(region()).expect("停止应成功");
        assert!(control.stopped.load(Ordering::Acquire));
        assert_eq!(session.status(), RecordingStatus::Stopping);
        let path = PathBuf::from("/tmp/snapin-recording.mp4");
        assert_eq!(
            session.handle_event(RecordingEvent::Finished { path: path.clone() }),
            RecordingTransition::Finished(path)
        );
    }

    #[test]
    fn test_mouse_press_consumption_is_cleared_on_release() {
        let mut session = RecordingSession::new(
            Arc::new(MockRecorder {
                control: Arc::new(MockControl::default()),
            }),
            Arc::new(|_| {}),
        );
        session.consume_mouse_press();
        assert!(session.take_mouse_press_consumed());
        assert!(!session.take_mouse_press_consumed());
    }

    #[test]
    fn test_toolbar_drag_tracks_pointer_delta_and_ends() {
        let mut session = RecordingSession::new(
            Arc::new(MockRecorder {
                control: Arc::new(MockControl::default()),
            }),
            Arc::new(|_| {}),
        );
        assert!(session.begin_toolbar_drag((100.0, 200.0)));
        assert!(session.drag_toolbar_to((130.0, 215.0)));
        assert_eq!(session.toolbar_offset(), (30.0, 15.0));
        assert!(session.end_toolbar_drag());
        assert!(!session.end_toolbar_drag());
    }

    #[test]
    fn test_restart_waits_for_cancel_then_reports_restart() {
        let control = Arc::new(MockControl::default());
        let recorder = Arc::new(MockRecorder {
            control: control.clone(),
        });
        let sink_events = Arc::new(Mutex::new(Vec::new()));
        let sink_events_for_callback = sink_events.clone();
        let sink: RecordingEventSink = Arc::new(move |event| {
            sink_events_for_callback
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(event);
        });
        let mut session = RecordingSession::new(recorder, sink);
        session.start(region()).expect("录制应可启动");
        session.handle_event(RecordingEvent::Started);
        session.request_restart();
        assert!(control.cancelled.load(Ordering::Acquire));
        assert_eq!(
            session.handle_event(RecordingEvent::Cancelled),
            RecordingTransition::Restart
        );
        assert_eq!(session.region(), Some(region()));
    }
}
