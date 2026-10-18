import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Gio from 'gi://Gio';

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
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._dbusImpl.export(
            Gio.DBus.session,
            '/org/gnome/Shell/Extensions/Snapin'
        );
    }

    disable() {
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
