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
use crate::snipper::window::annotation_builder::{
    render_draft_preview, render_draft_preview_offset, AnnotationDraftParams,
};
use crate::snipper::window::ocr_editor::OcrTextEditorState;
use crate::snipper::window::ocr_menu::OcrContextMenuState;
use crate::snipper::window::redaction::{is_redaction_applied, RedactionPreview};
use crate::snipper::window::state::long_capture::LongCanvasSpace;
use crate::snipper::window::state::{LongCaptureSession, RecordingSession, RecordingStatus};
use crate::status_badge;
use image::RgbaImage;
use snapin_annotation::AnnotationHistory;
use snapin_render::{blit_straight_rgba_region_to_pixmap, BlitRgbaRegion};
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
        let is_long_capture_editing = self
            .long_capture
            .as_ref()
            .is_some_and(|session| session.is_editing);

        // 1. 按当前会话模式选择底图与标注叠加方式：
        // 长图编辑态使用视口大小的离屏画布，确保模糊/马赛克/聚光灯等像素级效果随长图平移并严格裁剪。
        if recording_completed {
            if let Some(preview) = self.recording_preview {
                self.draw_recording_preview(pixmap, rect, preview);
            } else {
                self.fast_cutout_restore(pixmap, rect, canvas_size);
            }
        } else if is_long_capture_editing {
            self.render_long_capture_editing_contents(pixmap, rect, canvas_size);
        } else {
            // 录屏视频来自独立的实时 PipeWire 流，覆盖层继续显示冻结底图以避免 Wayland
            // 透明表面合成发黑；长截图捕获阶段同样使用冻结底图作为滚动拼接基准。
            self.fast_cutout_restore(pixmap, rect, canvas_size);
            self.render_screen_space_overlays(pixmap);
        }

        // 2. 屏幕空间识别高亮与画笔光标、边框句柄。
        if !recording_completed {
            self.render_detection_previews(pixmap);
            self.render_active_brush_cursor(pixmap, rect);
        }
        if !recording_active && !recording_completed {
            let show_handles = !self
                .long_capture
                .as_ref()
                .is_some_and(|session| session.is_capturing || session.is_editing);
            self.render_selection_borders_and_handles(pixmap, rect, show_handles);
            if !is_long_capture_editing {
                if let Some(selected) = self.selected_annotation {
                    self.render_annotation_selection(pixmap, &selected);
                }
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

    /// 按屏幕坐标系在主画布上叠加常规截图或录制暂停时的标注与待提交预览。
    fn render_screen_space_overlays(&self, pixmap: &mut PixmapMut) {
        if let Some(history) = self.annotations {
            history.replay_all(pixmap);
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
    }

    /// 在长图编辑态创建与当前可视长图视口等大的预乘离屏画布，先绘制长图切片再按视口偏移回放标注与选框，
    /// 最后整块写回屏幕选区区域以实现毫秒级裁剪与标注对齐。
    fn render_long_capture_editing_contents(
        &self,
        pixmap: &mut PixmapMut,
        rect: &Rect,
        canvas_size: CanvasSize,
    ) {
        let Some(session) = self.long_capture.as_deref() else {
            return;
        };
        let Some(space) = LongCanvasSpace::from_session(
            session,
            rect,
            (canvas_size.width as f32, canvas_size.height as f32),
        ) else {
            return;
        };

        let copy_w = space.visible_size.0 as u32;
        let copy_h = space.visible_size.1 as u32;
        let Some(mut view_pixmap) = Pixmap::new(copy_w, copy_h) else {
            return;
        };

        blit_straight_rgba_region_to_pixmap(
            session.stitcher.canvas(),
            &mut view_pixmap.as_mut(),
            BlitRgbaRegion {
                src_x: space.canvas_offset.0 as u32,
                src_y: space.canvas_offset.1 as u32,
                dst_x: 0,
                dst_y: 0,
                width: copy_w,
                height: copy_h,
            },
        );
        self.render_canvas_space_overlays(&mut view_pixmap.as_mut(), &space);
        long_capture::copy_premultiplied_pixmap_region(
            pixmap,
            view_pixmap.as_ref(),
            space.viewport_screen.x as u32,
            space.viewport_screen.y as u32,
        );
    }

    /// 在离屏视口画布上按视口偏移回放已提交标注、文本会话、绘制中笔触及标注选中框。
    fn render_canvas_space_overlays(&self, view_pixmap: &mut PixmapMut, space: &LongCanvasSpace) {
        let (draw_ox, draw_oy) = space.viewport_draw_offset();
        if let Some(history) = self.annotations {
            history.replay_with_offset(view_pixmap, draw_ox, draw_oy);
        }
        if let Some(text_session) = self.active_text_input {
            text_session.render_offset(view_pixmap, draw_ox, draw_oy);
        }
        if let (Some(toolbar), Some((tool, points, badge))) = (self.toolbar, self.pending_stroke) {
            if let Some(params) =
                AnnotationDraftParams::from_toolbar_and_points(toolbar, *tool, points, badge)
            {
                render_draft_preview_offset(&params, view_pixmap, Point::new(draw_ox, draw_oy));
            }
        }
        if let Some(mut selected) = self.selected_annotation {
            selected.rect.x += draw_ox;
            selected.rect.y += draw_oy;
            self.render_annotation_selection(view_pixmap, &selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use snapin_annotation::action::AnnotationShape;
    use snapin_annotation::shapes::{RectAction, SpotlightAction, SpotlightShape};
    use snapin_annotation::LineStyle;

    fn test_canvas_with<'a>(
        base_bg: &'a Pixmap,
        original_rgba: &'a RgbaImage,
        selection: &'a SelectionState,
        magnifier: &'a Magnifier,
    ) -> SnipperCanvas<'a> {
        SnipperCanvas {
            base_bg,
            original_rgba,
            selection,
            cursor_pos: (0.0, 0.0),
            cursor_ready: false,
            hovered_window: None,
            magnifier,
            toolbar: None,
            annotations: None,
            ui_scale: 1.0,
            accent_color: (0, 122, 255),
            pending_stroke: None,
            active_text_input: None,
            status_message: None,
            selected_annotation: None,
            redaction_preview: None,
            qr_codes: None,
            ocr_menu: None,
            ocr_editor: None,
            recording: None,
            recording_preview: None,
            long_capture: None,
        }
    }

    fn pixel_at(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * pixmap.width() + x) * 4) as usize;
        [
            pixmap.data()[idx],
            pixmap.data()[idx + 1],
            pixmap.data()[idx + 2],
            pixmap.data()[idx + 3],
        ]
    }

    /// 横向编辑态必须使用水平偏移裁剪长图，并将结果对齐绘制到选区左上角。
    #[test]
    fn test_horizontal_editing_viewport_uses_x_offset() {
        let mut long_img = RgbaImage::from_pixel(20, 10, Rgba([10, 20, 30, 255]));
        for y in 0..10 {
            long_img.put_pixel(13, y, Rgba([200, 30, 40, 255]));
        }
        let mut session = LongCaptureSession::new(long_img);
        session.toggle_direction();
        session.enter_editing();
        session.viewport_offset_x = 10.0;

        let base_bg = Pixmap::new(20, 20).expect("测试画布可创建");
        let original = RgbaImage::from_pixel(20, 20, Rgba([0, 0, 0, 255]));
        let selection = SelectionState::Locked {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            active_drag: None,
        };
        let magnifier = Magnifier::default();
        let mut canvas = test_canvas_with(&base_bg, &original, &selection, &magnifier);
        canvas.long_capture = Some(&mut session);

        let mut out = Pixmap::new(20, 20).expect("输出画布可创建");
        canvas.render(&mut out.as_mut());

        assert_eq!(pixel_at(&out, 3, 5), [200, 30, 40, 255]);
        assert_eq!(pixel_at(&out, 4, 5), [10, 20, 30, 255]);
    }

    /// 长图编辑态下标注必须随视口滚动平移，且 Spotlight 等像素级效果严格裁剪在选区内。
    #[test]
    fn test_long_capture_editing_scrolls_and_clips_annotations_inside_selection() {
        let base_rgba = RgbaImage::from_pixel(30, 40, Rgba([15, 15, 15, 255]));
        let base_bg = snapin_render::rgba_image_to_pixmap(&base_rgba).expect("背景底图可创建");
        let original = RgbaImage::from_pixel(30, 40, Rgba([15, 15, 15, 255]));
        let long_img = RgbaImage::from_pixel(20, 60, Rgba([30, 90, 200, 255]));
        let mut session = LongCaptureSession::new(long_img);
        session.enter_editing();

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Spotlight(SpotlightAction {
            rect: Rect::new(2.0, 22.0, 16.0, 16.0),
            intensity: 180,
            shape: SpotlightShape::Rectangle,
            outline: false,
            outline_width: 2.0,
            outline_color: (255, 255, 255, 255).into(),
            line_style: LineStyle::Solid,
        }));
        history.push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(4.0, 25.0, 12.0, 10.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        let selection = SelectionState::Locked {
            rect: Rect::new(5.0, 5.0, 20.0, 20.0),
            active_drag: None,
        };
        let magnifier = Magnifier::default();
        let mut out = Pixmap::new(30, 40).expect("输出画布可创建");

        // 1. 偏移量为 0：矩形位于 y=25..35，处于 0..20 视口外，仅聚光灯聚焦到选区外；选区外部像素保持背景色。
        session.viewport_offset_y = 0.0;
        {
            let mut canvas = test_canvas_with(&base_bg, &original, &selection, &magnifier);
            canvas.annotations = Some(&history);
            canvas.long_capture = Some(&mut session);
            canvas.render(&mut out.as_mut());
        }
        assert_eq!(
            pixel_at(&out, 1, 1),
            [15, 15, 15, 255],
            "聚光灯与超界标注不得污染选区外桌面区域"
        );
        assert_ne!(
            pixel_at(&out, 12, 12),
            [255, 0, 0, 255],
            "偏移为 0 时下方未滚动到的红色矩形不得出现在当前视口"
        );

        // 2. 向下滚动 15px：画布 y=25..35 平移至屏幕 y=5+(25-15)=15..25（被裁剪在选区底边 y=25）。
        session.viewport_offset_y = 15.0;
        {
            let mut canvas = test_canvas_with(&base_bg, &original, &selection, &magnifier);
            canvas.annotations = Some(&history);
            canvas.long_capture = Some(&mut session);
            canvas.render(&mut out.as_mut());
        }
        assert_eq!(
            pixel_at(&out, 12, 20),
            [255, 0, 0, 255],
            "滚动进视口后，画布红框应准确对齐并呈现在屏幕 (12, 20)"
        );
        assert_eq!(
            pixel_at(&out, 12, 26),
            [15, 15, 15, 255],
            "红框超出选区底边 (y=25) 的部分必须被严格裁剪"
        );
        assert_eq!(
            pixel_at(&out, 1, 1),
            [15, 15, 15, 255],
            "滚动后选区外部仍不受 Spotlight 暗化污染"
        );
    }
}
