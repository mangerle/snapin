//! 放大镜二级属性栏专属控件绘制。

use super::super::layout::{SubControl, SubDropdown};
use super::super::Toolbar;
use super::sub_bar_helpers::{
    render_caret, render_magnifier_scale_slider, render_sub_shape_ellipse, render_sub_shape_rect,
    render_toggle_text,
};
use super::widgets::{
    fill_button_highlight, render_magnifier_connector_preview, MagnifierConnectorPreviewSpec,
};
use crate::geometry::Rect;
use snapin_annotation::MagnifierShape;
use tiny_skia::{Color, PixmapMut};

/// 绘制放大镜专属控件；不属于放大镜时返回 `false`，交回通用属性栏分支。
pub(super) fn render_magnifier_sub_control(
    toolbar: &Toolbar,
    control: SubControl,
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    highlight: Color,
) -> bool {
    match control {
        SubControl::MagnifierShapeRect => {
            let active = toolbar.sub_options.magnifier_shape == MagnifierShape::Rectangle;
            if active {
                fill_button_highlight(pixmap, rect, highlight, s);
            }
            render_sub_shape_rect(pixmap, rect.x + 12.0 * s, center_y, s, active);
        }
        SubControl::MagnifierShapeEllipse => {
            let active = toolbar.sub_options.magnifier_shape == MagnifierShape::Ellipse;
            if active {
                fill_button_highlight(pixmap, rect, highlight, s);
            }
            render_sub_shape_ellipse(pixmap, rect.x + 12.0 * s, center_y, s, active);
        }
        SubControl::MagnifierScale => render_magnifier_scale_slider(
            pixmap,
            rect,
            center_y,
            s,
            toolbar.sub_options.magnifier_scale,
            toolbar.focused_sub_control == Some(SubControl::MagnifierScale),
        ),
        SubControl::MagnifierSmooth => render_toggle_text(
            pixmap,
            rect,
            s,
            "抗锯齿",
            toolbar.sub_options.magnifier_smooth,
        ),
        SubControl::MagnifierConnector => {
            let open = toolbar.open_dropdown == Some(SubDropdown::MagnifierConnector);
            if open {
                fill_button_highlight(pixmap, rect, highlight, s);
            }
            render_magnifier_connector_preview(
                pixmap,
                MagnifierConnectorPreviewSpec {
                    x0: rect.x + 8.0 * s,
                    x1: rect.x + rect.width - 18.0 * s,
                    center_y,
                    scale: s,
                    connector: toolbar.sub_options.magnifier_connector,
                    color: Color::from_rgba8(45, 48, 56, 255),
                },
            );
            render_caret(pixmap, rect.x + rect.width - 9.0 * s, center_y, s);
        }
        SubControl::MagnifierShadow => render_toggle_text(
            pixmap,
            rect,
            s,
            "阴影",
            toolbar.sub_options.magnifier_shadow,
        ),
        _ => return false,
    }
    true
}
