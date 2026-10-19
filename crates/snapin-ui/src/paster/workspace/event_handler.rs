//! 贴图集中工作区事件调度与窗口事件响应。

use super::super::manager::PasterManager;
use super::super::window::events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
};
use super::super::window::render::render_paster_to_surface;
use super::proxy::{PasterAppEvent, WorkspaceStatus};
use super::window_builder::{create_new_paster_window, PasterInstance};
use snapin_core::config::Config;
use std::collections::HashMap;
use winit::event::WindowEvent;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::WindowId;

/// 支持的循环轮转贴图最大分组数量
pub const TOTAL_PIN_GROUPS: usize = 4;

/// 处理工作区收到的自定义指令
pub fn handle_workspace_user_event(
    event: PasterAppEvent,
    instances: &mut HashMap<WindowId, PasterInstance>,
    manager: &mut PasterManager,
    cfg: &mut Config,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    match event {
        PasterAppEvent::ReloadConfig(new_cfg) => {
            *cfg = new_cfg.clone();
            for inst in instances.values_mut() {
                inst.state.config = new_cfg.clone();
            }
            tracing::info!("集中贴图工作区已完成全局配置热更新");
        }
        PasterAppEvent::OpenPaster(item) => {
            if let Err(e) = create_new_paster_window(item, instances, manager, cfg, elwt) {
                tracing::error!("在当前事件循环中创建新贴图窗口失败: {e}");
            }
        }
        PasterAppEvent::ToggleVisibility => {
            let new_visible = manager.toggle_current_group_visibility();
            let active = manager.active_group();
            for inst in instances.values_mut() {
                if inst.group_id == active {
                    inst.state.item.is_visible = new_visible;
                    inst.window.set_visible(new_visible);
                }
            }
            tracing::info!(
                "已切换当前贴图组 ({active}) 显隐状态: {}",
                if new_visible {
                    "全部显示"
                } else {
                    "全部隐藏"
                }
            );
        }
        PasterAppEvent::SwitchGroup(target) => {
            let next_group = match target {
                Some(id) => id,
                None => (manager.active_group() + 1) % TOTAL_PIN_GROUPS,
            };
            manager.switch_group(next_group);
            for inst in instances.values_mut() {
                let visible = inst.group_id == next_group;
                inst.state.item.is_visible = visible;
                inst.window.set_visible(visible);
            }
            tracing::info!("已切换活动贴图分组至: {next_group}");
        }
        PasterAppEvent::SetVisibility(visible) => {
            let active = manager.active_group();
            for inst in instances.values_mut() {
                if inst.group_id == active {
                    inst.state.item.is_visible = visible;
                    inst.window.set_visible(visible);
                }
            }
        }
        PasterAppEvent::ClosePaster(id) => {
            if let Some((&win_id, _)) = instances.iter().find(|(_, inst)| inst.item_id == id) {
                if let Some(removed) = instances.remove(&win_id) {
                    manager.remove_paster(removed.item_id);
                    tracing::info!("已通过指令关闭贴图: {id}");
                }
            }
        }
        PasterAppEvent::GetStatus(responder) => {
            let status = WorkspaceStatus {
                active_group: manager.active_group(),
                paster_count: manager.count_in_active_group(),
            };
            let _ = responder.send(status);
        }
        PasterAppEvent::Shutdown => {
            instances.clear();
            elwt.exit();
        }
    }
}

/// 处理特定窗口触发的 Winit 窗口事件
pub fn handle_workspace_window_event(
    window_id: WindowId,
    event: WindowEvent,
    instances: &mut HashMap<WindowId, PasterInstance>,
    manager: &mut PasterManager,
    exit_on_all_closed: bool,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    let mut should_close = false;

    if let Some(inst) = instances.get_mut(&window_id) {
        match event {
            WindowEvent::CloseRequested => {
                should_close = true;
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if handle_paster_key(
                    &mut inst.state,
                    &key_event.logical_key,
                    key_event.state,
                    &inst.window,
                ) {
                    inst.window.request_redraw();
                }
                if inst.state.should_close {
                    should_close = true;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if handle_paster_cursor_move(&mut inst.state, (position.x, position.y)) {
                    inst.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if handle_paster_scroll(&mut inst.state, delta, &inst.window) {
                    inst.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if handle_paster_mouse_button(&mut inst.state, button, state, &inst.window) {
                    inst.window.request_redraw();
                }
                if inst.state.should_close {
                    should_close = true;
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let s = scale_factor as f32;
                inst.state.ui_scale = s;
                inst.state.context_menu.set_ui_scale(s);
                inst.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let size = inst.window.inner_size();
                if let Err(e) = render_paster_to_surface(
                    &mut inst.state,
                    &mut inst.surface,
                    size.width,
                    size.height,
                ) {
                    tracing::error!("贴图渲染帧缓冲失败: {e}");
                }
            }
            _ => {}
        }
    }

    if should_close {
        if let Some(removed) = instances.remove(&window_id) {
            manager.remove_paster(removed.item_id);
            if let Some(observer) = super::get_paster_observer() {
                observer.on_paster_closed(window_id);
            }
            tracing::info!(
                "贴图已安全关闭: {}，工作区剩余活动贴图: {}",
                removed.item_id,
                instances.len()
            );
        }
        if exit_on_all_closed && instances.is_empty() {
            tracing::info!("所有单次会话贴图窗口均已关闭，平滑退出工作区");
            elwt.exit();
        }
    }
}
