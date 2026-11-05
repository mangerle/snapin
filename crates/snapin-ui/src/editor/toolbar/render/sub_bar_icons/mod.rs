//! 二级栏关闭态语义图标。

mod common;
mod icons;
mod mosaic;

pub(super) use common::SubBarIconSpec;
pub(super) use icons::{
    render_arrow_toggle_icon, render_badge_sequence_icon, render_clear_annotations_icon,
    render_eraser_mode_icon, render_font_family_icon, render_font_size_icon, render_line_mode_icon,
    render_magnifier_shadow_icon, render_magnifier_smooth_icon, render_marker_blend_icon,
    render_marker_shape_icon, render_opacity_icon, render_padding_icon,
    render_spotlight_outline_icon, render_text_background_icon, render_text_stroke_icon,
};
pub(super) use mosaic::{render_mosaic_mode_icon, render_mosaic_shape_icon};
