import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import St from 'gi://St';
import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import GLib from 'gi://GLib';

// 定义 D-Bus 接口：
//   1. PlaceWindowByTitle —— Wayland 禁止客户端自行摆放或提级窗口，贴图窗口的绝对定位与
//      置顶必须由拥有特权的 Mutter 扩展代为执行。
//   2. CaptureFullscreen   —— 用合成器内部对象直接把整屏写进给定文件流，无快门白闪。
//
// 曾经还有第三个方法 CaptureFullscreenRaw（在 shell 进程内 read_pixels 取原始像素），
// 已永久移除：该调用的数组返回值缺少长度注解，在 GNOME 50 的 GJS 上会命中
// gi/arg.cpp 断言并 `Bail out!` 以 SIGABRT 终止 gnome-shell，表现为按一次截图就被登出。
// 它在本机历史上从未成功过一次，而 CaptureFullscreen 成功过 46 次（258~1170ms）。
const IFACE_XML = `
<node>
  <interface name="org.gnome.Shell.Extensions.Snapin">
    <method name="PlaceWindowByTitle">
      <arg type="s" direction="in" name="windowTitle"/>
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
      <arg type="b" direction="out" name="placed"/>
    </method>
    <method name="CaptureFullscreen">
      <arg type="s" direction="in" name="targetPath"/>
      <arg type="b" direction="out" name="success"/>
    </method>
  </interface>
</node>`;

// 托盘菜单条目：以数据表驱动构建，避免重复的 connect 样板并防止同一命令被多处重复注册
const TRAY_MENU = [
    { label: '截屏', command: 'snapin snip' },
    { label: '截屏并自动复制', command: 'snapin snip --auto-copy' },
    { label: '自定义截屏', command: 'snapin snip --custom' },
    { label: '贴图', command: 'snapin paste' },
    { label: '隐藏/显示所有贴图', command: 'snapin toggle' },
    // 不带分组参数时由守护进程轮转至下一个分组，硬编码分组会使其永远停在同一组
    { label: '切换到另一贴图组', command: 'snapin switch-group' },
    { label: '清空截屏历史', command: 'snapin clear-history' },
    { separator: true },
    { label: '首选项...', command: 'snapin prefs' },
    {
        label: '重新启动',
        command: 'bash -c "systemctl --user restart snapin 2>/dev/null || (snapin exit 2>/dev/null; sleep 0.5; snapin daemon &)"',
    },
    { label: '停止后台服务（保留托盘）', command: 'snapin exit' },
];

// 延迟放置请求的最长有效期：贴图标题含唯一 ID 不会复用，超时仅用于兜底回收异常未映射的请求
const PLACEMENT_TTL_US = 5 * 1000 * 1000;

