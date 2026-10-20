#!/usr/bin/env bash
# Snapin Debian (.deb) 自动化打包脚本

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# 从 Cargo.toml 提取全局唯一定义的版本号 (单一真实来源 Single Source of Truth)
CARGO_VERSION=$(grep -m 1 '^version = ' "${PROJECT_ROOT}/Cargo.toml" | cut -d '"' -f 2)
# 优先使用环境变量 VERSION，其次使用 GITHUB_REF_NAME (去除 v 前缀)，最后直接取 Cargo.toml 的版本
VERSION="${VERSION:-${GITHUB_REF_NAME:-}}"
VERSION="${VERSION#v}"
VERSION="${VERSION:-${CARGO_VERSION}}"

if [ -z "${VERSION}" ]; then
    echo "[错误] 无法从 Cargo.toml 解析项目版本号，请检查 Cargo.toml 文件！" >&2
    exit 1
fi
ARCH="amd64"
PACKAGE_NAME="snapin"
DEB_NAME="${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
OUTPUT_DIR="${PROJECT_ROOT}/target"
OUTPUT_DEB="${OUTPUT_DIR}/${DEB_NAME}"
STAGE_DIR="${OUTPUT_DIR}/deb-staging"

echo "[信息] 开始构建 Snapin Debian 安装包: ${OUTPUT_DEB}"

# 1. 确保最新发布版本可执行文件已就绪
echo "[信息] 编译发布版本目标..."
cargo build --release --manifest-path "${PROJECT_ROOT}/Cargo.toml"

# 2. 清理并初始化打包暂存目录
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/DEBIAN"
mkdir -p "${STAGE_DIR}/usr/bin"
mkdir -p "${STAGE_DIR}/usr/share/applications"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/scalable/status"
mkdir -p "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin@wangle.local"

# 3. 拷贝二进制可执行文件
cp "${PROJECT_ROOT}/target/release/snapin" "${STAGE_DIR}/usr/bin/snapin"
chmod 755 "${STAGE_DIR}/usr/bin/snapin"

# 4. 拷贝专属现代 SVG 矢量应用图标与系统状态托盘图标
cp "${PROJECT_ROOT}/assets/icons/snapin.svg" "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps/snapin.svg"
cp "${PROJECT_ROOT}/assets/icons/snapin-tray.svg" "${STAGE_DIR}/usr/share/icons/hicolor/scalable/status/snapin-tray.svg"

# 4.1 打包 PP-OCRv6 离线模型资产至 FHS 只读数据目录 /usr/share/snapin/models
#     模型为包自带的只读、架构无关数据，按 FHS 归 /usr/share；运行时的解析链已优先
#     读取 ~/.local/share/snapin/models/ppocrv6，用户需要换模型时自行覆盖即可，
#     严禁在 postinst 里往用户家目录写入（多用户重复拷贝且 dpkg 卸载无法回收）。
MODEL_SRC_DIR="${PROJECT_ROOT}/assets/models/ppocrv6"
MODEL_DST_DIR="${STAGE_DIR}/usr/share/snapin/models/ppocrv6"
if [ -d "${MODEL_SRC_DIR}" ]; then
    echo "[信息] 打包 PP-OCRv6 离线模型资产至 ${MODEL_DST_DIR}..."
    mkdir -p "${MODEL_DST_DIR}"
    if ! cp -r "${MODEL_SRC_DIR}/." "${MODEL_DST_DIR}/"; then
        echo "[错误] PP-OCRv6 模型资产拷贝失败，中止打包（否则会产出装完即 ModelNotFound 的包）" >&2
        exit 1
    fi
else
    echo "[警告] 未找到 ${MODEL_SRC_DIR}，本次软件包不含离线识别模型" >&2
fi

cat << 'EOF' > "${STAGE_DIR}/usr/share/applications/snapin.desktop"
[Desktop Entry]
Name=Snapin
Comment=纯 Rust Wayland 原生截图与置顶贴图工具
Exec=snapin
Icon=snapin
Terminal=false
Type=Application
Categories=Utility;Graphics;
Keywords=screenshot;snip;paste;pin;截图;贴图;
StartupNotify=false
Actions=Snip;Prefs;

