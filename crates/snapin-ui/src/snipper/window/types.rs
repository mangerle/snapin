//! 截图会话输出结果与交互配置类型定义。

use crate::paster::PasterItem;
use std::path::PathBuf;

/// 截图交互完成后的最终产出决策
#[derive(Debug)]
pub enum SnipperOutcome {
    /// 已复制选区图像到系统剪贴板
    CopiedImage,
    /// 已将 OCR 识别文字复制到系统剪贴板
    CopiedText(String),
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
    /// 自定义截屏（优先复用上次历史选区）
    Custom,
}
