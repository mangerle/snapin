//! 贴图窗口几何变换矩阵计算。
//!
//! 变换矩阵与翻转旋转算法已下沉至 [`snapin_ports::model`]。
//! 本模块重导出该接口以维持原有引用兼容。

#[allow(unused_imports)]
pub use snapin_ports::{
    flip_pixmap_horizontal, flip_pixmap_vertical, rotate_pixmap_ccw, rotate_pixmap_cw,
    PasterTransform,
};
