
use super::*;
use image::Rgba;
use tiny_skia::Color;

#[test]
fn test_create_dimmed_pixmap_fast_algorithm() {
    let mut img = RgbaImage::new(2, 2);
    img.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
    img.put_pixel(1, 1, Rgba([100, 100, 100, 200]));

    let dimmed = create_dimmed_pixmap_fast(&img).expect("定点暗化生成失败");
    assert_eq!(dimmed.width(), 2);
    assert_eq!(dimmed.height(), 2);

    let data = dimmed.data();
    // 验证纯白像素暗化结果：(255 * 107) >> 8 = 27285 >> 8 = 106
    assert_eq!(data[0], 106);
    assert_eq!(data[1], 106);
    assert_eq!(data[2], 106);
    assert_eq!(data[3], 255);
}

/// 脏区包围盒必须向外取整并裁剪到画布范围
#[test]
fn test_covering_pixel_bounds() {
    let rect = Rect {
        x: 10.4,
        y: 20.7,
        width: 30.2,
        height: 5.1,
    };
    assert_eq!(
        covering_pixel_bounds(&rect, 100, 100),
        Some((10, 20, 31, 6))
    );

    // 完全越界或零面积返回 None
    assert_eq!(
        covering_pixel_bounds(
            &Rect {
                x: -50.0,
                y: -50.0,
                width: 10.0,
                height: 10.0,
            },
            100,
            100,
        ),
        None
    );
    assert_eq!(
        covering_pixel_bounds(
            &Rect {
                x: 99.0,
                y: 99.0,
                width: 50.0,
                height: 50.0,
            },
            100,
            100,
        ),
        Some((99, 99, 1, 1))
    );
}

/// 窗口尺寸变化后底图必须匹配新缓冲尺寸，否则渲染会因切片长度不一致 panic。
#[test]
fn test_scale_pixmap_to_target_size() {
    let mut source = Pixmap::new(4, 2).expect("创建源位图失败");
    source.fill(Color::from_rgba8(10, 20, 30, 255));
    let scaled = scale_pixmap_to(&source, 8, 4).expect("缩放应成功");
    assert_eq!((scaled.width(), scaled.height()), (8, 4));
}
