//! 各类矢量标注图元的具体光栅化绘制实现。

pub mod arrow;
pub mod badge;
pub mod ellipse;
pub mod marker;
pub mod mosaic;
pub mod pen;
pub mod rect;
pub mod text;
pub mod translation;

pub use arrow::ArrowAction;
pub use badge::BadgeAction;
pub use ellipse::EllipseAction;
pub use marker::MarkerAction;
pub use mosaic::MosaicAction;
pub use pen::PenAction;
pub use rect::RectAction;
pub use text::TextAction;
pub use translation::{TranslationAction, TranslationShapeItem};
