//! 贴图标注工具栏配置、坐标映射与工具栏点击分发。
//!
//! 贴图窗口复用截图工具栏的布局与渲染实现，主栏在绘制工具与撤销/重做之外，
//! 还提供整图 OCR、整图翻译以及复制、保存、关闭等贴图动作。

use super::jobs::pixmap_to_rgba_image;
use super::menu_actions::{copy_paster_to_clipboard, save_paster_to_disk};
use super::state::PasterWindowState;
use crate::snipper::toolbar::{Toolbar, ToolbarItem};
use winit::window::Window;

/// 贴图窗口主栏按钮序列：绘制工具、撤销/重做、整图识别/翻译与贴图动作。
///
/// 文字工具通过窗口 IME 会话输入，与截图侧共用同一套 [`TextInputSession`]；
/// 不包含截图专属的贴图（ActionPin）按钮。
/// 分隔线由 `metrics::has_separator_before` 按按钮语义自动插入，
/// 因此撤销与关闭动作之前各会有一条分隔线。
///
/// [`TextInputSession`]: crate::snipper::window::text_input::TextInputSession
pub const PASTER_MAIN_ITEMS: &[ToolbarItem] = &[
    ToolbarItem::ToolRect,
    ToolbarItem::ToolBadge,
    ToolbarItem::ToolArrow,
    ToolbarItem::ToolPen,
    ToolbarItem::ToolMarker,
    ToolbarItem::ToolMosaic,
    ToolbarItem::ToolText,
    ToolbarItem::ActionUndo,
    ToolbarItem::ActionRedo,
    ToolbarItem::ActionOcr,
    ToolbarItem::ActionTranslate,
    ToolbarItem::ActionMouseThrough,
    ToolbarItem::ActionClose,
    ToolbarItem::ActionSave,
    ToolbarItem::ActionCopy,
];

/// 几何工具属性栏的保守预留宽度（界面像素）。
///
/// 窗口扩容时按此宽度为二级属性栏留出横向空间；当前主栏 13 个按钮的总宽更大，
/// 实际窗口宽度通常仍由主栏决定。抽成常量是为了以后增删属性控件时只需调整一处。
const SUB_BAR_MAX_WIDTH: f32 = 400.0;

/// 二级属性栏在工具条上方占用的高度（面板 36 + 与主栏间距 4，界面像素）。
///
/// 主栏必须下移这段距离，属性栏才能从主栏上方展开而不遮挡缩放后的图像。
const SUB_BAR_HEIGHT: f32 = 36.0;

/// 二级属性栏与主栏之间的间距（界面像素）
const SUB_BAR_GAP: f32 = 4.0;

/// 创建贴图专用工具栏。
pub(crate) fn create_paster_toolbar() -> Toolbar {
    let mut toolbar = Toolbar::default();
    toolbar.main_items = PASTER_MAIN_ITEMS;
    toolbar
}

