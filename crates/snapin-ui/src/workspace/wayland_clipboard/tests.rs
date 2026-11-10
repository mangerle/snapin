//! Wayland 原生剪贴板模块单元测试（离线覆盖编码、MIME 校验与席位焦点状态机）。

use super::dispatch::ClipboardOfferData;
use super::focus::{SeatFocusRegistry, SelectionTarget};
use super::{PreparedSelection, MIME_PNG, TEXT_MIMES};
use crate::ports::ClipboardWrite;
use image::{Rgba, RgbaImage};
use std::sync::Arc;

#[test]
fn test_prepared_selection_image_encodes_valid_png() {
    let img = RgbaImage::from_pixel(3, 2, Rgba([12, 34, 56, 255]));
    let prepared = PreparedSelection::from_write(&ClipboardWrite::Image(img))
        .expect("有效 RGBA 图像应当成功编码为 PNG");

    assert_eq!(prepared.mimes.as_ref(), &[MIME_PNG.to_string()]);
    assert!(
        prepared.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "输出字节必须包含标准 PNG 文件头"
    );

    let decoded = image::load_from_memory_with_format(&prepared.bytes, image::ImageFormat::Png)
        .expect("编码后的 PNG 应当可被正常解码")
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (3, 2));
    assert_eq!(decoded.get_pixel(0, 0), &Rgba([12, 34, 56, 255]));
}

#[test]
fn test_prepared_selection_rejects_zero_dimension_image() {
    let empty = RgbaImage::new(0, 10);
    let err = PreparedSelection::from_write(&ClipboardWrite::Image(empty))
        .expect_err("零宽高图像必须被拒绝");
    assert!(err.contains("0"));
}

#[test]
fn test_prepared_selection_text_offers_expected_mimes() {
    let text = "Snapin 截图测试 123".to_string();
    let prepared = PreparedSelection::from_write(&ClipboardWrite::Text(text.clone()))
        .expect("纯文本载荷应当构建成功");

    assert_eq!(prepared.bytes.as_ref(), text.as_bytes());
    let expected: Vec<String> = TEXT_MIMES.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(prepared.mimes.as_ref(), expected.as_slice());

    let offer = ClipboardOfferData {
        bytes: prepared.bytes,
        mimes: prepared.mimes,
    };
    assert!(offer.supports_mime("text/plain;charset=utf-8"));
    assert!(offer.supports_mime("UTF8_STRING"));
    assert!(!offer.supports_mime("image/png"));
}

#[test]
fn test_seat_focus_registry_tracks_serial_and_focus_transitions() {
    let mut registry = SeatFocusRegistry::default();
    assert_eq!(registry.active_target(), None);

    registry.ensure_seat(1);
    assert_eq!(registry.active_target(), None);

    registry.set_keyboard_focus(1, true, Some(100));
    assert_eq!(
        registry.active_target(),
        Some(SelectionTarget {
            seat_name: 1,
            serial: 100
        })
    );
    assert!(registry.seat_has_focus(1));

    registry.record_input_serial(1, 101);
    assert_eq!(
        registry.active_target(),
        Some(SelectionTarget {
            seat_name: 1,
            serial: 101
        })
    );

    // 键盘离开后不应继续宣称持有焦点。
    registry.set_keyboard_focus(1, false, None);
    assert_eq!(registry.active_target(), None);
    assert!(!registry.seat_has_focus(1));

    // 指针或触控激活仍可作为有效焦点来源。
    registry.set_pointer_focus(1, true, Some(102));
    assert_eq!(
        registry.active_target(),
        Some(SelectionTarget {
            seat_name: 1,
            serial: 102
        })
    );

    registry.set_pointer_focus(1, false, None);
    registry.set_touch_active(1, true, Some(103));
    assert_eq!(
        registry.active_target(),
        Some(SelectionTarget {
            seat_name: 1,
            serial: 103
        })
    );
}

#[test]
fn test_seat_focus_registry_prefers_latest_focused_seat() {
    let mut registry = SeatFocusRegistry::default();
    registry.set_pointer_focus(1, true, Some(10));
    registry.set_keyboard_focus(2, true, Some(20));
    assert_eq!(
        registry.active_target(),
        Some(SelectionTarget {
            seat_name: 2,
            serial: 20
        })
    );

    // 席位 2 失焦后回落到仍持焦点的席位 1。
    registry.set_keyboard_focus(2, false, None);
    assert_eq!(
        registry.active_target(),
        Some(SelectionTarget {
            seat_name: 1,
            serial: 10
        })
    );

    registry.remove_seat(1);
    assert_eq!(registry.active_target(), None);
    let _ = Arc::new(registry);
}
