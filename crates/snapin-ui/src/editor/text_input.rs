//! 文本标注实时输入会话管理与即时输入光标渲染。

use crate::editor::toolbar::SubToolOptions;
use crate::geometry::Point;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::TextAction;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions, TextMeasureOptions};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};
use winit::keyboard::{Key, NamedKey};

/// 正在进行的文本标注交互输入会话
#[derive(Debug, Clone)]
pub struct TextInputSession {
    /// 文本起始左上角坐标 (x, y)
    pub pos: Point,
    /// 已提交确认的文本字符内容
    pub text: String,
    /// 候选输入法预编辑拼音内容 (IME Preedit)
    pub preedit: String,
    /// 文字颜色 (r, g, b, a)
    pub color: (u8, u8, u8, u8),
    /// 字体字号大小 (受鼠标滚轮连续动态微调，范围 12.0 ~ 72.0)
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub font_family: Option<String>,
    pub stroke_width: f32,
    pub stroke_color: (u8, u8, u8, u8),
    pub background_color: Option<(u8, u8, u8, u8)>,
    pub background_opacity: u8,
    pub background_padding: f32,
    pub background_corner_radius: f32,
}

impl TextInputSession {
    /// 创建新的文本输入会话
    pub fn new(pos: (f32, f32), color: (u8, u8, u8, u8), font_size: f32) -> Self {
        Self {
            pos: Point::from(pos),
            text: String::with_capacity(64),
            preedit: String::new(),
            color,
            font_size: font_size.clamp(12.0, 72.0),
            bold: false,
            italic: false,
            font_family: None,
            stroke_width: 0.0,
            stroke_color: (255, 255, 255, 255),
            background_color: None,
            background_opacity: 0,
            background_padding: 4.0,
            background_corner_radius: 4.0,
        }
    }

    /// 追加单个字符
    pub fn push_char(&mut self, ch: char) {
        self.text.push(ch);
    }

