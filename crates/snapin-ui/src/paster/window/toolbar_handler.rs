//! 贴图底部快捷工具条交互命中检测与动作处理模块。

use super::menu_actions::{copy_paster_to_clipboard, save_paster_to_disk};
use super::state::PasterWindowState;
use winit::window::Window;

/// 检测并处理对贴图底部现代胶囊快捷工具条的点击
pub fn check_and_handle_toolbar_click(state: &mut PasterWindowState, window: &Window) -> bool {
    if !state.show_toolbar {
        return false;
    }

    let win_size = window.inner_size();
    let w = win_size.width as f32;
    let h = win_size.height as f32;
    let s = state.ui_scale;

    let bar_w = (230.0 * s).min(w - 16.0);
    let bar_h = 28.0 * s;
    if bar_w < 100.0 || h < bar_h + 16.0 {
        return false;
    }

    let bar_x = (w - bar_w) / 2.0;
    let bar_y = h - bar_h - 8.0 * s;

    let (cx, cy) = state.cursor_pos;
    if cx < bar_x || cx > bar_x + bar_w || cy < bar_y || cy > bar_y + bar_h {
        return false;
    }

    // 落在工具条胶囊内部，根据水平相对比例分发动作 (共 4 个按钮：复制, 保存, 旋转, 翻转)
    let rel_x = cx - bar_x;
    let item_w = bar_w / 4.0;
    let idx = (rel_x / item_w) as usize;

    match idx {
        0 => copy_paster_to_clipboard(&state.item),
        1 => save_paster_to_disk(state),
        2 => {
            state.rotate_cw(window);
        }
        _ => {
            state.flip_horizontal();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paster::manager::PasterItem;
    use tiny_skia::Pixmap;

    #[test]
    fn test_toolbar_hit_geometry_calculation() {
        let dummy = Pixmap::new(300, 200).expect("分配测试位图失败");
        let item = PasterItem::new(1, "test".to_string(), dummy, (0, 0));
        let mut state = PasterWindowState::new(item);
        state.show_toolbar = true;
        state.ui_scale = 1.0;

        let w = 300.0f32;
        let h = 200.0f32;
        let s = 1.0f32;
        let bar_w = (230.0 * s).min(w - 16.0); // 230.0
        let bar_h = 28.0 * s; // 28.0
        let bar_x = (w - bar_w) / 2.0; // (300 - 230) / 2 = 35.0
        let bar_y = h - bar_h - 8.0 * s; // 200 - 28 - 8 = 164.0

        // 落在工具条外部
        state.cursor_pos = (10.0, 10.0);
        let in_bar = state.cursor_pos.0 >= bar_x
            && state.cursor_pos.0 <= bar_x + bar_w
            && state.cursor_pos.1 >= bar_y
            && state.cursor_pos.1 <= bar_y + bar_h;
        assert!(!in_bar);

        // 落在工具条内部 (第一个按钮: 复制)
        state.cursor_pos = (bar_x + 10.0, bar_y + 10.0);
        let in_bar = state.cursor_pos.0 >= bar_x
            && state.cursor_pos.0 <= bar_x + bar_w
            && state.cursor_pos.1 >= bar_y
            && state.cursor_pos.1 <= bar_y + bar_h;
        assert!(in_bar);

        let item_w = bar_w / 4.0;
        let idx = ((state.cursor_pos.0 - bar_x) / item_w) as usize;
        assert_eq!(idx, 0, "点击应命中复制按钮");

        // 落在工具条内部 (第三个按钮: 旋转)
        state.cursor_pos = (bar_x + item_w * 2.5, bar_y + 10.0);
        let idx = ((state.cursor_pos.0 - bar_x) / item_w) as usize;
        assert_eq!(idx, 2, "点击应命中旋转按钮");
    }
}