export default class SnapinHelperExtension extends Extension {
    enable() {
        // 1. 导出 Mutter 特权窗口操作与原生截屏 D-Bus 服务
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._dbusImpl.export(
            Gio.DBus.session,
            '/org/gnome/Shell/Extensions/Snapin'
        );

        // 尚未映射窗口的放置请求登记表，键为窗口标题
        this._pendingPlacements = new Map();
        // 已连接的窗口标题监听器，disable 时统一断开，防止扩展重载后闭包泄漏
        this._titleWatchers = new Set();

        // 2. 创建 GNOME 顶栏原生托盘指示器与菜单（0.5 对齐使菜单在托盘正下方居中弹出）
        this._indicator = new PanelMenu.Button(0.5, 'Snapin Indicator', false);
        this._indicator.menu.setSourceAlignment(0.5);
        this._indicator.set_style('-natural-hpadding: 6px; -minimum-hpadding: 4px; margin: 0; padding: 0;');

        let box = new St.BoxLayout({
            style_class: 'panel-status-indicators-box',
        });
        let iconFile = Gio.File.new_for_path(this.path + '/icons/snapin-tray.svg');
        let icon;
        if (iconFile.query_exists(null)) {
            icon = new St.Icon({
                gicon: new Gio.FileIcon({ file: iconFile }),
                style_class: 'system-status-icon',
                style: 'margin: 0; padding: 0;',
            });
        } else {
            icon = new St.Icon({
                icon_name: 'image-crop-symbolic',
                style_class: 'system-status-icon',
                style: 'margin: 0; padding: 0;',
            });
        }
        box.add_child(icon);
        this._indicator.add_child(box);

        for (let entry of TRAY_MENU) {
            let item;
            if (entry.separator) {
                item = new PopupMenu.PopupSeparatorMenuItem();
            } else {
                item = new PopupMenu.PopupMenuItem(entry.label);
                item.connect('activate', () => {
                    GLib.spawn_command_line_async(entry.command);
                });
            }
            this._indicator.menu.addMenuItem(item);
        }

        // 将托盘添加到顶栏右侧状态栏区域
        Main.panel.addToStatusArea('snapin-indicator', this._indicator, 1, 'right');

        // 3. 监听全局窗口创建事件：贴图窗口一诞生立即强制置顶，并设为 UTILITY 类型从 Dock 隐藏
        this._windowCreatedId = global.display.connect('window-created', (_display, metaWin) => {
            this._applyPinAttributes(metaWin);
            if (!metaWin) return;
            // 部分窗口在 map 之后才写入标题，补挂一次性监听确保属性最终生效
            const watcher = { window: metaWin, id: 0 };
            watcher.id = metaWin.connect('notify::title', () => {
                this._applyPinAttributes(metaWin);
                metaWin.disconnect(watcher.id);
                this._titleWatchers.delete(watcher);
            });
            this._titleWatchers.add(watcher);
        });
    }

