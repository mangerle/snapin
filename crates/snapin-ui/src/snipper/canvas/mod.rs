//! 选区全屏复合光栅化渲染管线。
//!
//! 基于 tiny-skia 实现全屏暗色遮罩、选区镂空聚焦、8 手柄线框以及像素放大镜复合渲染。

mod overlays;
mod selection;

use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::{Tool, Toolbar};
use crate::geometry::Point;
use crate::ports::QrCodeItem;
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window::annotation_builder::{render_draft_preview, AnnotationDraftParams};
use crate::snipper::window::ocr_panel::OcrPanelState;
use crate::snipper::window::redaction::{is_redaction_applied, RedactionPreview};
use crate::status_badge;
use image::RgbaImage;
use snapin_annotation::AnnotationHistory;
use snapin_render::{with_thread_local_text_renderer, TextBoxOptions};
use tiny_skia::{
    Paint, PathBuilder, Pixmap, PixmapMut, Rect as SkiaRect, Stroke, StrokeDash, Transform,
};

/// 正在拉伸绘制中的待提交标注笔触元数据 (工具类型, 轨迹点集合, 预设色值)
pub type PendingStroke<'a> = (&'a Tool, &'a [Point], u32);

/// 选中标注的编辑框信息
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectedAnnotation {
    /// 标注的轴对齐包围盒
    pub rect: Rect,
    /// 是否允许通过手柄拉伸（原位翻译块只允许整体移动）
    pub resizable: bool,
}

/// 全屏选区画布渲染上下文
pub struct SnipperCanvas<'a> {
    pub base_bg: &'a Pixmap,
    pub original_rgba: &'a RgbaImage,
    pub selection: &'a SelectionState,
    pub cursor_pos: (f64, f64),
    /// 是否已收到首个光标位置；未就绪时隐藏放大镜，避免在左上角绘制占位框
    pub cursor_ready: bool,
    /// 未拉框时鼠标悬停的窗口矩形，用于窗口截图预览
    pub hovered_window: Option<Rect>,
    pub magnifier: &'a Magnifier,
    pub toolbar: Option<&'a Toolbar>,
    pub annotations: Option<&'a AnnotationHistory>,
    pub ui_scale: f32,
    pub accent_color: (u8, u8, u8),
    pub pending_stroke: Option<PendingStroke<'a>>,
    pub active_text_input: Option<&'a TextInputSession>,
    pub status_message: Option<&'a str>,
    /// 当前选中的已提交标注编辑框
    pub selected_annotation: Option<SelectedAnnotation>,
    /// 智能打码可点击文字区域
    pub redaction_preview: Option<&'a RedactionPreview>,
    /// 最近一次二维码识别结果的包围盒
    pub qr_codes: Option<&'a [QrCodeItem]>,
    /// OCR 结果面板
    pub ocr_panel: Option<&'a OcrPanelState>,
}

impl<'a> SnipperCanvas<'a> {
    /// 执行完整帧的高性能复合渲染并写入输出 pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        pixmap.data_mut().copy_from_slice(self.base_bg.data());
        if let Some(rect) = self.active_selection_rect() {
            if rect.width > 1.0 && rect.height > 1.0 {
                self.render_selection_contents(pixmap, &rect);
            }
        } else {
            self.render_idle_overlay(pixmap, true);
        }
        self.render_status_message(pixmap);
    }

    /// 返回当前创建中或已锁定的选区矩形。
    fn active_selection_rect(&self) -> Option<Rect> {
        match self.selection {
            SelectionState::Idle => None,
            SelectionState::Creating { start, current } => {
                Some(Rect::from_points(*start, *current))
            }
            SelectionState::Locked { rect, .. } => Some(*rect),
        }
    }

    /// 绘制选区底图、标注、实时交互反馈、边框与工具栏。
    fn render_selection_contents(&self, pixmap: &mut PixmapMut, rect: &Rect) {
        let (width, height) = (pixmap.width(), pixmap.height());
        self.fast_cutout_restore(pixmap, rect, width, height);
        if let Some(history) = self.annotations {
            history.replay_all(pixmap);
        }
        if let Some(panel) = self.ocr_panel {
            self.render_ocr_panel(pixmap, panel);
        }
        if let Some(text_session) = self.active_text_input {
            text_session.render(pixmap);
        }
        if let (Some(toolbar), Some((tool, points, badge))) = (self.toolbar, self.pending_stroke) {
            self.render_pending_preview(
                pixmap,
                selection::PendingPreviewSpec {
                    toolbar,
                    tool,
                    points,
                    badge_number: badge,
                },
            );
        }
        self.render_detection_previews(pixmap);
        self.render_active_brush_cursor(pixmap, rect);
        self.render_selection_borders_and_handles(pixmap, rect);
        if let Some(selected) = self.selected_annotation {
            self.render_annotation_selection(pixmap, &selected);
        }
        if matches!(self.selection, SelectionState::Locked { .. }) {
            if let Some(toolbar) = self.toolbar {
                toolbar.render(pixmap);
            }
        }
    }
}
