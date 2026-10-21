//! 贴图窗口输入区域矩形计算。
//!
//! 透明扩窗后 Wayland 普通 surface 仍会整块参与输入命中，透明空白必须通过
//! `wl_region` 显式排除，否则会吞掉本应落到桌面的点击。本模块只负责把当前布局
//! 快照换算为整数矩形，不触碰任何协议对象，便于脱离合成器验证边界夹取、去重
//! 以及高 DPI 下的 surface-local 坐标换算。

use super::state::PasterWindowState;
use crate::paster::context_menu::ContextMenuState;
use crate::snipper::selection::Rect;

/// 右键菜单二级子菜单的面板宽度（界面像素），与菜单命中逻辑保持一致。
const CONTEXT_SUBMENU_WIDTH: f32 = 160.0;

/// 输入区域计算所需的布局快照，所有矩形均使用窗口物理像素坐标。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct InputRegionLayout {
    /// 窗口物理宽度
    pub window_w: f32,
    /// 窗口物理高度
    pub window_h: f32,
    /// 缩放后的图像宽度
    pub image_w: f32,
    /// 缩放后的图像高度
    pub image_h: f32,
    /// 工具条主栏面板（工具条显示时存在）
    pub toolbar_main: Option<Rect>,
    /// 工具条二级属性栏面板（已展开时存在）
    pub toolbar_sub: Option<Rect>,
    /// 工具条展开的下拉菜单面板
    pub toolbar_dropdown: Option<Rect>,
    /// 右键菜单主面板（已展开时存在）
    pub context_menu: Option<Rect>,
    /// 右键菜单二级子菜单面板（已展开时存在）
    pub context_submenu: Option<Rect>,
}

/// 计算窗口可交互区域矩形列表（窗口物理像素）。
///
/// 顺序固定为图像、主栏、属性栏、工具条下拉菜单、右键菜单、右键子菜单；
/// 每个矩形先按窗口边界夹取，再向上取整到像素边界，确保边缘按钮不会因取整
/// 而失去命中；完全落在窗口外的矩形被丢弃，重复矩形只保留一份。
pub(crate) fn compute_input_rects(layout: &InputRegionLayout) -> Vec<(i32, i32, i32, i32)> {
    let mut rects = Vec::with_capacity(6);
    let image = Rect {
        x: 0.0,
        y: 0.0,
        width: layout.image_w,
        height: layout.image_h,
    };
    push_clamped(&mut rects, image, layout.window_w, layout.window_h);

    let sources = [
        layout.toolbar_main,
        layout.toolbar_sub,
        layout.toolbar_dropdown,
        layout.context_menu,
        layout.context_submenu,
    ];
    for rect in sources.into_iter().flatten() {
        push_clamped(&mut rects, rect, layout.window_w, layout.window_h);
    }
    rects
}

/// 将一个布局矩形夹取到窗口边界并去重后追加到结果集。
fn push_clamped(rects: &mut Vec<(i32, i32, i32, i32)>, rect: Rect, window_w: f32, window_h: f32) {
    let Some(rect) = clamp_rect(rect, window_w, window_h) else {
        return;
    };
    if !rects.contains(&rect) {
        rects.push(rect);
    }
}

/// 将浮点布局矩形换算为窗口内整数像素矩形，完全越界或空尺寸时返回 `None`。
fn clamp_rect(rect: Rect, window_w: f32, window_h: f32) -> Option<(i32, i32, i32, i32)> {
    let max_w = window_w.max(0.0);
    let max_h = window_h.max(0.0);
    let left = rect.x.floor().clamp(0.0, max_w);
    let top = rect.y.floor().clamp(0.0, max_h);
    let right = (rect.x + rect.width).ceil().clamp(0.0, max_w);
    let bottom = (rect.y + rect.height).ceil().clamp(0.0, max_h);
    let width = (right - left).round() as i32;
    let height = (bottom - top).round() as i32;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((left as i32, top as i32, width, height))
}

