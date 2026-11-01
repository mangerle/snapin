//! 贴图窗口渲染缓存：帧缓冲复用与输入区域布局缓存。
//!
//! 这两者都属于“可丢弃的表现层缓存”，不参与窗口语义状态；尺寸变化或布局变化时
//! 只需整体失效，不应与文档、工具栏或任务状态耦合。

use crate::paster::window::input_region::InputRegionCache;
use tiny_skia::Pixmap;

/// 贴图窗口可复用的渲染缓存。
#[derive(Default)]
pub(crate) struct FrameCache {
    pixmap: Option<Pixmap>,
    /// 上次提交给 Wayland 的输入区域及其布局签名
    pub(crate) input_region: Option<InputRegionCache>,
}

impl FrameCache {
    /// 取出与窗口尺寸匹配的帧缓冲；尺寸变化时才重新分配。
    pub(crate) fn take_pixmap(&mut self, width: u32, height: u32) -> Option<Pixmap> {
        let needs_resize = self
            .pixmap
            .as_ref()
            .is_none_or(|pixmap| pixmap.width() != width || pixmap.height() != height);
        if needs_resize {
            self.pixmap = Pixmap::new(width, height);
        }
        self.pixmap.take()
    }

    /// 归还渲染帧缓冲，供下一帧复用。
    pub(crate) fn restore_pixmap(&mut self, pixmap: Pixmap) {
        self.pixmap = Some(pixmap);
    }
}
