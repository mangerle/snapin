//! 矢量标注引擎与图元操作管理模块。

pub mod action;
pub mod shapes;

pub use action::{AnnotationAction, AnnotationHistory, AnnotationShape};
pub use shapes::{
    ArrowAction, BadgeAction, EllipseAction, MarkerAction, MosaicAction, PenAction, RectAction,
    TextAction, TranslationAction, TranslationShapeItem,
};
