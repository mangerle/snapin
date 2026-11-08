//! 屏幕录制的明确阶段状态。

/// 录制会话所处的明确阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingStatus {
    /// 尚未按下录制
    Idle,
    /// 已提交后台任务，等待原生采集器就绪
    Starting,
    /// 正在采集
    Active,
    /// 已暂停采集
    Paused,
    /// 已停止采集，正在编码收尾
    Stopping,
    /// 已成功落盘，正在展示结果与内嵌预览
    Completed,
}

impl RecordingStatus {
    /// 是否处于已经开始且尚未进入编码收尾的状态。
    pub const fn is_recording(self) -> bool {
        matches!(self, Self::Active | Self::Paused)
    }

    /// 是否仍处于屏幕透明采集阶段。
    pub const fn is_capturing(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Active | Self::Paused | Self::Stopping
        )
    }
}
