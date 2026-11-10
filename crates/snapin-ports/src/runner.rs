//! GUI 窗口会话驱动与工作区表现层契约（六边形架构 Driver 契约）。
//!
//! # 设计原理
//!
//! 截图覆盖层与贴图工作区是图形视图，其实际生命周期与事件循环受宿主平台环境制约。
//! 若应用编排层直接依赖 Winit 的 `EventLoop` 或特定 UI 代理，就无法在 CI 或无显示服务环境下
//! 进行业务逻辑回归。
//!
//! 本模块抽离通用的工作区表现层驱动契约 [`WorkspaceDriver`]：
//! - 守护进程与服务调度仅面向契约派发截图覆盖层、贴图、状态切换与停机指令；
//! - 测试环境提供开箱即用的 [`MockWorkspaceDriver`]，支持毫秒级无头模拟验证；
//! - 真实的 `snapin-ui` 通过实现该契约接入，保持各层职责清晰隔离。

use crate::desktop::DesktopPort;
use crate::error::PortError;
use crate::id::GroupId;
use crate::jobs::MediaJobRunner;
use crate::model::PasterItem;
use crate::recording::ScreenRecorder;
use async_trait::async_trait;
use snapin_config::config::Config;
use snapin_foundation::{PostCaptureAction, SnipMode};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 贴图工作区当前状态快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspaceStatus {
    /// 当前活动贴图分组
    pub active_group: GroupId,
    /// 存活贴图总数量
    pub paster_count: usize,
}

/// 可见窗口几何边界矩形（物理像素坐标与尺寸）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowRect {
    /// 窗口左上角物理像素 X 坐标
    pub x: i32,
    /// 窗口左上角物理像素 Y 坐标
    pub y: i32,
    /// 窗口物理像素宽度
    pub width: u32,
    /// 窗口物理像素高度
    pub height: u32,
}

/// 截图会话所需的宿主能力集合。
pub struct SnipperRuntime {
    /// 本次会话使用的配置快照
    pub config: Config,
    /// 截图触发模式
    pub mode: SnipMode,
    /// 首次锁定选区后自动执行的动作
    pub post_action: Option<PostCaptureAction>,
    /// 桌面副作用端口
    pub desktop: Arc<dyn DesktopPort>,
    /// 媒体任务执行端口
    pub media: Arc<dyn MediaJobRunner>,
    /// 浏览器录屏与媒体编码端口
    pub recorder: Arc<dyn ScreenRecorder>,
}

/// 截图交互完成后的最终产出决策。
#[derive(Debug)]
pub enum SnipperOutcome {
    /// 已复制选区图像到系统剪贴板
    CopiedImage,
    /// 已将 OCR 识别文字复制到系统剪贴板
    CopiedText(String),
    /// 已将取色器颜色值复制到系统剪贴板
    CopiedColor(String),
    /// 已保存至磁盘文件
    Saved(PathBuf),
    /// 屏幕录制已完成并保存至磁盘
    RecordingSaved(PathBuf),
    /// 已定在屏幕上转换为置顶贴图
    Pinned(PasterItem),
    /// 用户主动取消并退出
    Cancelled,
}

/// 一次截图覆盖层启动所需的全部数据。
pub struct SnipperLaunch {
    /// 全屏底图
    pub bg_image: image::RgbaImage,
    /// 底图左上角在虚拟桌面中的物理像素原点
    ///
    /// Windows 多屏或缩放环境不能总是依赖系统全屏窗口自动匹配截图尺寸；
    /// 宿主据此把无边框覆盖层精确放到抓屏内容对应的物理桌面区域，避免重采样。
    pub screen_origin: (i32, i32),
    /// 窗口候选矩形
    pub windows: Vec<WindowRect>,
    /// 会话配置、模式与端口
    pub runtime: SnipperRuntime,
}

/// 抽象工作区与窗口驱动契约。
///
/// 应用层与命令调度通过该契约与图形界面解耦，无论底层采用 Winit 还是 Mock 都可以无缝替换。
#[async_trait]
pub trait WorkspaceDriver: Send + Sync {
    /// 请求工作区打开全屏截图覆盖层
    ///
    /// # Errors
    ///
    /// 宿主 GUI 事件循环已退出或事件投递失败时返回 [`PortError`]。
    fn open_snipper(&self, launch: SnipperLaunch) -> Result<(), PortError>;

    /// 请求工作区打开新贴图窗口
    ///
    /// # Errors
    ///
    /// 宿主 GUI 事件循环已退出或事件投递失败时返回 [`PortError`]。
    fn open_paster(&self, item: PasterItem) -> Result<(), PortError>;

    /// 触发配置热重载
    ///
    /// # Errors
    ///
    /// 宿主 GUI 事件循环已退出、事件投递失败或等待配置应用超时时返回 [`PortError`]。
    fn reload_config(&self, cfg: Config) -> Result<(), PortError>;

