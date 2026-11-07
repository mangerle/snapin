//! 选区录屏白色悬浮工具栏。
//!
//! # 设计原理
//!
//! 工具栏按 PixPin 录制面板的信息层级排列：左侧显示时长、帧率与录制控制，中间
//! 是录制时可用的标注工具，右侧是撤销、移动、关闭与重启。标注图标继续复用
//! `snapin-editor` 的矢量统一实现，录制专用图标只补充缺失语义。
//!
//! 布局、渲染与命中测试共用同一份坐标结果，避免悬停和点击热区逐渐漂移。

mod icons;
mod layout;
mod menu;
mod render;

pub(crate) use layout::{calculate_recording_layout, RecordingLayoutRequest};
pub(crate) use render::{render_recording_toolbar, RecordingRenderContext};

#[cfg(test)]
mod tests;
