//! 贴图窗口外观与交互开关。
//!
//! 这些字段都只影响“窗口如何展示/是否接收鼠标”，与贴图文档、任务槽或渲染缓存
//! 无关；集中为 PasterChrome 后，窗口样式逻辑与内容状态可以独立演进。

/// 贴图窗口外观与交互开关。
#[derive(Debug, Clone)]
pub(crate) struct PasterChrome {
    /// 是否显示标注工具栏
    pub(crate) show_toolbar: bool,
    /// 鼠标穿透：图像区域不参与输入命中
    pub(crate) mouse_through: bool,
    /// 是否绘制窗口阴影
    pub(crate) has_shadow: bool,
    /// 缩放时是否使用双线性平滑
    pub(crate) smooth_scaling: bool,
}

impl Default for PasterChrome {
    fn default() -> Self {
        Self {
            show_toolbar: false,
            mouse_through: false,
            has_shadow: true,
            smooth_scaling: true,
        }
    }
}
