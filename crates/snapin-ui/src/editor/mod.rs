//! 可复用标注编辑组件：工具栏、文本输入与图元草稿工厂。
//!
//! 截图窗口与贴图窗口共用同一套标注编辑能力。把这些组件从 `snipper` 子系统中
//! 提取出来，可避免贴图模块反向依赖截图模块，也避免两边各维护一份工具栏与
//! 文本输入状态机。

pub(crate) mod annotation_builder;
pub(crate) mod text_input;
pub(crate) mod toolbar;
