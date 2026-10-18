//! 贴图窗口几何变换矩阵计算。
//!
//! 负责缩放因子限制、中心锚点缩放计算以及 90 度旋转与镜像翻转。

/// 贴图几何变换状态
#[derive(Debug, Clone, PartialEq)]
pub struct PasterTransform {
    /// 缩放倍率 (0.1 ~ 10.0)
    pub scale: f32,
    /// 旋转角度 (0, 90, 180, 270)
    pub rotation_deg: u16,
    /// 水平镜像翻转
    pub flip_horizontal: bool,
    /// 垂直镜像翻转
    pub flip_vertical: bool,
    /// 窗口不透明度 (0.1 ~ 1.0)
    pub opacity: f32,
}

impl Default for PasterTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation_deg: 0,
            flip_horizontal: false,
            flip_vertical: false,
            opacity: 1.0,
        }
    }
}

impl PasterTransform {
    /// 按照给定的滚轮角度增量调整缩放倍率，限制在 10% 到 1000% 之间
    pub fn zoom(&mut self, delta_y: f32) {
        let factor = if delta_y > 0.0 { 1.1 } else { 0.9 };
        self.scale = (self.scale * factor).clamp(0.1, 10.0);
    }

    /// 顺时针旋转 90 度 (向右旋转)
    pub fn rotate_clockwise(&mut self) {
        self.rotation_deg = (self.rotation_deg + 90) % 360;
    }

    /// 逆时针旋转 90 度 (向左旋转)
    pub fn rotate_counter_clockwise(&mut self) {
        self.rotation_deg = (self.rotation_deg + 270) % 360;
    }

    /// 精确设置缩放比例
    pub fn set_scale(&mut self, s: f32) {
        self.scale = s.clamp(0.1, 10.0);
    }

    /// 水平翻转切换
    pub fn toggle_flip_horizontal(&mut self) {
        self.flip_horizontal = !self.flip_horizontal;
    }

    /// 垂直翻转切换
    pub fn toggle_flip_vertical(&mut self) {
        self.flip_vertical = !self.flip_vertical;
    }

    /// 调整不透明度
    pub fn adjust_opacity(&mut self, delta: f32) {
        self.opacity = (self.opacity + delta).clamp(0.1, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_clamp() {
        let mut tf = PasterTransform::default();
        tf.zoom(100.0);
        assert!(tf.scale > 1.0);

        for _ in 0..100 {
            tf.zoom(100.0);
        }
        assert_eq!(tf.scale, 10.0);

        for _ in 0..100 {
            tf.zoom(-100.0);
        }
        assert_eq!(tf.scale, 0.1);
    }

    #[test]
    fn test_rotation() {
        let mut tf = PasterTransform::default();
        assert_eq!(tf.rotation_deg, 0);
        tf.rotate_clockwise();
        assert_eq!(tf.rotation_deg, 90);
        tf.rotate_clockwise();
        tf.rotate_clockwise();
        tf.rotate_clockwise();
        assert_eq!(tf.rotation_deg, 0);

        tf.rotate_counter_clockwise();
        assert_eq!(tf.rotation_deg, 270);
        tf.rotate_counter_clockwise();
        assert_eq!(tf.rotation_deg, 180);

        tf.set_scale(0.5);
        assert_eq!(tf.scale, 0.5);
    }
}
