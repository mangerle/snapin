# Snapin

Snapin 是专为 Linux Wayland（Ubuntu 26 GNOME）打造的纯 Rust 截图与贴图工具。它使用 winit + tiny-skia 完成纯 CPU 软件光栅化，依赖配套 GNOME Shell 扩展获得窗口置顶、精确定位与零白闪原生截屏能力。

## 功能

### 截图

- 区域框选、8 锚点缩放、选区平移与历史选区回溯。
- 配套扩展零白闪截屏；扩展不可用时自动降级到 XDG Desktop Portal。
- 鼠标跟随像素放大镜、网格线与 HEX / RGB / HSL 取色。
- 矩形、椭圆、箭头、画笔、荧光笔、文字、序号、马赛克标注。
- 描边线型、填充、圆角、箭头样式、序号样式与调色板。
- 撤销 / 重做、复制、快捷保存、贴图置顶、OCR、原位翻译。
- `snip --auto-copy` 框选后自动复制；`snip --translate` 框选后自动翻译。
- `snip --fullscreen` 直接锁定整屏选区，保留标注工具栏，可继续复制、保存、贴图、OCR 或翻译。
- `snip --delay N` 在捕获前等待 N 秒（0 到 60），方便截取菜单或悬浮提示。

### 贴图

- 从剪贴板图像、文本或颜色值生成置顶贴图，也可从截图选区生成。
- 集中多窗口工作区、4 个贴图分组、批量显隐。
- 滚轮无级缩放、旋转、翻转、不透明度调节、窗口阴影与平滑缩放开关。
- 真透明 ARGB8888 共享内存呈现通路。
- 贴图内标注、IME 文字输入、撤销 / 重做。
- 整图 OCR 与整图原位翻译。
- 复制、快捷保存、系统保存对话框另存为、在文件夹中查看。
- 双击行为、右键菜单与可隐藏工具条。

### OCR 与翻译

- 内置 PP-OCRv6 small ONNX 模型，离线识别，不上传图像。
- 翻译服务商：谷歌免费接口、微软必应免费接口、百度、网易有道、阿里云。
- 目标语言：中文、英语、日语、韩语、法语、德语、西班牙语、俄语。
- 原位翻译支持智能取色填充与磨砂模糊两种背景覆盖模式。

### 系统集成

- 配套 GNOME Shell 扩展：窗口置顶、精确落位、原生截屏、顶栏托盘。
- 5 个可自定义全局快捷键。
- systemd 用户服务、开机自启、桌面通知。
- Slint 设置中心，保存后热重载守护进程配置。

## 环境要求

- Linux Wayland 会话，推荐 Ubuntu 26 GNOME（扩展声明支持 GNOME 45 至 51）。
- systemd 用户会话与 XDG Desktop Portal。
- OCR 需要系统安装 `libonnxruntime1.23`；deb 包已声明该依赖。
- 构建需要 Rust stable 与以下系统库：

```bash
sudo apt-get install -y \
  libwayland-dev \
  libxkbcommon-dev \
  libfontconfig1-dev \
  libfreetype6-dev \
  libdbus-1-dev \
  pkg-config
```

## 构建

```bash
cargo build --release
```

生成 Debian 安装包：

```bash
./scripts/build-deb.sh
sudo dpkg -i target/snapin_*.deb
```

deb 包会安装可执行文件、PP-OCRv6 模型、GNOME 扩展与 systemd 用户服务。扩展 JavaScript 无法热更新，首次安装或升级扩展代码后需要注销并重新登录一次；未重登前截图会自动降级到 Portal 通道。

开发环境也可单独部署扩展：

```bash
snapin install-extension
# 或
./extension/install.sh
```

## 使用

默认快捷键：

| 功能 | 快捷键 |
| --- | --- |
| 区域截图 | `Ctrl+Alt+A` |
| 截图后自动翻译 | `Ctrl+Alt+D` |
| 贴图 | `F1` |
| 批量隐藏 / 显示贴图 | `Ctrl+F3` |
| 切换贴图分组 | `Shift+F3` |

截图工具栏常用操作：

| 操作 | 按键 / 动作 |
| --- | --- |
| 复制并退出 | `Ctrl+C` |
| 保存并退出 | `Ctrl+S` |
| 贴图置顶 | `Enter` 或 `F3` |
| 文字识别 | `O` |
| 原位翻译 | `T` |
| 撤销 / 重做 | `Ctrl+Z` / `Ctrl+Y` |
| 上一个 / 下一个历史选区 | `,` / `.` |
| 取消截图 | `Esc` |

贴图窗口常用操作：

| 操作 | 按键 |
| --- | --- |
| 复制图像 | `C` |
| 快捷保存 | `S` |
| 顺时针 / 逆时针旋转 | `3` / `4` |
| 水平 / 垂直翻转 | `H` / `V` |
| 降低 / 提高不透明度 | `1` / `2` |
| 按设置的缩放锚点缩放 | 鼠标滚轮 |
| 关闭贴图 | `Esc`（双击行为见设置页） |

## 命令行

```text
snapin snip [--auto-copy] [--custom] [--translate] [--fullscreen] [--delay SECONDS]
snapin paste
snapin toggle
snapin switch-group [ID]
snapin clear-history
snapin prefs
snapin daemon
snapin exit
snapin install-extension
snapin sync-hotkeys
snapin pin-file <PATH> [X] [Y]
```

## 配置与数据目录

- 配置：`~/.config/snapin/config.toml`（权限 0600，包含翻译 API 凭证）。
- 缓存：`~/.cache/snapin/`。
- 运行时套接字与临时快照：`$XDG_RUNTIME_DIR/snapin/`。
- OCR 模型：系统只读路径为 `/usr/share/snapin/models/ppocrv6`；用户可将覆盖模型放到 `~/.local/share/snapin/models/ppocrv6`，或在设置页 / `ocr.local.model_dir` 指定目录。

## 服务与日志

```bash
systemctl --user status snapin
systemctl --user restart snapin
journalctl --user -u snapin -f
```

## 工作区结构

- `snapin`：CLI 入口、守护进程调度与截图 / 贴图流程编排。
- `snapin-core`：配置模型、标注图元、文本排版与路径约定。
- `snapin-ui`：winit 截图窗口与集中贴图工作区、工具栏、渲染与交互。
- `snapin-platform`：Portal、GNOME 扩展 D-Bus、剪贴板、热键、通知、存储与自启动。
- `snapin-service`：PP-OCRv6 推理与多服务商翻译。
- `snapin-settings`：Slint 设置中心及配置双向同步。
- `snapin-daemon`：本地 IPC 协议、服务端与客户端。

## 许可证

Apache License 2.0，详见 [LICENSE](LICENSE)。

Copyright 2026 wangle
