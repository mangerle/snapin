//! Snapin 主程序构建脚本。
//!
//! 在 Windows 平台下将原生应用图标资源编译嵌入 PE 二进制文件，
//! 确保桌面快捷方式、开始菜单及任务栏正确显示高分辨率图标，避免显示空白方框。

fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/icon.ico");
        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2,PerMonitor</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"#,
        );
        if let Err(error) = res.compile() {
            eprintln!("为 Windows 可执行文件嵌入图标与清单资源失败: {error}");
        }
    }
}
