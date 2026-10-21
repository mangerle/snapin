//! 贴图标注工具栏配置、坐标映射与工具栏点击分发。
//!
//! 贴图窗口复用截图工具栏的布局与渲染实现，但主栏只保留绘制类工具与撤销/重做，
//! 避免把截图专属的 OCR、翻译、保存、关闭等动作带进贴图场景。

use super::state::PasterWindowState;
use crate::snipper::selection::Rect;
use crate::snipper::toolbar::{Toolbar, ToolbarItem};

/// 贴图窗口主栏按钮序列：形状、序号、箭头、画笔、荧光笔、马赛克、撤销、重做。
///
/// 不包含文字工具（依赖 IME 输入，需单独实现）以及截图专属的 OCR、翻译、贴图、保存、复制、关闭。
/// 分隔线由 `metrics::has_separator_before` 按按钮语义自动插入，因此撤销/重做前会有一条分隔线。
pub const PASTER_MAIN_ITEMS: &[ToolbarItem] = &[
    ToolbarItem::ToolRect,
    ToolbarItem::ToolBadge,
    ToolbarItem::ToolArrow,
    ToolbarItem::ToolPen,
    ToolbarItem::ToolMarker,
    ToolbarItem::ToolMosaic,
    ToolbarItem::ActionUndo,
    ToolbarItem::ActionRedo,
];

/// 创建贴图专用工具栏。
pub(crate) fn create_paster_toolbar() -> Toolbar {
    let mut toolbar = Toolbar::default();
    toolbar.main_items = PASTER_MAIN_ITEMS;
    toolbar
}

/// 将窗口屏幕坐标换算为图像像素坐标，并夹取到图像范围内。
///
/// 前提假设：贴图图像从窗口客户区左上角 (0, 0) 开始，按 `transform.scale` 等比缩放绘制；
/// 边框绘制在图像内部、不占用额外布局。因此屏幕坐标除以缩放倍数即可得到图像像素坐标，
/// 超出图像边界的光标位置必须夹取，保证笔迹不会落在图像之外。
pub(crate) fn screen_to_image_point(
    pos: (f32, f32),
    scale: f32,
    width: u32,
    height: u32,
) -> (f32, f32) {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    (
        (pos.0 / scale).clamp(0.0, width as f32),
        (pos.1 / scale).clamp(0.0, height as f32),
    )
}

/// 将图像像素坐标换算为窗口屏幕坐标。
///
/// 与 `screen_to_image_point` 使用同一缩放假设，供标注预览按屏幕坐标绘制。
pub(crate) fn image_to_screen_point(pos: (f32, f32), scale: f32) -> (f32, f32) {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    (pos.0 * scale, pos.1 * scale)
}

/// 判断屏幕坐标是否落在缩放后的贴图图像内部。
pub(crate) fn is_point_inside_image(pos: (f32, f32), scale: f32, width: u32, height: u32) -> bool {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    let scaled_w = width as f32 * scale;
    let scaled_h = height as f32 * scale;
    pos.0 >= 0.0 && pos.1 >= 0.0 && pos.0 <= scaled_w && pos.1 <= scaled_h
}

