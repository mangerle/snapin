//! 标注图元命令模式与撤销/重做控制。
//!
//! 将每一次绘制操作封装为具备 Execute 与 Undo 能力的 Command 对象，
//! 采用命令模式维护 Undo/Redo 历史栈。

use crate::geometry::Rect;
use crate::shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MagnifierAction, MarkerAction,
    MosaicAction, PenAction, PolylineAction, RectAction, SizeParam, SpotlightAction, TextAction,
    TranslationAction,
};
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 所有支持的闭合矢量标注图元枚举 (代数数据类型，天然支持无堆分配存储、值拷贝与 Serde 序列化)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationShape {
    Rect(RectAction),
    Ellipse(EllipseAction),
    Arrow(ArrowAction),
    Pen(PenAction),
    Polyline(PolylineAction),
    Marker(MarkerAction),
    Mosaic(MosaicAction),
    Blur(BlurAction),
    Spotlight(SpotlightAction),
    Magnifier(MagnifierAction),
    Badge(BadgeAction),
    Text(TextAction),
    Translation(TranslationAction),
}

impl AnnotationShape {
    /// 执行带坐标偏移量的绘制
    pub fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        match self {
            Self::Rect(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Ellipse(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Arrow(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Pen(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Polyline(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Marker(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Mosaic(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Blur(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Spotlight(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Magnifier(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Badge(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Text(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Translation(a) => a.draw_offset(pixmap, offset_x, offset_y),
        }
    }

    /// 执行绘制（偏置量默认为 0.0）
    pub fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }

    /// 动态调节矩形圆角半径，仅矩形图元生效
    pub fn adjust_corner_radius(&mut self, radius: f32) {
        if let AnnotationShape::Rect(action) = self {
            action.adjust_corner_radius(radius);
        }
    }

    /// 按尺寸语义调节图元：描边类只接受 [`SizeParam::Stroke`]，
    /// 文本类只接受 [`SizeParam::Font`]，传错语义时不做任何修改。
    pub fn adjust_size(&mut self, param: SizeParam) {
        match self {
            Self::Rect(a) => a.adjust_size(param),
            Self::Ellipse(a) => a.adjust_size(param),
            Self::Arrow(a) => a.adjust_size(param),
            Self::Pen(a) => a.adjust_size(param),
            Self::Polyline(a) => a.adjust_size(param),
            Self::Marker(a) => a.adjust_size(param),
            Self::Mosaic(a) => a.adjust_size(param),
            Self::Blur(a) => a.adjust_size(param),
            Self::Spotlight(a) => a.adjust_size(param),
            Self::Magnifier(a) => a.adjust_size(param),
            Self::Badge(a) => a.adjust_size(param),
            Self::Text(a) => a.adjust_size(param),
            Self::Translation(a) => a.adjust_size(param),
        }
    }

    /// 返回图元的轴对齐包围盒
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Rect(a) => a.bounds(),
            Self::Ellipse(a) => a.bounds(),
            Self::Arrow(a) => a.bounds(),
            Self::Pen(a) => a.bounds(),
            Self::Polyline(a) => a.bounds(),
            Self::Marker(a) => a.bounds(),
            Self::Mosaic(a) => a.bounds(),
            Self::Blur(a) => a.bounds(),
            Self::Spotlight(a) => a.bounds(),
            Self::Magnifier(a) => a.bounds(),
            Self::Badge(a) => a.bounds(),
            Self::Text(a) => a.bounds(),
            Self::Translation(a) => a.bounds(),
        }
    }

    /// 整体平移图元
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Rect(a) => a.translate(dx, dy),
            Self::Ellipse(a) => a.translate(dx, dy),
            Self::Arrow(a) => a.translate(dx, dy),
            Self::Pen(a) => a.translate(dx, dy),
            Self::Polyline(a) => a.translate(dx, dy),
            Self::Marker(a) => a.translate(dx, dy),
            Self::Mosaic(a) => a.translate(dx, dy),
            Self::Blur(a) => a.translate(dx, dy),
            Self::Spotlight(a) => a.translate(dx, dy),
            Self::Magnifier(a) => a.translate(dx, dy),
            Self::Badge(a) => a.translate(dx, dy),
            Self::Text(a) => a.translate(dx, dy),
            Self::Translation(a) => a.translate(dx, dy),
        }
    }

    /// 按包围盒映射缩放图元
    pub fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        match self {
            Self::Rect(a) => a.resize_from_bounds(from, to),
            Self::Ellipse(a) => a.resize_from_bounds(from, to),
            Self::Arrow(a) => a.resize_from_bounds(from, to),
            Self::Pen(a) => a.resize_from_bounds(from, to),
            Self::Polyline(a) => a.resize_from_bounds(from, to),
            Self::Marker(a) => a.resize_from_bounds(from, to),
            Self::Mosaic(a) => a.resize_from_bounds(from, to),
            Self::Blur(a) => a.resize_from_bounds(from, to),
            Self::Spotlight(a) => a.resize_from_bounds(from, to),
            Self::Magnifier(a) => a.resize_from_bounds(from, to),
            Self::Badge(a) => a.resize_from_bounds(from, to),
            Self::Text(a) => a.resize_from_bounds(from, to),
            Self::Translation(a) => a.resize_from_bounds(from, to),
        }
    }

    /// 是否允许通过包围盒手柄拉伸缩放
    ///
    /// 原位翻译块是复合图元，只支持整体移动；其余图元均支持手柄拉伸。
    pub fn supports_resize(&self) -> bool {
        !matches!(self, Self::Translation(_))
    }
}

impl From<TranslationAction> for AnnotationShape {
    fn from(action: TranslationAction) -> Self {
        Self::Translation(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::AnnotationHistory;
    use crate::style::{BadgeSequence, BadgeStyle, LineStyle};

    /// 构造测试用共享矩形，避免测试中反复书写结构体字段。
    fn rect_bounds(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn test_undo_redo_stack() {
        let mut history = AnnotationHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        let rect_shape = AnnotationShape::Rect(RectAction {
            rect: (0.0, 0.0, 10.0, 10.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        });

        history.push(rect_shape);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        assert!(history.undo());
        assert!(!history.can_undo());
        assert!(history.can_redo());

        assert!(history.redo());
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_badge_number_is_derived_from_history() {
        use crate::shapes::{BadgeAction, TextAction};

        let mut history = AnnotationHistory::default();
        assert_eq!(history.next_badge_number(), 1);

        history.push(AnnotationShape::Badge(BadgeAction {
            center: (10.0, 10.0),
            radius: 12.0,
            number: 1,
            color: (255, 0, 0, 255).into(),
            style: BadgeStyle::Filled,
            sequence: BadgeSequence::Number,
            ..BadgeAction::default()
        }));
        history.push(AnnotationShape::Badge(BadgeAction {
            center: (30.0, 10.0),
            radius: 12.0,
            number: 2,
            color: (255, 0, 0, 255).into(),
            style: BadgeStyle::Filled,
            sequence: BadgeSequence::Number,
            ..BadgeAction::default()
        }));
        assert_eq!(history.next_badge_number(), 3);

        history.push(AnnotationShape::Text(TextAction::plain(
            (0.0, 0.0),
            "说明".to_string(),
            (0, 0, 0, 255).into(),
            16.0,
        )));
        history.undo();
        assert_eq!(history.next_badge_number(), 3);
        history.undo();
        assert_eq!(history.next_badge_number(), 2);
        history.redo();
        assert_eq!(history.next_badge_number(), 3);
    }

    #[test]
    fn test_adjust_last_size_only_matches_shape_kind() {
        use crate::shapes::{PenAction, TextAction};

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Pen(PenAction {
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            line_style: LineStyle::Solid,
        }));
        assert!(history.adjust_last_stroke_width(8.0));
        assert!(!history.adjust_last_font_size(30.0));

        history.push(AnnotationShape::Text(TextAction::plain(
            (0.0, 0.0),
            "说明".to_string(),
            (0, 0, 0, 255).into(),
            16.0,
        )));
        assert!(!history.adjust_last_stroke_width(12.0));
        assert!(history.adjust_last_font_size(30.0));
    }

    /// 马赛克也属于描边尺寸类图元，必须能被滚轮调粗细。
    #[test]
    fn test_adjust_last_stroke_width_covers_mosaic() {
        use crate::shapes::MosaicAction;

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Mosaic(MosaicAction {
            rect: (0.0, 0.0, 20.0, 20.0),
            cell_size: 4,
            points: Vec::new(),
            brush_width: 0.0,
        }));
        assert!(history.adjust_last_stroke_width(10.0));
        match history.shape_at(0) {
            Some(AnnotationShape::Mosaic(action)) => assert_eq!(action.cell_size, 20),
            other => panic!("应为马赛克图元，实际: {other:?}"),
        }
    }

    /// 序号注释的右对应文字字号，滚轮调字号必须能命中 Badge 图元。
    #[test]
    fn test_adjust_last_font_size_covers_badge_text() {
        use crate::shapes::BadgeAction;

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Badge(BadgeAction {
            font_size: 12.0,
            ..BadgeAction::default()
        }));
        assert!(history.adjust_last_font_size(30.0));
        match history.shape_at(0) {
            Some(AnnotationShape::Badge(action)) => assert_eq!(action.font_size, 30.0),
            other => panic!("应为序号图元，实际: {other:?}"),
        }
    }

    /// 所有带线型控件的工具都必须让实线与虚线产生不同像素结果。
    #[test]
    fn test_line_styles_affect_all_stroke_tools() {
        use crate::shapes::{
            ArrowAction, EllipseAction, PenAction, PolylineAction, SpotlightAction,
        };
        use crate::{ArrowHeadStyle, ArrowStyle, SpotlightShape};
        use tiny_skia::{Color, Pixmap};

        fn render(shape: &AnnotationShape) -> Vec<u8> {
            let mut pixmap = Pixmap::new(140, 100).expect("创建测试位图失败");
            pixmap.fill(Color::TRANSPARENT);
            shape.draw(&mut pixmap.as_mut());
            pixmap.data().to_vec()
        }

        let ellipse = |style| {
            AnnotationShape::Ellipse(EllipseAction {
                rect: (10.0, 10.0, 120.0, 80.0),
                color: (255, 0, 0, 255).into(),
                stroke_width: 4.0,
                is_filled: false,
                line_style: style,
            })
        };
        let pen = |style| {
            AnnotationShape::Pen(PenAction {
                points: vec![(10.0, 20.0), (130.0, 20.0)],
                color: (255, 0, 0, 255).into(),
                stroke_width: 4.0,
                line_style: style,
            })
        };
        let polyline = |style| {
            AnnotationShape::Polyline(PolylineAction {
                points: vec![(10.0, 20.0), (130.0, 20.0)],
                color: (255, 0, 0, 255).into(),
                stroke_width: 4.0,
                line_style: style,
                start_head: ArrowHeadStyle::None,
                end_head: ArrowHeadStyle::None,
            })
        };
        let arrow = |style| {
            AnnotationShape::Arrow(ArrowAction {
                start: (10.0, 50.0),
                end: (130.0, 50.0),
                color: (255, 0, 0, 255).into(),
                stroke_width: 4.0,
                style: ArrowStyle::Solid,
                line_style: style,
                start_head: ArrowHeadStyle::None,
                end_head: ArrowHeadStyle::Triangle,
            })
        };
        let spotlight = |style| {
            AnnotationShape::Spotlight(SpotlightAction {
                rect: (10.0, 10.0, 120.0, 80.0),
                intensity: 160,
                shape: SpotlightShape::Rectangle,
                outline: true,
                outline_width: 3.0,
                outline_color: (255, 255, 255, 255).into(),
                line_style: style,
            })
        };

        for (name, solid, dashed) in [
            (
                "ellipse",
                ellipse(LineStyle::Solid),
                ellipse(LineStyle::Dashed),
            ),
            ("pen", pen(LineStyle::Solid), pen(LineStyle::Dashed)),
            (
                "polyline",
                polyline(LineStyle::Solid),
                polyline(LineStyle::Dashed),
            ),
            ("arrow", arrow(LineStyle::Solid), arrow(LineStyle::Dashed)),
            (
                "spotlight",
                spotlight(LineStyle::Solid),
                spotlight(LineStyle::Dashed),
            ),
        ] {
            assert_ne!(render(&solid), render(&dashed), "{name} 的线型未生效");
        }
    }

    #[test]
    fn test_ellipse_and_badge_action() {
        use crate::shapes::{BadgeAction, EllipseAction};
        use tiny_skia::Pixmap;

        let mut pixmap = Pixmap::new(100, 100).unwrap();
        let mut mut_pix = pixmap.as_mut();

        let ellipse = EllipseAction {
            rect: (10.0, 10.0, 80.0, 60.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            line_style: LineStyle::Solid,
        };
        ellipse.draw(&mut mut_pix);

        let badge = BadgeAction {
            center: (50.0, 50.0),
            radius: 15.0,
            number: 1,
            color: (0, 128, 255, 255).into(),
            style: BadgeStyle::Filled,
            sequence: BadgeSequence::Number,
            ..BadgeAction::default()
        };
        badge.draw(&mut mut_pix);

        // 验证气泡内部确实绘制出了纯白数字像素
        let data = pixmap.data();
        let has_white_digit = (40..60).any(|y| {
            (40..60).any(|x| {
                let idx = ((y * 100 + x) * 4) as usize;
                data[idx] > 200 && data[idx + 1] > 200 && data[idx + 2] > 200
            })
        });
        assert!(
            has_white_digit,
            "BadgeAction 必须在中心成功绘制出纯白色数字！"
        );
    }

    #[test]
    fn test_annotation_history_serde_roundtrip() {
        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (5.0, 5.0, 20.0, 20.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        let toml_str = toml::to_string(&history).expect("序列化 AnnotationHistory 成功");
        let deserialized: AnnotationHistory =
            toml::from_str(&toml_str).expect("反序列化 AnnotationHistory 成功");
        assert_eq!(history, deserialized);
    }

    #[test]
    fn test_replay_with_offset() {
        use tiny_skia::Pixmap;
        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (100.0, 100.0, 50.0, 50.0),
            color: (0, 255, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: true,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        let mut pixmap = Pixmap::new(50, 50).unwrap();
        // 传入 -100, -100 偏置量，应正好绘制在 0..50 范围内
        history.replay_with_offset(&mut pixmap.as_mut(), -100.0, -100.0);

        let pixel = pixmap.pixel(25, 25).unwrap();
        assert_eq!(pixel.green(), 255);
        assert_eq!(pixel.alpha(), 255);
    }

    #[test]
    fn test_translation_action_undo_redo_and_draw() {
        use crate::shapes::TranslationShapeItem;
        use tiny_skia::Pixmap;

        let mut history = AnnotationHistory::default();
        let item = TranslationShapeItem {
            rect: (10.0, 10.0, 80.0, 30.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 255, 255, 255).into(),
            text_color: (0, 0, 0, 255).into(),
            font_size: 16.0,
            cover_mode: crate::config::enums::CoverMode::SolidColor,
        };

        let action = TranslationAction::new(vec![item]);
        history.push(action.into());

        assert!(history.can_undo());

        let mut pixmap = Pixmap::new(100, 100).unwrap();
        history.replay_all(&mut pixmap.as_mut());

        // 验证擦除区域左上角 (13, 13) 像素已填充白色背景
        let pixel = pixmap.pixel(13, 13).unwrap();
        assert_eq!(pixel.red(), 255);
        assert_eq!(pixel.green(), 255);
        assert_eq!(pixel.blue(), 255);

        assert!(history.undo());
        assert!(!history.can_undo());
        assert!(history.redo());
        assert!(history.can_undo());
    }

    /// 各类图元的包围盒、平移与按包围盒缩放必须正确生效
    #[test]
    fn test_shape_geometry_editing() {
        use crate::shapes::{ArrowAction, BadgeAction, PenAction, TextAction};
        use crate::style::{ArrowStyle, BadgeStyle};

        let mut rect = AnnotationShape::Rect(RectAction {
            rect: (10.0, 20.0, 100.0, 80.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        });
        assert_eq!(rect.bounds(), Some(rect_bounds(10.0, 20.0, 100.0, 80.0)));
        rect.translate(5.0, -5.0);
        assert_eq!(rect.bounds(), Some(rect_bounds(15.0, 15.0, 100.0, 80.0)));
        rect.resize_from_bounds(
            rect_bounds(15.0, 15.0, 100.0, 80.0),
            rect_bounds(0.0, 0.0, 200.0, 160.0),
        );
        assert_eq!(rect.bounds(), Some(rect_bounds(0.0, 0.0, 200.0, 160.0)));

        // 箭头按包围盒线性映射起终点
        let mut arrow = AnnotationShape::Arrow(ArrowAction {
            start: (0.0, 0.0),
            end: (100.0, 50.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            style: ArrowStyle::Solid,
            line_style: crate::style::LineStyle::Solid,
            start_head: crate::style::ArrowHeadStyle::None,
            end_head: crate::style::ArrowHeadStyle::Triangle,
        });
        arrow.resize_from_bounds(
            rect_bounds(0.0, 0.0, 100.0, 50.0),
            rect_bounds(0.0, 0.0, 200.0, 100.0),
        );
        match arrow {
            AnnotationShape::Arrow(a) => {
                assert_eq!(a.start, (0.0, 0.0));
                assert_eq!(a.end, (200.0, 100.0));
            }
            _ => panic!("预期箭头图元"),
        }

        // 画笔点集整体平移
        let mut pen = AnnotationShape::Pen(PenAction {
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            line_style: LineStyle::Solid,
        });
        pen.translate(5.0, 5.0);
        match pen {
            AnnotationShape::Pen(p) => assert_eq!(p.points, vec![(5.0, 5.0), (15.0, 15.0)]),
            _ => panic!("预期画笔图元"),
        }

        // 序号半径按包围盒平均比例缩放
        let mut badge = AnnotationShape::Badge(BadgeAction {
            center: (50.0, 50.0),
            number: 1,
            color: (0, 128, 255, 255).into(),
            radius: 10.0,
            style: BadgeStyle::Filled,
            sequence: BadgeSequence::Number,
            ..BadgeAction::default()
        });
        badge.resize_from_bounds(
            rect_bounds(40.0, 40.0, 20.0, 20.0),
            rect_bounds(30.0, 30.0, 40.0, 40.0),
        );
        match badge {
            AnnotationShape::Badge(b) => {
                assert_eq!(b.center, (50.0, 50.0));
                assert_eq!(b.radius, 20.0);
            }
            _ => panic!("预期序号图元"),
        }

        // 文本按高度比例缩放字号
        let mut text = AnnotationShape::Text(TextAction::plain(
            (10.0, 20.0),
            "说明".to_string(),
            (0, 0, 0, 255).into(),
            16.0,
        ));
        assert_eq!(text.bounds(), Some(rect_bounds(6.0, 16.0, 40.0, 28.0)));
        text.resize_from_bounds(
            rect_bounds(6.0, 16.0, 40.0, 28.0),
            rect_bounds(0.0, 0.0, 80.0, 56.0),
        );
        match text {
            AnnotationShape::Text(t) => {
                assert_eq!(t.pos, (8.0, 8.0));
                assert_eq!(t.font_size, 32.0);
            }
            _ => panic!("预期文本图元"),
        }
    }

    /// 命中测试必须优先返回更靠上（后绘制）的标注
    #[test]
    fn test_hit_test_prefers_topmost_shape() {
        use crate::shapes::EllipseAction;

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (100.0, 100.0, 100.0, 100.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
        history.push(AnnotationShape::Ellipse(EllipseAction {
            rect: (120.0, 120.0, 50.0, 50.0),
            color: (0, 255, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: false,
            line_style: LineStyle::Solid,
        }));

        assert_eq!(history.hit_test((130.0, 130.0)), Some(1));
        assert_eq!(history.hit_test((110.0, 110.0)), Some(0));
        assert_eq!(history.hit_test((500.0, 500.0)), None);
    }

    /// 编辑替换必须只改动目标索引，并拒绝越界索引
    #[test]
    fn test_replace_shape_at_index() {
        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (0.0, 0.0, 10.0, 10.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        let mut moved = history.shape_at(0).expect("索引 0 必须存在").clone();
        moved.translate(5.0, 5.0);
        assert!(history.replace(0, moved));
        assert_eq!(
            history.bounds_at(0),
            Some(rect_bounds(5.0, 5.0, 10.0, 10.0))
        );
        assert!(!history.replace(
            9,
            AnnotationShape::Rect(RectAction {
                rect: (0.0, 0.0, 1.0, 1.0),
                color: (255, 0, 0, 255).into(),
                stroke_width: 1.0,
                is_filled: false,
                corner_radius: 0.0,
                line_style: LineStyle::Solid,
            })
        ));
    }
}
