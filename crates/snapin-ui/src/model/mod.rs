//! UI 共享图像模型：贴图图元与几何变换。
//!
//! # 设计原理
//!
//! 贴图图元模型已下沉至 [`snapin_ports::model`] 作为整个工作区与应用层的领域契约。
//! 本模块重导出该契约，以保持表现层内部调用的向后兼容与统一。

pub mod transform;

pub use snapin_ports::{GroupId, PasterId, PasterItem, PasterTransform, ShapeId};
