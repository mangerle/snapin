//! 各类矢量标注图元的具体光栅化绘制实现。

pub mod arrow;
pub mod marker;
pub mod mosaic;
pub mod pen;
pub mod rect;

pub use arrow::ArrowAction;
pub use marker::MarkerAction;
pub use mosaic::MosaicAction;
pub use pen::PenAction;
pub use rect::RectAction;
