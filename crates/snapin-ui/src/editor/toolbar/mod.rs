//! 截图工具栏与二级属性选择器。
//!
//! 提供矩形、箭头、直线/折线、画笔、荧光笔、橡皮擦、马赛克/模糊/智能打码、
//! 聚光灯、区域放大镜与文本等标注工具切换，
//! 以及撤销、重做、定在屏幕(贴图)、保存、复制、关闭等动作。
//! 支持自适应屏幕边缘检测，防止超出可视区域。

mod controller;
pub mod icons;
mod interaction;
pub mod items;
mod layout;
mod metrics;
pub mod render;
mod sub_controls;

#[cfg(test)]
mod tests;

pub use items::{LineMode, MosaicMode, SubToolOptions, Tool, ToolbarAction, ToolbarItem};
pub(crate) use layout::SubControl;
mod state;
pub use state::Toolbar;
pub(crate) use state::{clear_remembered_sub_options, remembered_sub_options};

/// 记录并拒绝错误的二级栏控件路由。
///
/// 分组处理函数只接受本组控件，路由错误属于编程缺陷而不是用户输入。调试构建
/// 直接 panic 让测试尽早暴露；发布构建降级为错误日志并安全返回，避免一次点击
/// 触发整个截图窗口崩溃。这样既保留不变量检查，又不把断言留给生产用户。
pub(crate) fn reject_unexpected_control(group: &str, control: SubControl) {
    #[cfg(debug_assertions)]
    {
        panic!("二级栏控件分组路由异常，分组: {group}，控件: {control:?}");
    }
    #[cfg(not(debug_assertions))]
    {
        tracing::error!("二级栏控件分组路由异常，分组: {group}，控件: {control:?}");
    }
}
