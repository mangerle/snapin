//! Snapin 构建脚本：编译 Slint 声明式 UI 模板。

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("ui/translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/settings.slint", config)
        .map_err(|error| std::io::Error::other(format!("编译 Slint UI 模板失败: {error}")))?;
    Ok(())
}