    /// 唤醒宿主事件处理队列
    fn wake_host(&self);

    /// 批量切换当前分组内所有贴图的显隐状态
    ///
    /// # Errors
    ///
    /// 宿主 GUI 事件循环已退出或事件投递失败时返回 [`PortError`]。
    fn toggle_visibility(&self) -> Result<(), PortError>;

    /// 强制显示当前活动分组贴图
    ///
    /// # Errors
    ///
    /// 宿主 GUI 事件循环已退出或事件投递失败时返回 [`PortError`]。
    fn show_workspace(&self) -> Result<(), PortError>;

    /// 切换活动贴图分组
    ///
    /// # Errors
    ///
    /// 宿主 GUI 事件循环已退出或事件投递失败时返回 [`PortError`]。
    fn switch_group(&self, group_id: Option<GroupId>) -> Result<(), PortError>;

    /// 异步获取工作区当前活动状态快照
    async fn get_status(&self) -> Result<WorkspaceStatus, PortError>;

    /// 请求平滑退出
    fn request_shutdown(&self);

    /// 请求平滑停机并等待确认
    async fn shutdown_and_wait(&self) -> Result<(), PortError>;
}

/// 专用于无头环境与单元测试的 Mock 驱动实现。
///
/// 记录所有接收到的指令与数据，便于测试中断言行为。
#[derive(Default)]
pub struct MockWorkspaceDriver {
    /// 接收到的截图启动载荷
    pub snipper_launches: Mutex<Vec<SnipperLaunch>>,
    /// 接收到的贴图项
    pub paster_items: Mutex<Vec<PasterItem>>,
    /// 接收到的重载配置
    pub reloaded_configs: Mutex<Vec<Config>>,
    /// 显隐切换调用次数
    pub visibility_toggles: AtomicUsize,
    /// 显示工作区调用次数
    pub workspace_shows: AtomicUsize,
    /// 分组切换请求列表
    pub group_switches: Mutex<Vec<Option<GroupId>>>,
    /// 宿主唤醒调用次数
    pub host_wakes: AtomicUsize,
    /// 是否收到停机请求
    pub shutdown_requested: AtomicBool,
    /// 当前模拟活动分组
    pub active_group: Mutex<GroupId>,
    /// 当前模拟存活贴图数量
    pub paster_count: AtomicUsize,
}

impl MockWorkspaceDriver {
    /// 创建带有指定贴图数量与分组的 Mock 驱动。
    pub fn new(active_group: GroupId, paster_count: usize) -> Self {
        Self {
            active_group: Mutex::new(active_group),
            paster_count: AtomicUsize::new(paster_count),
            ..Default::default()
        }
    }
}

#[async_trait]
impl WorkspaceDriver for MockWorkspaceDriver {
    fn open_snipper(&self, launch: SnipperLaunch) -> Result<(), PortError> {
        let mut list = self
            .snipper_launches
            .lock()
            .map_err(|_| PortError::new("mock_workspace", "Mock 锁中毒"))?;
        list.push(launch);
        Ok(())
    }

    fn open_paster(&self, item: PasterItem) -> Result<(), PortError> {
        let mut list = self
            .paster_items
            .lock()
            .map_err(|_| PortError::new("mock_workspace", "Mock 锁中毒"))?;
        list.push(item);
        self.paster_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn reload_config(&self, cfg: Config) -> Result<(), PortError> {
        let mut list = self
            .reloaded_configs
            .lock()
            .map_err(|_| PortError::new("mock_workspace", "Mock 锁中毒"))?;
        list.push(cfg);
        Ok(())
    }

    fn wake_host(&self) {
        self.host_wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn toggle_visibility(&self) -> Result<(), PortError> {
        self.visibility_toggles.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn show_workspace(&self) -> Result<(), PortError> {
        self.workspace_shows.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn switch_group(&self, group_id: Option<GroupId>) -> Result<(), PortError> {
        let mut list = self
            .group_switches
            .lock()
            .map_err(|_| PortError::new("mock_workspace", "Mock 锁中毒"))?;
        list.push(group_id);
        if let Some(gid) = group_id {
            let mut current = self
                .active_group
                .lock()
                .map_err(|_| PortError::new("mock_workspace", "Mock 锁中毒"))?;
            *current = gid;
        }
        Ok(())
    }

    async fn get_status(&self) -> Result<WorkspaceStatus, PortError> {
        let group = *self
            .active_group
            .lock()
            .map_err(|_| PortError::new("mock_workspace", "Mock 锁中毒"))?;
        let count = self.paster_count.load(Ordering::SeqCst);
        Ok(WorkspaceStatus {
            active_group: group,
            paster_count: count,
        })
    }

    fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
    }

    async fn shutdown_and_wait(&self) -> Result<(), PortError> {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        Ok(())
    }
}
