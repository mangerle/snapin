//! 屏幕录制端口契约。
//!
//! # 设计原理
//!
//! 录制会话是跨越多帧、持续数分钟的长生命周期任务，不能复用一次性
//! [`MediaJobRunner`](crate::MediaJobRunner) 的“提交后只回传一个结果”模型：
//! 截图窗口需要在开始、暂停、恢复、停止与失败时立即更新工具栏，同时后台仍需
//! 持续采集和编码。这里用独立端口把控制面与事件面分开：
//! - 控制面由 [`RecordingHandle`] 提供同步的暂停、恢复、停止与取消；
//! - 事件面通过有界的 UI 事件代理回传状态，不在端口层直接依赖 Winit。
//!
//! 端口只接收已经裁剪语义明确的物理像素区域，平台实现负责选择显示器与处理
//! 系统权限，媒体实现负责编码与落盘。
//!
//! # 代价与权衡
//!
//! 动态分发与事件回调增加少量装箱开销，但录制每一帧都要经过采集、色彩转换和
//! 编码，这部分开销可以忽略。取消是协作式的，已经进入原生采集或单帧编码的
//! 工作不会被内核级抢占，但会尽快停止后续帧并清理未完成文件。

use super::error::PortError;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// 录屏输出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordingFormat {
    /// 动图 GIF。
    Gif,
    /// 无损动图 WebP。
    #[default]
    WebP,
    /// H.264 编码的 MP4 视频。
    Mp4,
}

impl RecordingFormat {
    /// 输出文件扩展名，不含点号。
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Mp4 => "mp4",
        }
    }

    /// 工具栏展示名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            Self::WebP => "WebP",
            Self::Mp4 => "MP4",
        }
    }
}

/// 待录制的物理像素区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingRegion {
    /// 区域左上角在全屏快照中的物理像素 X 坐标。
    pub x: u32,
    /// 区域左上角在全屏快照中的物理像素 Y 坐标。
    pub y: u32,
    /// 区域宽度。
    pub width: u32,
    /// 区域高度。
    pub height: u32,
}

impl RecordingRegion {
    /// 判断区域是否具备可编码的最小尺寸。
    pub const fn is_valid(self) -> bool {
        self.width >= 2 && self.height >= 2
    }
}

/// 一次录制使用的参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingOptions {
    /// 选区物理像素区域。
    pub region: RecordingRegion,
    /// 目标帧率。
    pub fps: u32,
    /// 输出格式。
    pub format: RecordingFormat,
    /// 是否尝试包含鼠标指针。
    pub include_cursor: bool,
    /// 是否请求系统声音；平台无法采集时会在启动事件中明确降级。
    pub include_system_audio: bool,
    /// 是否对鼠标位置绘制高亮。
    pub highlight_cursor: bool,
}

impl RecordingOptions {
    /// 将帧率夹取到 GIF、WebP 与 MP4 均可稳定处理的常用范围。
    pub fn normalized_fps(self) -> Self {
        let mut normalized = self;
        normalized.fps = self.fps.clamp(10, 60);
        normalized
    }
}

/// 一帧压缩后的回放预览。
///
/// 预览以 JPEG 字节保存而不是常驻 RGBA，可将高分辨率预览的内存占用压缩到原来的
/// 几十分之一；界面只解码当前显示帧，因此清晰度提升不会转化为整段录制的高额内存。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingPreviewFrame {
    /// 预览像素宽度。
    pub width: u32,
    /// 预览像素高度。
    pub height: u32,
    /// JPEG 编码数据。
    pub data: Arc<[u8]>,
}

impl RecordingPreviewFrame {
    /// 使用已编码预览数据创建帧。
    pub fn new(width: u32, height: u32, data: impl Into<Arc<[u8]>>) -> Self {
        Self {
            width,
            height,
            data: data.into(),
        }
    }
}

