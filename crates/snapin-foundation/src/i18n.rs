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
///
/// 入参使用 `&str` 而非 `&'static str`，允许调用方传入运行时拼接的键；
/// 命中目录的分支返回 `'static` 文案，未命中时回退为调用方原文。
pub fn tr(zh: &str) -> &str {
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
        "清空全部标注" => "Clear All Annotations",
        "橡皮擦画笔" => "Eraser Brush",
        "橡皮擦框选" => "Eraser Box",
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
        "遮罩强度" => "Mask Strength",
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
        "屏幕录制" => "Screen Recording",
        "正在准备屏幕录制" => "Preparing screen recording",
        "屏幕录制工具栏已打开，点击录制按钮开始" => {
            "Screen recording toolbar opened. Click record to start"
        }
        "当前系统声音采集尚未接入，本次将录制无声视频" => {
            "System audio capture is not available yet; this recording will be silent"
        }
        "当前采集通道不保证包含鼠标指针，鼠标高亮将不写入视频" => {
            "The capture backend cannot guarantee cursor inclusion; cursor highlight will not be embedded"
        }
        "正在启动屏幕录制" => "Starting screen recording",
        "正在编码录屏" => "Encoding screen recording",
        "屏幕录制已暂停" => "Screen recording paused",
        "录制期间选区位置已锁定" => "Selection is locked while recording",
        "录制区域无效" => "Invalid recording region",
        "开始或停止录制" => "Start or stop recording",
        "暂停或继续" => "Pause or resume",
        "系统声音" => "System audio",
        "鼠标指针" => "Mouse cursor",
        "录制设置" => "Recording settings",
        "输出格式" => "Output format",
        "切换帧率" => "Switch frame rate",
        "帧率已切换为 {}FPS" => "Frame rate switched to {} FPS",
        "录制已保存: {}" => "Recording saved: {}",
        "录制已完成，文件: {}" => "Recording completed: {}",
        "打开文件" => "Open file",
        "复制文件路径" => "Copy file path",
        "已复制录制文件路径" => "Recording file path copied",
        "播放或暂停预览" => "Play or pause preview",
        "重新录制" => "Record again",
        "撤销标注" => "Undo annotation",
        "重新开始" => "Restart",
        "选区过小，无法开始屏幕录制" => "Selection is too small to start screen recording",
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
        "旧标注已锁定，按住 Alt 点击可重新编辑" => {
            "This annotation is locked. Hold Alt and click to edit it."
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
        "复制并关闭" => "Copy and close",
        "仅复制" => "Copy only",
        "另存为 .md" => "Save as .md",
        "整理排版" => "Format layout",
        "取消" => "Cancel",
        "已整理文字排版" => "Text layout formatted",
        "已清空识别文字" => "Recognized text cleared",
        "识别文字不能为空" => "Recognized text cannot be empty",
        "创建贴图缓冲区失败" => "Failed to allocate the pin buffer",
        "单击复制颜色，Esc 取消" => "Click to copy the color; Esc to cancel",
        "已复制图像到剪贴板" => "Image copied to clipboard",
        "已复制文字到剪贴板" => "Text copied to clipboard",
        "已剪切文字到剪贴板" => "Text cut to clipboard",
        "未获得有效翻译结果" => "No valid translation result",
        "正在翻译文字..." => "Translating text...",
        "正在识别文字..." => "Recognizing text...",
        "鼠标穿透已关闭" => "Mouse-through disabled",
        "鼠标穿透开启时不能隐藏工具条，请先关闭穿透" => {
            "The toolbar cannot be hidden while mouse-through is enabled. Disable mouse-through first."
        }
        "鼠标穿透已开启，点击工具条上的穿透按钮恢复" => {
            "Mouse-through is enabled. Click the mouse-through button in the toolbar to restore interaction."
        }
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
        "请等待当前复制任务完成" => "Please wait for the current copy task to finish",
        "请等待当前剪贴板任务完成" => "Please wait for the current clipboard task to finish",
        "当前剪贴板为空" => "The clipboard is empty",
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
        "长截图" => "Long Capture",
        "选区过小，无法进入长截图" => "Selection is too small to enter long capture",
        "选区过小，无法开始长截图录制" => "Selection is too small to start long capture",
        "请先结束屏幕录制，再进入长截图" => {
            "Finish screen recording before entering long capture"
        }
        "长截图已在进行中" => "Long capture is already in progress",
        "请先完成或关闭长截图，再开始屏幕录制" => {
            "Finish or close long capture before starting screen recording"
        }
        "长截图录制中：在选区内滚动鼠标自动拼接" => {
            "Long capture recording: scroll mouse in selection to stitch automatically"
        }
        "长截图已停止：可调整选区或裁剪，点击开始重新录制" => {
            "Long capture stopped: adjust the selection or crop, then start again"
        }
        "已切换长截图主方向并重置本轮拼接" => {
            "Switched long capture direction and reset the current stitch"
        }
        "已执行上裁剪 (30px)" => "Cropped head (30px)",
        "已执行下裁剪 (30px)" => "Cropped tail (30px)",
        "已执行左裁剪 (30px)" => "Cropped left edge (30px)",
        "已执行右裁剪 (30px)" => "Cropped right edge (30px)",
        "已开启反向滚动自动裁剪" => "Auto-crop on reverse scrolling enabled",
        "已关闭反向滚动自动裁剪" => "Auto-crop on reverse scrolling disabled",
        "尺寸手柄" => "Size Grip",
        "垂直" => "Vertical",
        "水平" => "Horizontal",
        "裁剪" => "Crop",
        "自动裁剪" => "Auto Crop",
        "长图预览" => "Long Capture Preview",
        "已进入长图编辑框：滚轮滚动浏览长图，可使用标注工具" => {
            "Entered long capture editor: scroll to navigate, annotation tools available"
        }
        "抓屏分辨率与当前画布不一致，已保留原底图" => {
            "Capture resolution differs from current canvas; kept previous frame"
        }
        "长截图画布已达最大安全上限，已暂停追加后续内容" => {
            "Long capture canvas reached safety limit; paused appending"
        }
        "已有已拼接内容，再次点击方向按钮将清空重拍" => {
            "Stitched content exists; click Direction again to reset"
        }
        "已达最小尺寸，无法继续裁剪" => "Minimum size reached; cannot crop further",
        "已裁入首屏内部并暂停录制：可按 Ctrl+Z 撤销裁剪，或点击开始重录" => {
            "Cropped into initial frame and paused: Ctrl+Z to undo, or click Start to re-record"
        }
        "已撤销上一次裁剪" => "Undid last crop",
        "暂无可撤销的裁剪记录" => "No crop action to undo",
        "再次点击确认清空画布并切换方向" => {
            "Click again to confirm clearing canvas and switching direction"
        }
        "当前纵向截图 (点击切换横向)" => "Vertical capture (click for horizontal)",
        "当前横向截图 (点击切换纵向)" => "Horizontal capture (click for vertical)",
        "停止截图" => "Stop capture",
        "开始截图" => "Start capture",
        "停止自动滚动 (A)" => "Stop auto-scroll (A)",
        "自动滚到底部 (A)" => "Auto-scroll to end (A)",
        "自动滚到底录制中：再次点击自动滚动按钮或滚动鼠标可随时停止" => {
            "Auto-scrolling: click Auto-Scroll or scroll manually to stop"
        }
        "已自动滚动到底部并停止" => "Reached end and stopped auto-scrolling",
        "自动滚动连续未能匹配新画面，已安全停止" => {
            "Auto-scroll stopped safely after consecutive unmatched frames"
        }
        "已停止自动滚动，请手动滚动或调整选区" => {
            "Auto-scroll stopped; scroll manually or adjust selection"
        }
        "处理队列繁忙，已限流等待对齐" => {
            "Processing queue is busy; throttling capture for alignment"
        }
        "滚动过快，已限流等待拼接对齐" => {
            "Scrolling too fast; throttling for stitch alignment"
        }
        "编辑" => "Edit",
        "拖动工具栏" => "Drag toolbar",
        "上裁剪 (30px)" => "Crop top (30px)",
        "左裁剪 (30px)" => "Crop left (30px)",
        "下裁剪 (30px)" => "Crop bottom (30px)",
        "右裁剪 (30px)" => "Crop right (30px)",
        "撤销裁剪 (Ctrl+Z)" => "Undo crop (Ctrl+Z)",
        "反向滚动时自动裁剪多余画面" => "Auto-crop excess content on reverse scroll",
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
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static LANGUAGE_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// 获取语言切换测试锁；前序用例失败导致锁中毒时仍继续，避免连带失败。
    fn lock_language() -> std::sync::MutexGuard<'static, ()> {
        LANGUAGE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 查找工作区根目录；以源码目录发布时不具备跨 crate 扫描条件，测试直接跳过。
    fn workspace_root() -> Option<PathBuf> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir.parent()?.parent()?.to_path_buf();
        root.join("crates/snapin-ui").is_dir().then_some(root)
    }

    /// 递归收集工作区内的 Rust 源码文件。
    fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("target") {
                    collect_rust_files(&path, files);
                }
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    /// 提取源码中所有 `tr` 调用携带的第一段字符串字面量。
    ///
    /// 这里只服务于测试：逐字节识别调用点，允许参数跨行，忽略 `trf!` 与其他
    /// 以 `tr` 结尾的标识符。生产代码仍以 [`tr`] 的运行时查表为准。
    fn collect_tr_keys(source: &str, keys: &mut Vec<String>) {
        let bytes = source.as_bytes();
        let mut index = 0;
        while index + 3 <= bytes.len() {
            if bytes[index] != b't' || bytes[index + 1] != b'r' || bytes[index + 2] != b'(' {
                index += 1;
                continue;
            }
            if index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_') {
                index += 1;
                continue;
            }
            let mut cursor = index + 3;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() || bytes[cursor] != b'"' {
                index += 1;
                continue;
            }
            cursor += 1;
            let mut value = Vec::new();
            let mut terminated = false;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'\\' if cursor + 1 < bytes.len() => {
                        value.push(bytes[cursor]);
                        value.push(bytes[cursor + 1]);
                        cursor += 2;
                    }
                    b'"' => {
                        terminated = true;
                        cursor += 1;
                        break;
                    }
                    byte => {
                        value.push(byte);
                        cursor += 1;
                    }
                }
            }
            if terminated {
                if let Ok(key) = String::from_utf8(value) {
                    keys.push(key);
                }
            }
            index = cursor;
        }
    }

    /// 防止新增用户文案时只写中文、忘记补英文翻译而静默回退。
    ///
    /// 该测试等价于一次“翻译目录完整性”门禁：英文模式下每个调用点都必须
    /// 命中目录，否则 [`tr`] 会原样返回中文键，界面出现中英混排。
    #[test]
    fn test_every_user_facing_key_has_english_translation() {
        let _guard = lock_language();
        let Some(root) = workspace_root() else {
            return;
        };
        let mut files = Vec::new();
        collect_rust_files(&root.join("crates"), &mut files);

        set_language(UiLanguage::EnUs);
        let mut missing = Vec::new();
        for file in files {
            let Ok(source) = std::fs::read_to_string(&file) else {
                continue;
            };
            let mut keys = Vec::new();
            collect_tr_keys(&source, &mut keys);
            for key in keys {
                if tr(&key) == key.as_str() {
                    missing.push(format!("{}: {key}", file.display()));
                }
            }
        }
        set_language(UiLanguage::ZhCn);

        assert!(
            missing.is_empty(),
            "以下用户文案缺少英文翻译，请补充到 i18n 目录:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn test_language_switches_static_and_formatted_text() {
        let _guard = lock_language();
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
