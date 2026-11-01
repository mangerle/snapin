//! UI 共享图像模型：贴图图元与几何变换。
//!
//! 截图会话的“置为贴图”输出与贴图窗口本身都使用同一份 [`PasterItem`] 模型。
//! 将模型从 `paster` 子系统提取到 crate 根，避免截图模块为了返回贴图结果而
//! 反向依赖贴图子系统；几何变换同样被贴图窗口与模型共享。

pub mod transform;

use tiny_skia::Pixmap;

pub use transform::PasterTransform;

/// 单张贴图的数据上下文（像素数据由所属窗口独占）。
#[derive(Debug)]
pub struct PasterItem {
    /// 全局唯一贴图标识
    pub id: u64,
    /// 窗口标题
    pub title: String,
    /// 当前展示图像（含已提交标注）
    pub pixmap: Pixmap,
    /// 初始窗口位置
    pub position: (i32, i32),
    /// 缩放与不透明度变换
    pub transform: PasterTransform,
    /// 所属分组 ID
    pub group_id: usize,
}

impl PasterItem {
    /// 构造新的贴图图元项
    pub fn new(id: u64, title: String, pixmap: Pixmap, position: (i32, i32)) -> Self {
        Self {
            id,
            title,
            pixmap,
            position,
            transform: PasterTransform::default(),
            group_id: 0,
        }
    }
}
