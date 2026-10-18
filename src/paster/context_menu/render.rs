//! 贴图右键上下文菜单的现代化毛玻璃视觉光栅化渲染。

use super::{ContextMenuState, MenuItem};
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 创建开启抗锯齿的基础画笔
fn make_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    paint
}

/// 纯 Rust 高精度文本渲染上下文
pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }
}

impl TextRenderer {
    /// 在指定坐标绘制中文或英文排版文字
    pub fn draw_text(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
    ) {
        let metrics = Metrics::new(font_size, font_size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let c_color = CosmicColor::rgba(
            (color.red() * 255.0) as u8,
            (color.green() * 255.0) as u8,
            (color.blue() * 255.0) as u8,
            (color.alpha() * 255.0) as u8,
        );

        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            c_color,
            |gx, gy, _w, _h, c| {
                let px = x as i32 + gx;
                let py = y as i32 + gy;
                if px >= 0 && px < width && py >= 0 && py < height {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    let data = pixmap.data_mut();
                    let src_a = c.a() as f32 / 255.0;
                    let dst_a = data[idx + 3] as f32 / 255.0;
                    let out_a = src_a + dst_a * (1.0 - src_a);
                    if out_a > 0.0 {
                        let src_r = c.r() as f32 * src_a;
                        let src_g = c.g() as f32 * src_a;
                        let src_b = c.b() as f32 * src_a;
                        let dst_r = data[idx] as f32 * dst_a;
                        let dst_g = data[idx + 1] as f32 * dst_a;
                        let dst_b = data[idx + 2] as f32 * dst_a;
                        data[idx] = ((src_r + dst_r * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 1] = ((src_g + dst_g * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 2] = ((src_b + dst_b * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 3] = (out_a * 255.0) as u8;
                    }
                }
            },
        );
    }
}

/// 绘制圆角矩形路径辅助方法
fn add_round_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let r = r.min(w / 2.0).min(h / 2.0);
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
}

/// 菜单面板布局参数
#[derive(Debug, Clone, Copy)]
pub struct MenuLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub item_height: f32,
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

    let main_items = ContextMenuState::build_main_items(show_toolbar, has_shadow);
    let main_layout = MenuLayout {
        x: menu.x,
        y: menu.y,
        width: menu.width,
        item_height: menu.item_height,
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
            width: 160.0,
            item_height: menu.item_height,
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
    let (x, y, width, item_height) = (layout.x, layout.y, layout.width, layout.item_height);
    let total_h = items
        .iter()
        .map(|it| if it.is_separator { 8.0 } else { item_height })
        .sum::<f32>()
        + 10.0;

    // 1. 绘制现代微深色毛玻璃卡片背景
    let mut pb_bg = PathBuilder::new();
    add_round_rect(&mut pb_bg, x, y, width, total_h, 8.0);
    if let Some(path) = pb_bg.finish() {
        let bg_paint = make_paint(Color::from_rgba8(26, 28, 40, 245));
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        let stroke_paint = make_paint(Color::from_rgba8(255, 255, 255, 28));
        let stroke = Stroke {
            width: 1.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    // 2. 逐行绘制菜单项内容
    let mut cur_y = y + 5.0;
    for (idx, item) in items.iter().enumerate() {
        if item.is_separator {
            let mut pb_sep = PathBuilder::new();
            pb_sep.move_to(x + 12.0, cur_y + 4.0);
            pb_sep.line_to(x + width - 12.0, cur_y + 4.0);
            if let Some(path) = pb_sep.finish() {
                let p = make_paint(Color::from_rgba8(255, 255, 255, 20));
                let stroke = Stroke {
                    width: 1.0,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
            }
            cur_y += 8.0;
            continue;
        }

        let is_hovered = hovered_idx == Some(idx);
        if is_hovered {
            let mut pb_hover = PathBuilder::new();
            add_round_rect(
                &mut pb_hover,
                x + 5.0,
                cur_y + 2.0,
                width - 10.0,
                item_height - 4.0,
                5.0,
            );
            if let Some(path) = pb_hover.finish() {
                let hp = make_paint(Color::from_rgba8(59, 130, 246, 220));
                pixmap.fill_path(
                    &path,
                    &hp,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        // 绘制复选框 / 文字 / 展开箭头
        let text_color = if is_hovered {
            Color::from_rgba8(255, 255, 255, 255)
        } else {
            Color::from_rgba8(230, 230, 240, 255)
        };

        if item.is_checkbox {
            render_checkbox_mark(
                pixmap,
                x + 12.0,
                cur_y + (item_height - 14.0) / 2.0,
                item.is_checked,
                is_hovered,
            );
            text_renderer.draw_text(pixmap, item.label, x + 32.0, cur_y + 6.0, 13.0, text_color);
        } else {
            text_renderer.draw_text(pixmap, item.label, x + 14.0, cur_y + 6.0, 13.0, text_color);
        }

        if item.submenu.is_some() {
            render_arrow_mark(
                pixmap,
                x + width - 16.0,
                cur_y + item_height / 2.0,
                is_hovered,
            );
        }

        cur_y += item_height;
    }
}

/// 绘制复选框小图标
fn render_checkbox_mark(pixmap: &mut PixmapMut, x: f32, y: f32, checked: bool, is_hovered: bool) {
    let mut pb = PathBuilder::new();
    add_round_rect(&mut pb, x, y, 14.0, 14.0, 3.0);
    if let Some(path) = pb.finish() {
        if checked {
            let p = make_paint(Color::from_rgba8(34, 197, 94, 255));
            pixmap.fill_path(
                &path,
                &p,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            // 白色小对勾
            let mut pb_check = PathBuilder::new();
            pb_check.move_to(x + 3.0, y + 7.0);
            pb_check.line_to(x + 6.0, y + 10.5);
            pb_check.line_to(x + 11.0, y + 3.5);
            if let Some(cp) = pb_check.finish() {
                let cp_paint = make_paint(Color::from_rgba8(255, 255, 255, 255));
                let stroke = Stroke {
                    width: 1.8,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&cp, &cp_paint, &stroke, Transform::identity(), None);
            }
        } else {
            let stroke_color = if is_hovered {
                (255, 255, 255, 180)
            } else {
                (160, 160, 180, 150)
            };
            let p = make_paint(Color::from_rgba8(
                stroke_color.0,
                stroke_color.1,
                stroke_color.2,
                stroke_color.3,
            ));
            let stroke = Stroke {
                width: 1.2,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
        }
    }
}

/// 绘制二级菜单向右展开小指示箭头
fn render_arrow_mark(pixmap: &mut PixmapMut, cx: f32, cy: f32, is_hovered: bool) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 3.0, cy - 4.5);
    pb.line_to(cx + 2.0, cy);
    pb.line_to(cx - 3.0, cy + 4.5);
    if let Some(path) = pb.finish() {
        let c = if is_hovered {
            (255, 255, 255, 255)
        } else {
            (180, 180, 200, 200)
        };
        let p = make_paint(Color::from_rgba8(c.0, c.1, c.2, c.3));
        let stroke = Stroke {
            width: 1.5,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
    }
}