/// 计算显示标注工具条时窗口需要的最小宽度与图像下方的额外高度。
///
/// 返回值中的宽度是窗口整体最小宽度（图像较窄时会据此扩宽窗口），
/// 高度只是「图像底部以下」所需的增量，由调用方叠加到缩放后的图像高度上。
/// 宽度需同时容纳主栏与二级属性栏，高度需覆盖属性栏预留区、主栏本身与上下边距。
///
/// 抽成纯函数既方便对多种界面倍率做单元测试，也避免布局与窗口扩容各自维护一套常量。
pub(crate) fn toolbar_required_extent(ui_scale: f32, main_w: f32, main_h: f32) -> (f32, f32) {
    let ui_scale = ui_scale.max(0.1);
    let margin = 8.0 * ui_scale;
    let width = (main_w + margin * 2.0).max(SUB_BAR_MAX_WIDTH * ui_scale + margin * 2.0);
    // 高度在工具条下方预留二级栏空间：主栏紧贴图像（间距仅一个 margin），
    // 属性栏需要时浮在主栏上方（与截图一致），不会把工具条整体推远。
    let height = margin + main_h + SUB_BAR_GAP * ui_scale + SUB_BAR_HEIGHT * ui_scale + margin;
    (width, height)
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
/// 工具条锚定在「缩放后的图像」正下方：`x` 固定为左边距，`y` 为图像底边加上
/// 二级属性栏预留高度。贴图窗口会按 [`toolbar_required_extent`] 扩宽并加高，
/// 因此小贴图上的工具条不会再被窗口边缘裁掉；透明扩容区域不会渲染任何像素。
///
/// 始终左对齐：二级属性栏从主栏左端向右展开，靠左摆放能为属性栏与下拉菜单
/// 留出最大可用空间；窗口宽度不足时由窗口扩容兜底，而不是把工具条压缩或右移。
pub(crate) fn refresh_toolbar_layout(
    state: &mut PasterWindowState,
    window_width: f32,
    window_height: f32,
) {
    let ui_scale = state.ui_scale.max(0.1);
    state.toolbar.ui_scale = ui_scale;

    let scale = state.item.transform.scale.max(f32::EPSILON);
    let scaled_h = (state.item.pixmap.height() as f32 * scale).max(1.0);
    let margin = 8.0 * ui_scale;
    let bar_x = margin;
    // 工具条紧贴图像下边缘，只留一个 margin；二级栏需要时浮在主栏上方
    let bar_y = scaled_h + margin;

    // 先更新窗口边界，place_main_bar 内部的属性栏布局才能按正确的屏幕尺寸避让
    state
        .toolbar
        .set_screen_bounds(window_width.max(1.0), window_height.max(1.0));
    state.toolbar.place_main_bar(bar_x, bar_y);
}

/// 检测并处理贴图标注工具栏的点击，返回是否已消费本次点击。
///
/// 命中范围包含主栏、二级属性栏与展开的下拉菜单；绘制工具由工具栏内部切换激活状态，
/// 撤销/重做、整图识别/翻译、复制、保存、关闭等动作在此分发。
/// 识别与翻译任务异步执行，提交成功后立即返回，结果由工作区事件循环回填。
pub(crate) fn handle_toolbar_click(
    state: &mut PasterWindowState,
    window_size: (f32, f32),
    window: &Window,
) -> bool {
    if !state.show_toolbar {
        return false;
    }
    refresh_toolbar_layout(state, window_size.0, window_size.1);

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
            ToolbarItem::ActionOcr => submit_paster_ocr(state),
            ToolbarItem::ActionTranslate => submit_paster_translate(state),
            ToolbarItem::ActionMouseThrough => {
                state.set_mouse_through(!state.mouse_through, window);
            }
            ToolbarItem::ActionCopy => {
                copy_paster_to_clipboard(state);
            }
            ToolbarItem::ActionSave => save_paster_to_disk(state),
            ToolbarItem::ActionClose => state.should_close = true,
            _ => {}
        }
    }
    true
}

/// 提交整图文字识别任务；已有任务或执行器不可用时只更新顶部提示。
fn submit_paster_ocr(state: &mut PasterWindowState) {
    if state.job_pending {
        state.set_status_message("请等待当前任务完成");
        return;
    }
    let Some(executor) = state.job_executor.clone() else {
        state.set_status_message("后台任务执行器不可用");
        return;
    };
    let image = match pixmap_to_rgba_image(&state.item.pixmap) {
        Ok(image) => image,
        Err(message) => {
            state.set_status_message(format!("读取贴图像素失败: {message}"));
            return;
        }
    };

    match executor.submit_ocr(image) {
        Ok(()) => {
            state.job_pending = true;
            state.set_status_message("正在识别文字...");
        }
        Err(message) => state.set_status_message(format!("提交文字识别任务失败: {message}")),
    }
}

