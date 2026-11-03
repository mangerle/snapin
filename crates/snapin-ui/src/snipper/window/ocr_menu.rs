//! 文字识别主按钮的右键菜单状态、命中与绘制。

use crate::drawing::{add_round_rect, make_paint, RoundRect};
use crate::geometry::Rect;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Color, PixmapMut, Stroke, Transform};

/// 文字识别右键菜单动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcrMenuAction {
    /// 识别文字、复制全文并关闭截图窗口。
    CopyText,
    /// 识别文字并打开全文编辑框。
    EditText,
}

impl OcrMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::CopyText => "复制文字",
            Self::EditText => "编辑文字",
        }
    }
}

/// 文字识别右键菜单的固定两行布局。
#[derive(Debug, Clone)]
pub(crate) struct OcrContextMenuState {
    rect: Rect,
    rows: [(OcrMenuAction, Rect); 2],
    hovered: Option<OcrMenuAction>,
    scale: f32,
}

impl OcrContextMenuState {
    /// 根据文字识别按钮位置打开菜单，并自动避让屏幕边缘。
    pub(crate) fn open(anchor: Rect, screen_width: f32, screen_height: f32, scale: f32) -> Self {
        let scale = scale.max(0.5);
        let width = 136.0 * scale;
        let row_height = 30.0 * scale;
        let padding = 4.0 * scale;
        let height = row_height * 2.0 + padding * 2.0;
        let margin = 8.0 * scale;
        let x = (anchor.x + anchor.width - width)
            .clamp(margin, (screen_width - width - margin).max(margin));
        let below_y = anchor.y + anchor.height + 4.0 * scale;
        let y = if below_y + height <= screen_height - margin {
            below_y
        } else {
            (anchor.y - height - 4.0 * scale).max(margin)
        };
        let rect = Rect {
            x,
            y,
            width,
            height,
        };
        let first = Rect {
            x: x + padding,
            y: y + padding,
            width: width - padding * 2.0,
            height: row_height,
        };
        let second = Rect {
            y: first.y + row_height,
            ..first
        };
        Self {
            rect,
            rows: [
                (OcrMenuAction::CopyText, first),
                (OcrMenuAction::EditText, second),
            ],
            hovered: None,
            scale,
        }
    }

    /// 返回指定坐标对应的菜单动作。
    pub(crate) fn hit_test(&self, x: f32, y: f32) -> Option<OcrMenuAction> {
        self.rows
            .iter()
            .find_map(|(action, rect)| rect.contains(x, y).then_some(*action))
    }

    /// 更新悬停行；返回悬停状态是否变化。
    pub(crate) fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let hovered = self.hit_test(x, y);
        let changed = hovered != self.hovered;
        self.hovered = hovered;
        changed
    }

    /// 返回指定动作的菜单行矩形，供交互测试精确定位。
    #[cfg(test)]
    pub(crate) fn row_rect(&self, action: OcrMenuAction) -> Option<Rect> {
        self.rows
            .iter()
            .find_map(|(candidate, rect)| (*candidate == action).then_some(*rect))
    }

    /// 绘制菜单面板、悬停底色与两行文字。
    pub(crate) fn render(&self, pixmap: &mut PixmapMut) {
        self.render_panel_surface(pixmap);
        self.render_rows(pixmap);
    }

    fn render_panel_surface(&self, pixmap: &mut PixmapMut) {
        let mut panel = tiny_skia::PathBuilder::new();
        add_round_rect(
            &mut panel,
            RoundRect {
                x: self.rect.x,
                y: self.rect.y,
                width: self.rect.width,
                height: self.rect.height,
                radius: 7.0 * self.scale,
            },
        );
        let Some(path) = panel.finish() else {
            return;
        };
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(255, 255, 255, 252)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(203, 213, 225, 245)),
            &Stroke {
                width: self.scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }

    fn render_rows(&self, pixmap: &mut PixmapMut) {
        with_thread_local_text_renderer(|renderer| {
            for (action, row) in self.rows {
                let hovered = self.hovered == Some(action);
                if hovered {
                    let mut hover = tiny_skia::PathBuilder::new();
                    add_round_rect(
                        &mut hover,
                        RoundRect {
                            x: row.x + 3.0 * self.scale,
                            y: row.y + 2.0 * self.scale,
                            width: row.width - 6.0 * self.scale,
                            height: row.height - 4.0 * self.scale,
                            radius: 5.0 * self.scale,
                        },
                    );
                    if let Some(path) = hover.finish() {
                        pixmap.fill_path(
                            &path,
                            &make_paint(Color::from_rgba8(37, 99, 235, 242)),
                            tiny_skia::FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    }
                }
                renderer.draw_text_rgba(
                    pixmap,
                    snapin_foundation::i18n::tr(action.label()),
                    TextDrawOptions::new(
                        row.x + 12.0 * self.scale,
                        row.y + 7.0 * self.scale,
                        14.0 * self.scale,
                        if hovered {
                            (255, 255, 255, 255)
                        } else {
                            (30, 41, 59, 255)
                        },
                    ),
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 菜单必须完整落在屏幕内，并能精确命中两个动作。
    #[test]
    fn test_ocr_context_menu_hit_test() {
        let menu =
            OcrContextMenuState::open(Rect::new(1900.0, 1000.0, 30.0, 30.0), 1920.0, 1080.0, 1.0);
        assert!(menu.rect.x >= 8.0);
        assert!(menu.rect.y >= 8.0);
        assert_eq!(
            menu.hit_test(menu.rows[0].1.x + 2.0, menu.rows[0].1.y + 2.0),
            Some(OcrMenuAction::CopyText)
        );
        assert_eq!(
            menu.hit_test(menu.rows[1].1.x + 2.0, menu.rows[1].1.y + 2.0),
            Some(OcrMenuAction::EditText)
        );
    }

    /// 菜单必须绘制可见面板与两项文字区域。
    #[test]
    fn test_ocr_context_menu_renders_visible_pixels() {
        let menu =
            OcrContextMenuState::open(Rect::new(330.0, 100.0, 32.0, 32.0), 800.0, 400.0, 1.0);
        let mut pixmap = Pixmap::new(800, 400).expect("测试位图应可分配");
        pixmap.fill(Color::TRANSPARENT);
        menu.render(&mut pixmap.as_mut());
        assert!(pixmap
            .data()
            .iter()
            .skip(3)
            .step_by(4)
            .any(|alpha| *alpha > 0));
    }
}
