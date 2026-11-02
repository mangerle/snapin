import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import GLib from 'gi://GLib';

// 定义 D-Bus 接口：
//   1. PlaceWindowByTitle —— Wayland 禁止客户端自行摆放或提级窗口，贴图窗口的绝对定位与
//      置顶必须由拥有特权的 Mutter 扩展代为执行。
//   2. CaptureFullscreen   —— 用合成器内部对象直接把整屏写进给定文件流，无快门白闪。
//   3. ListWindows         —— 枚举 Mutter 当前所有可见窗口几何，为快速吸附提供依据。
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
    <method name="ListWindows">
      <arg type="s" direction="out" name="windowsJson"/>
    </method>
  </interface>
</node>`;

// 延迟放置请求的最长有效期：贴图标题含唯一 ID 不会复用，超时仅用于兜底回收异常未映射的请求
const PLACEMENT_TTL_US = 5 * 1000 * 1000;

// 窗口映射完成前 Mutter 会静默丢弃 move_frame，故在首次放置后按这些延迟（毫秒）重放。
// 取值覆盖 Wayland 客户端提交首帧的常见耗时，过短会放空，过长会与用户拖动产生明显争抢。
const PLACEMENT_REPLAY_DELAYS_MS = [60, 180];

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

        // 2. 监听全局窗口创建事件：贴图窗口一诞生立即强制置顶，并设为 UTILITY 类型从 Dock 隐藏
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
        let scale = this._windowScale(metaWin);
        this._replayMove(metaWin, Math.round(x / scale), Math.round(y / scale));
    }

    /**
     * 按逻辑坐标移动窗口，并在窗口完成映射后重放同一落点。
     *
     * 设计原理：window-created 触发时 Wayland 客户端往往尚未提交首帧，此时调用
     * move_frame 会被 Mutter 静默丢弃（实测窗口停留在合成器默认位置）；只依赖
     * notify::title 也不行，因为标题通常早于创建信号写入，补挂监听不会再触发。
     * 因此在首次尝试后按固定延迟重放已解析好的绝对坐标。
     *
     * 重放复用同一坐标而不重新采样鼠标位置，避免多次取值造成窗口漂移；重放次数固定，
     * 不做无限重试，以免窗口销毁后残留定时器或与用户拖动长期争抢位置。
     * @param {Meta.Window} metaWin 目标窗口
     * @param {number} x 逻辑坐标 X
     * @param {number} y 逻辑坐标 Y
     */
    _replayMove(metaWin, x, y) {
        if (typeof metaWin.move_frame !== 'function') return;

        const move = () => {
            try {
                metaWin.move_frame(true, x, y);
            } catch (e) {
                console.error('Snapin 贴图窗口定位失败:', e);
            }
        };

        move();
        for (const delay of PLACEMENT_REPLAY_DELAYS_MS) {
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
                move();
                return GLib.SOURCE_REMOVE;
            });
        }
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
        this._replayMove(metaWin, targetX, targetY);
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

    /**
     * 枚举当前活动工作区可见窗口的几何矩形。
     *
     * 坐标使用逻辑像素并附带所在显示器缩放比，由 Rust 侧换算为物理像素；
     * 返回 JSON 字符串而不是复杂 GJS 数组，避免跨 D-Bus 边界编解码风险。
     * 顺序为窗口堆叠顺序（底层到顶层），调用方反向查找顶层窗口。
     * @returns {string} 窗口 JSON 数组
     */
    ListWindows() {
        try {
            const activeWorkspace = global.workspace_manager.get_active_workspace();
            const allowedTypes = new Set([
                Meta.WindowType.NORMAL,
                Meta.WindowType.DIALOG,
                Meta.WindowType.MODAL_DIALOG,
                Meta.WindowType.UTILITY,
            ]);
            const windows = [];
            for (const actor of global.get_window_actors()) {
                const win = actor.meta_window;
                if (!win || win.minimized) {
                    continue;
                }
                if (!allowedTypes.has(win.get_window_type())) {
                    continue;
                }
                if (!win.located_on_workspace(activeWorkspace) && !win.is_on_all_workspaces()) {
                    continue;
                }
                const title = win.get_title() || '';
                const wmClass = (win.get_wm_class() || '').toLowerCase();
                if (title === 'Snapin' || wmClass.includes('snapin')) {
                    continue;
                }
                const rect = win.get_frame_rect();
                if (!rect || rect.width <= 0 || rect.height <= 0) {
                    continue;
                }
                const monitor = win.get_monitor();
                const scale = monitor >= 0 ? global.display.get_monitor_scale(monitor) : 1;
                windows.push({
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    scale,
                });
            }
            return JSON.stringify(windows);
        } catch (e) {
            console.error('Snapin 枚举窗口失败:', e);
            return '[]';
        }
    }
}
