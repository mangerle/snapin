<p align="center">
  <img src="assets/icons/snapin.svg" width="96" height="96" alt="Snapin">
</p>

<h1 align="center">Snapin</h1>

<p align="center">纯 Rust 打造的跨平台截图与贴图工具（Linux / Windows / macOS）</p>

<p align="center"><a href="README.en.md">English</a></p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License: Apache-2.0"></a>
  <a href="LICENSES/LicenseRef-Slint-Royalty-free-2.0.md"><img src="https://img.shields.io/badge/Slint-Royalty--free%202.0-2379f4.svg" alt="Slint: Royalty-free 2.0"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.96%2B-dea584.svg" alt="Rust 1.96+"></a>
  <a href="#环境要求"><img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-2ea44f.svg" alt="Platform: Linux / Windows / macOS"></a>
  <a href="#环境要求"><img src="https://img.shields.io/badge/Linux-Wayland%20%7C%20GNOME%2045--51-4a86cf.svg" alt="Linux: Wayland / GNOME 45-51"></a>
</p>

Snapin 是纯 Rust 打造的跨平台截图与贴图工具，支持 Linux、Windows 与 macOS。
截图窗口使用 winit + tiny-skia 完成纯 CPU 软件光栅化；Linux 上配合 GNOME Shell 扩展
获得窗口置顶、精确定位与零白闪原生截屏能力，Windows / macOS 通过 xcap 实现截屏；
贴图在 Linux Wayland 下使用自建 ARGB8888 共享内存表面实现真透明，其他平台回退到 softbuffer 通路。

## 目录

