import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import St from 'gi://St';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

// 定义 D-Bus 接口：接收贴图窗口标题，并在 Mutter 合成器内部将其置顶
const IFACE_XML = `
<node>
  <interface name="org.gnome.Shell.Extensions.Snapin">
    <method name="PinWindowByTitle">
      <arg type="s" direction="in" name="windowTitle"/>
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

        let icon = new St.Icon({
            icon_name: 'image-crop-symbolic',
            style_class: 'system-status-icon',
        });
        this._indicator.add_child(icon);

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
    }

    disable() {
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
     * 在 Mutter 窗口树中查找指定标题的贴图窗口并调用 make_above 置顶。
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
                return true;
            }
        }
        return false;
    }
}
