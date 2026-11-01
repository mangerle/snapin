//! 截图画布与贴图窗口共用的顶部状态提示胶囊。
//!
//! 两个窗口的可用宽度、内边距与配色略有差异，但布局、圆角、描边与文本绘制
//! 必须保持同一套几何规则，否则同一句提示在截图与贴图中会出现视觉漂移。

use crate::drawing::add_round_rect;
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{FillRule, PathBuilder, PixmapMut, Stroke, Transform};

/// 状态提示胶囊的视觉与布局参数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct StatusBadgeStyle {
    /// 最小界面缩放因子
    pub(crate) min_scale: f32,
    /// 水平内边距（界面像素）
    pub(crate) padding_x: f32,
    /// 垂直内边距（界面像素）
    pub(crate) padding_y: f32,
    /// 与窗口顶部的距离（界面像素）
    pub(crate) top_margin: f32,
    /// ASCII 字符估算宽度
    pub(crate) ascii_char_width: f32,
    /// 非 ASCII 字符估算宽度
    pub(crate) wide_char_width: f32,
    /// 可用宽度扣除的左右边距（界面像素）
    pub(crate) available_margin: f32,
    /// 可用宽度下限系数
    pub(crate) min_available_factor: f32,
    /// 可用宽度绝对下限
    pub(crate) min_available_floor: f32,
    /// 水平位置绝对下限系数
    pub(crate) min_x_factor: f32,
    /// 胶囊背景色
    pub(crate) fill: (u8, u8, u8, u8),
    /// 胶囊描边色
    pub(crate) border: (u8, u8, u8, u8),
    /// 提示文字色
    pub(crate) text: (u8, u8, u8, u8),
}

/// 截图全屏画布使用的提示样式。
pub(crate) const SNIPPER_STATUS_STYLE: StatusBadgeStyle = StatusBadgeStyle {
    min_scale: 0.5,
    padding_x: 14.0,
    padding_y: 8.0,
    top_margin: 16.0,
    ascii_char_width: 7.5,
    wide_char_width: 13.0,
    available_margin: 16.0,
    min_available_factor: 120.0,
    min_available_floor: 0.0,
    min_x_factor: 8.0,
    fill: (28, 30, 38, 238),
    border: (255, 255, 255, 36),
    text: (240, 243, 250, 255),
};

/// 贴图窗口使用的提示样式。
pub(crate) const PASTER_STATUS_STYLE: StatusBadgeStyle = StatusBadgeStyle {
    min_scale: 0.1,
    padding_x: 12.0,
    padding_y: 7.0,
    top_margin: 12.0,
    ascii_char_width: 7.0,
    wide_char_width: 13.0,
    available_margin: 16.0,
    min_available_factor: 0.0,
    min_available_floor: 1.0,
    min_x_factor: 0.0,
    fill: (20, 22, 28, 210),
    border: (255, 255, 255, 36),
    text: (240, 243, 250, 255),
};

/// 状态提示胶囊的布局结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StatusBadgeLayout {
    /// 实际缩放因子
    pub(crate) scale: f32,
    /// 胶囊左上角 X
    pub(crate) box_x: f32,
    /// 胶囊左上角 Y
    pub(crate) box_y: f32,
    /// 胶囊宽度
    pub(crate) box_w: f32,
    /// 胶囊高度
    pub(crate) box_h: f32,
    /// 水平文字内边距
    pub(crate) padding_x: f32,
    /// 垂直文字内边距
    pub(crate) padding_y: f32,
    /// 文字字号
    pub(crate) font_size: f32,
}

/// 按可用宽度与样式计算胶囊布局。
pub(crate) fn layout(
    message: &str,
    available_width: f32,
    ui_scale: f32,
    style: StatusBadgeStyle,
) -> StatusBadgeLayout {
    let scale = ui_scale.max(style.min_scale);
    let font_size = 13.0 * scale;
    let padding_x = style.padding_x * scale;
    let padding_y = style.padding_y * scale;
    let estimated_width = message
        .chars()
        .map(|ch| {
            if ch.is_ascii() {
                style.ascii_char_width
            } else {
                style.wide_char_width
            }
        })
        .sum::<f32>()
        * scale;
    let available = (available_width - style.available_margin * scale)
        .max(style.min_available_factor * scale)
        .max(style.min_available_floor);
    let box_width = (estimated_width + padding_x * 2.0).min(available);
    let box_height = font_size * 1.4 + padding_y * 2.0;
    let box_x = ((available_width - box_width) / 2.0).max(style.min_x_factor * scale);

    StatusBadgeLayout {
        scale,
        box_x,
        box_y: style.top_margin * scale,
        box_w: box_width,
        box_h: box_height,
        padding_x,
        padding_y,
        font_size,
    }
}

/// 将状态提示胶囊绘制到目标位图。
pub(crate) fn render(
    pixmap: &mut PixmapMut,
    message: &str,
    layout: &StatusBadgeLayout,
    style: StatusBadgeStyle,
) {
    if message.is_empty() || layout.box_w <= 0.0 || layout.box_h <= 0.0 {
        return;
    }
    let radius = 6.0 * layout.scale;
    let mut path = PathBuilder::new();
    add_round_rect(
        &mut path,
        layout.box_x,
        layout.box_y,
        layout.box_w,
        layout.box_h,
        radius,
    );
    if let Some(path) = path.finish() {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(style.fill.0, style.fill.1, style.fill.2, style.fill.3);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut border = tiny_skia::Paint::default();
        border.set_color_rgba8(
            style.border.0,
            style.border.1,
            style.border.2,
            style.border.3,
        );
        let stroke = Stroke {
            width: layout.scale,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }

    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            message,
            layout.box_x + layout.padding_x,
            layout.box_y + layout.padding_y,
            layout.font_size,
            style.text,
        );
    });
}

/// 状态提示框外扩 1 像素的脏区矩形。
pub(crate) fn dirty_rect(layout: &StatusBadgeLayout) -> crate::geometry::Rect {
    let margin = layout.scale;
    crate::geometry::Rect {
        x: layout.box_x - margin,
        y: layout.box_y - margin,
        width: layout.box_w + margin * 2.0,
        height: layout.box_h + margin * 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 窄窗口下胶囊不得超出可用宽度，且保持居中。
    #[test]
    fn test_layout_clamps_to_available_width() {
        let layout = layout("一段很长的状态提示文字", 100.0, 1.0, PASTER_STATUS_STYLE);
        assert!(layout.box_w <= 100.0);
        assert!(layout.box_x >= 0.0);
        assert!(layout.box_w > 0.0);
    }

    /// 空消息的绘制函数必须安全返回。
    #[test]
    fn test_render_ignores_empty_message() {
        let mut pixmap = tiny_skia::Pixmap::new(32, 32).expect("创建测试位图失败");
        let layout = layout("", 32.0, 1.0, SNIPPER_STATUS_STYLE);
        render(&mut pixmap.as_mut(), "", &layout, SNIPPER_STATUS_STYLE);
    }
}