/// 提交整图翻译任务；目标语言与背景覆盖模式取自当前配置。
fn submit_paster_translate(state: &mut PasterWindowState) {
    if state.job_pending {
        state.set_status_message("请等待当前任务完成");
        return;
    }
    let Some(executor) = state.job_executor.clone() else {
        state.set_status_message("后台任务执行器不可用");
        return;
    };
    let image = match pixmap_to_rgba_image(&state.item.pixmap) {
        Ok(image) => image,
        Err(message) => {
            state.set_status_message(format!("读取贴图像素失败: {message}"));
            return;
        }
    };

    match executor.submit_translate(image, state.config.clone()) {
        Ok(()) => {
            state.job_pending = true;
            state.set_status_message("正在翻译文字...");
        }
        Err(message) => state.set_status_message(format!("提交翻译任务失败: {message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paster::manager::PasterItem;
    use snapin_core::annotation::shapes::TranslationShapeItem;
    use snapin_core::annotation::AnnotationShape;
    use snapin_core::config::CoverMode;
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

    /// 贴图主栏必须包含复制、保存、关闭与整图识别/翻译动作，且不得混入截图专属按钮
    #[test]
    fn 贴图主栏包含动作按钮且排除截图专属项() {
        let expected = [
            ToolbarItem::ToolRect,
            ToolbarItem::ToolBadge,
            ToolbarItem::ToolArrow,
            ToolbarItem::ToolPen,
            ToolbarItem::ToolMarker,
            ToolbarItem::ToolMosaic,
            ToolbarItem::ToolText,
            ToolbarItem::ActionUndo,
            ToolbarItem::ActionRedo,
            ToolbarItem::ActionOcr,
            ToolbarItem::ActionTranslate,
            ToolbarItem::ActionMouseThrough,
            ToolbarItem::ActionClose,
            ToolbarItem::ActionSave,
            ToolbarItem::ActionCopy,
        ];
        assert_eq!(PASTER_MAIN_ITEMS, &expected);
        assert!(
            PASTER_MAIN_ITEMS
                .windows(2)
                .any(|pair| { pair == [ToolbarItem::ToolMosaic, ToolbarItem::ToolText] }),
            "文字工具必须紧跟马赛克按钮"
        );
        assert!(
            PASTER_MAIN_ITEMS
                .windows(2)
                .any(|pair| { pair == [ToolbarItem::ToolText, ToolbarItem::ActionUndo] }),
            "文字工具必须位于撤销按钮之前"
        );
        assert!(
            PASTER_MAIN_ITEMS
                .windows(2)
                .any(|pair| { pair == [ToolbarItem::ActionRedo, ToolbarItem::ActionOcr] }),
            "文字识别按钮必须紧跟重做按钮"
        );
        assert!(
            PASTER_MAIN_ITEMS
                .windows(2)
                .any(|pair| { pair == [ToolbarItem::ActionOcr, ToolbarItem::ActionTranslate] }),
            "翻译按钮必须紧跟文字识别按钮"
        );

        for action in [
            ToolbarItem::ActionOcr,
            ToolbarItem::ActionTranslate,
            ToolbarItem::ActionMouseThrough,
            ToolbarItem::ActionClose,
            ToolbarItem::ActionSave,
            ToolbarItem::ActionCopy,
        ] {
            assert!(
                PASTER_MAIN_ITEMS.contains(&action),
                "主栏应包含动作按钮 {action:?}"
            );
        }
        assert!(
            !PASTER_MAIN_ITEMS.contains(&ToolbarItem::ActionPin),
            "贴图窗口不应出现“贴图置顶”按钮"
        );
        assert!(
            PASTER_MAIN_ITEMS.contains(&ToolbarItem::ToolText),
            "贴图主栏必须提供文字标注工具"
        );
    }

    /// 注入翻译图元后历史长度增加、展示图像被修改，撤销后恢复为原始底图
    #[test]
    fn 翻译结果注入可从底图撤销() {
        let mut state = test_state(120, 80);
        let base_pixels = state.base_pixmap.data().to_vec();
        let original_item_pixels = state.item.pixmap.data().to_vec();
        assert_eq!(
            original_item_pixels, base_pixels,
            "创建贴图时展示图像应与底图一致"
        );

        let items = vec![
            TranslationShapeItem {
                rect: (10.0, 10.0, 60.0, 24.0),
                original: "Hello".to_string(),
                translated: "你好".to_string(),
                bg_color: (255, 0, 0, 255),
                text_color: (0, 0, 0, 255),
                font_size: 14.0,
                cover_mode: CoverMode::SolidColor,
            },
            TranslationShapeItem {
                rect: (10.0, 44.0, 80.0, 24.0),
                original: "World".to_string(),
                translated: "世界".to_string(),
                bg_color: (0, 80, 255, 255),
                text_color: (255, 255, 255, 255),
                font_size: 14.0,
                cover_mode: CoverMode::SolidColor,
            },
        ];

        let injected = state.apply_translation_items(items);
        assert_eq!(injected, 2, "应报告注入了两段翻译");
        assert_eq!(
            state.annotations.undo_stack().len(),
            1,
            "整图翻译应作为一条复合标注进入历史"
        );
        assert_ne!(
            state.item.pixmap.data(),
            base_pixels.as_slice(),
            "翻译注入后展示图像必须与底图不同"
        );

        assert!(state.undo_annotation(), "翻译标注应支持撤销");
        assert_eq!(state.annotations.undo_stack().len(), 0);
        assert_eq!(
            state.item.pixmap.data(),
            base_pixels.as_slice(),
            "撤销翻译后必须精确恢复底图"
        );
    }

    /// 工具条所需尺寸在 1.0 与 1.67 倍率下按界面像素等比缩放
    #[test]
    fn 工具条所需尺寸随界面倍率缩放() {
        let mut toolbar = create_paster_toolbar();
        let mut baseline: Option<(f32, f32)> = None;
        for scale in [1.0f32, 1.67] {
            toolbar.ui_scale = scale;
            let main_w = toolbar.main_width();
            let main_h = toolbar.main_height();
            let (width, height) = toolbar_required_extent(scale, main_w, main_h);

            assert!(width >= main_w, "扩窗宽度必须容纳主栏：{width} < {main_w}");
            assert!(
                height >= main_h,
                "扩窗高度必须容纳主栏：{height} < {main_h}"
            );

            match baseline {
                None => baseline = Some((width, height)),
                Some((base_w, base_h)) => {
                    let ratio = scale / 1.0;
                    assert!(
                        (width / base_w - ratio).abs() < 0.01,
                        "宽度应按界面倍率线性缩放：{width} / {base_w} 不等于 {ratio}"
                    );
                    assert!(
                        (height / base_h - ratio).abs() < 0.01,
                        "高度应按界面倍率线性缩放：{height} / {base_h} 不等于 {ratio}"
                    );
                }
            }
        }

        // 主栏较窄时窗口宽度由属性栏预留宽度兜底
        assert_eq!(toolbar_required_extent(1.0, 100.0, 40.0), (416.0, 96.0));
    }

    /// 小贴图上工具条紧贴图像下方（仅 8 像素间距），属性栏浮在主栏上方
    #[test]
    fn 工具条锚定在图像下方() {
        let mut state = test_state(140, 80);
        state.ui_scale = 1.0;
        state.show_toolbar = true;
        state.toolbar.active_tool = Some(ToolbarItem::ToolRect);

        refresh_toolbar_layout(&mut state, 508.0, 180.0);

        assert_eq!(state.toolbar.main_rect.x, 8.0);
        assert_eq!(
            state.toolbar.main_rect.y, 88.0,
            "主栏应紧贴图像底边下方 8 像素"
        );
        let sub = state.toolbar.sub_rect.expect("矩形工具应展开属性栏");
        assert!(
            sub.y + sub.height <= state.toolbar.main_rect.y + 0.01,
            "属性栏应位于主栏上方，实际 y: {} 高: {}",
            sub.y,
            sub.height
        );
        assert!(sub.y >= 0.0, "属性栏不得超出窗口顶部，实际 y: {}", sub.y);
        assert!(
            state.toolbar.main_rect.y + state.toolbar.main_rect.height <= 180.0 - 8.0,
            "主栏底边应留出 8 像素下边距"
        );
    }
}
