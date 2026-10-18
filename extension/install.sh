#!/usr/bin/env bash
# Snapin GNOME Shell 扩展一键安装脚本
set -e

EXT_UUID="snapin-helper@wangle.local"
TARGET_DIR="$HOME/.local/share/gnome-shell/extensions/$EXT_UUID"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "正在部署 Snapin GNOME Shell 扩展至: $TARGET_DIR"
mkdir -p "$TARGET_DIR"
cp "$SCRIPT_DIR/metadata.json" "$TARGET_DIR/"
cp "$SCRIPT_DIR/extension.js" "$TARGET_DIR/"

echo "正在尝试启用扩展..."
gnome-extensions enable "$EXT_UUID" 2>/dev/null || true

echo "扩展部署成功！"