- [功能](#功能)
- [环境要求](#环境要求)
- [安装](#安装)
- [使用](#使用)
- [命令行](#命令行)
- [配置与数据目录](#配置与数据目录)
- [服务与日志](#服务与日志)
- [常见问题](#常见问题)
- [已知限制](#已知限制)
- [从源码构建](#从源码构建)
- [项目结构](#项目结构)
- [参与贡献](#参与贡献)
- [许可证](#许可证)

## 功能

### 截图

- 区域框选、8 锚点缩放、选区平移与历史选区回溯。
- 配套扩展零白闪截屏；扩展不可用时自动降级到 XDG Desktop Portal。
- 鼠标跟随像素放大镜、网格线与 HEX / RGB / HSL 取色。
- 窗口截图：光标悬停高亮窗口，短按直接吸附窗口边界，拖动仍可自由框选（需配套扩展在线）。
- 固定比例与固定尺寸选区、Tab 窗口循环、方向键像素微调；Linux GNOME 扩展通道可包含鼠标指针。
- 矩形、椭圆、直线/折线、箭头、画笔、荧光笔、橡皮擦、文字、序号、马赛克/模糊/智能打码、聚光灯与区域放大镜标注。
- 数值尺寸采用“单圆点 + 数字”并支持滚轮微调；提供描边线型、填充、最高 90 像素圆角、起止箭头、荧光笔线条/矩形与混合模式下拉、序号注释字体/描边/导向线/箭头、文字背景不透明度/圆角/内边距、马赛克/模糊/智能马赛克、画笔/矩形与强度滑块、聚光灯形状/遮罩强度/描边、放大镜倍率/平滑/连接线与调色板。
- 撤销 / 重做、复制、快捷保存、贴图置顶、OCR、原位翻译。
- 智能打码：OCR 识别选区全部文字，预览后选择马赛克或模糊并一次性应用。
- 二维码识别：自动发现选区中的二维码，复制内容，按 `Ctrl+Enter` 打开安全网页链接。
- 文字识别：左键识别后直接复制全文并关闭截图；右键可选择“复制文字”或“编辑文字”，编辑模式使用包含完整识别文本的多行编辑框。
- 美化导出：在设置中心配置圆角、阴影、渐变背景、水印、原始尺寸和社交平台尺寸，复制、保存与贴图输出统一应用。
- 标注二次编辑：新画完的标注自动选中，可整体拖动或用包围盒手柄拉伸；旧标注默认锁定，按住 Alt 点击可重新解锁编辑。
- 工具栏按钮悬浮提示：在按钮下方固定显示工具名与快捷键；贴图窗口没有截图侧的键位，只显示名称。
- `snip --auto-copy` 框选后自动复制；`snip --translate` 框选后自动翻译。
- `snip --fullscreen` 直接锁定整屏选区，保留标注工具栏，可继续复制、保存、贴图、OCR 或翻译。
- `snip --delay N` 在捕获前等待 N 秒（0 到 60），方便截取菜单或悬浮提示。
- `snapin pick-color` 打开全屏取色器，按设置的 HEX/RGB/HSL 格式复制光标颜色；单击复制，Esc 取消。

### 贴图

- 从剪贴板图像、文本或颜色值生成置顶贴图，也可从截图选区生成。
- 集中多窗口工作区、4 个贴图分组、批量显隐。
- 滚轮无级缩放、旋转、翻转、不透明度调节、窗口阴影与平滑缩放开关。
- 真透明 ARGB8888 共享内存呈现通路。
- 贴图内标注、IME 文字输入、撤销 / 重做。
- 整图 OCR 与整图原位翻译。
- 复制、快捷保存、系统保存对话框另存为、在文件夹中查看。
- 鼠标穿透：图像区域点击直接落到下层窗口，工具条保留为恢复交互的手柄。
- 双击行为、右键菜单与可隐藏工具条。

### OCR 与翻译

- 内置 PP-OCRv6 small ONNX 模型，离线识别，不上传图像。
- 翻译服务商：谷歌免费接口、微软必应免费接口、百度、网易有道、阿里云。
- 目标语言：中文、英语、日语、韩语、法语、德语、西班牙语、俄语。
- 原位翻译支持智能取色填充与磨砂模糊两种背景覆盖模式。

使用翻译功能时，识别出的文本会按所选服务商发送到对应接口；OCR 本身完全在本地完成。

### 系统集成

- **[Linux]** 配套 GNOME Shell 扩展：窗口置顶、精确落位、原生截屏、顶栏托盘。
- 10 个可自定义全局快捷键（Linux 同步到 GNOME，Windows / macOS 通过原生热键注册）。
- 配置导入导出；导入执行严格 schema 与未知字段校验，导出包含翻译凭证且不额外加密。
- **[Linux]** systemd 用户服务、开机自启、桌面通知。
- Slint 设置中心，保存后热重载守护进程配置。

## 环境要求

### Linux（完整功能）

- Wayland 会话，推荐 Ubuntu 24.04+ GNOME。
- GNOME Shell 45 至 51（其他桌面环境可用 XDG Portal 截屏，但全局快捷键、托盘与窗口置顶可能不可用）。
- systemd 用户会话与 XDG Desktop Portal。
- OCR 需要系统安装 `libonnxruntime1.23`；deb 包已声明该依赖。
- X11 会话未做适配，部分功能（如贴图真透明、窗口置顶）无法正常工作，不保证可用。

### Windows

- Windows 10 及以上。
- 以管理员权限运行或在系统"键盘快捷键"中授权，方可注册全局热键。

### macOS

- macOS 系统需在"系统设置 → 隐私与安全性 → 屏幕录制"中为 Snapin 授权，截屏功能才可正常使用。

构建需要 Rust 1.96 或更高版本；Linux 另需以下系统库：

```bash
sudo apt-get install -y \
  libwayland-dev \
  libxkbcommon-dev \
  libfontconfig1-dev \
  libfreetype6-dev \
  libdbus-1-dev \
  pkg-config
```

## 安装

### Debian 包（推荐）

从 [Releases](https://github.com/mangerle/snapin/releases) 下载最新的 `snapin_*_amd64.deb`，
使用 `apt` 安装以自动解析运行时依赖：

```bash
sudo apt install ./snapin_*_amd64.deb
```

安装完成后请注销并重新登录一次。GNOME Shell 会缓存扩展模块，首次安装或升级扩展代码后，
必须重新登录才能加载新版本；未重新登录前截图会自动降级到 XDG Portal 通道（可用，但带快门闪屏）。

检查安装状态：

```bash
snapin --version
systemctl --user status snapin
gnome-extensions info snapin@wangle.local
```

打开设置中心：

```bash
snapin prefs
```

### 从源码构建

```bash
git clone https://github.com/mangerle/snapin.git
cd snapin
cargo build --release
```

### 本地安装与跨平台打包

Snapin 采用 Tauri 官方打包核心 `cargo-packager` 实现跨平台零脚本声明式打包。

首先安装打包工具：

```bash
cargo install cargo-packager --locked
```

打包前需确保已编译最新的 Release 版本二进制文件：

- **Linux (Debian / Ubuntu)**：
  ```bash
  # 一键编译并打包 DEB
  cargo build --release -p snapin && cargo packager --release -p snapin --formats deb
  sudo apt install ./target/release/snapin_*_amd64.deb
  ```

- **Windows (向导安装包与便携包)**：
  ```bash
  # 编译并生成 NSIS 中文向导安装包 (.exe) 与免安装绿色版 (.zip)
  cargo build --release -p snapin
  cargo packager --release -p snapin --formats nsis,zip
  ```

- **macOS (苹果 DMG 镜像包)**：
  ```bash
  # 编译并生成标准拖拽式 DMG 镜像 (.dmg)
  cargo build --release -p snapin
  cargo packager --release -p snapin --formats dmg
  ```

开发环境也可单独部署 GNOME Shell 扩展：

```bash
snapin install-extension
# 或
./extension/install.sh
```

若只运行 `cargo build --release` 而不打包，需要自行部署扩展、systemd 用户服务与运行时依赖。
开发调试时可先停止已安装的守护进程，再在前台运行：

```bash
systemctl --user stop snapin
cargo run -p snapin -- daemon
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
| 智能打码 | `Ctrl+Alt+R` |
| 二维码识别 | `Ctrl+Alt+Q` |
| 美化导出 | `Ctrl+Alt+B` |
| 贴图工作区 | `Ctrl+Alt+W` |
| 剪贴板历史 | `Ctrl+Shift+V` |

截图工具栏常用操作：

| 操作 | 按键 / 动作 |
| --- | --- |
| 复制并退出 | `Ctrl+C` |
| 保存并退出 | `Ctrl+S` |
| 贴图置顶 | `Enter` 或 `F3` |
| 文字识别 | `O` |
| 编辑识别文字 | `Shift+O` |
| 智能打码 | `R` |
| 二维码识别 | `Q`（位于文字工具之后） |
| 切换固定比例 / 尺寸 | `1` 到 `6` |
| 循环窗口候选 | `Tab` / `Shift+Tab` |
| 微调选区位置 | 方向键 / `Shift` + 方向键 |
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
snapin snip [--auto-copy] [--custom] [--translate] [--fullscreen] [--redact] [--qr] [--beautify] [--ocr-panel] [--delay SECONDS]
snapin pick-color
snapin paste
snapin toggle
snapin switch-group [ID]
snapin workspace
snapin clipboard-history
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
- OCR 模型：系统只读路径为 `/usr/share/snapin/models/ppocrv6`；用户可将覆盖模型放到
  `~/.local/share/snapin/models/ppocrv6`，或在设置页 / `ocr.local.model_dir` 指定目录。

配置文件可能包含百度、有道、阿里云等翻译服务的密钥，请勿直接提交到公开仓库或粘贴到 Issue 中。

## 服务与日志

```bash
systemctl --user status snapin
systemctl --user restart snapin
journalctl --user -u snapin -f
```

常用维护命令：

```bash
snapin sync-hotkeys    # 重新同步 GNOME 全局快捷键
snapin exit            # 退出当前守护进程
```

`snapin snip`、`snapin pick-color`、`snapin paste` 与 `snapin pin-file` 在守护进程未运行时会自动派生守护进程并等待 IPC 就绪；所有截图与贴图窗口始终由守护进程主线程的同一事件循环托管。设置中心作为独立进程启动，不会在守护进程内创建第二个 GUI 事件循环。

## 常见问题

### 快捷键没有反应

1. 确认守护进程在运行：`systemctl --user status snapin`。
2. 重新同步快捷键：`snapin sync-hotkeys`。
3. 检查是否与其他应用的快捷键冲突：GNOME 设置 → 键盘 → 键盘快捷键。
4. 查看日志：`journalctl --user -u snapin -b --no-pager | tail -n 100`。

### 截图出现快门闪屏，或窗口无法置顶

这通常表示 GNOME Shell 扩展没有加载，截图走了 XDG Portal 降级通道。安装或升级扩展后必须
注销并重新登录一次，然后检查：

```bash
gnome-extensions info snapin@wangle.local
gnome-extensions enable snapin@wangle.local
```

### OCR 提示模型缺失或无法初始化

1. 确认 deb 包已安装模型：`ls /usr/share/snapin/models/ppocrv6`。
2. 确认系统存在 ONNX Runtime：`ldconfig -p | grep onnxruntime`。
3. 如使用自定义模型，在设置页或 `config.toml` 的 `ocr.local.model_dir` 指定目录。

### 翻译失败

- 免费翻译接口可能因网络、频率限制或上游策略变化而临时不可用，可切换服务商后重试。
- 百度、有道、阿里云需要先在设置页填写对应凭证。
- 详细错误会写入 `journalctl --user -u snapin`，可据此判断是网络、鉴权还是响应解析问题。

### 开机自启没有生效

Snapin 优先使用 systemd 用户服务。检查：

```bash
systemctl --user is-enabled snapin
systemctl --user status snapin
```

### 卸载后扩展目录仍然存在

deb 在安装时会把扩展同步到当前用户的
`~/.local/share/gnome-shell/extensions/snapin@wangle.local`。卸载后如仍残留，可手动删除该目录
并注销重新登录。

## 已知限制

- **[Linux]** Wayland 会话为完整体验的前提；X11 未做适配，不保证可用。
- **[Linux]** GNOME Shell 扩展是完整体验的前提。其他桌面环境只能使用 XDG Portal 截屏等部分能力，
  全局快捷键、托盘与窗口置顶可能不可用。
- 暂不支持跨显示器拼接截图或滚动截图。
- 截图默认不包含鼠标指针；Linux GNOME 扩展通道可原生包含，Windows / macOS 当前截屏通道仍不包含。
- 不提供云端上传、分享链接与历史图库。
- **[Linux]** GNOME Shell 扩展的 JavaScript 无法热更新，升级扩展代码后必须注销并重新登录。
- OCR 使用 PP-OCRv6 small 模型，超小字号或复杂排版可能需要用户替换更大的模型。

## 从源码构建

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo check --all-targets
cargo test
```

打包脚本会读取 `Cargo.toml` 中的版本号，将可执行文件、PP-OCRv6 模型、GNOME Shell 扩展、
systemd 用户服务与第三方许可文件一并装入 deb 包。

发布新版本时：

1. 更新 `Cargo.toml` 中的 `version`，并同步 workspace 内全部内部 path 依赖的版本号。
2. 若修改了扩展的 `extension.js` 或 D-Bus 接口，必须提升 `extension/metadata.json` 的 `version`。
3. 运行打包脚本并本地安装验证。
4. 打 `v*` 标签并推送，GitHub Actions 会构建 deb 并创建 Release。

## 项目结构

- `snapin`：CLI 解析与进程装配，不承载业务编排。
- `snapin-app`：应用层用例编排、媒体任务执行器、守护进程运行时与系统集成调度。
- `snapin-runtime`：统一后台任务运行时，提供有界准入、任务内阻塞子任务入口、任务追踪、取消令牌与 panic 感知。
- `snapin-updater`：自更新清单检查、下载验签与崩溃回滚，隔离网络与 `shipup` 依赖。
- `snapin-storage`：图像编码保存、文件名模板、选区缓存与原生像素载荷校验。
- `snapin-foundation`：错误、路径、几何、国际化、触发模式，以及美化与翻译共享枚举等无重型 UI 依赖的基础契约。
- `snapin-config`：强类型配置模型、迁移、语义校验与安全持久化。
- `snapin-render`：文本测量、排版、光栅化与美化导出渲染。
- `snapin-annotation`：标注图元、撤销重做历史与 Tiny-Skia 绘制实现。
- `snapin-ui`：纯表现层，提供 winit 截图窗口、贴图工作区、工具栏、渲染与交互；通过 `ports` 请求宿主能力，不直接读写系统剪贴板、配置文件或网络服务。
- `snapin-platform`：Portal、GNOME 扩展 D-Bus、剪贴板、热键、通知、窗口与自启动等桌面集成。
- `snapin-service`：PP-OCRv6 推理与多服务商翻译。
- `snapin-settings`：Slint 设置中心及配置双向同步。
- `snapin-daemon`：本地 IPC 协议、服务端与客户端。
- `extension`：配套 GNOME Shell 扩展。

核心依赖方向为 `snapin-foundation` ← `snapin-config`，`snapin-render` 与 `snapin-annotation` 只依赖基础契约，`snapin-annotation` 再依赖渲染层；渲染与标注不再反向依赖配置 schema，美化参数到配置的映射留在配置 crate。`snapin-ui` 只直接依赖这些核心契约，不依赖平台、存储或服务 crate。`snapin-app` 负责把 UI 端口与基础设施实现装配起来，`snapin-platform`、`snapin-storage`、`snapin-daemon` 只依赖所需的轻量核心 crate，不再传递引入 Tiny-Skia 或 Cosmic-Text。

GUI 生命周期统一由常驻守护进程主线程的单一 winit 事件循环托管，贴图窗口与截图覆盖层共存于同一循环；托盘与全局热键通过事件回调唤醒主循环，不再定时轮询。命令行进程只负责 IPC，若守护进程未运行会自动派生并等待其就绪；设置中心作为独立 Slint 进程运行，不会在守护进程内创建第二个事件循环。

## 参与贡献

欢迎提交 Issue 与 Pull Request。开始之前请阅读：

- [CONTRIBUTING.md](CONTRIBUTING.md)：开发环境、质量门禁与提交流程。
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)：社区行为准则。
- [SECURITY.md](SECURITY.md)：安全漏洞的私密报告渠道。

提交代码前请确保 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、
`cargo check --all-targets` 与 `cargo test` 全部通过。

## 许可证

- 项目自有源代码：[Apache License 2.0](LICENSE)。
- 设置中心使用 [Slint](https://slint.dev)，依据
  [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](LICENSES/LicenseRef-Slint-Royalty-free-2.0.md)
  使用；设置中心的“关于”页保留 `AboutSlint` 归属标识。
- PP-OCRv6 模型、ONNX Runtime、Rust 依赖与图标等第三方组件声明见
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)；Slint 归属说明见 [NOTICE](NOTICE)。

Copyright 2026 wangle