/// 按当前图像尺寸、缩放与界面倍率刷新工具栏布局。
///
/// 工具栏锚定在「缩放后的图像」左下角内部：窗口可能因右键菜单被临时撑大，
/// 若按窗口尺寸摆放，工具条会漂移到图像之外的透明区域。左侧与底部各留 8 个界面像素。
///
/// 之所以选择左下而不是右下：二级属性栏固定从主栏左端向右展开，
/// 而贴图主栏按钮比截图精简、总宽度小于二级栏，靠右摆放会让色板等控件溢出图像被裁掉；
/// 靠左摆放能为向右展开的二级栏留出最大可用空间。
pub(crate) fn refresh_toolbar_layout(state: &mut PasterWindowState) {
    let ui_scale = state.ui_scale.max(0.1);
    state.toolbar.ui_scale = ui_scale;

    let scale = state.item.transform.scale.max(f32::EPSILON);
    let scaled_w = (state.item.pixmap.width() as f32 * scale).max(1.0);
    let scaled_h = (state.item.pixmap.height() as f32 * scale).max(1.0);
    let margin = 8.0 * ui_scale;
    let bar_w = state.toolbar.main_width();
    let bar_h = state.toolbar.main_height();

    // selection 的右边界即主栏左边界 + 主栏宽度，因此把 selection.x 设为左边距即可实现左对齐；
    // 底部则让主栏底边距图像底边 8 个界面像素。
    let selection = Rect {
        x: margin,
        y: 0.0,
        width: bar_w,
        height: (scaled_h - margin * 2.0 - bar_h).max(0.0),
    };
    state.toolbar.update_layout(&selection, scaled_w, scaled_h);
}

