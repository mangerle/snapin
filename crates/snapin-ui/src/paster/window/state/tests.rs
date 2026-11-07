use super::*;
use tiny_skia::Pixmap;

/// 构造一张最小的测试贴图状态
fn test_state() -> PasterWindowState {
    let pixmap = Pixmap::new(8, 8).expect("创建测试位图失败");
    let item = PasterItem::new(1, "测试贴图".to_string(), pixmap, (0, 0));
    PasterWindowState::new(item)
}

/// 状态提示在 3 秒有效期内可见，到达期限后自动过期并可被清除
#[test]
fn 状态提示按有效期过期() {
    let mut state = test_state();
    let now = Instant::now();
    assert!(state.is_status_expired(now), "没有提示时应视为已过期");

    state.set_status_message_at("已复制识别文字（4 字符）", now);
    assert!(!state.is_status_expired(now));
    assert!(
        !state.is_status_expired(now + Duration::from_millis(2_999)),
        "未满 3 秒时提示必须保持有效"
    );
    assert!(
        state.is_status_expired(now + STATUS_MESSAGE_TTL),
        "到达 3 秒后提示必须过期"
    );

    assert!(!state.clear_expired_status(now + Duration::from_millis(1)));
    assert!(
        state.clear_expired_status(now + STATUS_MESSAGE_TTL),
        "过期提示应被成功清除"
    );
    assert!(state.status_message.is_none());
    assert!(
        !state.clear_expired_status(now + STATUS_MESSAGE_TTL),
        "重复清除不应报告发生了清除"
    );
}

/// 状态提示的失效时刻应严格等于设置时刻加上有效期
#[test]
fn 状态提示失效时刻可计算() {
    let mut state = test_state();
    let now = Instant::now();
    assert!(state.status_deadline().is_none());

    state.set_status_message_at("翻译完成", now);
    assert_eq!(state.status_deadline(), Some(now + STATUS_MESSAGE_TTL));
}

/// 尺寸不变的连续帧必须复用同一块像素缓冲，不能每帧重新分配。
#[test]
fn test_frame_pixmap_is_reused_for_same_size() {
    let mut state = test_state();
    let first = state.take_frame_pixmap(32, 16).expect("应可分配帧缓冲");
    let pointer = first.data().as_ptr();
    state.restore_frame_pixmap(first);

    let second = state.take_frame_pixmap(32, 16).expect("应可复用帧缓冲");
    assert_eq!(second.data().as_ptr(), pointer);
    state.restore_frame_pixmap(second);
}

/// 窗口尺寸变化时必须重新分配匹配尺寸的帧缓冲。
#[test]
fn test_frame_pixmap_resizes_with_window() {
    let mut state = test_state();
    let first = state.take_frame_pixmap(32, 16).expect("应可分配帧缓冲");
    state.restore_frame_pixmap(first);

    let resized = state.take_frame_pixmap(64, 32).expect("应可重新分配");
    assert_eq!(resized.width(), 64);
    assert_eq!(resized.height(), 32);
    state.restore_frame_pixmap(resized);
}

/// 鼠标穿透开启时隐藏工具条会让输入区域变为空集，必须被守卫拒绝
#[test]
fn test_mouse_through_blocks_toolbar_hide() {
    let mut state = test_state();
    state.chrome.show_toolbar = true;
    assert!(state.can_set_toolbar_visible(false), "未穿透时应允许隐藏");

    state.chrome.mouse_through = true;
    assert!(
        !state.can_set_toolbar_visible(false),
        "穿透开启后不得隐藏唯一可交互手柄"
    );
    assert!(
        state.can_set_toolbar_visible(true),
        "显示工具条不应被穿透守卫拦截"
    );
}

#[test]
fn test_zoom_anchor_ratio() {
    assert_eq!(
        zoom_anchor_ratio(ZoomAnchor::Cursor, (100.0, 50.0), 400.0, 100.0),
        (0.25, 0.5)
    );
    assert_eq!(
        zoom_anchor_ratio(ZoomAnchor::Center, (100.0, 50.0), 400.0, 100.0),
        (0.5, 0.5)
    );
    assert_eq!(
        zoom_anchor_ratio(ZoomAnchor::Cursor, (900.0, 900.0), 400.0, 100.0),
        (1.0, 1.0)
    );
}
