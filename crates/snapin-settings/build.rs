//! Snapin 构建脚本：编译 Slint 声明式 UI 模板。

fn main() {
    slint_build::compile("ui/settings.slint").expect("编译 Slint UI 模板失败");
}
