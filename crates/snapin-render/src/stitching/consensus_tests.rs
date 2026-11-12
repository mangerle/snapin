//! 2 级由粗到细金字塔、多通道色差边缘、全局轻度曝光容忍与 ZNCC 多频带结构共识测试。

use super::matcher::estimate_shift;
use super::types::{StitchDirection, StitchOptions};
use image::{Rgba, RgbaImage};

/// 生成拟真桌面文档界面（含固定顶栏/底栏、语法高亮彩色短代码块、细分割线与分页文本行）。
fn render_realistic_code_doc(width: u32, height: u32, scroll_y: u32, luma_bias: i16) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([28, 31, 38, 255]));
    let header = 32u32;
    let footer = 24u32;

    for y in 0..height {
        if y < header {
            let tab = if (y / 8).is_multiple_of(2) { 14 } else { 0 };
            for x in 0..width {
                let btn = (((x / 36) * 23) % 45) as u8;
                img.put_pixel(x, y, Rgba([42 + btn, 46 + tab, 58, 255]));
            }
            continue;
        }
        if y >= height - footer {
            let rel = (y - (height - footer)) as u8;
            for x in 0..width {
                let seg = if ((x / 40) % 2) == 0 { 32 } else { 0 };
                img.put_pixel(x, y, Rgba([34 + seg, 40 + rel * 2, 72, 255]));
            }
            continue;
        }

        let doc_y = y + scroll_y;
        let line_idx = doc_y / 22;
        let in_line = doc_y % 22;
        for x in 0..width {
            let mut r = 28i16;
            let mut g = 31i16;
            let mut b = 38i16;

            // 每隔一行淡淡的斑马底色 + 细腻语法高亮代码块。
            if line_idx.is_multiple_of(2) {
                r += 4;
                g += 5;
                b += 6;
            }
            if in_line == 21 {
                r += 16;
                g += 18;
                b += 22;
            } else if (5..15).contains(&in_line) && x >= 48 && x < width - 48 {
                let indent = ((line_idx * 17) % 5) * 16;
                let span = 90 + ((line_idx * 53) % (width - 180));
                if x >= 48 + indent && x < (48 + indent + span).min(width - 48) {
                    let seed = (line_idx * 131 + (x / 6) * 19 + in_line * 7) % 100;
                    if seed < 65 {
                        // 用行号决定彩色关键字 / 字符串 / 普通变量色调。
                        match (line_idx + x / 28) % 3 {
                            0 => (r, g, b) = (110, 175, 245),
                            1 => (r, g, b) = (155, 210, 125),
                            _ => (r, g, b) = (225, 165, 110),
                        }
                    }
                }
            }
            let rr = (r + luma_bias).clamp(0, 255) as u8;
            let gg = (g + luma_bias).clamp(0, 255) as u8;
            let bb = (b + luma_bias).clamp(0, 255) as u8;
            img.put_pixel(x, y, Rgba([rr, gg, bb, 255]));
        }
    }
    img
}

#[test]
fn test_pyramid_large_viewport_realistic_ui_scroll_and_brightness_shift() {
    // 480×860 拟真代码编辑器界面：单次大步长 +410px（进入 2 级粗金字塔）并伴随全局 -6 轻微调暗。
    let width = 480u32;
    let height = 860u32;
    let options = StitchOptions::default();

    let f0 = render_realistic_code_doc(width, height, 0, 0);
    let f1 = render_realistic_code_doc(width, height, 410, 0);
    let res1 = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(
        res1.is_matched(options.confidence_threshold),
        "2 级金字塔应当准确完成大屏幕大幅度对齐: {res1:?}"
    );
    assert_eq!(res1.offset, 410);
    assert!(res1.sticky_header_size >= 28);
    assert!(res1.sticky_footer_size >= 20);

    // 全局亮度轻微调暗 -6 后进行向上 -270px 回滚。
    let f2_dim = render_realistic_code_doc(width, height, 140, -6);
    let res2 = estimate_shift(&f1, &f2_dim, StitchDirection::Vertical, &options);
    assert!(
        res2.is_matched(options.confidence_threshold),
        "含全局轻度亮度漂移的大幅度回滚应通过 ZNCC 共识: {res2:?}"
    );
    assert_eq!(res2.offset, -270);
}

#[test]
fn test_isoluminant_colored_badges_preserve_structural_matching() {
    // 构造 Rec.601 灰度亮度严格相等（~125），仅靠 RGB 色度对立变化区分的彩色块序列。
    let width = 160u32;
    let height = 240u32;
    let red_iso = Rgba([218, 86, 86, 255]); // (218*77 + 86*150 + 86*29) >> 8 = 125
    let green_iso = Rgba([54, 152, 85, 255]); // (54*77 + 152*150 + 85*29) >> 8 = 125
    let blue_iso = Rgba([100, 128, 200, 255]); // ~125

    let build_iso = |scroll: u32| -> RgbaImage {
        let mut img = RgbaImage::from_pixel(width, height, blue_iso);
        for y in 0..height {
            let doc_y = y + scroll;
            let row = doc_y / 28;
            let in_row = doc_y % 28;
            if (6..22).contains(&in_row) {
                let bar_x0 = 24 + ((row * 29) % 50);
                let color = if (row + (row / 3)).is_multiple_of(2) {
                    red_iso
                } else {
                    green_iso
                };
                for x in bar_x0..(bar_x0 + 42).min(width - 20) {
                    img.put_pixel(x, y, color);
                }
            }
        }
        img
    };

    let options = StitchOptions::default();
    let f0 = build_iso(0);
    let f1 = build_iso(70);
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(
        res.is_matched(options.confidence_threshold),
        "同亮度彩色色块滚动不应因 Rec.601 灰度塌缩而失效: {res:?}"
    );
    assert_eq!(res.offset, 70);
}

#[test]
fn test_multi_band_zncc_rejects_partial_split_mismatch() {
    // 构造顶层 30% 低反差偶然一致但下方 70% 内容截然不同（如突然弹出整页弹窗或切屏）的场景。
    let width = 220u32;
    let height = 300u32;
    let mut f0 = render_realistic_code_doc(width, height, 0, 0);
    let mut f1 = render_realistic_code_doc(width, height, 50, 0);

    // 令两帧上方小块（y=0..80）强行一致并掩盖大部分细节，但下方 y=90..300 为完全相反相位乱序卡片。
    for y in 0..90 {
        for x in 0..width {
            let px = *f0.get_pixel(x, y);
            f1.put_pixel(x, y, px);
        }
    }
    for y in 90..height {
        for x in 0..width {
            let v = if ((x / 9) + ((y * 3) / 11)) % 2 == 0 {
                235
            } else {
                25
            };
            f1.put_pixel(x, y, Rgba([v, 255 - v, v / 2, 255]));
            let v0 = if ((x / 17) + (y / 13)) % 2 == 0 {
                200
            } else {
                55
            };
            f0.put_pixel(x, y, Rgba([v0, v0, 255 - v0, 255]));
        }
    }

    let options = StitchOptions::default();
    let res = estimate_shift(&f0, &f1, StitchDirection::Vertical, &options);
    assert!(
        !res.is_matched(options.confidence_threshold),
        "跨频带结构发生切屏分裂时应被多频带 ZNCC 共识拒绝错拼: {res:?}"
    );
}
