//! 截图窗口候选矩形的 UI 侧数据契约。

/// 可见窗口几何边界矩形（物理像素坐标与尺寸）。
///
/// 该类型属于截图表现层：窗口矩形只用于鼠标悬停高亮与吸附预览，
/// 具体如何从平台枚举窗口由应用层负责。UI 不直接依赖平台 crate。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    /// 窗口左上角物理像素 X 坐标
    pub x: i32,
    /// 窗口左上角物理像素 Y 坐标
    pub y: i32,
    /// 窗口物理像素宽度
    pub width: u32,
    /// 窗口物理像素高度
    pub height: u32,
}
