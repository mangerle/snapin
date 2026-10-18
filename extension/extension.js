import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import St from 'gi://St';
import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';

// 定义 D-Bus 接口：接收贴图窗口标题，并在 Mutter 合成器内部将其置顶；提供原生零闪烁极速截屏服务
const IFACE_XML = `
<node>
  <interface name="org.gnome.Shell.Extensions.Snapin">
    <method name="PinWindowByTitle">
      <arg type="s" direction="in" name="windowTitle"/>
      <arg type="b" direction="out" name="success"/>
    </method>
    <method name="CaptureFullscreen">
      <arg type="s" direction="in" name="targetPath"/>
      <arg type="b" direction="out" name="success"/>
    </method>
  </interface>
</node>`;

export default class SnapinHelperExtension extends Extension {
    enable() {
        // 1. 导出 Mutter 特权置顶 D-Bus 服务
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._dbusImpl.export(
            Gio.DBus.session,
            '/org/gnome/Shell/Extensions/Snapin'
        );

        // 2. 创建 GNOME 顶栏原生托盘指示器与菜单
        this._indicator = new PanelMenu.Button(0.0, 'Snapin Indicator', false);
        // 紧凑模式水平内边距，防止托盘按钮被横向过度拉伸导致药丸圆角过宽
        this._indicator.set_style('-natural-hpadding: 6px; -minimum-hpadding: 4px; margin: 0; padding: 0;');

        let box = new St.BoxLayout({
            style_class: 'panel-status-indicators-box',
        });
        let icon = new St.Icon({
            icon_name: 'image-crop-symbolic',
            style_class: 'system-status-icon',
            style: 'margin: 0; padding: 0;',
        });
        box.add_child(icon);
        this._indicator.add_child(box);

        // 添加托盘菜单项
        let snipItem = new PopupMenu.PopupMenuItem('区域截图 (F1)');
        snipItem.connect('activate', () => {
            GLib.spawn_command_line_async('snapin snip');
        });
        this._indicator.menu.addMenuItem(snipItem);

        let pasteItem = new PopupMenu.PopupMenuItem('固定贴图 / 便签 (F3)');
        pasteItem.connect('activate', () => {
            GLib.spawn_command_line_async('snapin paste');
        });
        this._indicator.menu.addMenuItem(pasteItem);

        let toggleItem = new PopupMenu.PopupMenuItem('显示 / 隐藏全部贴图');
        toggleItem.connect('activate', () => {
            GLib.spawn_command_line_async('snapin toggle');
        });
        this._indicator.menu.addMenuItem(toggleItem);

        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        let prefsItem = new PopupMenu.PopupMenuItem('首选项 / 设置...');
        prefsItem.connect('activate', () => {
            GLib.spawn_command_line_async('snapin prefs');
        });
        this._indicator.menu.addMenuItem(prefsItem);

        let syncItem = new PopupMenu.PopupMenuItem('同步系统快捷键');
        syncItem.connect('activate', () => {
            GLib.spawn_command_line_async('snapin sync-hotkeys');
        });
        this._indicator.menu.addMenuItem(syncItem);

        let exitItem = new PopupMenu.PopupMenuItem('退出 Snapin');
        exitItem.connect('activate', () => {
            GLib.spawn_command_line_async('killall snapin');
        });
        this._indicator.menu.addMenuItem(exitItem);

        // 将托盘添加到顶栏右侧状态栏区域
        Main.panel.addToStatusArea('snapin-indicator', this._indicator, 1, 'right');

        // 3. 监听全局窗口创建事件：贴图窗口一诞生立即强制置顶并设为 UTILITY 类型从 Dock / 任务栏中隐藏
        const applyPinAttributes = (metaWin) => {
            if (!metaWin) return;
            let title = metaWin.get_title();
            let wmClass = metaWin.get_wm_class ? metaWin.get_wm_class() : '';
            if ((title && (title.startsWith('pin_') || title.startsWith('clip_'))) || wmClass === 'snapin-pin') {
                metaWin.make_above();
                if (Meta.WindowType && typeof metaWin.set_type === 'function') {
                    metaWin.set_type(Meta.WindowType.UTILITY);
                }
            }
        };

        this._windowCreatedId = global.display.connect('window-created', (_display, metaWin) => {
            applyPinAttributes(metaWin);
            if (metaWin) {
                let notifyId = metaWin.connect('notify::title', () => {
                    applyPinAttributes(metaWin);
                    metaWin.disconnect(notifyId);
                });
            }
        });
    }

    disable() {
        if (this._windowCreatedId) {
            global.display.disconnect(this._windowCreatedId);
            this._windowCreatedId = null;
        }

        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }

        if (this._dbusImpl) {
            this._dbusImpl.unexport();
            this._dbusImpl = null;
        }
    }

    /**
     * 在 Mutter 窗口树中查找指定标题的贴图窗口并调用 make_above 置顶及设为 UTILITY 隐藏 Dock 图标。
     * @param {string} windowTitle 贴图窗口标题
     * @returns {boolean} 是否成功置顶
     */
    PinWindowByTitle(windowTitle) {
        if (!windowTitle) {
            return false;
        }
        let actors = global.get_window_actors();
        for (let actor of actors) {
            let metaWin = actor.get_meta_window();
            if (metaWin && metaWin.get_title() === windowTitle) {
                metaWin.make_above();
                if (Meta.WindowType && typeof metaWin.set_type === 'function') {
                    metaWin.set_type(Meta.WindowType.UTILITY);
                }
                return true;
            }
        }
        return false;
    }

    /**
     * 调用 Mutter 内部原生 Shell.Screenshot 实现零延迟、无快门白闪的全屏截屏
     * @param {string} targetPath 存储目标图片路径
     * @returns {boolean} 是否捕获成功
     */
    async CaptureFullscreen(targetPath) {
        if (!targetPath) {
            return false;
        }
        try {
            let screenshot = new Shell.Screenshot();
            let file = Gio.File.new_for_path(targetPath);
            let stream = file.replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
            await screenshot.screenshot(false, stream);
            stream.close(null);
            return true;
        } catch (e) {
            console.error('Snapin 原生截屏失败:', e);
            return false;
        }
    }
}