    /// 追加字符串切片
    pub fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
    }

    /// 接收输入法确认提交的中文/外文文本
    pub fn push_commit(&mut self, s: &str) {
        self.preedit.clear();
        self.text.push_str(s);
    }

    /// 接收输入法候选状态变动
    pub fn set_preedit(&mut self, s: &str) {
        self.preedit = s.to_string();
    }

    /// 删除末尾字符
    pub fn pop_char(&mut self) {
        if !self.preedit.is_empty() {
            self.preedit.pop();
        } else {
            self.text.pop();
        }
    }

    /// 动态调节字号
    pub fn adjust_font_size(&mut self, delta: f32) {
        self.font_size = (self.font_size + delta).clamp(12.0, 72.0);
    }

    /// 将工具栏当前文字样式应用到会话，保证输入预览与最终图元完全一致。
    pub fn apply_toolbar_options(&mut self, options: &SubToolOptions) {
        self.color = color_to_rgba(options.color);
        self.font_size = options.font_size;
        self.bold = options.text_bold;
        self.italic = options.text_italic;
        self.font_family = options.font_family.map(str::to_string);
        self.stroke_width = options.text_stroke_width;
        self.stroke_color = color_to_rgba(options.color);
        self.background_color = options.text_background_enabled.then_some(self.color);
        self.background_opacity = if options.text_background_enabled {
            options.text_background_opacity
        } else {
            0
        };
        self.background_padding = options.text_background_padding;
        self.background_corner_radius = options.text_background_corner_radius;
    }

    /// 结束输入并返回已确认文本与未提交预编辑拼接后的完整内容。
    pub fn finish_text(mut self) -> String {
        if !self.preedit.is_empty() {
            self.text.push_str(&self.preedit);
            self.preedit.clear();
        }
        self.text
    }

    /// 将当前会话转换为持久化标注动作
    pub fn commit(mut self) -> Option<AnnotationShape> {
        if !self.preedit.is_empty() {
            self.text.push_str(&self.preedit);
            self.preedit.clear();
        }
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(AnnotationShape::Text(TextAction {
                pos: self.pos,
                text: self.text,
                color: self.color.into(),
                font_size: self.font_size,
                bold: self.bold,
                italic: self.italic,
                font_family: self.font_family,
                stroke_width: self.stroke_width,
                stroke_color: self.stroke_color.into(),
                background_color: self.background_color.map(Into::into),
                background_opacity: self.background_opacity,
                padding: self.background_padding,
                corner_radius: self.background_corner_radius,
            }))
        }
    }

    /// 在画布上渲染当前输入的文字以及尾随光标与黑色边界框
    pub fn render(&self, pixmap: &mut PixmapMut) {
        self.render_scaled(pixmap, 1.0, false);
    }

    /// 按给定缩放倍率渲染输入框、文字与光标，可选为预编辑文本绘制下划线。
    ///
    /// 截图窗口的会话坐标本身就是屏幕物理像素，因此 [`Self::render`] 以 1.0 倍率调用；
    /// 贴图窗口的会话坐标是图像像素，渲染时必须乘以图像缩放倍率才能与屏幕对齐，
    /// 字号、内边距与光标也按同一倍率放大，保证预览与最终烘焙结果一致。
    /// `underline_preedit` 仅供贴图预览启用，避免改变截图侧既有的输入视觉效果。
    pub(crate) fn render_scaled(
        &self,
        pixmap: &mut PixmapMut,
        scale: f32,
        underline_preedit: bool,
    ) {
        let scale = if scale > f32::EPSILON { scale } else { 1.0 };
        let (x, y) = (self.pos.x * scale, self.pos.y * scale);
        // 缩小贴图时字号必须同步缩小；仅放大或 1:1 时保留 8 像素可读性下限
        let font_size = (self.font_size * scale).max(8.0 * scale.min(1.0));
        let line_height = font_size * 1.25;

        let display_text = if self.preedit.is_empty() {
            std::borrow::Cow::Borrowed(&self.text)
        } else {
            std::borrow::Cow::Owned(format!("{}{}", self.text, self.preedit))
        };

        let cursor_x = if display_text.is_empty() {
            let min_w = (font_size * 3.0).max(64.0 * scale);
            render_input_box(
                pixmap,
                InputBoxRect {
                    x: x - 6.0 * scale,
                    y: y - 4.0 * scale,
                    width: min_w,
                    height: line_height + 8.0 * scale,
                },
            );
            x
        } else {
            let frame = InputFrame {
                x,
                y,
                font_size,
                line_height,
                scale,
                text: display_text.as_ref(),
                underline_preedit,
            };
            self.draw_box_and_text(pixmap, &frame)
        };

        render_input_cursor(
            pixmap,
            TextCursorSpec {
                x: cursor_x,
                y,
                font_size,
                color: self.color,
            },
        );
    }

    /// 测量输入框范围、绘制黑色外框底衬与文字像素，并返回末尾光标 X 坐标
    fn draw_box_and_text(&self, pixmap: &mut PixmapMut, frame: &InputFrame<'_>) -> f32 {
        with_thread_local_text_renderer(|renderer| {
            let max_w = pixmap.width() as f32;
            let max_h = pixmap.height() as f32;
            let (line_w, cursor_offset) = renderer.measure_text_input(
                frame.text,
                TextMeasureOptions {
                    font_size: frame.font_size,
                    line_height: frame.line_height,
                    max_width: max_w,
                    max_height: max_h,
                },
            );
            draw_input_background(pixmap, frame, line_w);
            renderer.draw_text_rgba(
                pixmap,
                frame.text,
                TextDrawOptions::new(frame.x, frame.y, frame.font_size, self.color),
            );
            if frame.underline_preedit && !self.preedit.is_empty() {
                let (_, text_cursor) = renderer.measure_text_input(
                    &self.text,
                    TextMeasureOptions {
                        font_size: frame.font_size,
                        line_height: frame.line_height,
                        max_width: max_w,
                        max_height: max_h,
                    },
                );
                render_preedit_underline(
                    pixmap,
                    PreeditUnderline {
                        start_x: preedit_start_x(frame, self.text.is_empty(), text_cursor),
                        end_x: frame.x + cursor_offset,
                        y: frame.y + frame.font_size * 1.15,
                        scale: frame.scale,
                    },
                );
            }

            frame.x + cursor_offset
        })
    }
}

