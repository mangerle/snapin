//! 截图会话生命周期、输出与宿主端口状态。

use crate::ports::{DesktopPort, NoopScreenRecorder, RecordingEventSink, ScreenRecorder};
use crate::snipper::window::types::{SnipMode, SnipperOutcome};
use snapin_config::config::Config;
use snapin_foundation::PostCaptureAction;
use std::sync::Arc;

/// 截图会话的配置、宿主能力与最终输出状态。
pub(crate) struct SnipperSession {
    /// 截图最终产出决策。
    pub(crate) outcome: Option<SnipperOutcome>,
    /// 画布顶部提示。
    pub(crate) status_message: Option<String>,
    /// 桌面副作用端口。
    pub(crate) desktop: Arc<dyn DesktopPort>,
    /// 屏幕录制与媒体编码端口。
    pub(crate) recorder: Arc<dyn ScreenRecorder>,
    /// 录制状态事件投递器。
    pub(crate) recording_events: RecordingEventSink,
    /// 本次会话配置快照。
    pub(crate) config: Config,
    /// 本次截图触发模式。
    pub(crate) mode: SnipMode,
    /// 首次锁定选区后的自动动作。
    pub(crate) post_action: Option<PostCaptureAction>,
}

impl SnipperSession {
    /// 使用宿主端口和初始配置构建会话状态。
    pub(crate) fn new(desktop: Arc<dyn DesktopPort>, mode: SnipMode) -> Self {
        Self {
            outcome: None,
            status_message: None,
            desktop,
            recorder: Arc::new(NoopScreenRecorder),
            recording_events: Arc::new(|_| {}),
            config: Config::default(),
            mode,
            post_action: None,
        }
    }
}
