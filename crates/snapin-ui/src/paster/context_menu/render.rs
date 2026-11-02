//! 贴图右键上下文菜单的现代化毛玻璃视觉光栅化渲染。

use super::{ContextMenuState, MenuItem};
use crate::drawing::{add_round_rect, make_paint, RoundRect};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

pub use snapin_render::{TextDrawOptions, TextRenderer};

/// 菜单面板布局参数
#[derive(Debug, Clone, Copy)]
pub struct MenuLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub item_height: f32,
    pub scale: f32,
}

/// 上下文菜单整帧渲染配置。
#[derive(Debug, Clone, Copy)]
pub struct ContextMenuRenderOptions {
    pub show_toolbar: bool,
    pub has_shadow: bool,
    pub smooth_scaling: bool,
}

/// 单个菜单面板的布局、菜单项与悬停状态。
#[derive(Debug, Clone, Copy)]
struct MenuPanelSpec<'a> {
    layout: MenuLayout,
    items: &'a [MenuItem],
    hovered_idx: Option<usize>,
}

/// 绘制整个上下文菜单及其二级子菜单
pub fn render_context_menu(
    menu: &ContextMenuState,
    pixmap: &mut PixmapMut,
    text_renderer: &mut TextRenderer,
    options: ContextMenuRenderOptions,
) {
    if !menu.is_open {
        return;
    }

    let s = menu.ui_scale;
    let main_items = ContextMenuState::build_main_items(options.show_toolbar, options.has_shadow);
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
        MenuPanelSpec {
            layout: main_layout,
            items: &main_items,
            hovered_idx: menu.hovered_main_idx,
        },
    );

    // 2. 渲染二级子菜单面板 (若处于展开状态)
    if let Some((sub_type, sx, sy)) = menu.active_submenu {
        let sub_items = ContextMenuState::build_sub_items(sub_type, options.smooth_scaling);
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
            MenuPanelSpec {
                layout: sub_layout,
                items: &sub_items,
                hovered_idx: menu.hovered_sub_idx,
            },
        );
    }
}

/// 渲染单个菜单面板列表
fn render_menu_panel(
    pixmap: &mut PixmapMut,
    text_renderer: &mut TextRenderer,
    panel: MenuPanelSpec<'_>,
) {
    let MenuPanelSpec {
        layout,
        items,
        hovered_idx,
    } = panel;
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
    draw_menu_shadow(pixmap, layout, total_h);
    draw_menu_body(pixmap, layout, total_h);
}

/// 绘制菜单面板阴影。
fn draw_menu_shadow(pixmap: &mut PixmapMut, layout: MenuLayout, total_h: f32) {
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: layout.x,
            y: layout.y + 2.0 * layout.scale,
            width: layout.width,
            height: total_h,
            radius: 8.0 * layout.scale,
        },
    );
    if let Some(path) = builder.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(0, 0, 0, 36)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制菜单面板白色底板与边框。
