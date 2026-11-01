//! 截图会话输出结果与交互配置类型定义。

use crate::model::PasterItem;
use crate::ports::{DesktopPort, MediaJobRunner};
use snapin_core::config::Config;
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

/// 截图触发模式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnipMode {
    /// 常规交互截图
    #[default]
    Normal,
    /// 截屏并自动复制（选定区域后立即自动复制并退出，无需点击确认）
    AutoCopy,
    /// 框选后自动翻译（首次锁定选区时立即执行整图翻译，保留工具栏便于继续处理）
    Translate,
    /// 自定义截屏（优先复用上次历史选区）
    Custom,
    /// 全屏截图（进入会话即锁定整屏选区，工具栏可继续标注或复制/保存/贴图）
    Fullscreen,
    /// 全屏取色器（无选区，单击复制光标处颜色并退出）
    PickColor,
}