/// 绘制输入框黑色外框与底衬。
fn draw_input_background(pixmap: &mut PixmapMut, frame: &InputFrame<'_>, line_width: f32) {
    let min_width = (frame.font_size * 3.0).max(64.0 * frame.scale);
    let box_width = (line_width + 16.0 * frame.scale).max(min_width);
    let box_height = frame.line_height + 8.0 * frame.scale;
    render_input_box(
        pixmap,
        InputBoxRect {
            x: frame.x - 6.0 * frame.scale,
            y: frame.y - 4.0 * frame.scale,
            width: box_width,
            height: box_height,
        },
    );
}

/// 返回预编辑下划线相对输入框的起始 X 坐标。
fn preedit_start_x(frame: &InputFrame<'_>, text_empty: bool, text_cursor: f32) -> f32 {
    if text_empty {
        frame.x
    } else {
        frame.x + text_cursor
    }
}

/// 单帧输入框渲染参数，避免缩放渲染路径平铺过多入参。
struct InputFrame<'a> {
    x: f32,
    y: f32,
    font_size: f32,
    line_height: f32,
    scale: f32,
    text: &'a str,
    underline_preedit: bool,
}

/// 输入框矩形参数。
struct InputBoxRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// 绘制文字输入过程中的黑色外边框与高对比度浅白背景衬底
fn render_input_box(pixmap: &mut PixmapMut, input_box: InputBoxRect) {
    let InputBoxRect {
        x: box_x,
        y: box_y,
        width: box_w,
        height: box_h,
    } = input_box;
    let mut pb = PathBuilder::new();
    let r = 2.5;
    pb.move_to(box_x + r, box_y);
    pb.line_to(box_x + box_w - r, box_y);
    pb.quad_to(box_x + box_w, box_y, box_x + box_w, box_y + r);
    pb.line_to(box_x + box_w, box_y + box_h - r);
    pb.quad_to(
        box_x + box_w,
        box_y + box_h,
        box_x + box_w - r,
        box_y + box_h,
    );
    pb.line_to(box_x + r, box_y + box_h);
    pb.quad_to(box_x, box_y + box_h, box_x, box_y + box_h - r);
    pb.line_to(box_x, box_y + r);
    pb.quad_to(box_x, box_y, box_x + r, box_y);
    pb.close();

    if let Some(path) = pb.finish() {
        // 1. 半透明微白底色，确保在深色或复杂底图上文字和光标依然清晰
        let mut bg_paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        bg_paint.set_color_rgba8(255, 255, 255, 210);
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        // 2. 经典深黑外边框
        let mut border_paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        border_paint.set_color_rgba8(15, 23, 42, 240);
        let stroke = Stroke {
            width: 1.5,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }
}

/// 预编辑文本下划线参数。
struct PreeditUnderline {
    start_x: f32,
    end_x: f32,
    y: f32,
    scale: f32,
}

/// 绘制预编辑文本下划线，示意该段文字仍处于输入法候选状态。
fn render_preedit_underline(pixmap: &mut PixmapMut, underline: PreeditUnderline) {
    let PreeditUnderline {
        start_x: x0,
        end_x: x1,
        y,
        scale,
    } = underline;
    if x1 <= x0 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y);
    pb.line_to(x1, y);
    if let Some(path) = pb.finish() {
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        paint.set_color_rgba8(15, 23, 42, 230);
        let stroke = Stroke {
            width: 1.5 * scale.max(0.1),
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 输入光标竖线参数。
struct TextCursorSpec {
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
}

/// 绘制输入光标指示竖线
fn render_input_cursor(pixmap: &mut PixmapMut, cursor: TextCursorSpec) {
    let TextCursorSpec {
        x: cx,
        y: cy,
        font_size,
        color,
    } = cursor;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy);
    pb.line_to(cx, cy + font_size * 1.15);
    if let Some(path) = pb.finish() {
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        paint.set_color_rgba8(color.0, color.1, color.2, 255);
        let stroke = Stroke {
            width: 2.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 文本会话按键处理结果。
///
/// 会话只负责维护字符内容；提交/取消需要访问标注历史与画布，因此把动作返回给
/// 截图/贴图各自的状态层执行，避免共享层反向依赖具体窗口状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSessionAction {
    /// 按键已被输入会话消费，内容已更新
    Handled,
    /// 请求提交当前文本
    Commit,
    /// 请求取消当前文本
    Cancel,
    /// 与文本输入无关，交回调用方处理
    Ignored,
}

/// 将 tiny-skia 颜色转换为持久化图元使用的 RGBA 元组。
fn color_to_rgba(color: tiny_skia::Color) -> (u8, u8, u8, u8) {
    (
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        (color.alpha() * 255.0).round() as u8,
    )
}

/// 统一处理文本输入会话中的按键。
///
/// Esc 统一取消当前输入、Enter 提交，避免截图侧提交而贴图侧取消的行为分叉；
/// Ctrl 组合键不写入普通字符，交由窗口快捷键处理。
pub fn handle_session_key(
    session: &mut TextInputSession,
    key: &Key,
    ctrl_pressed: bool,
) -> TextSessionAction {
    match key {
        Key::Named(NamedKey::Escape) => TextSessionAction::Cancel,
        Key::Named(NamedKey::Enter) => TextSessionAction::Commit,
        Key::Named(NamedKey::Backspace) => {
            session.pop_char();
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::Space) => {
            session.push_char(' ');
            TextSessionAction::Handled
        }
        Key::Character(text) if !ctrl_pressed && !text.is_empty() => {
            session.push_str(text.as_str());
            TextSessionAction::Handled
        }
        _ => TextSessionAction::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn test_text_input_session_lifecycle() {
        let mut session = TextInputSession::new((50.0, 50.0), (255, 0, 0, 255), 18.0);
        assert_eq!(session.text, "");

        session.push_char('H');
        session.push_str("ello 世界");
        assert_eq!(session.text, "Hello 世界");

        session.pop_char();
        assert_eq!(session.text, "Hello 世");

        // 验证 IME 预编辑与提交
        session.set_preedit("jie");
        assert_eq!(session.preedit, "jie");
        session.push_commit("界！");
        assert_eq!(session.text, "Hello 世界！");
        assert_eq!(session.preedit, "");

        session.adjust_font_size(4.0);
        assert_eq!(session.font_size, 22.0);

        let action = session.commit();
        assert!(action.is_some());
    }

    #[test]
    fn test_empty_text_input_commit_none() {
        let session = TextInputSession::new((50.0, 50.0), (255, 0, 0, 255), 18.0);
        assert!(session.commit().is_none());
    }

    #[test]
    fn test_text_input_render_pixels() {
        let mut pixmap = Pixmap::new(200, 100).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);

        let mut session = TextInputSession::new((10.0, 20.0), (255, 255, 255, 255), 20.0);
        session.push_str("测试文本");
        session.render(&mut pixmap.as_mut());

        // 验证绘制了非黑色像素 (包含文字与光标)
        let has_content = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_empty_text_input_renders_box() {
        let mut pixmap = Pixmap::new(200, 100).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);

        let session = TextInputSession::new((10.0, 20.0), (255, 0, 0, 255), 20.0);
        assert!(session.text.is_empty());
        session.render(&mut pixmap.as_mut());

        // 验证空输入时仍绘制了黑色外框底衬与光标
        let has_pixels = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_pixels);
    }

    /// Esc 统一取消、Enter 提交，Ctrl 组合字符不进入文本内容。
    #[test]
    fn test_shared_text_session_key_actions() {
        let mut session = TextInputSession::new((0.0, 0.0), (255, 255, 255, 255), 16.0);
        assert_eq!(
            handle_session_key(&mut session, &Key::Named(NamedKey::Escape), false),
            TextSessionAction::Cancel
        );
        assert_eq!(
            handle_session_key(&mut session, &Key::Named(NamedKey::Enter), false),
            TextSessionAction::Commit
        );
        assert_eq!(
            handle_session_key(&mut session, &Key::Character("a".into()), true),
            TextSessionAction::Ignored
        );
        assert_eq!(
            handle_session_key(&mut session, &Key::Character("a".into()), false),
            TextSessionAction::Handled
        );
        assert_eq!(session.text, "a");
    }
}
