//! 贴图文字标注会话：输入生命周期、坐标换算、即时渲染与 IME 联动。
//!
//! 贴图图像按 `transform.scale` 缩放呈现，而文字图元始终以图像像素坐标持久化。
//! 因此会话内部保存图像坐标，渲染时再统一乘以缩放倍数；文本输入、预编辑与
//! 字号夹取规则完全复用截图侧的 [`TextInputSession`]，仅在预览渲染上增加
//! 缩放换算与预编辑下划线，避免两套输入状态机产生行为漂移。

use super::annotation::image_to_screen_point;
use super::state::PasterWindowState;
use crate::snipper::toolbar::ToolbarItem;
use crate::snipper::window::text_input::TextInputSession;
use tiny_skia::{Color, Pixmap};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

impl PasterWindowState {
    /// 当前是否处于文字输入会话中。
    pub(crate) fn has_active_text_session(&self) -> bool {
        self.active_text_input.is_some()
    }

    /// 在点击位置开启文字会话；点击点必须落在缩放后的图像内部。
    ///
    /// 若已有会话，会先按常规路径提交（空文本自动丢弃），再在新位置开启；
    /// 坐标在这里一次性换算为图像像素，后续 IME 事件与渲染都不再重复换算。
    pub(crate) fn begin_text_session(&mut self, screen_pos: (f32, f32)) -> bool {
        if self.toolbar.active_tool != Some(ToolbarItem::ToolText) {
            return false;
        }
        if !self.screen_point_inside_image(screen_pos) {
            return false;
        }

        self.commit_text_session();
        let color = color_to_rgba(self.toolbar.sub_options.color);
        let font_size = self.toolbar.sub_options.font_size;
        let pos = self.image_point(screen_pos);
        self.active_text_input = Some(TextInputSession::new(pos, color, font_size));
        true
    }

    /// 提交流程：空文本不产生图元；有内容时先烘焙像素再入历史，保证可撤销。
    ///
    /// 返回是否实际产生了新图元，供测试与调用方判断是否需要重绘。
    pub(crate) fn commit_text_session(&mut self) -> bool {
        let Some(session) = self.active_text_input.take() else {
            return false;
        };
        let Some(shape) = session.commit() else {
            return false;
        };
        shape.draw(&mut self.item.pixmap.as_mut());
        self.annotations.push(shape);
        true
    }

    /// 取消当前会话，已输入但未提交的文字直接丢弃。
    pub(crate) fn cancel_text_session(&mut self) -> bool {
        self.active_text_input.take().is_some()
    }

