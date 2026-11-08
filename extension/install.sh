#!/usr/bin/env bash
# Snapin GNOME Shell 扩展一键安装脚本
set -e

EXT_UUID="snapin@wangle"
LEGACY_UUIDS=("snapin@wangle.local" "snapin-helper@wangle.local" "snapin-helper@wangle")
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
TARGET_DIR="$DATA_HOME/gnome-shell/extensions/$EXT_UUID"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "正在清理历史版本 GNOME Shell 扩展..."
for uuid in "${LEGACY_UUIDS[@]}"; do
    gnome-extensions disable "$uuid" 2>/dev/null || true
    rm -rf "$DATA_HOME/gnome-shell/extensions/$uuid"
done

echo "正在部署 Snapin GNOME Shell 扩展至: $TARGET_DIR"
mkdir -p "$TARGET_DIR"
cp "$SCRIPT_DIR/metadata.json" "$TARGET_DIR/"
cp "$SCRIPT_DIR/extension.js" "$TARGET_DIR/"
cp -r "$SCRIPT_DIR/icons" "$TARGET_DIR/" 2>/dev/null || true

echo "正在启用扩展..."
gnome-extensions enable "$EXT_UUID" 2>/dev/null || true

# Shell 不会立即扫描刚复制的新 UUID；同步启用列表，确保下次登录自动加载。
if command -v gjs >/dev/null 2>&1; then
    gjs -c "const Gio = imports.gi.Gio; const settings = new Gio.Settings({schema_id: 'org.gnome.shell'}); const uuid = '$EXT_UUID'; const legacy = new Set(['snapin@wangle.local', 'snapin-helper@wangle.local', 'snapin-helper@wangle']); for (const key of ['enabled-extensions', 'disabled-extensions']) { const values = settings.get_strv(key).filter(item => !legacy.has(item)); if (key === 'enabled-extensions' && !values.includes(uuid)) values.push(uuid); settings.set_strv(key, values); }" 2>/dev/null || true
fi

echo "扩展部署成功！首次迁移或接口升级后请注销并重新登录。"