[Desktop Action Snip]
Name=Take Screenshot
Name[zh_CN]=开始截图
Exec=snapin snip

[Desktop Action Prefs]
Name=Preferences
Name[zh_CN]=偏好设置
Exec=snapin prefs
EOF

# 6. 生成 systemd 用户服务单元 (现代 Linux 桌面标准常驻守护机制)
mkdir -p "${STAGE_DIR}/usr/lib/systemd/user"
cat << 'EOF' > "${STAGE_DIR}/usr/lib/systemd/user/snapin.service"
[Unit]
Description=Snapin 截图与贴图守护进程
Documentation=https://github.com/wangle/snapin
PartOf=graphical-session.target
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/snapin daemon
Restart=on-failure
RestartSec=2s
Slice=app.slice

[Install]
WantedBy=graphical-session.target
EOF

# 7. 拷贝配套 GNOME Shell 扩展及其图标
cp "${PROJECT_ROOT}/extension/metadata.json" "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin@wangle.local/metadata.json"
cp "${PROJECT_ROOT}/extension/extension.js" "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin@wangle.local/extension.js"
mkdir -p "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin@wangle.local/icons"
cp "${PROJECT_ROOT}/extension/icons/snapin-tray.svg" "${STAGE_DIR}/usr/share/gnome-shell/extensions/snapin@wangle.local/icons/snapin-tray.svg"

# 7.1 PP-OCRv6 模型资产已在第 4.1 步统一打包，此处不得重复拷贝：
#     目标目录已存在时 cp -r 会再套一层 ppocrv6/ppocrv6，使软件包白带一倍体积。

# 8. 生成 DEBIAN/control 控制文件
cat << EOF > "${STAGE_DIR}/DEBIAN/control"
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: graphics
Priority: optional
Architecture: ${ARCH}
Maintainer: wangle <wangle@local>
Depends: libc6 (>= 2.34), libonnxruntime1.23
Description: 专为 Linux Wayland (Ubuntu GNOME) 打造的纯 Rust 原生截图与贴图工具
 Snapin 是一款对标 Snipaste 的纯原生 Rust Wayland 截图软件。
 支持像素级十字放大镜取色、8 锚点选区拖拽、矢量标注画笔、荧光记号笔、
 马赛克遮蔽、撤销重做栈，以及剪贴板色彩与文本自动便签化置顶贴图。
EOF

# 9. 生成 DEBIAN/prerm 卸载或升级前触发脚本
cat << 'EOF' > "${STAGE_DIR}/DEBIAN/prerm"
#!/usr/bin/env bash
set -e

# 在卸载或升级覆盖前停止所有已登录用户的守护进程
if which loginctl > /dev/null 2>&1; then
    for user_name in $(loginctl list-users --no-legend 2>/dev/null | awk '{print $2}'); do
        user_id=$(id -u "$user_name" 2>/dev/null || true)
        if [ -n "$user_id" ] && [ "$user_id" -ge 1000 ]; then
            runuser -u "$user_name" -- env XDG_RUNTIME_DIR="/run/user/$user_id" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$user_id/bus" systemctl --user stop snapin.service 2>/dev/null || true
            pkill -u "$user_name" -x snapin 2>/dev/null || true
        fi
    done
fi

exit 0
EOF
chmod 755 "${STAGE_DIR}/DEBIAN/prerm"

# 10. 生成 DEBIAN/postinst 安装后触发脚本
cat << 'EOF' > "${STAGE_DIR}/DEBIAN/postinst"
#!/usr/bin/env bash
set -e

# 清理可能残留的独立设置桌面快捷方式（收敛为单一主应用图标）
rm -f /usr/share/applications/snapin-settings.desktop

# 刷新桌面数据库与图标缓存
if which update-desktop-database > /dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi

if which gtk-update-icon-cache > /dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi

