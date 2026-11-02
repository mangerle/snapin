//! 截图会话输出结果与交互配置类型定义。

use crate::model::PasterItem;
use crate::ports::{DesktopPort, MediaJobRunner};
use snapin_core::config::Config;
use snapin_core::PostCaptureAction;
use std::path::PathBuf;
use std::sync::Arc;

/// 截图会话所需的宿主能力集合。
///
/// 把配置、触发模式与端口打包传入，避免会话构造函数平铺四个以上参数；
/// 同时明确“截图会话只消费这些能力”，不在内部隐式读取配置或启动后台线程。
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
}

/// 截图交互完成后的最终产出决策
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
    /// 已定在屏幕上转换为置顶贴图
    Pinned(PasterItem),
    /// 用户主动取消并退出
    Cancelled,
}

/// 一次截图覆盖层启动所需的全部数据。
pub struct SnipperLaunch {
    /// 全屏底图
    pub bg_image: image::RgbaImage,
    /// 窗口候选矩形
    pub windows: Vec<crate::snipper::WindowRect>,
    /// 会话配置、模式与端口
    pub runtime: SnipperRuntime,
}

pub use snapin_core::SnipMode;
