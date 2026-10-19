//! Snapin 业务流程子模块聚合门面。

pub mod cleaner;
pub mod daemon;
pub mod installer;
pub mod lifecycle;
pub mod paste;
pub mod snip;

use snapin_ui::paster::{register_paster_observer, PasterLifecycleObserver};
use snapin_ui::winit::window::WindowId;
use std::sync::Arc;
use std::time::Duration;

/// 为集中贴图工作区注册基于配套 GNOME 扩展的平台特权置顶观察者
pub fn setup_paster_platform_observer() {
    struct GnomeExtensionPasterObserver;

    impl PasterLifecycleObserver for GnomeExtensionPasterObserver {
        fn on_paster_created(&self, title: &str, _window_id: WindowId) {
            let t = title.to_string();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if let Err(e) = snapin_platform::pin_window_above(&t).await {
                        tracing::warn!("请求 GNOME 扩展置顶窗口失败 [{t}]: {e}");
                    }
                });
            } else {
                std::thread::Builder::new()
                    .name("snapin-paster-pin".to_string())
                    .spawn(move || {
                        std::thread::sleep(Duration::from_millis(50));
                        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            let _ = rt.block_on(snapin_platform::pin_window_above(&t));
                        }
                    })
                    .ok();
            }
        }

        fn on_paster_closed(&self, _window_id: WindowId) {}
    }

    register_paster_observer(Arc::new(GnomeExtensionPasterObserver));
}
