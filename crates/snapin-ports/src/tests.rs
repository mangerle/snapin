//! 端口契约单元测试。

use crate::*;
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 无操作端口必须保持幂等且不产生副作用，供测试安全复用
#[test]
fn noop_desktop_is_safe() {
    let desktop = NoopDesktop;
    assert!(desktop.load_last_selection().is_none());
    assert!(desktop
        .write_clipboard(
            ClipboardWrite::Text("hello".to_string()),
            Box::new(|_, _| {})
        )
        .is_ok());
    assert!(desktop
        .write_clipboard(
            ClipboardWrite::Image(RgbaImage::new(1, 1)),
            Box::new(|_, _| {})
        )
        .is_ok());
    assert!(desktop.read_clipboard(Box::new(|_, _| {})).is_ok());
}

/// 取消闭包应可被重复调用而不 panic
#[test]
fn job_cancel_is_repeatable() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let cancel: JobCancel = Box::new(move || flag.store(true, Ordering::Release));
    cancel();
    cancel();
    assert!(cancelled.load(Ordering::Acquire));
}

/// 端口错误应包含操作名称与原因，便于直接展示给用户
#[test]
fn port_error_display_contains_context() {
    let error = PortError::with_kind(PortErrorKind::Unavailable, "写入剪贴板", "系统剪贴板不可用");
    assert_eq!(error.to_string(), "写入剪贴板失败: 系统剪贴板不可用");
    assert_eq!(error.kind, PortErrorKind::Unavailable);
}
