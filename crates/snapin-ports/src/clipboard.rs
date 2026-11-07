//! 剪贴板任务契约与结果类型。

use super::error::PortError;
use crate::id::PasterId;
use image::RgbaImage;
use std::sync::Arc;

/// 剪贴板后台任务标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClipboardJobId(u64);

impl ClipboardJobId {
    /// 由宿主分配单调递增的任务标识。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 读取原始任务序号。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 剪贴板任务对应的 UI 所有者。
///
/// 截图覆盖层同一时刻最多一个，使用无载荷变体即可精确定位；贴图窗口使用稳定
/// [`PasterId`] 路由，避免窗口重建或窗口系统 ID 变化导致结果投递到错误窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipboardTarget {
    /// 当前截图覆盖层。
    Snipper,
    /// 指定贴图窗口。
    Paster(PasterId),
}

/// 待写入系统剪贴板的内容。
#[derive(Debug)]
pub enum ClipboardWrite {
    /// 纯文本内容。
    Text(String),
    /// RGBA 图像内容。
    Image(RgbaImage),
}

/// 后台读取到的当前剪贴板载荷。
#[derive(Debug, Clone)]
pub enum ClipboardPayload {
    /// 当前剪贴板图像。
    Image(Arc<RgbaImage>),
    /// 当前剪贴板文本。
    Text(String),
    /// 当前剪贴板为空或不包含可处理内容。
    Empty,
}

/// 剪贴板后台任务结果。
#[derive(Debug, Clone)]
pub enum ClipboardResult {
    /// 写入操作已由系统剪贴板确认成功。
    Written,
    /// 读取操作返回的当前载荷快照。
    Payload(ClipboardPayload),
}

/// 剪贴板任务完成回调；最多调用一次。
pub type ClipboardSink = Box<dyn FnOnce(ClipboardJobId, Result<ClipboardResult, PortError>) + Send>;

/// 将剪贴板完成结果安全投递回 GUI 事件循环的线程安全回调。
pub type ClipboardEventSink =
    Arc<dyn Fn(ClipboardTarget, ClipboardJobId, Result<ClipboardResult, PortError>) + Send + Sync>;

/// 构造不执行任何动作的剪贴板事件投递器，供无宿主测试和纯渲染场景使用。
pub fn noop_clipboard_event_sink() -> ClipboardEventSink {
    Arc::new(|_, _, _| {})
}
