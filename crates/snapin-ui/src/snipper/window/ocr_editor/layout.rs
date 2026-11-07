//! 文字识别全文编辑框布局计算与鼠标命中测试。

use super::state::OcrTextEditorState;
use crate::geometry::Rect;

/// 面板基准尺寸与间距。
pub(crate) const PANEL_WIDTH: f32 = 680.0;
pub(crate) const PANEL_HEIGHT: f32 = 460.0;
pub(crate) const SCREEN_MARGIN: f32 = 16.0;
pub(crate) const HEADER_HEIGHT: f32 = 42.0;
pub(crate) const FOOTER_HEIGHT: f32 = 46.0;
pub(crate) const PADDING: f32 = 14.0;
pub(crate) const SCROLLBAR_WIDTH: f32 = 8.0;

/// 编辑框内的操作按钮标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcrEditorButton {
    /// 复制全文并关闭窗口 (Ctrl+Enter)
    CopyAndClose,
    /// 仅复制全文或选中文本
    CopyOnly,
    /// 一键另存为 Markdown (.md) 文件
    SaveMarkdown,
    /// 一键排版整理
    Format,
    /// 清空内容
    Clear,
    /// 取消并关闭 (Esc)
    Cancel,
    /// 标题栏右上角关闭按钮
    CloseHeader,
}

impl OcrEditorButton {
    /// 返回按钮显示标签。
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::CopyAndClose => "复制并关闭",
            Self::CopyOnly => "仅复制",
            Self::SaveMarkdown => "另存为 .md",
            Self::Format => "整理排版",
            Self::Clear => "清空",
            Self::Cancel => "取消",
            Self::CloseHeader => "×",
        }
    }

    /// 是否为主强调色按钮。
    pub(crate) fn is_primary(&self) -> bool {
        matches!(self, Self::CopyAndClose)
    }
}

/// 滚动条命中测试结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollbarHit {
    Thumb,
    Track,
}

/// 编辑框完整排版布局信息。
#[derive(Debug, Clone)]
pub(crate) struct OcrEditorLayout {
    pub(crate) panel: Rect,
    pub(crate) header: Rect,
    pub(crate) close_button: Rect,
    pub(crate) body: Rect,
    pub(crate) count_badge: Rect,
    pub(crate) buttons: Vec<(OcrEditorButton, Rect)>,
    pub(crate) scrollbar_track: Option<Rect>,
    pub(crate) scrollbar_thumb: Option<Rect>,
    pub(crate) scale: f32,
}

impl OcrEditorLayout {
    /// 命中测试按钮。
    pub(crate) fn hit_test_button(&self, x: f32, y: f32) -> Option<OcrEditorButton> {
        if self.close_button.contains(x, y) {
            return Some(OcrEditorButton::CloseHeader);
        }
        for (btn, rect) in &self.buttons {
            if rect.contains(x, y) {
                return Some(*btn);
            }
        }
        None
    }

    /// 命中测试标题栏（排除关闭按钮，用于触发窗口拖拽移动）。
    pub(crate) fn hit_test_header_drag(&self, x: f32, y: f32) -> bool {
        self.header.contains(x, y) && !self.close_button.contains(x, y)
    }

    /// 命中测试正文区域。
    pub(crate) fn hit_test_body(&self, x: f32, y: f32) -> bool {
        self.body.contains(x, y)
    }

    /// 命中测试滚动条。
    pub(crate) fn hit_test_scrollbar(&self, x: f32, y: f32) -> Option<ScrollbarHit> {
        if let Some(thumb) = self.scrollbar_thumb {
            if thumb.contains(x, y) {
                return Some(ScrollbarHit::Thumb);
            }
        }
        if let Some(track) = self.scrollbar_track {
            if track.contains(x, y) {
                return Some(ScrollbarHit::Track);
            }
        }
        None
    }
}