/// 检测并处理贴图标注工具栏的点击，返回是否已消费本次点击。
///
/// 命中范围包含主栏、二级属性栏与展开的下拉菜单；动作按钮目前只有撤销与重做，
/// 其余绘制工具由工具栏内部自行切换激活状态。
pub(crate) fn handle_toolbar_click(state: &mut PasterWindowState) -> bool {
    if !state.show_toolbar {
        return false;
    }
    refresh_toolbar_layout(state);

    let (cx, cy) = state.cursor_pos;
    if !state.toolbar.contains_point(cx, cy) {
        return false;
    }

    if let Some(item) = state.toolbar.handle_click(cx, cy) {
        match item {
            ToolbarItem::ActionUndo => {
                state.undo_annotation();
            }
            ToolbarItem::ActionRedo => {
                state.redo_annotation();
            }
            _ => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paster::manager::PasterItem;
    use snapin_core::annotation::AnnotationShape;
    use tiny_skia::{Color, Pixmap};

    /// 构造一张指定尺寸、内容全透明的测试贴图状态
    fn test_state(width: u32, height: u32) -> PasterWindowState {
        let pixmap = Pixmap::new(width, height).expect("创建测试位图失败");
        let item = PasterItem::new(1, "测试贴图".to_string(), pixmap, (0, 0));
        PasterWindowState::new(item)
    }

    /// 以矩形工具提交一笔填充矩形，返回提交后的可撤销图元
    fn commit_filled_rect(
        state: &mut PasterWindowState,
        start: (f32, f32),
        end: (f32, f32),
        color: Color,
    ) -> AnnotationShape {
        state.toolbar.active_tool = Some(ToolbarItem::ToolRect);
        state.toolbar.sub_options.filled = true;
        state.toolbar.sub_options.color = color;
        state.pending_stroke_points.clear();
        state.pending_stroke_points.push(start);
        state.pending_stroke_points.push(end);
        assert!(state.commit_pending_stroke(), "矩形标注应提交成功");
        state
            .annotations
            .undo_stack()
            .last()
            .cloned()
            .expect("提交后撤销栈应至少包含一个图元")
    }

    /// 缩放 1.0 与 1.6667 下屏幕坐标与图像坐标应正确互转
    #[test]
    fn 屏幕与图像坐标换算支持不同缩放() {
        assert_eq!(
            screen_to_image_point((100.0, 50.0), 1.0, 200, 100),
            (100.0, 50.0)
        );

        let (x, y) = screen_to_image_point((100.0, 100.0), 1.6667, 200, 100);
        assert!(
            (x - 59.9988).abs() < 0.01,
            "缩放 1.6667 时横坐标应约为 60，实际 {x}"
        );
        assert!(
            (y - 59.9988).abs() < 0.01,
            "缩放 1.6667 时纵坐标应约为 60，实际 {y}"
        );

        let (sx, sy) = image_to_screen_point((60.0, 30.0), 1.6667);
        assert!(
            (sx - 100.002).abs() < 0.01,
            "图像横坐标应按缩放放大，实际 {sx}"
        );
        assert!(
            (sy - 50.001).abs() < 0.01,
            "图像纵坐标应按缩放放大，实际 {sy}"
        );
    }

    /// 超出图像范围的屏幕坐标必须夹取到图像边界
    #[test]
    fn 坐标换算会夹取到图像边界() {
        assert_eq!(
            screen_to_image_point((-20.0, -10.0), 1.0, 200, 100),
            (0.0, 0.0)
        );
        assert_eq!(
            screen_to_image_point((900.0, 900.0), 1.0, 200, 100),
            (200.0, 100.0)
        );
        assert_eq!(
            screen_to_image_point((-5.0, 500.0), 1.6667, 200, 100),
            (0.0, 100.0)
        );
    }

    /// 图像内外判定应覆盖边界，并按缩放倍数换算宽高
    #[test]
    fn 图像内外判定遵循缩放后的边界() {
        assert!(is_point_inside_image((0.0, 0.0), 1.0, 200, 100));
        assert!(is_point_inside_image((200.0, 100.0), 1.0, 200, 100));
        assert!(!is_point_inside_image((200.5, 100.0), 1.0, 200, 100));
        assert!(is_point_inside_image((333.34, 166.67), 1.6667, 200, 100));
        assert!(!is_point_inside_image((333.35, 166.67), 1.6667, 200, 100));
    }

    /// 提交一笔后历史长度增加、图像像素被修改，而底图保持原始内容
    #[test]
    fn 提交一笔会写入图像并记录历史() {
        let mut state = test_state(100, 80);
        let original = state.item.pixmap.data().to_vec();

        state.toolbar.active_tool = Some(ToolbarItem::ToolPen);
        state.toolbar.sub_options.custom_stroke_width = 4.0;
        assert!(state.begin_stroke((10.0, 10.0)));
        assert!(state.update_stroke((80.0, 60.0)));
        assert!(state.commit_pending_stroke());

        assert_eq!(
            state.annotations.undo_stack().len(),
            1,
            "提交一笔后撤销栈长度应为 1"
        );
        assert_ne!(
            state.item.pixmap.data(),
            original.as_slice(),
            "提交一笔后展示图像必须被修改"
        );
        assert_eq!(
            state.base_pixmap.data(),
            original.as_slice(),
            "底图必须保持创建贴图时的原始像素"
        );
        assert!(
            state.pending_stroke_points.is_empty(),
            "提交后待提交点集必须清空"
        );
    }

    /// 画两笔后撤销一次，图像应与「只画第一笔」的重建结果逐像素一致；重做后恢复两笔
    #[test]
    fn 撤销重做按底图重放标注() {
        let mut state = test_state(64, 64);

        let first_shape = commit_filled_rect(
            &mut state,
            (4.0, 4.0),
            (24.0, 24.0),
            Color::from_rgba8(239, 68, 68, 255),
        );
        commit_filled_rect(
            &mut state,
            (32.0, 32.0),
            (56.0, 56.0),
            Color::from_rgba8(59, 130, 246, 255),
        );
        let two_stroke_pixels = state.item.pixmap.data().to_vec();
        assert_eq!(state.annotations.undo_stack().len(), 2);

        assert!(state.undo_annotation(), "存在两笔标注时应允许撤销");
        assert_eq!(state.annotations.undo_stack().len(), 1);

        // 期望图：透明底图上仅重放第一笔
        let mut expected = Pixmap::new(64, 64).expect("创建期望位图失败");
        first_shape.draw(&mut expected.as_mut());
        assert_eq!(
            state.item.pixmap.data(),
            expected.data(),
            "撤销后图像应等价于只画第一笔"
        );

        assert!(state.redo_annotation(), "存在被撤销的标注时应允许重做");
        assert_eq!(state.annotations.undo_stack().len(), 2);
        assert_eq!(
            state.item.pixmap.data(),
            two_stroke_pixels.as_slice(),
            "重做后图像应恢复两笔标注"
        );
    }
}
