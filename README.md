<p align="center">
  <img src="assets/icons/snapin.svg" width="96" height="96" alt="Snapin">
</p>

<h1 align="center">Snapin</h1>

<p align="center">纯 Rust 打造的原生 Wayland 截图与贴图工具</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License: Apache-2.0"></a>
  <a href="LICENSES/LicenseRef-Slint-Royalty-free-2.0.md"><img src="https://img.shields.io/badge/Slint-Royalty--free%202.0-2379f4.svg" alt="Slint: Royalty-free 2.0"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-stable-dea584.svg" alt="Rust stable"></a>
  <a href="#环境要求"><img src="https://img.shields.io/badge/platform-Linux%20Wayland-2ea44f.svg" alt="Platform: Linux Wayland"></a>
  <a href="#环境要求"><img src="https://img.shields.io/badge/GNOME-45--51-4a86cf.svg" alt="GNOME 45-51"></a>
</p>

Snapin 是专为 Linux Wayland（推荐 Ubuntu 24.04+ GNOME）打造的纯 Rust 截图与贴图工具。
截图窗口使用 winit + tiny-skia 完成纯 CPU 软件光栅化，配合 GNOME Shell 扩展获得窗口置顶、
精确定位与零白闪原生截屏能力；贴图使用自建 ARGB8888 共享内存表面实现真透明。

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
- 矩形、椭圆、箭头、画笔、荧光笔、文字、序号、马赛克、局部模糊、聚光灯标注。
- 描边线型、填充、圆角、箭头样式、序号样式与调色板。
- 撤销 / 重做、复制、快捷保存、贴图置顶、OCR、原位翻译。
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

- 配套 GNOME Shell 扩展：窗口置顶、精确落位、原生截屏、顶栏托盘。
- 5 个可自定义全局快捷键。
- systemd 用户服务、开机自启、桌面通知。
- Slint 设置中心，保存后热重载守护进程配置。

## 环境要求

- Linux Wayland 会话，推荐 Ubuntu 24.04+ GNOME。
- GNOME Shell 45 至 51；其他桌面环境只能使用部分功能，详见[已知限制](#已知限制)。
- systemd 用户会话与 XDG Desktop Portal。
- OCR 需要系统安装 `libonnxruntime1.23`；deb 包已声明该依赖。若发行版仓库没有提供该包，
  需要先自行提供兼容版本，否则 `apt` 会因依赖不满足而拒绝安装。
- 不支持 X11 会话。

构建需要 Rust stable 与以下系统库：

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

生成并安装 Debian 包：

```bash
./scripts/build-deb.sh
sudo apt install ./target/snapin_*_amd64.deb
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
snapin pick-color
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

- 仅支持 Wayland；不支持 X11 会话。
- GNOME Shell 扩展是完整体验的前提。其他桌面环境只能使用 XDG Portal 截屏等部分能力，
  全局快捷键、托盘与窗口置顶可能不可用。
- 暂不支持跨显示器拼接截图或滚动截图。
- 截图默认不包含鼠标指针。
- 不提供云端上传、分享链接与历史图库。
- GNOME Shell 扩展的 JavaScript 无法热更新，升级扩展代码后必须注销并重新登录。
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

1. 更新 `Cargo.toml` 中的 `version`，并同步六个内部 path 依赖的版本号。
2. 若修改了扩展的 `extension.js` 或 D-Bus 接口，必须提升 `extension/metadata.json` 的 `version`。
3. 运行打包脚本并本地安装验证。
4. 打 `v*` 标签并推送，GitHub Actions 会构建 deb 并创建 Release。

## 项目结构

- `snapin`：CLI 入口、守护进程调度与截图 / 贴图流程编排。
- `snapin-core`：配置模型、标注图元、文本排版与路径约定。
- `snapin-ui`：winit 截图窗口与集中贴图工作区、工具栏、渲染与交互。
- `snapin-platform`：Portal、GNOME 扩展 D-Bus、剪贴板、热键、通知、存储与自启动。
- `snapin-service`：PP-OCRv6 推理与多服务商翻译。
- `snapin-settings`：Slint 设置中心及配置双向同步。
- `snapin-daemon`：本地 IPC 协议、服务端与客户端。
- `extension`：配套 GNOME Shell 扩展。

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
