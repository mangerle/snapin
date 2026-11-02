//! 贴图右键上下文菜单的现代化毛玻璃视觉光栅化渲染。

use super::{ContextMenuState, MenuItem};
use crate::drawing::{add_round_rect, make_paint};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

pub use snapin_render::TextRenderer;

/// 菜单面板布局参数
#[derive(Debug, Clone, Copy)]
pub struct MenuLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub item_height: f32,
    pub scale: f32,
}

/// 绘制整个上下文菜单及其二级子菜单
pub fn render_context_menu(
    menu: &ContextMenuState,
    pixmap: &mut PixmapMut,
    text_renderer: &mut TextRenderer,
    show_toolbar: bool,
    has_shadow: bool,
    smooth_scaling: bool,
) {
    if !menu.is_open {
        return;
    }

    let s = menu.ui_scale;
    let main_items = ContextMenuState::build_main_items(show_toolbar, has_shadow);
    let main_layout = MenuLayout {
        x: menu.x,
        y: menu.y,
        width: menu.width,
        item_height: menu.item_height,
        scale: s,
    };

    // 1. 渲染主菜单面板
    render_menu_panel(
        pixmap,
        text_renderer,
        main_layout,
        &main_items,
        menu.hovered_main_idx,
    );

    // 2. 渲染二级子菜单面板 (若处于展开状态)
    if let Some((sub_type, sx, sy)) = menu.active_submenu {
        let sub_items = ContextMenuState::build_sub_items(sub_type, smooth_scaling);
        let sub_layout = MenuLayout {
            x: sx,
            y: sy,
            width: 160.0 * s,
            item_height: menu.item_height,
            scale: s,
        };
        render_menu_panel(
            pixmap,
            text_renderer,
            sub_layout,
            &sub_items,
            menu.hovered_sub_idx,
        );
    }
}

/// 渲染单个菜单面板列表
fn render_menu_panel(
    pixmap: &mut PixmapMut,
    text_renderer: &mut TextRenderer,
    layout: MenuLayout,
    items: &[MenuItem],
    hovered_idx: Option<usize>,
) {
    let s = layout.scale;
    let (y, item_height) = (layout.y, layout.item_height);
    let sep_h = 8.0 * s;
    let pad_v = 5.0 * s;
    let total_h = super::ContextMenuState::items_total_height(items, item_height, s);

    render_panel_surface(pixmap, layout, total_h);

    let mut cur_y = y + pad_v;
    for (idx, item) in items.iter().enumerate() {
        if item.is_separator {
            render_separator(pixmap, layout, cur_y);
            cur_y += sep_h;
            continue;
        }

        render_menu_row(
            pixmap,
            text_renderer,
            layout,
            MenuRowSpec {
                item,
                top: cur_y,
                hovered: hovered_idx == Some(idx),
            },
        );
        cur_y += item_height;
    }
}