/// 将窗口物理像素矩形换算为 Wayland surface-local 逻辑坐标。
///
/// winit 在 Wayland 下按窗口缩放因子设置 buffer scale（或使用 fractional-scale 视口），
/// `wl_region` 坐标属于 surface-local 逻辑坐标系；若直接提交物理像素，高 DPI 下
/// 区域会被放大到整个表面，透明空白依旧会吞掉点击。左/上边界向下取整、
/// 右/下边界向上取整，保证换算后完整覆盖原布局，不会因取整丢失边缘命中。
pub(crate) fn physical_rects_to_surface(
    rects: &[(i32, i32, i32, i32)],
    scale: f32,
) -> Vec<(i32, i32, i32, i32)> {
    let scale = scale.max(f32::EPSILON);
    let mut converted = Vec::with_capacity(rects.len());
    for &(x, y, width, height) in rects {
        if width <= 0 || height <= 0 {
            continue;
        }
        let left = (x as f32 / scale).floor() as i32;
        let top = (y as f32 / scale).floor() as i32;
        let right = (x.saturating_add(width) as f32 / scale).ceil() as i32;
        let bottom = (y.saturating_add(height) as f32 / scale).ceil() as i32;
        let rect = (left, top, right - left, bottom - top);
        if rect.2 > 0 && rect.3 > 0 && !converted.contains(&rect) {
            converted.push(rect);
        }
    }
    converted
}

/// 从当前窗口状态收集输入区域布局快照。
///
/// 工具条隐藏时忽略其残留布局，右键菜单关闭时同理，避免透明区域仍被声明为可交互。
pub(crate) fn input_layout_from_state(
    state: &PasterWindowState,
    window_w: f32,
    window_h: f32,
) -> InputRegionLayout {
    let scale = state.item.transform.scale.max(f32::EPSILON);
    let (toolbar_main, toolbar_sub, toolbar_dropdown) = if state.show_toolbar {
        (
            Some(state.toolbar.main_rect),
            state.toolbar.sub_rect,
            state.toolbar.dropdown_rect(),
        )
    } else {
        (None, None, None)
    };
    let (context_menu, context_submenu) = context_menu_rects(state);

    InputRegionLayout {
        window_w,
        window_h,
        image_w: state.item.pixmap.width() as f32 * scale,
        image_h: state.item.pixmap.height() as f32 * scale,
        toolbar_main,
        toolbar_sub,
        toolbar_dropdown,
        context_menu,
        context_submenu,
    }
}

