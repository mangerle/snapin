//! 选区全屏复合光栅化渲染管线。
//!
//! 基于 tiny-skia 实现全屏暗色遮罩、选区镂空聚焦、8 手柄线框以及像素放大镜复合渲染。

pub(crate) mod long_capture;
mod overlays;
pub(crate) mod recording;
mod selection;

use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::{Tool, Toolbar};
use crate::geometry::Point;
use crate::ports::QrCodeItem;
use crate::snipper::magnifier::Magnifier;
use crate::snipper::selection::{Rect, SelectionState};
use crate::snipper::window::annotation_builder::{render_draft_preview, AnnotationDraftParams};
use crate::snipper::window::ocr_editor::OcrTextEditorState;
use crate::snipper::window::ocr_menu::OcrContextMenuState;
use crate::snipper::window::redaction::{is_redaction_applied, RedactionPreview};
use crate::snipper::window::state::{LongCaptureSession, RecordingSession, RecordingStatus};
use crate::status_badge;
use image::RgbaImage;
use snapin_annotation::AnnotationHistory;
use tiny_skia::{
    FilterQuality, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect as SkiaRect, Stroke,
    StrokeDash, Transform,
};

/// 正在拉伸绘制中的待提交标注笔触元数据 (工具类型, 轨迹点集合, 预设色值)
pub type PendingStroke<'a> = (&'a Tool, &'a [Point], u32);

/// 选区画布的像素尺寸。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanvasSize {
    width: u32,
    height: u32,
}

impl CanvasSize {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

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
    /// 文字识别主按钮右键菜单
    pub ocr_menu: Option<&'a OcrContextMenuState>,
    /// 文字识别全文编辑框
    pub ocr_editor: Option<&'a OcrTextEditorState>,
    /// 选区屏幕录制会话（若激活）
    pub recording: Option<&'a RecordingSession>,
    /// 结果页当前内嵌预览帧。
    pub recording_preview: Option<&'a Pixmap>,
    /// 滚动拼接长截图会话（若激活）
    pub long_capture: Option<&'a mut LongCaptureSession>,
}

impl<'a> SnipperCanvas<'a> {
    /// 执行完整帧的高性能复合渲染并写入输出 pixmap
    pub fn render(&mut self, pixmap: &mut PixmapMut) {
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
    fn render_selection_contents(&mut self, pixmap: &mut PixmapMut, rect: &Rect) {
        let canvas_size = CanvasSize::new(pixmap.width(), pixmap.height());
        let recording_status = self.recording.map(RecordingSession::status);
        let recording_active = recording_status.is_some_and(|status| status.is_capturing());
        let recording_completed =
            recording_status.is_some_and(|status| status == RecordingStatus::Completed);

        // 1. 选择选区底图来源：录屏结果预览优先，其次为长截图编辑画布，最后透出屏幕原图。
        if recording_completed {
            if let Some(preview) = self.recording_preview {
                self.draw_recording_preview(pixmap, rect, preview);
            } else {
                self.fast_cutout_restore(pixmap, rect, canvas_size);
            }
        } else if self
            .long_capture
            .as_ref()
            .is_some_and(|session| session.is_editing)
        {
            if let Some(session) = &self.long_capture {
                long_capture::render_long_canvas_viewport(pixmap, session, rect, canvas_size);
            }
        } else {
            // 录屏视频来自独立的实时 PipeWire 流，覆盖层继续显示冻结底图以避免 Wayland
            // 透明表面合成发黑；长截图捕获阶段同样使用冻结底图作为滚动拼接基准。
            self.fast_cutout_restore(pixmap, rect, canvas_size);
        }

        // 2. 录制完成后的结果页只展示预览，不回放交互标注与手柄。
        if !recording_completed {
            if let Some(history) = self.annotations {
                history.replay_all(pixmap);
            }
            if let Some(text_session) = self.active_text_input {
                text_session.render(pixmap);
            }
            if let (Some(toolbar), Some((tool, points, badge))) =
                (self.toolbar, self.pending_stroke)
            {
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
        }
        if !recording_active && !recording_completed {
            let show_handles = !self
                .long_capture
                .as_ref()
                .is_some_and(|session| session.is_capturing || session.is_editing);
            self.render_selection_borders_and_handles(pixmap, rect, show_handles);
            if let Some(selected) = self.selected_annotation {
                self.render_annotation_selection(pixmap, &selected);
            }
        }

        // 3. 录屏工具栏、长截图专用控制栏与常规工具栏三选一。
        if let Some(session) = self.recording {
            if let (Some(toolbar), Some(selection)) = (self.toolbar, self.active_selection_rect()) {
                recording::render_recording_toolbar(
                    pixmap,
                    recording::RecordingRenderContext {
                        session,
                        toolbar,
                        selection: &selection,
                        screen_size: (pixmap.width(), pixmap.height()),
                        ui_scale: self.ui_scale,
                    },
                );
            }
        } else if let Some(session) = self.long_capture.as_mut() {
            let ui_scale = self.ui_scale;
            long_capture::render_long_capture_dock(pixmap, session, rect, canvas_size, ui_scale);
            if session.is_editing {
                if let Some(toolbar) = self.toolbar {
                    toolbar.render(pixmap);
                }
            } else {
                long_capture::render_long_capture_control_bar(
                    pixmap,
                    session,
                    rect,
                    canvas_size,
                    ui_scale,
                );
            }
        } else if matches!(self.selection, SelectionState::Locked { .. }) {
            if let Some(toolbar) = self.toolbar {
                toolbar.render(pixmap);
            }
        }

        if let Some(menu) = self.ocr_menu {
            menu.render(pixmap);
        }
        if let Some(editor) = self.ocr_editor {
            editor.render(pixmap, self.ui_scale);
        }
    }
}
