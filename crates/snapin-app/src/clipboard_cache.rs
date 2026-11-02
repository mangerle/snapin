//! 剪贴板内容的无锁快照缓存。
//!
//! 剪贴板轮询已在后台线程持续读取系统内容；UI 菜单不应再次同步访问系统剪贴板。
//! 本缓存把最近一次快照以 `ArcSwap` 发布，读路径只做一次原子指针加载。

use arc_swap::ArcSwapOption;
use image::RgbaImage;
use snapin_storage::ClipboardEntryKind;
use std::sync::Arc;

/// 最近一次剪贴板内容。
#[derive(Clone)]
pub(crate) enum ClipboardContent {
    Text {
        text: String,
        kind: ClipboardEntryKind,
    },
    Image(Arc<RgbaImage>),
}

/// 进程级剪贴板快照缓存。
pub(crate) struct ClipboardCache {
    current: ArcSwapOption<ClipboardContent>,
}

impl ClipboardCache {
    pub(crate) fn new() -> Self {
        Self {
            current: ArcSwapOption::empty(),
        }
    }

    pub(crate) fn store(&self, content: ClipboardContent) {
        self.current.store(Some(Arc::new(content)));
    }

    pub(crate) fn image(&self) -> Option<Arc<RgbaImage>> {
        match self.current.load_full().as_deref() {
            Some(ClipboardContent::Image(image)) => Some(image.clone()),
            Some(ClipboardContent::Text { .. }) | None => None,
        }
    }

    pub(crate) fn text(&self) -> Option<String> {
        match self.current.load_full().as_deref() {
            Some(ClipboardContent::Text { text, .. }) => Some(text.clone()),
            Some(ClipboardContent::Image(_)) | None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_cache_publishes_single_latest_content() {
        let cache = ClipboardCache::new();
        cache.store(ClipboardContent::Text {
            text: "第一段".to_string(),
            kind: ClipboardEntryKind::Text,
        });
        assert_eq!(cache.text().as_deref(), Some("第一段"));
        assert!(cache.image().is_none());

        let image = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255])));
        cache.store(ClipboardContent::Image(image));
        assert_eq!(
            cache.image().expect("应返回图像").get_pixel(0, 0).0,
            [1, 2, 3, 255]
        );
        assert!(cache.text().is_none());
    }
}
