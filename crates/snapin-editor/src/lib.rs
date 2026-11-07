//! Snapin 通用标注交互与组件引擎。
//!
//! # 设计原理
//!
//! 截图窗口与贴图窗口共用同一套标注交互与编辑能力。将这些组件从 UI 单体中下沉
//! 为独立 crate，解除了表现层对庞大窗口生命周期的反向绑定，并使工具栏布局排版、
//! 实时文字排版编辑、笔迹草稿流转具备独立的单元测试与零 GUI 后端依赖的高速构建能力。

pub mod annotation_builder;
pub mod annotation_session;
pub mod drawing;
pub mod geometry;
pub mod stroke_session;
pub mod text_input;
pub mod toolbar;

pub use annotation_session::AnnotationSession;
pub use stroke_session::StrokeSession;
