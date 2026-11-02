//! Snapin 标注图元、历史与绘制引擎。

pub mod action;
pub mod document;
pub mod history;
pub mod shapes;
pub mod style;

pub(crate) use snapin_config::config;
pub(crate) use snapin_foundation::geometry;
pub(crate) use snapin_render as render;

pub use crate::geometry::{Point, Rect, RectI};
pub use action::AnnotationShape;
pub use document::AnnotationDocument;
pub use history::AnnotationHistory;
pub use shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MagnifierAction, MagnifierConnector,
    MagnifierShape, MarkerAction, MarkerBlendMode, MarkerShape, MarkerStrokeSpec, MosaicAction,
    PenAction, PenStrokeSpec, PolylineAction, RectAction, SizeParam, SpotlightAction,
    SpotlightShape, TextAction, TranslationAction, TranslationShapeItem,
};
pub use style::{
    ArrowHeadStyle, ArrowStyle, BadgeSequence, BadgeStyle, ColorRgba, LineStyle, StrokePathSpec,
};
