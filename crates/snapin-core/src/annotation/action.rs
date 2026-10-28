//! 标注图元命令模式与撤销/重做控制。
//!
//! 将每一次绘制操作封装为具备 Execute 与 Undo 能力的 Command 对象，
//! 采用命令模式维护 Undo/Redo 历史栈。

use crate::annotation::shapes::{
    ArrowAction, BadgeAction, BlurAction, EllipseAction, MarkerAction, MosaicAction, PenAction,
    RectAction, SpotlightAction, TextAction, TranslationAction,
};
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 标注图元的轴对齐包围盒 (x, y, width, height)，单位像素
pub type ShapeRect = (f32, f32, f32, f32);

/// 单次矢量标注行为的 Trait 抽象
pub trait AnnotationAction: Send + Sync {
    /// 执行带坐标偏移量的绘制（在全屏绘制时 offset 为 0，局部裁剪时 offset 为 -crop_x, -crop_y）
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32);

    /// 执行绘制（默认 offset 为 0.0）
    fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }

    /// 动态调节标注尺寸 (粗细或字体大小)
    fn adjust_size(&mut self, _stroke_w: f32, _font_sz: f32) {}

    /// 动态调节矩形圆角半径；非矩形图元默认忽略
    fn adjust_corner_radius(&mut self, _radius: f32) {}

    /// 返回图元的轴对齐包围盒；没有有效几何（空文本、退化线段等）时返回 None
    fn bounds(&self) -> Option<ShapeRect>;

    /// 整体平移图元
    fn translate(&mut self, dx: f32, dy: f32);

    /// 按包围盒映射缩放图元：`from` 为拖拽开始时的包围盒，`to` 为手柄拖拽后的新包围盒。
    ///
    /// 每次拖拽都基于原始形状与原始包围盒重新计算，避免逐帧累积缩放误差。
    fn resize_from_bounds(&mut self, _from: ShapeRect, _to: ShapeRect) {}

    /// 是否允许通过包围盒手柄拉伸缩放；复合图元（原位翻译块）只支持整体移动
    fn supports_resize(&self) -> bool {
        true
    }
}

