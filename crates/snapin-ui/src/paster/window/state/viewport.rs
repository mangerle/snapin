//! 贴图窗口视口状态：缩放倍率与光标位置。
//!
//! 这些值只描述“图像如何映射到窗口、光标在哪里”，不参与文档内容或后台任务；
//! 单独成组后，坐标换算与布局代码有清晰的输入边界。

/// 贴图窗口视口状态。
#[derive(Debug, Clone)]
pub(crate) struct PasterViewport {
    /// 界面缩放倍率（HiDPI 与用户工具栏倍率）
    pub(crate) ui_scale: f32,
    /// 当前光标在窗口客户区中的物理坐标
    pub(crate) cursor_pos: (f32, f32),
}

impl Default for PasterViewport {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            cursor_pos: (0.0, 0.0),
        }
    }
}