/// 一次成功录制返回的最终产物与内嵌预览数据。
///
/// 预览帧由采集循环按固定间隔生成高分辨率 JPEG，而不是录制结束后重新解码媒体文件。
/// 这样 GIF、WebP 与 MP4 共用同一条预览通路，也避免常驻保存大量 RGBA 像素；
/// 代价是预览帧率低于最终视频，界面显示当前帧时需要执行一次内存解码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingArtifact {
    /// 最终媒体文件路径。
    pub path: PathBuf,
    /// 有效录制时长，不包含暂停区间。
    pub duration: Duration,
    /// 高分辨率压缩预览帧，按时间顺序排列。
    pub preview_frames: Arc<Vec<RecordingPreviewFrame>>,
}

/// 录制后台回传事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingEvent {
    /// 原生采集器与编码器已准备就绪，计时从此开始。
    Started,
    /// 录制进度；用于状态提示，并作为界面单调时钟的防回退兜底。
    Progress {
        /// 已录制时长。
        elapsed: Duration,
        /// 已成功写入的帧数。
        frames: u64,
    },
    /// 录制已暂停。
    Paused,
    /// 录制已恢复。
    Resumed,
    /// 录制完成并成功落盘。
    Finished {
        /// 最终文件路径、有效时长与高分辨率压缩预览帧。
        artifact: RecordingArtifact,
    },
    /// 用户取消，部分文件已清理。
    Cancelled,
    /// 录制已启动但部分请求能力发生降级。
    Warning {
        /// 可展示的中文提示。
        message: String,
    },
    /// 录制或编码失败。
    Failed {
        /// 可直接展示的中文错误描述。
        message: String,
    },
}

/// 录制事件接收器。
pub type RecordingEventSink = Arc<dyn Fn(RecordingEvent) + Send + Sync>;

/// 长生命周期录制控制器。
pub trait RecordingHandle: Send + Sync {
    /// 暂停采集；已采集但尚未编码的帧允许继续完成编码。
    ///
    /// # Errors
    ///
    /// 后台录制已结束或控制通道不可用时返回 [`PortError`]。
    fn pause(&self) -> Result<(), PortError>;

    /// 恢复采集。
    ///
    /// # Errors
    ///
    /// 后台录制已结束或控制通道不可用时返回 [`PortError`]。
    fn resume(&self) -> Result<(), PortError>;

    /// 请求停止采集、完成尾部编码并回传 [`RecordingEvent::Finished`]。
    ///
    /// # Errors
    ///
    /// 后台录制已结束或控制通道不可用时返回 [`PortError`]。
    fn stop(&self) -> Result<(), PortError>;

    /// 更新需要叠加到每个采集帧上的透明标注层；`None` 表示清除。
    fn update_overlay(&self, overlay: Option<image::RgbaImage>);

    /// 请求取消录制并删除未完成文件。
    fn cancel(&self);
}

/// 屏幕录制服务端口。
pub trait ScreenRecorder: Send + Sync {
    /// 启动一次录制并返回控制句柄。
    ///
    /// # Errors
    ///
    /// 录制区域非法、后台执行器已停机、权限不足或原生采集器无法初始化时返回
    /// [`PortError`]。启动成功只表示任务已被接收，真正开始采集需等待
    /// [`RecordingEvent::Started`]。
    fn start(
        &self,
        options: RecordingOptions,
        sink: RecordingEventSink,
    ) -> Result<Box<dyn RecordingHandle>, PortError>;
}

/// 无操作录屏实现，供纯 UI 测试与不支持录屏的宿主降级使用。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopScreenRecorder;

impl ScreenRecorder for NoopScreenRecorder {
    fn start(
        &self,
        _options: RecordingOptions,
        _sink: RecordingEventSink,
    ) -> Result<Box<dyn RecordingHandle>, PortError> {
        Err(PortError::with_kind(
            super::error::PortErrorKind::Unavailable,
            "启动屏幕录制",
            "当前宿主未提供屏幕录制能力",
        ))
    }
}
