//! Snapin 构建脚本：编译 Slint 声明式 UI 模板。

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint_build::compile("ui/settings.slint")
        .map_err(|error| std::io::Error::other(format!("编译 Slint UI 模板失败: {error}")))?;
    Ok(())
}
