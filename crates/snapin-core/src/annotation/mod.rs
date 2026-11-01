//! 矢量标注引擎与图元操作管理模块。

pub mod action;
pub mod document;
pub mod history;
pub mod shapes;
pub mod style;

pub use crate::geometry::Rect;
pub use action::AnnotationShape;
pub use document::AnnotationDocument;
pub use history::AnnotationHistory;
pub use shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MarkerAction, MosaicAction, PenAction,
    RectAction, SpotlightAction, TextAction, TranslationAction, TranslationShapeItem,
};
pub use style::{ArrowStyle, BadgeStyle, ColorRgba, LineStyle};