    /// 处理会话中的按键：回车提交、Esc 取消、退格删除、其余可见字符直接录入。
    ///
    /// 该分支由 [`super::events::handle_paster_key`] 在会话激活时优先调用，
    /// 因此 Esc 不会继续落到「关闭贴图」的全局分支上。
    pub(crate) fn handle_text_session_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_text_session();
                true
            }
            Key::Named(NamedKey::Enter) => {
                self.commit_text_session();
                true
            }
            Key::Named(NamedKey::Backspace) => match &mut self.active_text_input {
                Some(session) => {
                    session.pop_char();
                    true
                }
                None => false,
            },
            Key::Named(NamedKey::Space) => match &mut self.active_text_input {
                Some(session) => {
                    session.push_char(' ');
                    true
                }
                None => false,
            },
            Key::Character(text) if !text.is_empty() => match &mut self.active_text_input {
                Some(session) => {
                    session.push_str(text.as_str());
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// 接收输入法确认提交的文本。
    pub(crate) fn apply_ime_commit(&mut self, text: &str) -> bool {
        match &mut self.active_text_input {
            Some(session) => {
                session.push_commit(text);
                true
            }
            None => false,
        }
    }

    /// 接收输入法预编辑文本，用于即时渲染与下划线提示。
    pub(crate) fn apply_ime_preedit(&mut self, text: &str) -> bool {
        match &mut self.active_text_input {
            Some(session) => {
                session.set_preedit(text);
                true
            }
            None => false,
        }
    }
}

/// 将当前会话按图像缩放倍率渲染到屏幕画布上。
///
/// 会话坐标与字号均为图像像素，乘以缩放倍数后才能与缩放后的贴图对齐；
/// 预编辑文本额外绘制下划线，提交后该下划线会随会话一起消失。
pub(crate) fn render_text_session_preview(state: &PasterWindowState, target: &mut Pixmap) {
    let Some(session) = &state.active_text_input else {
        return;
    };
    let scale = state.item.transform.scale.max(f32::EPSILON);
    session.render_scaled(&mut target.as_mut(), scale, true);
}

/// 根据会话启停同步 IME 开关，并在会话持续期间刷新候选框锚点。
///
/// `was_active` 由调用方在事件处理前采样，只有状态真正发生翻转时才调用
/// `set_ime_allowed`，避免高频输入事件反复触发平台 IME 对象增删。
pub(crate) fn sync_ime_state(state: &PasterWindowState, window: &Window, was_active: bool) {
    let active = state.has_active_text_session();
    if active != was_active {
        window.set_ime_allowed(active);
    }
    if active {
        refresh_ime_cursor_area(state, window);
    }
}

/// 将 IME 候选框锚定到当前会话在屏幕上的位置。
pub(crate) fn refresh_ime_cursor_area(state: &PasterWindowState, window: &Window) {
    let Some(session) = &state.active_text_input else {
        return;
    };
    let scale = state.item.transform.scale.max(f32::EPSILON);
    let (x, y) = image_to_screen_point(session.pos, scale);
    let font_size = (session.font_size * scale).max(8.0);
    window.set_ime_cursor_area(
        PhysicalPosition::new(x.round() as i32, y.round() as i32),
        PhysicalSize::new((24.0 * scale).max(8.0) as u32, font_size.ceil() as u32),
    );
}

/// 将工具栏前景色转换为标注图元使用的 RGBA 字节四元组。
fn color_to_rgba(color: Color) -> (u8, u8, u8, u8) {
    (
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        (color.alpha() * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paster::manager::PasterItem;
    use snapin_core::annotation::AnnotationShape;
    use tiny_skia::Pixmap;

    /// 构造一张启用文字工具的测试贴图状态
    fn text_state(width: u32, height: u32) -> PasterWindowState {
        let pixmap = Pixmap::new(width, height).expect("创建测试位图失败");
        let item = PasterItem::new(1, "测试贴图".to_string(), pixmap, (0, 0));
        let mut state = PasterWindowState::new(item);
        state.toolbar.active_tool = Some(ToolbarItem::ToolText);
        state
    }

    /// 提交会话后历史增加、展示图像被修改，且撤销可精确恢复底图
    #[test]
    fn 提交文字会话会写入图像并记录历史() {
        let mut state = text_state(200, 100);
        let original = state.item.pixmap.data().to_vec();

        assert!(state.begin_text_session((10.0, 10.0)));
        state
            .active_text_input
            .as_mut()
            .expect("会话应处于激活状态")
            .push_str("文字标注");

        assert!(state.commit_text_session(), "非空文本应成功提交");
        assert!(state.active_text_input.is_none(), "提交后会话必须结束");
        assert_eq!(
            state.annotations.undo_stack().len(),
            1,
            "提交后撤销栈长度应为 1"
        );
        assert_ne!(
            state.item.pixmap.data(),
            original.as_slice(),
            "提交后展示图像必须被修改"
        );

        match &state.annotations.undo_stack()[0] {
            AnnotationShape::Text(action) => {
                assert_eq!(action.text, "文字标注");
                assert_eq!(action.pos, (10.0, 10.0), "坐标必须使用图像像素");
            }
            other => panic!("应生成文字图元，实际为 {other:?}"),
        }

        assert!(state.undo_annotation(), "文字图元应支持撤销");
        assert_eq!(
            state.item.pixmap.data(),
            original.as_slice(),
            "撤销后必须精确恢复底图"
        );
    }

    /// 取消会话不写入图像、不产生历史
    #[test]
    fn 取消文字会话不产生图元() {
        let mut state = text_state(120, 80);
        let original = state.item.pixmap.data().to_vec();

        assert!(state.begin_text_session((20.0, 20.0)));
        state
            .active_text_input
            .as_mut()
            .expect("会话应处于激活状态")
            .push_str("不提交");
        assert!(state.cancel_text_session());

        assert!(state.active_text_input.is_none());
        assert!(
            state.annotations.undo_stack().is_empty(),
            "取消后不得入历史"
        );
        assert_eq!(
            state.item.pixmap.data(),
            original.as_slice(),
            "取消后展示图像必须保持不变"
        );
    }

    /// 空文本提交不产生图元，也不修改图像
    #[test]
    fn 空文本提交不产生图元() {
        let mut state = text_state(120, 80);
        let original = state.item.pixmap.data().to_vec();

        assert!(state.begin_text_session((20.0, 20.0)));
        assert!(!state.commit_text_session(), "空文本不应产生图元");

        assert!(state.active_text_input.is_none());
        assert!(state.annotations.undo_stack().is_empty());
        assert_eq!(state.item.pixmap.data(), original.as_slice());
    }

    /// 屏幕点击点必须按当前缩放换算为图像坐标
    #[test]
    fn 文字会话坐标按缩放换算() {
        let mut state = text_state(200, 100);
        state.item.transform.scale = 1.0;
        assert!(state.begin_text_session((30.0, 20.0)));
        assert_eq!(
            state
                .active_text_input
                .as_ref()
                .expect("会话应处于激活状态")
                .pos,
            (30.0, 20.0)
        );

        state.cancel_text_session();
        state.item.transform.scale = 1.67;
        assert!(state.begin_text_session((100.2, 83.5)));
        let pos = state
            .active_text_input
            .as_ref()
            .expect("会话应处于激活状态")
            .pos;
        assert!(
            (pos.0 - 60.0).abs() < 0.01,
            "缩放 1.67 时横坐标应约为 60，实际 {}",
            pos.0
        );
        assert!(
            (pos.1 - 50.0).abs() < 0.01,
            "缩放 1.67 时纵坐标应约为 50，实际 {}",
            pos.1
        );
    }

    /// 会话激活时 Esc 只取消会话，不得触发关闭贴图
    #[test]
    fn 会话激活时esc只取消会话() {
        let mut state = text_state(80, 60);
        assert!(state.begin_text_session((8.0, 8.0)));
        state
            .active_text_input
            .as_mut()
            .expect("会话应处于激活状态")
            .push_str("取消");

        assert!(state.handle_text_session_key(&Key::Named(NamedKey::Escape)));
        assert!(state.active_text_input.is_none());
        assert!(!state.should_close, "Esc 取消会话时不得关闭贴图");
        assert!(state.annotations.undo_stack().is_empty());
    }
}