# 对当前处于登录状态的所有普通桌面用户执行免登出热更新与服务自启
if which loginctl > /dev/null 2>&1; then
    for user_name in $(loginctl list-users --no-legend 2>/dev/null | awk '{print $2}'); do
        user_id=$(id -u "$user_name" 2>/dev/null || true)
        if [ -z "$user_id" ] || [ "$user_id" -lt 1000 ]; then
            continue
        fi

        user_home=$(eval echo "~$user_name")
        runtime_dir="/run/user/$user_id"

        # 1. 清理旧版扩展目录，同步最新扩展至用户目录
        old_user_ext_dir="$user_home/.local/share/gnome-shell/extensions/snapin-helper@wangle.local"
        rm -rf "$old_user_ext_dir" 2>/dev/null || true

        # 清理旧版可能冲突的 autostart 桌面文件，统一由 systemd user 服务调度守护进程
        rm -f /etc/xdg/autostart/snapin.desktop 2>/dev/null || true
        rm -f "$user_home/.config/autostart/snapin.desktop" 2>/dev/null || true

        user_ext_dir="$user_home/.local/share/gnome-shell/extensions/snapin@wangle.local"
        mkdir -p "$user_ext_dir"
        cp -r /usr/share/gnome-shell/extensions/snapin@wangle.local/* "$user_ext_dir/" 2>/dev/null || true
        chown -R "$user_name:$user_name" "$user_ext_dir" 2>/dev/null || true

        # 2. 清理旧版扩展并置为启用态。
        #    注意：gnome-extensions enable 只会重新实例化扩展对象，不会重新读取磁盘上的 JS ——
        #    GJS 按 URI 缓存扩展模块，GNOME 41+ 又禁用了 org.gnome.Shell.Eval，Wayland 下也没有
        #    Alt+F2 r。因此扩展代码变更必须等用户注销重登（或重启 gnome-shell 会话）才真正生效，
        #    运行中的 shell 在此期间仍是旧代码，切勿在提示文案里宣称"无需注销"。
        if [ -d "$runtime_dir" ]; then
            runuser -u "$user_name" -- env XDG_RUNTIME_DIR="$runtime_dir" DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus" gnome-extensions disable snapin-helper@wangle.local 2>/dev/null || true
            runuser -u "$user_name" -- env XDG_RUNTIME_DIR="$runtime_dir" DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus" gnome-extensions enable snapin@wangle.local 2>/dev/null || true

            # 清理旧进程残留，并通过 systemd user 服务拉起最新守护进程
            pkill -u "$user_name" -x snapin 2>/dev/null || true
            runuser -u "$user_name" -- env XDG_RUNTIME_DIR="$runtime_dir" DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus" systemctl --user daemon-reload 2>/dev/null || true
            runuser -u "$user_name" -- env XDG_RUNTIME_DIR="$runtime_dir" DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus" systemctl --user enable --now snapin.service 2>/dev/null || true
        fi
    done
fi

echo "=========================================================="
echo " Snapin 截图与贴图工具已成功安装！"
echo " 1. 后台守护服务与命令行已即时更新（systemctl --user status snapin）。"
echo " 2. 系统快捷键与自启配置已同步就绪。"
echo " 3. 【重要】GNOME Shell 扩展的 JS 代码无法热更新：GJS 会按 URI 缓存扩展模块，"
echo "    运行中的 shell 仍是旧版本。请注销并重新登录一次，新扩展才会生效。"
echo "    未重登前截图会自动降级到 XDG Portal 通道（可用，但带快门闪屏）。"
echo "=========================================================="

exit 0
EOF
chmod 755 "${STAGE_DIR}/DEBIAN/postinst"

# 9. 调用 dpkg-deb 生成安装包
echo "[信息] 正在打包生成 .deb 文件..."
mkdir -p "${OUTPUT_DIR}"
dpkg-deb --build --root-owner-group "${STAGE_DIR}" "${OUTPUT_DEB}"

# 10. 清理临时打包目录
rm -rf "${STAGE_DIR}"

echo "[成功] 打包完成！安装包输出路径: ${OUTPUT_DEB}"
echo "[提示] 可通过以下命令直接进行本地安装与测试:"
echo "       sudo dpkg -i ${OUTPUT_DEB}"
