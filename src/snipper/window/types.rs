//! 截图会话输出结果与交互配置类型定义。

use crate::paster::PasterItem;
use image::RgbaImage;
use std::path::PathBuf;

/// 截图交互完成后的最终产出决策
#[derive(Debug)]
pub enum SnipperOutcome {
    /// 已复制选区图像到系统剪贴板
    Copied(RgbaImage),
    /// 已保存至磁盘文件
    Saved(PathBuf),
    /// 已定在屏幕上转换为置顶贴图
    Pinned(PasterItem),
    /// 用户主动取消并退出
    Cancelled,
}
