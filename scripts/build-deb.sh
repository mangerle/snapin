#!/usr/bin/env bash
# Snapin Debian (.deb) 自动化打包脚本

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="0.1.0"
ARCH="amd64"
PACKAGE_NAME="snapin"
DEB_NAME="${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
STAGE_DIR="${PROJECT_ROOT}/dist/deb-staging"

echo "[信息] 开始构建 Snapin Debian 安装包: ${DEB_NAME}"

# 1. 确保最新发布版本可执行文件已就绪
echo "[信息] 编译发布版本目标..."
cargo build --release --manifest-path "${PROJECT_ROOT}/Cargo.toml"

# 2. 清理并初始化打包暂存目录
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/DEBIAN"
mkdir -p "${STAGE_DIR}/usr/bin"
mkdir -p "${STAGE_DIR}/usr/share/applications"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin-helper@wangle.local"

# 3. 拷贝二进制可执行文件
cp "${PROJECT_ROOT}/target/release/snapin" "${STAGE_DIR}/usr/bin/snapin"
chmod 755 "${STAGE_DIR}/usr/bin/snapin"

# 4. 生成专属 SVG 矢量图标
cat << 'EOF' > "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps/snapin.svg"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3B82F6"/>
      <stop offset="100%" stop-color="#1D4ED8"/>
    </linearGradient>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="120%">
      <feDropShadow dx="0" dy="4" stdDeviation="6" flood-color="#000" flood-opacity="0.3"/>
    </filter>
  </defs>
  <!-- 主圆角底色背景 -->
  <rect x="8" y="8" width="112" height="112" rx="24" fill="url(#bgGrad)" filter="url(#shadow)"/>
  
  <!-- 选区取景框四角指示标 -->
  <path d="M 28 44 L 28 32 L 40 32" fill="none" stroke="#FFFFFF" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M 100 44 L 100 32 L 88 32" fill="none" stroke="#FFFFFF" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M 28 84 L 28 96 L 40 96" fill="none" stroke="#FFFFFF" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M 100 84 L 100 96 L 88 96" fill="none" stroke="#FFFFFF" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>

  <!-- 内部中心浮动图钉 (Pin) 矢量符号 -->
  <circle cx="64" cy="54" r="14" fill="#F59E0B" stroke="#FFFFFF" stroke-width="3"/>
  <circle cx="64" cy="54" r="6" fill="#FFFFFF"/>
  <path d="M 64 68 L 64 88" stroke="#FFFFFF" stroke-width="5" stroke-linecap="round"/>
  <circle cx="64" cy="90" r="2.5" fill="#FFFFFF"/>
</svg>
EOF

# 5. 生成桌面快捷方式 entry 与系统开机自启项
cat << 'EOF' > "${STAGE_DIR}/usr/share/applications/snapin.desktop"
[Desktop Entry]
Name=Snapin
Name[zh_CN]=Snapin 截图贴图
Comment=纯 Rust Wayland 原生截图与置顶贴图工具
Exec=snapin daemon
Icon=snapin
Terminal=false
Type=Application
Categories=Utility;Graphics;
Keywords=screenshot;snip;paste;pin;截图;贴图;
StartupNotify=false
EOF

# 放入系统标准自启动目录，登录桌面时自动常驻后台
mkdir -p "${STAGE_DIR}/etc/xdg/autostart"
cp "${STAGE_DIR}/usr/share/applications/snapin.desktop" "${STAGE_DIR}/etc/xdg/autostart/snapin.desktop"

# 6. 拷贝配套 GNOME Shell 扩展
cp "${PROJECT_ROOT}/extension/metadata.json" "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin-helper@wangle.local/metadata.json"
cp "${PROJECT_ROOT}/extension/extension.js" "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin-helper@wangle.local/extension.js"

# 7. 生成 DEBIAN/control 控制文件
cat << EOF > "${STAGE_DIR}/DEBIAN/control"
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: graphics
Priority: optional
Architecture: ${ARCH}
Maintainer: wangle <wangle@local>
Depends: libc6 (>= 2.34)
Description: 专为 Linux Wayland (Ubuntu GNOME) 打造的纯 Rust 原生截图与贴图工具
 Snapin 是一款对标 Snipaste 的纯原生 Rust Wayland 截图软件。
 支持像素级十字放大镜取色、8 锚点选区拖拽、矢量标注画笔、荧光记号笔、
 马赛克遮蔽、撤销重做栈，以及剪贴板色彩与文本自动便签化置顶贴图。
EOF

# 8. 生成 DEBIAN/postinst 安装后触发脚本
cat << 'EOF' > "${STAGE_DIR}/DEBIAN/postinst"
#!/usr/bin/env bash
set -e

# 刷新桌面数据库与图标缓存
if which update-desktop-database > /dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi

if which gtk-update-icon-cache > /dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi

echo "=========================================================="
echo " Snapin 截图与贴图工具已成功安装！"
echo " 1. 已配置登录自动常驻后台（/etc/xdg/autostart/snapin.desktop）。"
echo " 2. 您可随时在应用程序列表中搜索并点击 Snapin 图标启动，"
echo "    或在终端输入 'snapin &'。"
echo " 3. 守护服务启动时将自动激活 GNOME 置顶扩展并绑定快捷键（F1 截图、F3 贴图）。"
echo "=========================================================="

exit 0
EOF
chmod 755 "${STAGE_DIR}/DEBIAN/postinst"

# 9. 调用 dpkg-deb 生成安装包
echo "[信息] 正在打包生成 .deb 文件..."
dpkg-deb --build --root-owner-group "${STAGE_DIR}" "${PROJECT_ROOT}/${DEB_NAME}"

# 10. 清理临时打包目录
rm -rf "${STAGE_DIR}"

echo "[成功] 打包完成！安装包输出路径: ${PROJECT_ROOT}/${DEB_NAME}"
echo "[提示] 可通过以下命令直接进行本地安装与测试:"
echo "       sudo dpkg -i ${PROJECT_ROOT}/${DEB_NAME}"
