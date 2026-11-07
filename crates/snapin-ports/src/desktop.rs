//! 桌面副作用端口与工作区状态存储契约。

use super::clipboard::{
    ClipboardJobId, ClipboardPayload, ClipboardResult, ClipboardSink, ClipboardWrite,
};
use super::error::PortError;
use snapin_foundation::Rect;
use std::path::PathBuf;

/// 桌面副作用端口。
///
/// 该特型只承载“用户动作产生的系统交互”，不包含业务决策。例如是否发送通知、
/// 是否自动复制保存结果等策略由应用层实现决定，UI 只负责调用与展示结果。
/// 所有方法都可能在 winit 事件循环线程调用，实现不得同步等待磁盘或系统服务。
pub trait DesktopPort: Send + Sync {
    /// 读取上次截图选区缓存；不存在或缓存损坏时返回 `None`。
    fn load_last_selection(&self) -> Option<Rect>;

    /// 更新内存中的本次截图选区，并异步提交持久化。
    fn save_last_selection(&self, rect: Rect);

    /// 提交剪贴板写入；真实写入结果通过 `sink` 异步回传。
    ///
    /// # Errors
    ///
    /// 写入任务无法提交时返回 [`PortError`]。返回成功只表示任务已被宿主接收，
    /// 调用方必须等待回调后才能向用户报告“已复制”。
    fn write_clipboard(
        &self,
        write: ClipboardWrite,
        sink: ClipboardSink,
    ) -> Result<ClipboardJobId, PortError>;

    /// 提交一次当前系统剪贴板读取，结果通过 `sink` 异步回传。
    ///
    /// # Errors
    ///
    /// 读取任务无法提交时返回 [`PortError`]。
    fn read_clipboard(&self, sink: ClipboardSink) -> Result<ClipboardJobId, PortError>;

    /// 在系统文件管理器中打开指定路径。
    fn open_in_file_manager(&self, path: PathBuf);

    /// 使用系统默认浏览器打开经过协议白名单校验的网页链接。
    ///
    /// # Errors
    ///
    /// 链接协议不受支持、内容非法或系统无法启动浏览器时返回 [`PortError`]。
    fn open_url(&self, url: &str) -> Result<(), PortError>;

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
    /// 该方法由 GUI 事件循环线程调用，实现不得执行磁盘 I/O、系统调用等待或其他
    /// 可能阻塞用户交互的工作；持久化必须转交宿主后台执行器。
    ///
    /// # Errors
    ///
    /// 无法创建目录、写入临时文件或原子替换失败时返回 [`PortError`]。
    fn save(&self, state: WorkspaceState) -> Result<(), PortError>;
}

/// 无操作桌面端口，供单元测试与不需要系统交互的渲染场景使用。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopDesktop;

impl DesktopPort for NoopDesktop {
    fn load_last_selection(&self) -> Option<Rect> {
        None
    }

    fn save_last_selection(&self, _rect: Rect) {}

    fn write_clipboard(
        &self,
        _write: ClipboardWrite,
        sink: ClipboardSink,
    ) -> Result<ClipboardJobId, PortError> {
        let job_id = ClipboardJobId::new(0);
        sink(job_id, Ok(ClipboardResult::Written));
        Ok(job_id)
    }

    fn read_clipboard(&self, sink: ClipboardSink) -> Result<ClipboardJobId, PortError> {
        let job_id = ClipboardJobId::new(0);
        sink(
            job_id,
            Ok(ClipboardResult::Payload(ClipboardPayload::Empty)),
        );
        Ok(job_id)
    }

    fn open_in_file_manager(&self, _path: PathBuf) {}

    fn open_url(&self, _url: &str) -> Result<(), PortError> {
        Ok(())
    }

    fn notify_copy(&self, _body: &str) {}
}