/// 绘制菜单面板的柔影、白色底板与边框。
fn render_panel_surface(pixmap: &mut PixmapMut, layout: MenuLayout, total_h: f32) {
    let (x, y, width, s) = (layout.x, layout.y, layout.width, layout.scale);
    let mut pb_shadow = PathBuilder::new();
    add_round_rect(&mut pb_shadow, x, y + 2.0 * s, width, total_h, 8.0 * s);
    if let Some(path) = pb_shadow.finish() {
        let shadow_paint = make_paint(Color::from_rgba8(0, 0, 0, 36));
        pixmap.fill_path(
            &path,
            &shadow_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let mut pb_bg = PathBuilder::new();
    add_round_rect(&mut pb_bg, x, y, width, total_h, 8.0 * s);
    if let Some(path) = pb_bg.finish() {
        let bg_paint = make_paint(Color::from_rgba8(255, 255, 255, 252));
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        let stroke_paint = make_paint(Color::from_rgba8(218, 224, 233, 240));
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制菜单项之间的细分隔线。
fn render_separator(pixmap: &mut PixmapMut, layout: MenuLayout, top: f32) {
    let s = layout.scale;
    let mut builder = PathBuilder::new();
    builder.move_to(layout.x + 12.0 * s, top + 4.0 * s);
    builder.line_to(layout.x + layout.width - 12.0 * s, top + 4.0 * s);
    if let Some(path) = builder.finish() {
        let paint = make_paint(Color::from_rgba8(226, 232, 240, 220));
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 单个菜单行的渲染参数。
struct MenuRowSpec<'a> {
    item: &'a MenuItem,
    top: f32,
    hovered: bool,
}

/// 绘制单行悬停底色、复选框、标题与子菜单指示箭头。
fn render_menu_row(
    pixmap: &mut PixmapMut,
    text_renderer: &mut TextRenderer,
    layout: MenuLayout,
    row: MenuRowSpec<'_>,
) {
    let s = layout.scale;
    let item_height = layout.item_height;
    if row.hovered {
        let mut builder = PathBuilder::new();
        add_round_rect(
            &mut builder,
            layout.x + 5.0 * s,
            row.top + 2.0 * s,
            layout.width - 10.0 * s,
            item_height - 4.0 * s,
            5.0 * s,
        );
        if let Some(path) = builder.finish() {
            let paint = make_paint(Color::from_rgba8(59, 130, 246, 240));
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    let text_color = if row.hovered {
        Color::from_rgba8(255, 255, 255, 255)
    } else {
        Color::from_rgba8(30, 41, 59, 255)
    };
    let font_size = 13.0 * s;
    let text_y = row.top + (item_height - font_size * 1.25) / 2.0;
    let text_x = if row.item.is_checkbox {
        render_checkbox_mark(
            pixmap,
            CheckboxMarkSpec {
                x: layout.x + 12.0 * s,
                y: row.top + (item_height - 14.0 * s) / 2.0,
                checked: row.item.is_checked,
                hovered: row.hovered,
                scale: s,
            },
        );
        layout.x + 32.0 * s
    } else {
        layout.x + 14.0 * s
    };
    text_renderer.draw_text(
        pixmap,
        row.item.label,
        text_x,
        text_y,
        font_size,
        text_color,
    );

    if row.item.submenu.is_some() {
        render_arrow_mark(
            pixmap,
            ArrowMarkSpec {
                center_x: layout.x + layout.width - 16.0 * s,
                center_y: row.top + item_height / 2.0,
                hovered: row.hovered,
                scale: s,
            },
        );
    }
}

/// 复选框图标的位置、状态与缩放参数。
struct CheckboxMarkSpec {
    x: f32,
    y: f32,
    checked: bool,
    hovered: bool,
    scale: f32,
}

/// 绘制复选框小图标。
fn render_checkbox_mark(pixmap: &mut PixmapMut, spec: CheckboxMarkSpec) {
    let (x, y, s) = (spec.x, spec.y, spec.scale);
    let size = 14.0 * s;
    let mut pb = PathBuilder::new();
    add_round_rect(&mut pb, x, y, size, size, 3.0 * s);
    if let Some(path) = pb.finish() {
        if spec.checked {
            let bg_color = if spec.hovered {
                Color::from_rgba8(255, 255, 255, 255)
            } else {
                Color::from_rgba8(59, 130, 246, 255)
            };
            let p = make_paint(bg_color);
            pixmap.fill_path(
                &path,
                &p,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            // 小对勾
            let check_color = if spec.hovered {
                Color::from_rgba8(59, 130, 246, 255)
            } else {
                Color::from_rgba8(255, 255, 255, 255)
            };
            let mut pb_check = PathBuilder::new();
            pb_check.move_to(x + 3.0 * s, y + 7.0 * s);
            pb_check.line_to(x + 6.0 * s, y + 10.5 * s);
            pb_check.line_to(x + 11.0 * s, y + 3.5 * s);
            if let Some(cp) = pb_check.finish() {
                let cp_paint = make_paint(check_color);
                let stroke = Stroke {
                    width: 1.8 * s,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&cp, &cp_paint, &stroke, Transform::identity(), None);
            }
        } else {
            let stroke_color = if spec.hovered {
                (255, 255, 255, 220)
            } else {
                (203, 213, 225, 255)
            };
            let p = make_paint(Color::from_rgba8(
                stroke_color.0,
                stroke_color.1,
                stroke_color.2,
                stroke_color.3,
            ));
            let stroke = Stroke {
                width: 1.2 * s,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
        }
    }
}

/// 子菜单指示箭头的中心位置、状态与缩放参数。
struct ArrowMarkSpec {
    center_x: f32,
    center_y: f32,
    hovered: bool,
    scale: f32,
}

/// 绘制二级菜单向右展开小指示箭头。
fn render_arrow_mark(pixmap: &mut PixmapMut, spec: ArrowMarkSpec) {
    let (cx, cy, s) = (spec.center_x, spec.center_y, spec.scale);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 3.0 * s, cy - 4.5 * s);
    pb.line_to(cx + 2.0 * s, cy);
    pb.line_to(cx - 3.0 * s, cy + 4.5 * s);
    if let Some(path) = pb.finish() {
        let c = if spec.hovered {
            (255, 255, 255, 255)
        } else {
            (148, 163, 184, 255)
        };
        let p = make_paint(Color::from_rgba8(c.0, c.1, c.2, c.3));
        let stroke = Stroke {
            width: 1.5 * s,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
    }
}
