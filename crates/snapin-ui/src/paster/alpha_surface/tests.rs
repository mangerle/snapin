use super::buffer::{buffer_byte_len, pack_premultiplied_rgba};
use super::*;
use tiny_skia::{Color, Pixmap};

/// 预乘 RGBA 像素必须按 Argb8888 位布局逐位打包（含半透明与全透明边界）
#[test]
fn test_pack_premultiplied_rgba() {
    // 不透明红：预乘后 alpha 与颜色通道均为满值
    assert_eq!(pack_premultiplied_rgba([255, 0, 0, 255]), 0xFFFF_0000);
    // 不透明绿与不透明蓝
    assert_eq!(pack_premultiplied_rgba([0, 255, 0, 255]), 0xFF00_FF00);
    assert_eq!(pack_premultiplied_rgba([0, 0, 255, 255]), 0xFF00_00FF);
    // 半透明黑（预乘值即阴影用的 rgba(0,0,0,90)）：alpha 位于最高字节
    assert_eq!(pack_premultiplied_rgba([0, 0, 0, 90]), 90u32 << 24);
    // 半透明红：预乘后 r=64、a=128，不做任何二次乘法
    assert_eq!(pack_premultiplied_rgba([64, 0, 0, 128]), 0x8040_0000);
    // 全透明像素必须打包为 0，保证透明区域不残留颜色
    assert_eq!(pack_premultiplied_rgba([0, 0, 0, 0]), 0);
}

/// 位图拷贝必须逐像素重排字节序，并保留预乘通道数值
#[test]
fn test_copy_pixmap_into_rewrites_premultiplied_pixels() {
    let mut pixmap = Pixmap::new(2, 1).expect("测试位图分配必须成功");
    pixmap.fill(Color::from_rgba8(255, 0, 0, 128));
    let mut dest = vec![0u32; 2];
    copy_pixmap_into(&mut dest, &pixmap);
    // tiny-skia 的 from_rgba8 会做非预乘到预乘转换：r=255,a=128 -> r≈128
    for packed in dest {
        let alpha = (packed >> 24) & 0xFF;
        let red = (packed >> 16) & 0xFF;
        assert_eq!(alpha, 128, "alpha 通道必须保留原值");
        assert!(
            (127..=129).contains(&red),
            "红色通道必须是预乘值，实际: {red}"
        );
        // 绿色与蓝色通道必须为零
        assert_eq!(packed & 0x0000_FFFF, 0, "未参与的颜色通道必须为零");
    }
}

/// 乒乓候选槽位在提交后必须交替，且仅在候选缓冲区被归还时才可写
#[test]
fn test_ping_pong_slot_selection() {
    let mut ping = PingPongState::default();
    // 初始状态：两块缓冲都未被占用（归还真），候选槽位 0 可写
    assert_eq!(ping.acquire_writable([true, true]), Some(0));
    ping.mark_committed();
    // 提交翻转后候选槽位变为 1
    assert_eq!(ping.acquire_writable([true, true]), Some(1));
    ping.mark_committed();
    // 候选槽位回到 0，但其缓冲仍被合成器持有：必须等待，不得改写槽位 1（更可能仍在被读取）
    assert_eq!(ping.acquire_writable([false, true]), None);
    // 两块都在途时同样必须等待
    assert_eq!(ping.acquire_writable([false, false]), None);
    // 槽位 0 收到 release 后恢复可写
    assert_eq!(ping.acquire_writable([true, false]), Some(0));
}

/// 缓冲区字节长度计算必须拒绝非法尺寸并防止算术溢出
#[test]
fn test_buffer_byte_len_guards() {
    assert_eq!(buffer_byte_len(4, 3).expect("合法尺寸必须成功"), 48);
    assert!(buffer_byte_len(0, 3).is_err(), "零宽度必须被拒绝");
    assert!(buffer_byte_len(3, 0).is_err(), "零高度必须被拒绝");
    assert!(buffer_byte_len(-1, 3).is_err(), "负尺寸必须被拒绝");
    assert!(
        buffer_byte_len(i32::MAX, i32::MAX).is_err(),
        "溢出尺寸必须被检查算术拦截而不 panic"
    );
}

/// 轮询超时必须安全截断到 poll 的 i32 毫秒范围。
#[test]
fn test_poll_timeout_saturates() {
    assert_eq!(duration_to_poll_timeout(Duration::ZERO), 0);
    assert_eq!(duration_to_poll_timeout(Duration::from_millis(25)), 25);
    assert_eq!(
        duration_to_poll_timeout(Duration::from_secs(u64::MAX)),
        i32::MAX
    );
}

/// Wayland 尺寸转换必须约束在 i32 范围内
#[test]
fn test_checked_wayland_size() {
    assert_eq!(
        checked_wayland_size(1920, 1080).expect("常规尺寸必须成功"),
        (1920, 1080)
    );
    assert!(checked_wayland_size(u32::MAX, 10).is_err());
    assert!(checked_wayland_size(10, u32::MAX).is_err());
}
