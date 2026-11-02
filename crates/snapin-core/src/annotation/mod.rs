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
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MagnifierAction, MagnifierConnector,
    MagnifierShape, MarkerAction, MarkerBlendMode, MarkerShape, MosaicAction, PenAction,
    PolylineAction, RectAction, SpotlightAction, SpotlightShape, TextAction, TranslationAction,
    TranslationShapeItem,
};
pub use style::{ArrowHeadStyle, ArrowStyle, BadgeSequence, BadgeStyle, ColorRgba, LineStyle};
