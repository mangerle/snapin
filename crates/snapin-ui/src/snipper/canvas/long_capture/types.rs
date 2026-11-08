//! 长截图控制栏按钮元数据与布局契约。

use crate::snipper::window::state::long_capture::{LongCaptureAction, LongCaptureSession};
use snapin_render::stitching::StitchDirection;

/// 控制栏按钮定义
#[derive(Debug, Clone, Copy)]
pub(crate) struct LongCaptureButtonDef {
    pub action: LongCaptureAction,
    pub tooltip: &'static str,
    pub width: f32,
}

/// 计算长截图主控制栏的按钮布局列表。
///
/// 功能顺序参考 PixPin 官方长截图控制栏；拖动手柄按 Snapin 常规工具栏习惯固定在右端，
/// 且只用于移动控制栏本身，不复用 PixPin 中“移动截图选区”的语义。
pub(crate) fn long_capture_main_button_defs(
    session: &LongCaptureSession,
    _selection_w: u32,
    _selection_h: u32,
) -> Vec<LongCaptureButtonDef> {
    let mut defs = Vec::with_capacity(10);

    // 1. 切换截图方向（垂直 / 水平 下拉胶囊）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::ToggleDirection,
        tooltip: match session.direction {
            StitchDirection::Vertical => "当前纵向截图 (点击切换横向)",
            StitchDirection::Horizontal => "当前横向截图 (点击切换纵向)",
        },
        width: 52.0,
    });

    // 2. 裁剪功能（剪刀图标，点击展开子浮条）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::ToggleCropSubMenu,
        tooltip: "裁剪",
        width: 28.0,
    });

    // 3. 开始/停止按钮（红框录制胶囊：播放三角 / 停止方块）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::ToggleCapture,
        tooltip: if session.is_capturing {
            "停止截图"
        } else {
            "开始截图"
        },
        width: 28.0,
    });

    // 4. 完成并二次编辑（带笔方框）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::EnterEditing,
        tooltip: "编辑",
        width: 28.0,
    });

    // 5. 保存（沿用编辑器现有软盘图标）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::Save,
        tooltip: "保存",
        width: 28.0,
    });

    // 6. 贴图（沿用编辑器现有图钉图标）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::Pin,
        tooltip: "贴图",
        width: 28.0,
    });

    // 7. 关闭（沿用编辑器现有叉号图标）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::Close,
        tooltip: "关闭",
        width: 28.0,
    });

    // 8. 复制（沿用编辑器现有叠层纸张图标）
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::Copy,
        tooltip: "复制",
        width: 28.0,
    });

    // 9. 工具栏拖动手柄固定在右端，与常规截图工具栏保持一致
    defs.push(LongCaptureButtonDef {
        action: LongCaptureAction::DragToolbar,
        tooltip: "拖动工具栏",
        width: 22.0,
    });

    defs
}

/// 计算选区尺寸信息区域宽度，保证渲染和命中测试使用同一布局。
pub(crate) fn long_capture_size_readout_width(selection_w: u32, selection_h: u32) -> f32 {
    if selection_w >= 1000 || selection_h >= 1000 {
        96.0
    } else {
        84.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> LongCaptureSession {
        let frame = image::RgbaImage::from_pixel(200, 300, image::Rgba([128, 128, 128, 255]));
        LongCaptureSession::new(frame)
    }

    #[test]
    fn test_drag_handle_is_last_and_same_function_actions_keep_editor_order() {
        let session = test_session();
        let defs = long_capture_main_button_defs(&session, 200, 300);
        let actions: Vec<_> = defs.iter().map(|def| def.action).collect();

        assert_eq!(
            actions,
            vec![
                LongCaptureAction::ToggleDirection,
                LongCaptureAction::ToggleCropSubMenu,
                LongCaptureAction::ToggleCapture,
                LongCaptureAction::EnterEditing,
                LongCaptureAction::Save,
                LongCaptureAction::Pin,
                LongCaptureAction::Close,
                LongCaptureAction::Copy,
                LongCaptureAction::DragToolbar,
            ]
        );
    }

    #[test]
    fn test_size_readout_width_tracks_four_digit_dimensions() {
        assert_eq!(long_capture_size_readout_width(999, 999), 84.0);
        assert_eq!(long_capture_size_readout_width(1000, 999), 96.0);
        assert_eq!(long_capture_size_readout_width(999, 1000), 96.0);
    }
}