fn draw_menu_body(pixmap: &mut PixmapMut, layout: MenuLayout, total_h: f32) {
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: layout.x,
            y: layout.y,
            width: layout.width,
            height: total_h,
            radius: 8.0 * layout.scale,
        },
    );
    let Some(path) = builder.finish() else {
        return;
    };
    pixmap.fill_path(
        &path,
        &make_paint(Color::from_rgba8(255, 255, 255, 252)),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    pixmap.stroke_path(
        &path,
        &make_paint(Color::from_rgba8(218, 224, 233, 240)),
        &Stroke {
            width: layout.scale,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
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
#[derive(Clone, Copy)]
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
    if row.hovered {
        render_row_hover(pixmap, layout, row.top);
    }
    render_row_label(pixmap, text_renderer, layout, row);
    if row.item.submenu.is_some() {
        render_arrow_mark(
            pixmap,
            ArrowMarkSpec {
                center_x: layout.x + layout.width - 16.0 * layout.scale,
                center_y: row.top + layout.item_height / 2.0,
                hovered: row.hovered,
                scale: layout.scale,
            },
        );
    }
}

/// 绘制菜单行的蓝色悬停底色。
fn render_row_hover(pixmap: &mut PixmapMut, layout: MenuLayout, top: f32) {
    let scale = layout.scale;
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: layout.x + 5.0 * scale,
            y: top + 2.0 * scale,
            width: layout.width - 10.0 * scale,
            height: layout.item_height - 4.0 * scale,
            radius: 5.0 * scale,
        },
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

/// 绘制复选框（可选）与菜单标签文本。
fn render_row_label(
    pixmap: &mut PixmapMut,
    text_renderer: &mut TextRenderer,
    layout: MenuLayout,
    row: MenuRowSpec<'_>,
) {
    let scale = layout.scale;
    let text_rgba = if row.hovered {
        (255, 255, 255, 255)
    } else {
        (30, 41, 59, 255)
    };
    let font_size = 13.0 * scale;
    let text_y = row.top + (layout.item_height - font_size * 1.25) / 2.0;
    let text_x = if row.item.is_checkbox {
        render_checkbox_mark(
            pixmap,
            CheckboxMarkSpec {
                x: layout.x + 12.0 * scale,
                y: row.top + (layout.item_height - 14.0 * scale) / 2.0,
                checked: row.item.is_checked,
                hovered: row.hovered,
                scale,
            },
        );
        layout.x + 32.0 * scale
    } else {
        layout.x + 14.0 * scale
    };
    text_renderer.draw_text_rgba(
        pixmap,
        row.item.label,
        TextDrawOptions::new(text_x, text_y, font_size, text_rgba),
    );
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
    let size = 14.0 * spec.scale;
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: spec.x,
            y: spec.y,
            width: size,
            height: size,
            radius: 3.0 * spec.scale,
        },
    );
    let Some(path) = builder.finish() else {
        return;
    };
    if spec.checked {
        render_checked_checkbox(pixmap, &path, spec);
    } else {
        render_unchecked_checkbox(pixmap, &path, spec);
    }
}

/// 绘制选中态复选框与对勾。
fn render_checked_checkbox(pixmap: &mut PixmapMut, path: &tiny_skia::Path, spec: CheckboxMarkSpec) {
    let background = if spec.hovered {
        Color::from_rgba8(255, 255, 255, 255)
    } else {
        Color::from_rgba8(59, 130, 246, 255)
    };
    pixmap.fill_path(
        path,
        &make_paint(background),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    let check_color = if spec.hovered {
        Color::from_rgba8(59, 130, 246, 255)
    } else {
        Color::from_rgba8(255, 255, 255, 255)
    };
    let mut check = PathBuilder::new();
    check.move_to(spec.x + 3.0 * spec.scale, spec.y + 7.0 * spec.scale);
    check.line_to(spec.x + 6.0 * spec.scale, spec.y + 10.5 * spec.scale);
    check.line_to(spec.x + 11.0 * spec.scale, spec.y + 3.5 * spec.scale);
    if let Some(check) = check.finish() {
        let stroke = Stroke {
            width: 1.8 * spec.scale,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        pixmap.stroke_path(
            &check,
            &make_paint(check_color),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制未选中态复选框边框。
fn render_unchecked_checkbox(
    pixmap: &mut PixmapMut,
    path: &tiny_skia::Path,
    spec: CheckboxMarkSpec,
) {
    let color = if spec.hovered {
        Color::from_rgba8(255, 255, 255, 220)
    } else {
        Color::from_rgba8(203, 213, 225, 255)
    };
    let stroke = Stroke {
        width: 1.2 * spec.scale,
        ..Default::default()
    };
    pixmap.stroke_path(
        path,
        &make_paint(color),
        &stroke,
        Transform::identity(),
        None,
    );
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
