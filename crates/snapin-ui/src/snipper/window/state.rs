//! 截图交互会话状态机模型与配置初始化。

use super::text_input::TextInputSession;
use super::types::{SnipMode, SnipperOutcome};
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{SelectionHistory, SelectionState};
use crate::snipper::toolbar::{Toolbar, ToolbarItem};
use snapin_core::annotation::AnnotationHistory;
use snapin_core::config::{Config, DefaultTool, InterfaceConfig, SnipConfig};
use winit::keyboard::ModifiersState;

/// 截图会话可变运行状态封装
pub struct SnipperState {
    pub selection: SelectionState,
    pub history: SelectionHistory,
    pub magnifier: Magnifier,
    pub toolbar: Toolbar,
    pub annotations: AnnotationHistory,
    pub cursor_pos: (f64, f64),
    pub modifiers: ModifiersState,
    pub pending_stroke_points: Vec<(f32, f32)>,
    pub next_badge_number: u32,
    pub active_text_input: Option<TextInputSession>,
    pub outcome: Option<SnipperOutcome>,
    pub config: Config,
    pub mode: SnipMode,
}

impl SnipperState {
    pub fn new() -> Self {
        Self {
            selection: SelectionState::Idle,
            history: SelectionHistory::default(),
            magnifier: Magnifier::default(),
            toolbar: Toolbar::default(),
            annotations: AnnotationHistory::default(),
            cursor_pos: (0.0, 0.0),
            modifiers: ModifiersState::default(),
            pending_stroke_points: Vec::with_capacity(64),
            next_badge_number: 1,
            active_text_input: None,
            outcome: None,
            config: Config::default(),
            mode: SnipMode::default(),
        }
    }

    /// 根据用户配置文件与触发模式构建初始状态
    pub fn from_config(cfg: Config, mode: SnipMode) -> Self {
        let mut state = Self::new();
        apply_interface_config(&mut state, &cfg.interface);
        apply_snip_config(&mut state, &cfg.snip, mode);
        state.config = cfg;
        state.mode = mode;
        state
    }
}

impl Default for SnipperState {
    fn default() -> Self {
        Self::new()
    }
}

/// 注入界面视觉与放大镜设置
fn apply_interface_config(state: &mut SnipperState, iface: &InterfaceConfig) {
    state.magnifier.enabled = iface.enable_magnifier;
    state.magnifier.scale = iface.magnifier_scale;
    state.magnifier.show_grid = iface.show_grid_lines;
    state.magnifier.format = iface.color_format;
    state.toolbar.ui_scale = iface.toolbar_scale;
}

/// 注入截图流程、默认标注工具与选区记忆设置
fn apply_snip_config(state: &mut SnipperState, snip: &SnipConfig, mode: SnipMode) {
    state.toolbar.sub_options.custom_stroke_width = snip.default_stroke_width;
    state.toolbar.sub_options.font_size = snip.default_font_size;
    state.toolbar.sub_options.color = snip.default_color.to_tiny_skia_color();
    // 初始进入截图时，未完成选区前不激活标注工具，确保左键优先用于选区拉框
    state.toolbar.active_tool = None;

    // 仅在明确触发“自定义截屏”模式时才恢复上次历史选区，常规截图始终从空白选区开始
    if mode == SnipMode::Custom {
        if let Some((lx, ly, lw, lh)) = snapin_platform::load_last_selection() {
            let rect = crate::snipper::selection::Rect {
                x: lx,
                y: ly,
                width: lw,
                height: lh,
            };
            state.selection = SelectionState::Locked {
                rect,
                active_drag: None,
            };
        }
    }
}

/// 在选区锁定后，若用户配置了默认标注工具，则激活对应的工具项
pub fn activate_default_tool_if_configured(state: &mut SnipperState) {
    state.toolbar.active_tool = match state.config.snip.default_tool {
        DefaultTool::Rectangle => Some(ToolbarItem::ToolRect),
        DefaultTool::Pen => Some(ToolbarItem::ToolPen),
        DefaultTool::Marker => Some(ToolbarItem::ToolMarker),
        DefaultTool::Arrow => Some(ToolbarItem::ToolArrow),
        DefaultTool::Text => Some(ToolbarItem::ToolText),
        DefaultTool::Badge => Some(ToolbarItem::ToolBadge),
        DefaultTool::None => None,
    };
}

/// 提交并关闭当前文本输入会话
pub fn commit_active_text_input(state: &mut SnipperState) {
    if let Some(session) = state.active_text_input.take() {
        if let Some(action) = session.commit() {
            state.annotations.push(action);
        }
    }
}
