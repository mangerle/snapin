//! UI 与宿主环境之间的端口契约。
//!
//! # 设计原理
//!
//! 截图与贴图窗口是纯表现层，但用户的复制、保存、OCR 与翻译请求必须落到真正的
//! 平台适配与后台服务上。若 UI 直接依赖 `snapin-platform` 与 `snapin-service`，
//! 表现层就会被迫承担应用层编排职责：自己读配置、拼保存路径、维护 OCR 引擎缓存，
//! 最终导致同一套流程在截图与贴图两侧各写一遍。
//!
//! 本模块只声明“UI 需要宿主做什么”的最小契约，具体实现在 `snapin-app` 中：
//! - [`DesktopPort`]：选区缓存、系统剪贴板与文件管理器等桌面副作用。
//! - [`WorkspaceStateStore`]：持久化活动分组等运行时状态，不写入用户配置。
//! - [`MediaJobRunner`]：把裁剪图交给后台执行 OCR、翻译与保存，并通过回调把结果送回窗口事件循环。
//!
//! # 代价与权衡
//!
//! 端口层让 UI 可以脱离真实平台与网络独立测试，但引入了一次动态分发与一次回调装箱。
//! 这些开销只发生在用户动作触发的低频路径上，相对于 OCR 推理与图像编码可以忽略。

use crate::snipper::selection::Rect;
use image::RgbaImage;
use snapin_core::annotation::shapes::TranslationShapeItem;
use snapin_core::config::CoverMode;
use std::fmt;
use std::path::PathBuf;

/// UI 端口操作失败原因。
///
/// 端口是表现层与宿主之间的边界，错误只需携带可直接展示的中文描述与操作类别，
/// 不向外泄漏平台或服务 crate 的第三方错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError {
    /// 失败的操作名称，例如“写入剪贴板”
    pub operation: String,
    /// 可直接展示给用户的中文原因
    pub message: String,
}

impl PortError {
    /// 构造端口错误
    pub fn new(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}失败: {}", self.operation, self.message)
    }
}

impl std::error::Error for PortError {}

/// 桌面副作用端口。
///
/// 该特型只承载“用户动作产生的系统交互”，不包含业务决策。例如是否发送通知、
/// 是否自动复制保存结果等策略由应用层实现决定，UI 只负责调用与展示结果。
pub trait DesktopPort: Send + Sync {
    /// 读取上次截图选区缓存；不存在或缓存损坏时返回 `None`。
    fn load_last_selection(&self) -> Option<Rect>;

    /// 保存本次截图选区缓存。
    fn save_last_selection(&self, rect: Rect);

    /// 将文本写入系统剪贴板。
    ///
    /// # Errors
    ///
    /// 剪贴板服务不可用或系统拒绝写入时返回 [`PortError`]。
    fn write_text_clipboard(&self, text: &str) -> Result<(), PortError>;

    /// 将图像写入系统剪贴板。
    ///
    /// # Errors
    ///
    /// 剪贴板服务不可用、图像尺寸非法或系统拒绝写入时返回 [`PortError`]。
    fn write_image_clipboard(&self, image: &RgbaImage) -> Result<(), PortError>;

    /// 读取系统剪贴板图像；没有图像内容时返回 `None`。
    fn read_clipboard_image(&self) -> Option<RgbaImage>;

    /// 读取系统剪贴板文本；没有文本内容时返回 `None`。
    fn read_clipboard_text(&self) -> Option<String>;

    /// 在系统文件管理器中打开指定路径。
    fn open_in_file_manager(&self, path: PathBuf);

    /// 按用户配置决定是否发送“已复制”桌面通知。
    ///
    /// UI 只描述发生了复制动作，通知开关与发送通道由应用层实现决定；
    /// 这样贴图窗口即可在保持纯表现层的同时保留原有通知行为。
    fn notify_copy(&self, body: &str);
}

/// 工作区运行时状态。
///
/// 这类状态描述当前会话位置（如活动分组），不属于用户偏好，不写入 `config.toml`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceState {
    /// 上次活动的贴图分组 ID
    pub active_group_id: usize,
}

/// 工作区运行时状态持久化端口。
pub trait WorkspaceStateStore: Send + Sync {
    /// 读取运行时状态；文件不存在时返回默认状态。
    ///
    /// # Errors
    ///
    /// 状态文件读取或解析失败时返回 [`PortError`]。
    fn load(&self) -> Result<WorkspaceState, PortError>;

    /// 写入运行时状态。
    ///
    /// # Errors
    ///
    /// 无法创建目录、写入临时文件或原子替换失败时返回 [`PortError`]。
    fn save(&self, state: WorkspaceState) -> Result<(), PortError>;
}

/// 后台媒体任务标识。
///
/// 每次提交由执行器分配单调递增 ID，结果回调必须携带同一 ID；窗口据此丢弃
/// 取消后仍可能到达的过期回调，避免旧任务覆盖新任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(u64);

