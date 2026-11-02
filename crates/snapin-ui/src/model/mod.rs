//! UI 共享图像模型：贴图图元与几何变换。
//!
//! 截图会话的“置为贴图”输出与贴图窗口本身都使用同一份 [`PasterItem`] 模型。
//! 将模型从 `paster` 子系统提取到 crate 根，避免截图模块为了返回贴图结果而
//! 反向依赖贴图子系统；几何变换同样被贴图窗口与模型共享。

pub mod transform;

use tiny_skia::Pixmap;

pub use transform::PasterTransform;

/// 贴图全局标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PasterId(u64);

impl PasterId {
    /// 由全局唯一序号构造贴图标识。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 读取原始序号，供日志与 IPC 边界转换。
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for PasterId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<u64> for PasterId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// 贴图分组标识。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupId(usize);

impl GroupId {
    /// 构造分组标识。
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// 读取原始分组序号。
    pub const fn get(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for GroupId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<usize> for GroupId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// 标注图元历史索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeId(usize);

impl ShapeId {
    /// 读取原始索引。
    pub const fn get(self) -> usize {
        self.0
    }
}

impl From<usize> for ShapeId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// 单张贴图的数据上下文（像素数据由所属窗口独占）。
#[derive(Debug)]
pub struct PasterItem {
    /// 全局唯一贴图标识
    pub id: PasterId,
    /// 窗口标题
    pub title: String,
    /// 当前展示图像（含已提交标注）
    pub pixmap: Pixmap,
    /// 初始窗口位置
    pub position: (i32, i32),
    /// 缩放与不透明度变换
    pub transform: PasterTransform,
    /// 所属分组 ID
    pub group_id: GroupId,
}

impl PasterItem {
    /// 构造新的贴图图元项
    pub fn new(
        id: impl Into<PasterId>,
        title: String,
        pixmap: Pixmap,
        position: (i32, i32),
    ) -> Self {
        Self {
            id: id.into(),
            title,
            pixmap,
            position,
            transform: PasterTransform::default(),
            group_id: GroupId::default(),
        }
    }
}