/// 计算 OCR 全文编辑框的完整几何布局。
pub(crate) fn compute_editor_layout(
    state: &OcrTextEditorState,
    screen_width: f32,
    screen_height: f32,
    scale: f32,
) -> OcrEditorLayout {
    let scale = scale.max(0.5);
    let width = (PANEL_WIDTH * scale).min((screen_width - SCREEN_MARGIN * 2.0 * scale).max(1.0));
    let height = (PANEL_HEIGHT * scale).min((screen_height - SCREEN_MARGIN * 2.0 * scale).max(1.0));

    // 计算面板绝对坐标：优先使用用户拖拽的位置，并防止移出屏幕
    let (panel_x, panel_y) = match state.custom_position {
        Some((cx, cy)) => {
            let max_x = (screen_width - width - SCREEN_MARGIN * scale).max(SCREEN_MARGIN * scale);
            let max_y = (screen_height - height - SCREEN_MARGIN * scale).max(SCREEN_MARGIN * scale);
            (
                cx.clamp(SCREEN_MARGIN * scale, max_x),
                cy.clamp(SCREEN_MARGIN * scale, max_y),
            )
        }
        None => ((screen_width - width) / 2.0, (screen_height - height) / 2.0),
    };

    let panel = Rect {
        x: panel_x,
        y: panel_y,
        width,
        height,
    };

    let header = Rect {
        x: panel.x,
        y: panel.y,
        width: panel.width,
        height: HEADER_HEIGHT * scale,
    };

    let close_size = 28.0 * scale;
    let close_button = Rect {
        x: panel.x + panel.width - close_size - 8.0 * scale,
        y: panel.y + (HEADER_HEIGHT * scale - close_size) / 2.0,
        width: close_size,
        height: close_size,
    };

    let padding = PADDING * scale;
    let body = Rect {
        x: panel.x + padding,
        y: panel.y + HEADER_HEIGHT * scale,
        width: (panel.width - padding * 2.0).max(1.0),
        height: (panel.height - HEADER_HEIGHT * scale - FOOTER_HEIGHT * scale).max(1.0),
    };

    let footer = Rect {
        x: panel.x + padding,
        y: panel.y + panel.height - FOOTER_HEIGHT * scale,
        width: panel.width - padding * 2.0,
        height: FOOTER_HEIGHT * scale,
    };

    // 统计标签区域
    let count_badge = Rect {
        x: footer.x,
        y: footer.y + (footer.height - 24.0 * scale) / 2.0,
        width: 140.0 * scale,
        height: 24.0 * scale,
    };

    // 底部实体按钮布局（从右向左紧凑排列）
    let btn_height = 28.0 * scale;
    let btn_spacing = 8.0 * scale;
    let btn_y = footer.y + (footer.height - btn_height) / 2.0;

    let button_defs = [
        (OcrEditorButton::CopyAndClose, 100.0 * scale),
        (OcrEditorButton::CopyOnly, 72.0 * scale),
        (OcrEditorButton::SaveMarkdown, 92.0 * scale),
        (OcrEditorButton::Format, 78.0 * scale),
        (OcrEditorButton::Clear, 58.0 * scale),
        (OcrEditorButton::Cancel, 58.0 * scale),
    ];

    let mut current_right = footer.x + footer.width;
    let mut buttons = Vec::with_capacity(button_defs.len());

    for (btn, w) in button_defs {
        let x = current_right - w;
        buttons.push((
            btn,
            Rect {
                x,
                y: btn_y,
                width: w,
                height: btn_height,
            },
        ));
        current_right = x - btn_spacing;
    }

    // 滚动条几何计算
    let (scrollbar_track, scrollbar_thumb) = if state.content_height > body.height {
        let track_width = SCROLLBAR_WIDTH * scale;
        let track = Rect {
            x: body.x + body.width - track_width - 2.0 * scale,
            y: body.y + 2.0 * scale,
            width: track_width,
            height: (body.height - 4.0 * scale).max(1.0),
        };
        let visible_ratio = (body.height / state.content_height).clamp(0.05, 1.0);
        let thumb_height = (track.height * visible_ratio).max(20.0 * scale);
        let max_scroll = (state.content_height - body.height).max(0.0);
        let scroll_ratio = if max_scroll > 0.0 {
            (state.scroll_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let thumb_y = track.y + (track.height - thumb_height) * scroll_ratio;
        let thumb = Rect {
            x: track.x,
            y: thumb_y,
            width: track.width,
            height: thumb_height,
        };
        (Some(track), Some(thumb))
    } else {
        (None, None)
    };

    OcrEditorLayout {
        panel,
        header,
        close_button,
        body,
        count_badge,
        buttons,
        scrollbar_track,
        scrollbar_thumb,
        scale,
    }
}
