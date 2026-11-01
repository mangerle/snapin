//! 声明式标量字段同步表。
//!
//! apply/extract 中大量字段只是“Slint 属性 ↔ Config 字段”的一对一拷贝。
//! 把这些字段集中到本表，新增字段时只需加一行，避免两个方向各自漏改。

use crate::SettingsWindow;
use slint::SharedString;
use snapin_core::config::Config;

macro_rules! bool_table {
    ($($section:ident . $field:ident => $setter:ident / $getter:ident),+ $(,)?) => {
        pub(crate) fn apply_scalar_bools(win: &SettingsWindow, cfg: &Config) {
            $( win.$setter(cfg.$section.$field); )+
        }

        pub(crate) fn extract_scalar_bools(win: &SettingsWindow, cfg: &mut Config) {
            $( cfg.$section.$field = win.$getter(); )+
        }
    };
}

macro_rules! number_table {
    ($($section:ident . $field:ident => $setter:ident / $getter:ident),+ $(,)?) => {
        pub(crate) fn apply_scalar_numbers(win: &SettingsWindow, cfg: &Config) {
            $( win.$setter(cfg.$section.$field); )+
        }

        pub(crate) fn extract_scalar_numbers(win: &SettingsWindow, cfg: &mut Config) {
            $( cfg.$section.$field = win.$getter(); )+
        }
    };
}

macro_rules! string_table {
    ($($section:ident . $field:ident => $setter:ident / $getter:ident),+ $(,)?) => {
        pub(crate) fn apply_scalar_strings(win: &SettingsWindow, cfg: &Config) {
            $( win.$setter(SharedString::from(cfg.$section.$field.as_str())); )+
        }

        pub(crate) fn extract_scalar_strings(win: &SettingsWindow, cfg: &mut Config) {
            $( cfg.$section.$field = win.$getter().to_string(); )+
        }
    };
}

bool_table! {
    general.autostart => set_autostart / get_autostart,
    general.notify_on_startup => set_notify_on_startup / get_notify_on_startup,
    general.auto_check_update => set_auto_check_update / get_auto_check_update,
    interface.enable_magnifier => set_enable_magnifier / get_enable_magnifier,
    interface.show_grid_lines => set_show_grid_lines / get_show_grid_lines,
    snip.remember_last_selection => set_remember_last_selection / get_remember_last_selection,
    output.notify_on_copy => set_notify_on_copy / get_notify_on_copy,
    output.notify_on_save => set_notify_on_save / get_notify_on_save,
    output.prompt_save_path => set_prompt_save_path / get_prompt_save_path,
}

number_table! {
    snip.default_stroke_width => set_default_stroke_width / get_default_stroke_width,
    snip.default_font_size => set_snip_font_size / get_snip_font_size,
}

string_table! {
    output.quick_save_dir => set_quick_save_dir / get_quick_save_dir,
    output.file_name_template => set_file_name_template / get_file_name_template,
}

/// 应用全部声明式标量字段。
pub(crate) fn apply_scalar_fields(win: &SettingsWindow, cfg: &Config) {
    apply_scalar_bools(win, cfg);
    apply_scalar_numbers(win, cfg);
    apply_scalar_strings(win, cfg);
}

/// 提取全部声明式标量字段。
pub(crate) fn extract_scalar_fields(win: &SettingsWindow, cfg: &mut Config) {
    extract_scalar_bools(win, cfg);
    extract_scalar_numbers(win, cfg);
    extract_scalar_strings(win, cfg);
}