/// 所有支持的闭合矢量标注图元枚举 (代数数据类型，天然支持无堆分配存储、值拷贝与 Serde 序列化)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationShape {
    Rect(RectAction),
    Ellipse(EllipseAction),
    Arrow(ArrowAction),
    Pen(PenAction),
    Marker(MarkerAction),
    Mosaic(MosaicAction),
    Blur(BlurAction),
    Spotlight(SpotlightAction),
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
            Self::Marker(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Mosaic(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Blur(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Spotlight(a) => a.draw_offset(pixmap, offset_x, offset_y),
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

    /// 动态调节图元尺寸 (粗细或字体大小)
    pub fn adjust_size(&mut self, stroke_w: f32, font_sz: f32) {
        match self {
            Self::Rect(a) => a.adjust_size(stroke_w, font_sz),
            Self::Ellipse(a) => a.adjust_size(stroke_w, font_sz),
            Self::Arrow(a) => a.adjust_size(stroke_w, font_sz),
            Self::Pen(a) => a.adjust_size(stroke_w, font_sz),
            Self::Marker(a) => a.adjust_size(stroke_w, font_sz),
            Self::Mosaic(a) => a.adjust_size(stroke_w, font_sz),
            Self::Blur(a) => a.adjust_size(stroke_w, font_sz),
            Self::Spotlight(a) => a.adjust_size(stroke_w, font_sz),
            Self::Badge(a) => a.adjust_size(stroke_w, font_sz),
            Self::Text(a) => a.adjust_size(stroke_w, font_sz),
            Self::Translation(a) => a.adjust_size(stroke_w, font_sz),
        }
    }

    /// 返回图元的轴对齐包围盒
    pub fn bounds(&self) -> Option<ShapeRect> {
        match self {
            Self::Rect(a) => a.bounds(),
            Self::Ellipse(a) => a.bounds(),
            Self::Arrow(a) => a.bounds(),
            Self::Pen(a) => a.bounds(),
            Self::Marker(a) => a.bounds(),
            Self::Mosaic(a) => a.bounds(),
            Self::Blur(a) => a.bounds(),
            Self::Spotlight(a) => a.bounds(),
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
            Self::Marker(a) => a.translate(dx, dy),
            Self::Mosaic(a) => a.translate(dx, dy),
            Self::Blur(a) => a.translate(dx, dy),
            Self::Spotlight(a) => a.translate(dx, dy),
            Self::Badge(a) => a.translate(dx, dy),
            Self::Text(a) => a.translate(dx, dy),
            Self::Translation(a) => a.translate(dx, dy),
        }
    }

    /// 按包围盒映射缩放图元
    pub fn resize_from_bounds(&mut self, from: ShapeRect, to: ShapeRect) {
        match self {
            Self::Rect(a) => a.resize_from_bounds(from, to),
            Self::Ellipse(a) => a.resize_from_bounds(from, to),
            Self::Arrow(a) => a.resize_from_bounds(from, to),
            Self::Pen(a) => a.resize_from_bounds(from, to),
            Self::Marker(a) => a.resize_from_bounds(from, to),
            Self::Mosaic(a) => a.resize_from_bounds(from, to),
            Self::Blur(a) => a.resize_from_bounds(from, to),
            Self::Spotlight(a) => a.resize_from_bounds(from, to),
            Self::Badge(a) => a.resize_from_bounds(from, to),
            Self::Text(a) => a.resize_from_bounds(from, to),
            Self::Translation(a) => a.resize_from_bounds(from, to),
        }
    }

    /// 是否允许通过包围盒手柄拉伸缩放
    pub fn supports_resize(&self) -> bool {
        match self {
            Self::Rect(a) => a.supports_resize(),
            Self::Ellipse(a) => a.supports_resize(),
            Self::Arrow(a) => a.supports_resize(),
            Self::Pen(a) => a.supports_resize(),
            Self::Marker(a) => a.supports_resize(),
            Self::Mosaic(a) => a.supports_resize(),
            Self::Blur(a) => a.supports_resize(),
            Self::Spotlight(a) => a.supports_resize(),
            Self::Badge(a) => a.supports_resize(),
            Self::Text(a) => a.supports_resize(),
            Self::Translation(a) => a.supports_resize(),
        }
    }
}

/// 标注历史管理器 (Undo / Redo 栈)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationHistory {
    undo_stack: Vec<AnnotationShape>,
    redo_stack: Vec<AnnotationShape>,
}

impl AnnotationHistory {
    /// 添加一条新的标注操作，清空 redo 栈
    pub fn push(&mut self, shape: AnnotationShape) {
        self.undo_stack.push(shape);
        self.redo_stack.clear();
    }

    /// 动态调节最近绘制的描边类图元粗细；最近图元不是描边类时不做任何修改。
    pub fn adjust_last_stroke_width(&mut self, stroke_width: f32) -> bool {
        let Some(last) = self.undo_stack.last_mut() else {
            return false;
        };
        match last {
            AnnotationShape::Rect(_)
            | AnnotationShape::Ellipse(_)
            | AnnotationShape::Arrow(_)
            | AnnotationShape::Pen(_)
            | AnnotationShape::Marker(_)
            | AnnotationShape::Blur(_)
            | AnnotationShape::Badge(_) => {
                last.adjust_size(stroke_width, 0.0);
                true
            }
            _ => false,
        }
    }

    /// 动态调节最近绘制矩形的圆角半径；最近图元不是矩形时不做任何修改。
    pub fn adjust_last_corner_radius(&mut self, radius: f32) -> bool {
        let Some(last) = self.undo_stack.last_mut() else {
            return false;
        };
        if !matches!(last, AnnotationShape::Rect(_)) {
            return false;
        }
        last.adjust_corner_radius(radius);
        true
    }

    /// 动态调节最近绘制的文本/译文图元字号；最近图元不是文本类时不做任何修改。
    pub fn adjust_last_font_size(&mut self, font_size: f32) -> bool {
        let Some(last) = self.undo_stack.last_mut() else {
            return false;
        };
        match last {
            AnnotationShape::Text(_) | AnnotationShape::Translation(_) => {
                last.adjust_size(0.0, font_size);
                true
            }
            _ => false,
        }
    }

    /// 根据当前生效的徽章图元推导下一个可用序号。
    ///
    /// 序号始终基于历史栈中仍然存在的最大值计算，撤销、重做与随机删除都不会造成重复编号。
    pub fn next_badge_number(&self) -> u32 {
        let max_number = self
            .undo_stack
            .iter()
            .filter_map(|shape| match shape {
                AnnotationShape::Badge(badge) => Some(badge.number),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        (max_number as u32).saturating_add(1)
    }

    /// 撤销上一次标注
    pub fn undo(&mut self) -> bool {
        if let Some(action) = self.undo_stack.pop() {
            self.redo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// 恢复被撤销的标注
    pub fn redo(&mut self) -> bool {
        if let Some(action) = self.redo_stack.pop() {
            self.undo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// 重新在目标 pixmap 上依次回放所有生效的标注图元
    pub fn replay_all(&self, pixmap: &mut PixmapMut) {
        self.replay_with_offset(pixmap, 0.0, 0.0);
    }

    /// 重新在目标局部 pixmap 上带平移偏移量回放所有生效的标注图元
    pub fn replay_with_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        for action in &self.undo_stack {
            action.draw_offset(pixmap, offset_x, offset_y);
        }
    }

    /// 当前是否有可撤销的操作
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 当前是否有可重做的操作
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 获取撤销栈切片借用
    pub fn undo_stack(&self) -> &[AnnotationShape] {
        &self.undo_stack
    }

    /// 从顶层向下命中测试标注图元，返回第一个包围盒包含该点的索引。
    ///
    /// 采用包围盒判定而非逐像素判定：编辑手柄本身留有余量，包围盒命中已足够，
    /// 同时避免对画笔、马赛克等复杂图元反复光栅化。
    pub fn hit_test(&self, point: (f32, f32)) -> Option<usize> {
        self.undo_stack
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, shape)| {
                let (x, y, w, h) = shape.bounds()?;
                let inside = point.0 >= x && point.0 <= x + w && point.1 >= y && point.1 <= y + h;
                inside.then_some(index)
            })
    }

    /// 获取指定索引图元的包围盒
    pub fn bounds_at(&self, index: usize) -> Option<ShapeRect> {
        self.undo_stack.get(index)?.bounds()
    }

    /// 获取指定索引图元的不可变借用
    pub fn shape_at(&self, index: usize) -> Option<&AnnotationShape> {
        self.undo_stack.get(index)
    }

    /// 用编辑后的图元替换指定索引；索引越界返回 false。
    ///
    /// 编辑不进入撤销栈：用户拖拽标注的过程中可能产生成百次中间状态，
    /// 只有最终落定结果保留在历史栈中，撤销语义仍是“撤销整次标注绘制”。
    pub fn replace(&mut self, index: usize, shape: AnnotationShape) -> bool {
        let Some(slot) = self.undo_stack.get_mut(index) else {
            return false;
        };
        *slot = shape;
        true
    }

    /// 清空所有标注历史
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
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
    use crate::annotation::style::{BadgeStyle, LineStyle};

    #[test]
    fn test_undo_redo_stack() {
        let mut history = AnnotationHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        let rect_shape = AnnotationShape::Rect(RectAction {
            rect: (0.0, 0.0, 10.0, 10.0),
            color: (255, 0, 0, 255),
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
        use crate::annotation::shapes::{BadgeAction, TextAction};

        let mut history = AnnotationHistory::default();
        assert_eq!(history.next_badge_number(), 1);

        history.push(AnnotationShape::Badge(BadgeAction {
            center: (10.0, 10.0),
            radius: 12.0,
            number: 1,
            color: (255, 0, 0, 255),
            style: BadgeStyle::Filled,
        }));
        history.push(AnnotationShape::Badge(BadgeAction {
            center: (30.0, 10.0),
            radius: 12.0,
            number: 2,
            color: (255, 0, 0, 255),
            style: BadgeStyle::Filled,
        }));
        assert_eq!(history.next_badge_number(), 3);

        history.push(AnnotationShape::Text(TextAction {
            pos: (0.0, 0.0),
            text: "说明".to_string(),
            color: (0, 0, 0, 255),
            font_size: 16.0,
        }));
        history.undo();
        assert_eq!(history.next_badge_number(), 3);
        history.undo();
        assert_eq!(history.next_badge_number(), 2);
        history.redo();
        assert_eq!(history.next_badge_number(), 3);
    }

    #[test]
    fn test_adjust_last_size_only_matches_shape_kind() {
        use crate::annotation::shapes::{PenAction, TextAction};

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Pen(PenAction {
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
        }));
        assert!(history.adjust_last_stroke_width(8.0));
        assert!(!history.adjust_last_font_size(30.0));

        history.push(AnnotationShape::Text(TextAction {
            pos: (0.0, 0.0),
            text: "说明".to_string(),
            color: (0, 0, 0, 255),
            font_size: 16.0,
        }));
        assert!(!history.adjust_last_stroke_width(12.0));
        assert!(history.adjust_last_font_size(30.0));
    }

    #[test]
    fn test_ellipse_and_badge_action() {
        use crate::annotation::shapes::{BadgeAction, EllipseAction};
        use tiny_skia::Pixmap;

        let mut pixmap = Pixmap::new(100, 100).unwrap();
        let mut mut_pix = pixmap.as_mut();

        let ellipse = EllipseAction {
            rect: (10.0, 10.0, 80.0, 60.0),
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
            is_filled: false,
            line_style: LineStyle::Solid,
        };
        ellipse.draw(&mut mut_pix);

        let badge = BadgeAction {
            center: (50.0, 50.0),
            radius: 15.0,
            number: 1,
            color: (0, 128, 255, 255),
            style: BadgeStyle::Filled,
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
            color: (255, 0, 0, 255),
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
            color: (0, 255, 0, 255),
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
        use crate::annotation::shapes::TranslationShapeItem;
        use tiny_skia::Pixmap;

        let mut history = AnnotationHistory::default();
        let item = TranslationShapeItem {
            rect: (10.0, 10.0, 80.0, 30.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 255, 255, 255),
            text_color: (0, 0, 0, 255),
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
        use crate::annotation::shapes::{ArrowAction, BadgeAction, PenAction, TextAction};
        use crate::annotation::style::{ArrowStyle, BadgeStyle};

        let mut rect = AnnotationShape::Rect(RectAction {
            rect: (10.0, 20.0, 100.0, 80.0),
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        });
        assert_eq!(rect.bounds(), Some((10.0, 20.0, 100.0, 80.0)));
        rect.translate(5.0, -5.0);
        assert_eq!(rect.bounds(), Some((15.0, 15.0, 100.0, 80.0)));
        rect.resize_from_bounds((15.0, 15.0, 100.0, 80.0), (0.0, 0.0, 200.0, 160.0));
        assert_eq!(rect.bounds(), Some((0.0, 0.0, 200.0, 160.0)));

        // 箭头按包围盒线性映射起终点
        let mut arrow = AnnotationShape::Arrow(ArrowAction {
            start: (0.0, 0.0),
            end: (100.0, 50.0),
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
            style: ArrowStyle::Solid,
        });
        arrow.resize_from_bounds((0.0, 0.0, 100.0, 50.0), (0.0, 0.0, 200.0, 100.0));
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
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
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
            color: (0, 128, 255, 255),
            radius: 10.0,
            style: BadgeStyle::Filled,
        });
        badge.resize_from_bounds((40.0, 40.0, 20.0, 20.0), (30.0, 30.0, 40.0, 40.0));
        match badge {
            AnnotationShape::Badge(b) => {
                assert_eq!(b.center, (50.0, 50.0));
                assert_eq!(b.radius, 20.0);
            }
            _ => panic!("预期序号图元"),
        }

        // 文本按高度比例缩放字号
        let mut text = AnnotationShape::Text(TextAction {
            pos: (10.0, 20.0),
            text: "说明".to_string(),
            color: (0, 0, 0, 255),
            font_size: 16.0,
        });
        assert_eq!(text.bounds(), Some((10.0, 20.0, 32.0, 20.0)));
        text.resize_from_bounds((10.0, 20.0, 32.0, 20.0), (0.0, 0.0, 64.0, 40.0));
        match text {
            AnnotationShape::Text(t) => {
                assert_eq!(t.pos, (0.0, 0.0));
                assert_eq!(t.font_size, 32.0);
            }
            _ => panic!("预期文本图元"),
        }
    }

    /// 命中测试必须优先返回更靠上（后绘制）的标注
    #[test]
    fn test_hit_test_prefers_topmost_shape() {
        use crate::annotation::shapes::EllipseAction;

        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (100.0, 100.0, 100.0, 100.0),
            color: (255, 0, 0, 255),
            stroke_width: 1.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
        history.push(AnnotationShape::Ellipse(EllipseAction {
            rect: (120.0, 120.0, 50.0, 50.0),
            color: (0, 255, 0, 255),
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
            color: (255, 0, 0, 255),
            stroke_width: 1.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        let mut moved = history.shape_at(0).expect("索引 0 必须存在").clone();
        moved.translate(5.0, 5.0);
        assert!(history.replace(0, moved));
        assert_eq!(history.bounds_at(0), Some((5.0, 5.0, 10.0, 10.0)));
        assert!(!history.replace(
            9,
            AnnotationShape::Rect(RectAction {
                rect: (0.0, 0.0, 1.0, 1.0),
                color: (255, 0, 0, 255),
                stroke_width: 1.0,
                is_filled: false,
                corner_radius: 0.0,
                line_style: LineStyle::Solid,
            })
        ));
    }
}