/// 计算右键菜单主面板与二级子菜单面板的矩形。
fn context_menu_rects(state: &PasterWindowState) -> (Option<Rect>, Option<Rect>) {
    let menu = &state.context_menu;
    if !menu.is_open {
        return (None, None);
    }

    let main_items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
    let main_height =
        ContextMenuState::items_total_height(&main_items, menu.item_height, menu.ui_scale);
    let main = Rect {
        x: menu.x,
        y: menu.y,
        width: menu.width,
        height: main_height,
    };

    let submenu = menu.active_submenu.map(|(kind, x, y)| {
        let sub_items = ContextMenuState::build_sub_items(kind, state.smooth_scaling);
        let sub_height =
            ContextMenuState::items_total_height(&sub_items, menu.item_height, menu.ui_scale);
        Rect {
            x,
            y,
            width: CONTEXT_SUBMENU_WIDTH * menu.ui_scale,
            height: sub_height,
        }
    });
    (Some(main), submenu)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造宽度足够容纳工具条的小贴图窗口布局
    fn base_layout() -> InputRegionLayout {
        InputRegionLayout {
            window_w: 508.0,
            window_h: 180.0,
            image_w: 140.0,
            image_h: 80.0,
            ..Default::default()
        }
    }

    /// 仅图像时输入区域只覆盖图像本身
    #[test]
    fn 输入区域仅包含图像() {
        let rects = compute_input_rects(&base_layout());
        assert_eq!(rects, vec![(0, 0, 140, 80)]);
    }

    /// 图像加工具条主栏时，透明扩容区域不被声明为可交互
    #[test]
    fn 输入区域包含图像与工具条主栏() {
        let mut layout = base_layout();
        layout.toolbar_main = Some(Rect {
            x: 8.0,
            y: 128.0,
            width: 492.0,
            height: 44.0,
        });
        assert_eq!(
            compute_input_rects(&layout),
            vec![(0, 0, 140, 80), (8, 128, 492, 44)]
        );
    }

    /// 展开属性栏后，其面板也必须参与输入命中
    #[test]
    fn 输入区域包含展开的属性栏() {
        let mut layout = base_layout();
        layout.toolbar_main = Some(Rect {
            x: 8.0,
            y: 128.0,
            width: 492.0,
            height: 44.0,
        });
        layout.toolbar_sub = Some(Rect {
            x: 8.0,
            y: 88.0,
            width: 468.0,
            height: 36.0,
        });
        assert_eq!(
            compute_input_rects(&layout),
            vec![(0, 0, 140, 80), (8, 128, 492, 44), (8, 88, 468, 36)]
        );
    }

    /// 下拉菜单与右键菜单位于图像之上时同样需要保留输入
    #[test]
    fn 输入区域包含下拉菜单与右键菜单() {
        let mut layout = base_layout();
        layout.toolbar_main = Some(Rect {
            x: 8.0,
            y: 128.0,
            width: 492.0,
            height: 44.0,
        });
        layout.toolbar_dropdown = Some(Rect {
            x: 20.0,
            y: 8.0,
            width: 116.0,
            height: 92.0,
        });
        layout.context_menu = Some(Rect {
            x: 200.0,
            y: 40.0,
            width: 170.0,
            height: 120.0,
        });
        layout.context_submenu = Some(Rect {
            x: 340.0,
            y: 50.0,
            width: 160.0,
            height: 80.0,
        });
        assert_eq!(
            compute_input_rects(&layout),
            vec![
                (0, 0, 140, 80),
                (8, 128, 492, 44),
                (20, 8, 116, 92),
                (200, 40, 170, 120),
                (340, 50, 160, 80)
            ]
        );
    }

    /// 越界矩形被夹取到窗口边界，完全越界的被丢弃，夹取后重复的只保留一份
    #[test]
    fn 输入区域会夹取边界并去重() {
        let layout = InputRegionLayout {
            window_w: 200.0,
            window_h: 100.0,
            image_w: 220.0,
            image_h: 120.0,
            toolbar_main: Some(Rect {
                x: -10.0,
                y: 80.0,
                width: 50.0,
                height: 40.0,
            }),
            toolbar_sub: Some(Rect {
                x: -10.0,
                y: 80.0,
                width: 50.0,
                height: 40.0,
            }),
            toolbar_dropdown: Some(Rect {
                x: 300.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            }),
            ..Default::default()
        };
        assert_eq!(
            compute_input_rects(&layout),
            vec![(0, 0, 200, 100), (0, 80, 40, 20)]
        );
    }

    /// 物理像素矩形必须按缩放因子换算为 surface-local 逻辑坐标
    #[test]
    fn 物理像素矩形换算为表面逻辑坐标() {
        assert_eq!(
            physical_rects_to_surface(&[(0, 0, 100, 80), (10, 20, 30, 40)], 2.0),
            vec![(0, 0, 50, 40), (5, 10, 15, 20)]
        );
        // 分数缩放时边界分别向下/向上取整，保证覆盖原物理区域
        assert_eq!(
            physical_rects_to_surface(&[(0, 0, 100, 80)], 1.67),
            vec![(0, 0, 60, 48)]
        );
    }
}