impl JobId {
    /// 由执行器分配任务序号。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 读取原始序号，供日志与调试展示。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 需要后台执行的一次性媒体任务。
#[derive(Debug)]
pub enum MediaJob {
    /// 对图像执行离线 OCR，返回识别全文。
    Ocr {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
    },
    /// 对图像执行 OCR 与逐段翻译，返回可直接注入标注历史的结果。
    Translate {
        /// 已完成标注合成的选区图像
        image: RgbaImage,
        /// 图像在全屏坐标系中的像素原点，贴图整图翻译固定为 `(0, 0)`
        origin: (u32, u32),
        /// 原位翻译背景覆盖模式
        cover_mode: CoverMode,
    },
    /// 保存截图并遵循应用层当前的输出配置。
    SaveScreenshot {
        /// 要保存的最终图像
        image: RgbaImage,
    },
    /// 保存贴图；由应用层决定快捷保存还是弹出系统保存对话框。
    SavePaster {
        /// 要保存的最终图像
        image: RgbaImage,
        /// 是否弹出系统保存对话框
        as_dialog: bool,
        /// 保存成功后是否在文件管理器中打开所在目录
        open_folder_after: bool,
    },
}

/// 媒体任务执行结果。
#[derive(Debug)]
pub enum MediaJobOutcome {
    /// OCR 成功，返回识别全文（可能为空字符串）
    Ocr {
        /// 识别出的完整文本
        text: String,
    },
    /// 翻译成功，返回待注入标注历史的翻译图元
    Translated {
        /// 已按原图坐标排版的翻译图元
        items: Vec<TranslationShapeItem>,
    },
    /// 保存成功
    Saved {
        /// 最终落盘路径
        path: PathBuf,
    },
    /// 用户在保存对话框中主动取消
    SaveCancelled,
    /// 任务执行失败
    Failed {
        /// 可直接展示的中文错误描述
        message: String,
    },
}

/// 取消句柄。
///
/// 使用装箱闭包而不是绑定 `tokio::task::AbortHandle`，使 UI 端口不依赖任何
/// 特定异步运行时；应用层可以在闭包里同时设置取消标志与 abort 底层任务。
pub type JobCancel = Box<dyn Fn() + Send + Sync>;

/// 后台媒体任务执行端口。
///
/// 实现方必须保证任务在独立线程或运行时中执行，绝不能阻塞调用本方法的窗口事件循环。
/// 回调最多被调用一次；取消是协作式的：实现方应在 OCR、网络翻译、排版与保存阶段之间
/// 检查取消标志并提前返回，丢弃最终结果。ONNX Runtime 单次同步推理无法从外部强制中断，
/// 因此取消只能阻止后续阶段，不能立即释放正在执行的推理线程。
pub trait MediaJobRunner: Send + Sync {
    /// 提交媒体任务并注册结果回调。
    ///
    /// # Errors
    ///
    /// 执行器已停止、队列已满或任务参数非法时返回 [`PortError`]。
    ///
    /// 返回的任务 ID 会原样传给结果回调；取消后执行器可以丢弃结果，但 UI 侧
    /// 仍以 ID 校验兜底，防止竞态回传污染状态。
    fn submit(
        &self,
        job: MediaJob,
        sink: Box<dyn FnOnce(JobId, MediaJobOutcome) + Send>,
    ) -> Result<(JobId, JobCancel), PortError>;
}

/// 无操作桌面端口，供单元测试与不需要系统交互的渲染场景使用。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopDesktop;

impl DesktopPort for NoopDesktop {
    fn load_last_selection(&self) -> Option<Rect> {
        None
    }

    fn save_last_selection(&self, _rect: Rect) {}

    fn write_text_clipboard(&self, _text: &str) -> Result<(), PortError> {
        Ok(())
    }

    fn write_image_clipboard(&self, _image: &RgbaImage) -> Result<(), PortError> {
        Ok(())
    }

    fn read_clipboard_image(&self) -> Option<RgbaImage> {
        None
    }

    fn read_clipboard_text(&self) -> Option<String> {
        None
    }

    fn open_in_file_manager(&self, _path: PathBuf) {}

    fn notify_copy(&self, _body: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// 无操作端口必须保持幂等且不产生副作用，供纯 UI 测试安全复用
    #[test]
    fn noop_desktop_is_safe() {
        let desktop = NoopDesktop;
        assert!(desktop.load_last_selection().is_none());
        assert!(desktop.write_text_clipboard("hello").is_ok());
        assert!(desktop.read_clipboard_text().is_none());
    }

    /// 取消闭包应可被 UI 侧重复调用而不 panic
    #[test]
    fn job_cancel_is_repeatable() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let cancel: JobCancel = Box::new(move || flag.store(true, Ordering::Release));
        cancel();
        cancel();
        assert!(cancelled.load(Ordering::Acquire));
    }

    /// 端口错误应包含操作名称与原因，便于直接展示给用户
    #[test]
    fn port_error_display_contains_context() {
        let error = PortError::new("写入剪贴板", "系统剪贴板不可用");
        assert_eq!(error.to_string(), "写入剪贴板失败: 系统剪贴板不可用");
    }
}
