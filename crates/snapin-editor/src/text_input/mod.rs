//! 文本标注实时输入会话管理、光标导航与即时渲染。

mod cursor;
mod render;
mod session;
#[cfg(test)]
mod tests;

pub use cursor::{handle_session_key, KeyModifiers, TextSessionAction};
pub use session::TextInputSession;
