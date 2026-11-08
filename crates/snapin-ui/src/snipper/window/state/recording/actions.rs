//! 录制工具栏动作与布局条目定义。
//!
//! 动作枚举从会话状态机中独立出来，便于工具栏渲染与事件分发共享同一套稳定契约，
//! 也避免会话文件持续膨胀。

use crate::editor::toolbar::Tool;

/// 录制工具栏中的一次性动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingAction {
    /// 开始、停止或进入编码收尾
    ToggleCapture,
    /// 暂停或恢复
    TogglePause,
    /// 切换系统声音请求
    ToggleAudio,
    /// 切换鼠标指针采集
    ToggleCursor,
    /// 打开或关闭格式菜单
    ToggleSettings,
    /// 切换帧率
    CycleFps,
    /// 撤销标注
    Undo,
    /// 拖动整条录制工具栏
    DragHandle,
    /// 移动工具栏或选区
    Move,
    /// 打开录制文件
    OpenFile,
    /// 复制录制文件路径
    CopyPath,
    /// 播放或暂停内嵌预览
    TogglePreview,
    /// 关闭结果面板或取消当前录制
    Close,
    /// 放弃当前录制或从结果页立即重新录制
    Restart,
}

impl RecordingAction {
    /// 工具栏悬浮提示名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::ToggleCapture => "开始或停止录制",
            Self::TogglePause => "暂停或继续",
            Self::ToggleAudio => "系统声音",
            Self::ToggleCursor => "鼠标指针",
            Self::ToggleSettings => "录制设置",
            Self::CycleFps => "切换帧率",
            Self::Undo => "撤销标注",
            Self::DragHandle => "拖动工具栏",
            Self::Move => "移动",
            Self::OpenFile => "打开文件",
            Self::CopyPath => "复制文件路径",
            Self::TogglePreview => "播放或暂停预览",
            Self::Close => "关闭",
            Self::Restart => "重新录制",
        }
    }
}

/// 录制工具栏按钮：录制动作与现有标注工具共用同一布局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingItem {
    /// 录制控制动作
    Action(RecordingAction),
    /// 复用主工具栏的标注工具
    Tool(Tool),
}
