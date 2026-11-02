//! 放大镜二级属性栏专属控件绘制。

use super::super::layout::{SubControl, SubDropdown};
use super::super::Toolbar;
use super::sub_bar_helpers::{
    render_caret, render_magnifier_scale_slider, render_sub_shape_ellipse, render_sub_shape_rect,
    render_toggle_text, SubBarGeometry,
};
use super::widgets::{
    fill_button_highlight, render_magnifier_connector_preview, MagnifierConnectorPreviewSpec,
};
use snapin_annotation::MagnifierShape;
use tiny_skia::{Color, PixmapMut};

/// 放大镜控件渲染所需的工具栏与共享几何参数。
pub(super) struct MagnifierControlSpec<'a> {
    pub toolbar: &'a Toolbar,
    pub control: SubControl,
    pub geometry: SubBarGeometry,
}

/// 绘制放大镜专属控件；不属于放大镜时返回 `false`，交回通用属性栏分支。
pub(super) fn render_magnifier_sub_control(
    pixmap: &mut PixmapMut,
    spec: MagnifierControlSpec<'_>,
) -> bool {
    match spec.control {
        SubControl::MagnifierShapeRect => {
            let active = spec.toolbar.sub_options.magnifier_shape == MagnifierShape::Rectangle;
            render_shape_control(pixmap, &spec, active, true);
        }
        SubControl::MagnifierShapeEllipse => {
            let active = spec.toolbar.sub_options.magnifier_shape == MagnifierShape::Ellipse;
            render_shape_control(pixmap, &spec, active, false);
        }
        SubControl::MagnifierScale => render_magnifier_scale_slider(
            pixmap,
            spec.geometry,
            spec.toolbar.sub_options.magnifier_scale,
            spec.toolbar.focused_sub_control == Some(SubControl::MagnifierScale),
        ),
        SubControl::MagnifierSmooth => render_toggle_text(
            pixmap,
            spec.geometry,
            "抗锯齿",
            spec.toolbar.sub_options.magnifier_smooth,
        ),
        SubControl::MagnifierConnector => render_connector_control(pixmap, &spec),
        SubControl::MagnifierShadow => render_toggle_text(
            pixmap,
            spec.geometry,
            "阴影",
            spec.toolbar.sub_options.magnifier_shadow,
        ),
        _ => return false,
    }
    true
}

/// 绘制放大镜矩形或椭圆形状选择控件。
fn render_shape_control(
    pixmap: &mut PixmapMut,
    spec: &MagnifierControlSpec<'_>,
    active: bool,
    rectangle: bool,
) {
    if active {
        fill_button_highlight(
            pixmap,
            &spec.geometry.rect,
            spec.geometry.highlight,
            spec.geometry.scale,
        );
    }
    if rectangle {
        render_sub_shape_rect(pixmap, spec.geometry, active);
    } else {
        render_sub_shape_ellipse(pixmap, spec.geometry, active);
    }
}

/// 绘制放大镜连接线下拉控件及展开箭头。
fn render_connector_control(pixmap: &mut PixmapMut, spec: &MagnifierControlSpec<'_>) {
    if spec.toolbar.open_dropdown == Some(SubDropdown::MagnifierConnector) {
        fill_button_highlight(
            pixmap,
            &spec.geometry.rect,
            spec.geometry.highlight,
            spec.geometry.scale,
        );
    }
    render_magnifier_connector_preview(
        pixmap,
        MagnifierConnectorPreviewSpec {
            x0: spec.geometry.rect.x + 8.0 * spec.geometry.scale,
            x1: spec.geometry.rect.x + spec.geometry.rect.width - 18.0 * spec.geometry.scale,
            center_y: spec.geometry.center_y,
            scale: spec.geometry.scale,
            connector: spec.toolbar.sub_options.magnifier_connector,
            color: Color::from_rgba8(45, 48, 56, 255),
        },
    );
    render_caret(
        pixmap,
        spec.geometry.rect.x + spec.geometry.rect.width - 9.0 * spec.geometry.scale,
        spec.geometry.center_y,
        spec.geometry.scale,
    );
}