    disable() {
        if (this._windowCreatedId) {
            global.display.disconnect(this._windowCreatedId);
            this._windowCreatedId = null;
        }

        for (let watcher of this._titleWatchers) {
            watcher.window.disconnect(watcher.id);
        }
        this._titleWatchers.clear();

        this._pendingPlacements = new Map();

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
     * 判断窗口是否为 Snapin 贴图窗口。
     * @param {Meta.Window} metaWin 目标窗口
     * @returns {boolean} 是否为受管贴图窗口
     */
    _isPasterWindow(metaWin) {
        let title = metaWin.get_title();
        let wmClass = metaWin.get_wm_class ? metaWin.get_wm_class() : '';
        if (wmClass === 'snapin-pin') return true;
        return !!title && (title.startsWith('pin_') || title.startsWith('clip_'));
    }

    /**
     * 按物理像素坐标移动窗口。Mutter 的 move_frame 接受逻辑坐标，故需按窗口缩放比换算。
     * @param {Meta.Window} metaWin 目标窗口
     * @param {number} x 物理像素 X 坐标
     * @param {number} y 物理像素 Y 坐标
     */
    _moveWindowByPhysical(metaWin, x, y) {
        if (typeof metaWin.move_frame !== 'function') return;
        let scale = this._windowScale(metaWin);
        metaWin.move_frame(true, Math.round(x / scale), Math.round(y / scale));
    }

    /**
     * 取窗口所在显示器的实际缩放倍率（优先小数倍率）。
     *
     * get_scale_factor() 只返回整数倍率，在 166%/125% 这类分数缩放下会得到 1，
     * 若据此把物理像素当逻辑坐标使用，落点会被整体放大（实测 166% 下偏移 1.667 倍）。
     * @param {Meta.Window} metaWin 目标窗口
     * @returns {number} 缩放倍率，恒大于 0
     */
    _windowScale(metaWin) {
        // 1. 首选显示器的小数缩放比（GNOME 3.36+）
        try {
            let fractional = global.display.get_monitor_scale(metaWin.get_monitor());
            if (fractional && fractional > 0) {
                return fractional;
            }
        } catch (e) {
            // 旧版本没有该接口，继续用后面的兜底
        }

        // 2. 用窗口自身的物理缓冲区与逻辑帧矩形之比反推真实倍率，不依赖任何版本接口
        try {
            let frame = metaWin.get_frame_rect();
            let buffer = metaWin.get_buffer_rect();
            if (frame.width > 0 && buffer.width > 0) {
                return buffer.width / frame.width;
            }
        } catch (e) {
            // 继续兜底
        }

        // 3. 最后退回整数倍率：分数缩放下会得到 1，仅保证不崩溃
        if (typeof metaWin.get_scale_factor === 'function') {
            return metaWin.get_scale_factor() || 1;
        }
        return 1;
    }

    /**
     * 为贴图窗口应用置顶、Dock 隐藏与最终定位。
     *
     * 定位策略：优先消费客户端经 D-Bus 提交的精确坐标请求（截图选区原地贴图、命令行指定位置）；
     * 无请求时才回退为就近对齐鼠标光标，避免 Mutter 把窗口丢到屏幕角落。
     * @param {Meta.Window} metaWin 目标窗口
     */
    _applyPinAttributes(metaWin) {
        if (!metaWin || !this._isPasterWindow(metaWin)) return;

        metaWin.make_above();
        if (Meta.WindowType && typeof metaWin.set_type === 'function') {
            metaWin.set_type(Meta.WindowType.UTILITY);
        }

        if (metaWin._snapinPlaced) return;
        metaWin._snapinPlaced = true;

        let title = metaWin.get_title();
        let pending = title && this._pendingPlacements.has(title)
            ? this._pendingPlacements.get(title)
            : null;
        if (pending) {
            this._pendingPlacements.delete(title);
            this._moveWindowByPhysical(metaWin, pending.x, pending.y);
            return;
        }

        let [mouseX, mouseY] = global.get_pointer();
        let rect = metaWin.get_frame_rect();
        let scale = this._windowScale(metaWin);
        let targetX = Math.max(10, mouseX - Math.floor(rect.width / scale / 2));
        let targetY = Math.max(30, mouseY - Math.floor(rect.height / scale / 2));
        if (typeof metaWin.move_frame === 'function') {
            metaWin.move_frame(true, targetX, targetY);
        }
    }

    /**
     * 在 Mutter 窗口树中按标题查找贴图窗口。
     * @param {string} windowTitle 贴图窗口标题
     * @returns {Meta.Window|null} 命中的窗口
     */
    _findPasterWindow(windowTitle) {
        for (let actor of global.get_window_actors()) {
            let metaWin = actor.get_meta_window();
            if (metaWin && metaWin.get_title() === windowTitle) {
                return metaWin;
            }
        }
        return null;
    }

    /** 回收超过有效期的延迟放置请求，防止异常未映射的标题无限堆积 */
    _prunePlacements() {
        let now = GLib.get_monotonic_time();
        for (let [title, placement] of this._pendingPlacements) {
            if (now - placement.at > PLACEMENT_TTL_US) {
                this._pendingPlacements.delete(title);
            }
        }
    }

    /**
     * 请求扩展将指定贴图窗口置顶并精确放置到给定物理像素坐标。
     *
     * 窗口可能尚未完成映射（事件循环与 D-Bus 调用存在竞态），此时登记为延迟请求，
     * 待 window-created 触发时立即生效，从而保证坐标语义不因时序而丢失。
     * @param {string} windowTitle 贴图窗口标题
     * @param {number} x 物理像素 X 坐标
     * @param {number} y 物理像素 Y 坐标
     * @returns {boolean} 是否已立即命中并完成放置
     */
    PlaceWindowByTitle(windowTitle, x, y) {
        if (!windowTitle) {
            return false;
        }

        let metaWin = this._findPasterWindow(windowTitle);
        if (metaWin) {
            metaWin.make_above();
            if (Meta.WindowType && typeof metaWin.set_type === 'function') {
                metaWin.set_type(Meta.WindowType.UTILITY);
            }
            metaWin._snapinPlaced = true;
            this._moveWindowByPhysical(metaWin, x, y);
            return true;
        }

        this._prunePlacements();
        this._pendingPlacements.set(windowTitle, {
            x,
            y,
            at: GLib.get_monotonic_time(),
        });
        return false;
    }

    /**
     * 调用 Mutter 内部原生 Shell.Screenshot 实现零延迟、无快门白闪的全屏截屏。
     *
     * 结果直接写入调用方指定的文件流，像素不跨 GJS 边界传递，因此不存在
     * read_pixels 那类数组编解码断言风险。
     * @param {string} targetPath 存储目标 PNG 路径
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
