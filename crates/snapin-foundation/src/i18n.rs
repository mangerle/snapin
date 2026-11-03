//! 轻量用户界面翻译目录。
//!
//! 日志始终使用中文；只有会直接展示给用户的标签、状态与通知经过本模块。

use crate::language::UiLanguage;
use std::sync::atomic::{AtomicU8, Ordering};

static LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// 更新全局界面语言。
pub fn set_language(language: UiLanguage) {
    LANGUAGE.store(
        match language {
            UiLanguage::ZhCn => 0,
            UiLanguage::EnUs => 1,
        },
        Ordering::Release,
    );
}

/// 当前界面语言。
pub fn language() -> UiLanguage {
    if LANGUAGE.load(Ordering::Acquire) == 1 {
        UiLanguage::EnUs
    } else {
        UiLanguage::ZhCn
    }
}

/// 翻译静态用户文案。
pub fn tr(zh: &'static str) -> &'static str {
    if language() == UiLanguage::ZhCn {
        return zh;
    }
    match zh {
        "撤销" => "Undo",
        "重做" => "Redo",
        "文字识别" => "Text Recognition",
        "智能打码" => "Smart Redaction",
        "二维码" => "QR Code",
        "翻译" => "Translate",
        "鼠标穿透" => "Mouse Through",
        "贴图置顶" => "Pin",
        "复制" => "Copy",
        "保存" => "Save",
        "关闭" => "Close",
        "矩形" => "Rectangle",
        "椭圆" => "Ellipse",
        "箭头" => "Arrow",
        "画笔" => "Pen",
        "荧光笔" => "Highlighter",
        "文字" => "Text",
        "序号" => "Badge",
        "序号注释" => "Numbered Annotation",
        "直线与折线" => "Line and Polyline",
        "直线" => "Line",
        "折线" => "Polyline",
        "马赛克与智能打码" => "Mosaic and Smart Redaction",
        "马赛克、模糊与智能打码" => "Mosaic, Blur and Smart Redaction",
        "马赛克" => "Mosaic",
        "模糊" => "Blur",
        "聚光灯" => "Spotlight",
        "放大镜" => "Magnifier",
        "橡皮擦" => "Eraser",
        "清空标注" => "Clear Annotations",
        "清空" => "Clear",
        "框选" => "Box",
        "线条" => "Line",
        "正片叠底" => "Multiply",
        "半透明" => "Translucent",
        "描边" => "Outline",
        "背景" => "Background",
        "粗细" => "Thickness",
        "序号大小" => "Badge Size",
        "橡皮擦大小" => "Eraser Size",
        "边框粗细" => "Border Width",
        "效果强度" => "Effect Strength",
        "线型" => "Line Style",
        "填充" => "Fill",
        "序号形态" => "Badge Shape",
        "箭头样式" => "Arrow Style",
        "颜色" => "Color",
        "字号" => "Font Size",
        "起点箭头" => "Start Arrowhead",
        "终点箭头" => "End Arrowhead",
        "粗体" => "Bold",
        "斜体" => "Italic",
        "文字描边" => "Text Outline",
        "文字背景" => "Text Background",
        "数字形态" => "Number Format",
        "序号减一" => "Decrease Number",
        "序号加一" => "Increase Number",
        "展开或收起文字设置" => "Expand or Collapse Text Settings",
        "绘制模式" => "Drawing Mode",
        "画笔模式" => "Brush Mode",
        "矩形模式" => "Rectangle Mode",
        "混合模式" => "Blend Mode",
        "直线或折线" => "Line or Polyline",
        "箭头开关" => "Arrow Toggle",
        "遮挡模式" => "Redaction Mode",
        "字体" => "Font",
        "背景不透明度" => "Background Opacity",
        "背景内边距" => "Background Padding",
        "描边颜色" => "Outline Color",
        "透明度" => "Opacity",
        "放大倍率" => "Magnification",
        "平滑" => "Smooth",
        "抗锯齿" => "Anti-aliasing",
        "阴影" => "Shadow",
        "连接线" => "Connector",
        "线段" => "Line",
        "框线" => "Frame",
        "叉线" => "Cross",
        "圆角" => "Radius",
        "边距" => "Padding",
        "导向线" => "Guide Line",
        "无" => "None",
        "展开" => "Expand",
        "收起" => "Collapse",
        "起点" => "Start",
        "终点" => "End",
        "系统默认" => "System Default",
        "强度" => "Strength",
        "实线" => "Solid",
        "虚线" => "Dashed",
        "点划线" => "Dash-dot",
        "双点划线" => "Double dash-dot",
        "实心三角" => "Filled triangle",
        "空心三角" => "Outline triangle",
        "端帽" => "Bar",
        "圆点" => "Circle",
        "点线" => "Dotted",
        "无线" => "None",
        "实线箭头" => "Solid Arrow",
        "虚线箭头" => "Dashed Arrow",
        "锥形箭头" => "Tapered Arrow",
        "截屏" => "Capture",
        "全屏截图" => "Fullscreen Capture",
        "取色器" => "Color Picker",
        "贴图" => "Pin",
        "隐藏/显示所有贴图" => "Hide/Show All Pins",
        "贴图工作区" => "Pin Workspace",
        "剪贴板历史" => "Clipboard History",
        "美化导出" => "Beautified Export",
        "Snapin 截图贴图" => "Snapin Capture and Pin",
        "服务已就绪：按 Ctrl+Alt+A 截图，按 F1 贴图" => {
            "Ready: press Ctrl+Alt+A to capture or F1 to pin"
        }
        "Snapin 截图" => "Snapin Capture",
        "Snapin 截图失败" => "Snapin Capture Failed",
        "无法在常驻界面中打开截图覆盖层，详情请查看运行日志" => {
            "Unable to open the capture overlay. Check the logs for details."
        }
        "屏幕捕获或截图初始化失败，详情请查看运行日志" => {
            "Screen capture failed. Check the logs for details."
        }
        "Snapin 贴图提示" => "Snapin Pin",
        "当前剪贴板为空，无法生成贴图" => {
            "The clipboard is empty; no pin can be created."
        }
        "Snapin 贴图已保存" => "Snapin Pin Saved",
        "；Ctrl+Enter 打开链接" => "; press Ctrl+Enter to open the link",
        "复制文字" => "Copy text",
        "编辑文字" => "Edit text",
        "编辑识别文字" => "Edit recognized text",
        "Ctrl+Enter 复制并关闭  Esc 取消" => "Ctrl+Enter to copy and close, Esc to cancel",
        "识别文字不能为空" => "Recognized text cannot be empty",
        "创建贴图缓冲区失败" => "Failed to allocate the pin buffer",
        "单击复制颜色，Esc 取消" => "Click to copy the color; Esc to cancel",
        "已复制图像到剪贴板" => "Image copied to clipboard",
        "未获得有效翻译结果" => "No valid translation result",
        "正在翻译文字..." => "Translating text...",
        "正在识别文字..." => "Recognizing text...",
        "鼠标穿透已关闭" => "Mouse-through disabled",
        "保存失败: 无法提交后台任务" => {
            "Save failed: unable to start the background task"
        }
        "保存失败: 配置写入任务异常" => "Save failed: configuration writer task failed",
        "剪贴板历史已清空" => "Clipboard history cleared",
        "已复制到剪贴板，正在创建贴图" => "Copied to clipboard; creating pin",
        "已还原为默认推荐配置（点击保存后生效）" => {
            "Default settings restored. Click Save to apply."
        }
        "正在保存配置并同步系统设置..." => {
            "Saving settings and syncing system configuration..."
        }
        "正在保存配置，请稍候..." => "Saving settings, please wait...",
        "正在处理配置，请稍候..." => "Processing configuration, please wait...",
        "正在选择要导入的配置文件..." => "Select a configuration file to import...",
        "正在选择配置导出位置..." => "Select an export location...",
        "配置导入失败: 无法提交后台任务" => {
            "Import failed: unable to start the background task"
        }
        "配置导出失败: 无法提交后台任务" => {
            "Export failed: unable to start the background task"
        }
        "首选项..." => "Preferences...",
        "退出 Snapin" => "Quit Snapin",
        "已复制识别文字" => "Recognized text copied",
        "未识别到文字" => "No text recognized",
        "未识别到二维码" => "No QR code found",
        "请等待当前任务完成" => "Please wait for the current task to finish",
        "已取消保存" => "Save cancelled",
        "已取消智能打码" => "Smart redaction cancelled",
        "已选择马赛克" => "Mosaic selected",
        "已选择模糊" => "Blur selected",
        "任务已取消" => "Task cancelled",
        "后台任务媒体执行器不可用" => "Background media executor is unavailable",
        "请先识别二维码" => "Recognize a QR code first",
        "已请求系统浏览器打开二维码链接" => {
            "Opening the QR link in the system browser"
        }
        "用户已取消本次截图" => "Capture cancelled",
        "复制图像失败" => "Failed to copy image",
        "复制识别文字失败" => "Failed to copy recognized text",
        "复制二维码内容失败" => "Failed to copy QR content",
        "打开二维码链接失败" => "Failed to open QR link",
        "合成截图失败" => "Failed to compose screenshot",
        "保存截图失败" => "Failed to save screenshot",
        "导出 OCR 文本失败" => "Failed to export OCR text",
        "美化渲染失败" => "Beautification failed",
        _ => zh,
    }
}

/// 英模式下将未本地化的底层错误收敛为英文提示，详细原因仍保留在中文日志中。
pub fn localize_error(message: &str) -> String {
    if language() == UiLanguage::EnUs && !message.is_ascii() {
        "Operation failed. Check the logs for details.".to_string()
    } else {
        message.to_string()
    }
}

/// 翻译并格式化带动态上下文的用户文案。
#[macro_export]
macro_rules! trf {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {{
        if $crate::i18n::language() == $crate::language::UiLanguage::EnUs {
            format!($en $(, $arg)*)
        } else {
            format!($zh $(, $arg)*)
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LANGUAGE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_language_switches_static_and_formatted_text() {
        let _guard = LANGUAGE_TEST_LOCK.lock().expect("语言测试锁必须可用");
        set_language(UiLanguage::ZhCn);
        assert_eq!(tr("保存"), "保存");
        assert_eq!(
            trf!("已复制 {} 个字符", "Copied {} characters", 3),
            "已复制 3 个字符"
        );

        set_language(UiLanguage::EnUs);
        assert_eq!(tr("保存"), "Save");
        assert_eq!(
            trf!("已复制 {} 个字符", "Copied {} characters", 3),
            "Copied 3 characters"
        );
        assert_eq!(tr("抗锯齿"), "Anti-aliasing");
        assert_eq!(tr("阴影"), "Shadow");
        assert_eq!(tr("线段"), "Line");
        assert_eq!(tr("框线"), "Frame");
        assert_eq!(tr("叉线"), "Cross");
        set_language(UiLanguage::ZhCn);
    }
}
