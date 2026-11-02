//! 核心共享几何契约。
//!
//! 标注包围盒、截图选区、OCR 文本区域与 UI 布局共用明确的点、浮点矩形和整数矩形
//! 类型，避免 `(f32, f32)`、`(f32, f32, f32, f32)` 在跨 crate 传递时丢失业务语义。
//! 所有类型保持值语义，适合高频复制与小型集合存储。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 二维浮点坐标点。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    /// 横坐标
    pub x: f32,
    /// 纵坐标
    pub y: f32,
}

impl Point {
    /// 构造二维坐标点。
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// 按向量偏移当前点。
    pub fn translated(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

impl From<(f32, f32)> for Point {
    fn from((x, y): (f32, f32)) -> Self {
        Self::new(x, y)
    }
}

impl From<Point> for (f32, f32) {
    fn from(point: Point) -> Self {
        (point.x, point.y)
    }
}

impl Serialize for Point {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (self.x, self.y).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (x, y) = <(f32, f32)>::deserialize(deserializer)?;
        Ok(Self::new(x, y))
    }
}

/// 轴对齐浮点矩形 (x, y, width, height)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// 左上角 X 坐标
    pub x: f32,
    /// 左上角 Y 坐标
    pub y: f32,
    /// 宽度
    pub width: f32,
    /// 高度
    pub height: f32,
}

/// 浮点矩形的语义别名。
pub type RectF = Rect;

impl Rect {
    /// 构造轴对齐矩形。
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 两个矩形是否存在面积交集。
    pub fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    /// 根据两个对角点构建正规化的矩形（确保宽高非负）。
    pub fn from_point_pair(first: Point, second: Point) -> Self {
        let x = first.x.min(second.x);
        let y = first.y.min(second.y);
        Self {
            x,
            y,
            width: (first.x - second.x).abs(),
            height: (first.y - second.y).abs(),
        }
    }

    /// 根据两个坐标元组构建正规化矩形。
    pub fn from_points(first: (f32, f32), second: (f32, f32)) -> Self {
        Self::from_point_pair(first.into(), second.into())
    }

    /// 判断某个坐标点是否落在矩形内部。
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    /// 判断某个点是否落在矩形内部。
    pub fn contains_point(&self, point: Point) -> bool {
        self.contains(point.x, point.y)
    }

    /// 转换为整数像素对齐矩形，所有边界夹取到给定图像范围。
    pub fn to_pixel_rect(&self, max_width: u32, max_height: u32) -> RectI {
        let x = (self.x.max(0.0) as u32).min(max_width);
        let y = (self.y.max(0.0) as u32).min(max_height);
        let width = (self.width as u32).min(max_width.saturating_sub(x));
        let height = (self.height as u32).min(max_height.saturating_sub(y));
        RectI::new(x, y, width, height)
    }

    /// 整数像素对齐包围盒 `(x, y, width, height)`。
    ///
    /// 该兼容入口保留给已经以元组解构的底层图像 API；新代码应优先使用
    /// [`Self::to_pixel_rect`]。
    pub fn to_pixel_bounds(&self, max_width: u32, max_height: u32) -> (u32, u32, u32, u32) {
        self.to_pixel_rect(max_width, max_height).into()
    }
}

/// 轴对齐整数矩形，用于图像像素裁剪与二维码/OCR 像素坐标。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RectI {
    /// 左上角 X 坐标
    pub x: u32,
    /// 左上角 Y 坐标
    pub y: u32,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
}

impl RectI {
    /// 构造整数像素矩形。
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 判断矩形是否包含有效面积。
    pub const fn is_non_empty(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

impl From<(u32, u32, u32, u32)> for RectI {
    fn from((x, y, width, height): (u32, u32, u32, u32)) -> Self {
        Self::new(x, y, width, height)
    }
}

impl From<RectI> for (u32, u32, u32, u32) {
    fn from(rect: RectI) -> Self {
        (rect.x, rect.y, rect.width, rect.height)
    }
}

impl Serialize for Rect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (self.x, self.y, self.width, self.height).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Rect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (x, y, width, height) = <(f32, f32, f32, f32)>::deserialize(deserializer)?;
        Ok(Self::new(x, y, width, height))
    }
}

impl Serialize for RectI {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (self.x, self.y, self.width, self.height).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RectI {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (x, y, width, height) = <(u32, u32, u32, u32)>::deserialize(deserializer)?;
        Ok(Self::new(x, y, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_normalization() {
        let rect = Rect::from_points((100.0, 200.0), (50.0, 80.0));
        assert_eq!(rect.x, 50.0);
        assert_eq!(rect.y, 80.0);
        assert_eq!(rect.width, 50.0);
        assert_eq!(rect.height, 120.0);
        assert!(rect.contains(60.0, 90.0));
        assert!(!rect.contains(10.0, 90.0));
    }

    #[test]
    fn test_pixel_rect_clamps_to_image() {
        let rect = Rect::new(-10.0, 5.5, 500.0, 30.0);
        assert_eq!(rect.to_pixel_rect(100, 40), RectI::new(0, 5, 100, 30));
    }

    /// Point 与矩形共享同一组语义转换，避免调用方重复手写字段顺序。
    #[test]
    fn test_point_translation_and_containment() {
        let point = Point::new(4.0, 8.0).translated(2.0, -3.0);
        assert_eq!(point, Point::new(6.0, 5.0));
        assert!(Rect::new(0.0, 0.0, 10.0, 10.0).contains_point(point));
    }

    /// 强类型几何在序列化层仍保持历史元组格式，避免已有标注数据失效。
    #[test]
    fn test_geometry_keeps_tuple_serialization_contract() {
        assert_eq!(
            serde_json::to_string(&Point::new(1.0, 2.0)).expect("序列化点失败"),
            "[1.0,2.0]"
        );
        assert_eq!(
            serde_json::to_string(&Rect::new(1.0, 2.0, 3.0, 4.0)).expect("序列化矩形失败"),
            "[1.0,2.0,3.0,4.0]"
        );

        let point: Point = serde_json::from_str("[5.0,6.0]").expect("旧格式点应可解析");
        let rect: Rect = serde_json::from_str("[7.0,8.0,9.0,10.0]").expect("旧格式矩形应可解析");
        assert_eq!(point, Point::new(5.0, 6.0));
        assert_eq!(rect, Rect::new(7.0, 8.0, 9.0, 10.0));
    }
}
