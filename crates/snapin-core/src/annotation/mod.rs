//! 矢量标注引擎与图元操作管理模块。

pub mod action;
pub mod shapes;
pub mod style;

pub use action::{AnnotationAction, AnnotationHistory, AnnotationShape};
pub use shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MarkerAction, MosaicAction, PenAction,
    RectAction, SpotlightAction, TextAction, TranslationAction, TranslationShapeItem,
};
pub use style::{ArrowStyle, BadgeStyle, LineStyle};
