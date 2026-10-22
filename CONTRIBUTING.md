# 贡献指南

感谢你愿意参与 Snapin。本文说明如何报告问题、搭建开发环境、提交修改，以及合并前需要通过的检查。

## 报告问题

- 功能缺陷与改进建议请通过 [GitHub Issues](https://github.com/mangerle/snapin/issues) 提交，
  并尽量使用仓库提供的 Issue 模板。
- 安全问题请不要公开提交，按 [SECURITY.md](SECURITY.md) 中的私密渠道报告。
- 提交问题前请先搜索是否已有相同主题，避免重复。

一个可复现的问题报告通常包含：

- Snapin 版本：`snapin --version`。
- 桌面环境、GNOME Shell 版本与会话类型：`echo $XDG_SESSION_TYPE`。
- 复现步骤、预期结果与实际结果。
- 相关日志：`journalctl --user -u snapin -b --no-pager | tail -n 100`。
- 涉及界面时，附上截图或录屏会更容易定位。

日志与配置可能包含翻译服务密钥、剪贴板内容或截图路径，提交前请先脱敏。

## 开发环境

需要 Linux Wayland、Rust stable 与以下系统库：

```bash
sudo apt-get install -y \
  libwayland-dev \
  libxkbcommon-dev \
  libfontconfig1-dev \
  libfreetype6-dev \
  libdbus-1-dev \
  pkg-config
```

OCR 功能需要系统提供 `libonnxruntime1.23`，其余功能不依赖 OCR 运行时。

```bash
git clone https://github.com/mangerle/snapin.git
cd snapin
cargo build
```

开发时可先停止已安装的守护进程，再在前台运行：

```bash
systemctl --user stop snapin
cargo run -p snapin -- daemon
```

设置中心：

```bash
cargo run -p snapin -- prefs
```

## 质量门禁

向仓库提交代码前，必须在工作区根目录依次执行并通过：

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo check --all-targets
cargo test
```

要求零 Clippy 警告；不要通过添加 `#[allow(...)]` 绕过检查。新增或修改核心逻辑时，
请补充对应的单元测试，覆盖正常路径、边界值与错误分支。

## 编码约定

- 代码注释、日志与用户可见文案使用中文；禁止使用 Emoji。
- 生产代码中禁止裸 `unwrap()` / `expect()`；错误应通过 `?`、模式匹配或安全降级处理。
- 日志统一使用 `log` / `tracing` 门面，禁止提交 `println!`、`eprintln!`、`dbg!`。
- 并发场景使用有界通道；禁止跨 `.await` 持有同步锁。
- 优先使用 `use` 引入类型，避免不必要的全限定路径。
- 修改业务逻辑、字段或函数签名时，同步更新相关注释与文档。
- 详细规范见仓库根目录的 [AGENTS.md](AGENTS.md)。

## 提交流程

1. Fork 仓库并从 `master` 创建功能分支。
2. 每次提交只解决一个问题，避免把无关重构、格式化或文档修改混在同一提交中。
3. 提交信息遵循 Conventional Commits，例如：

   ```text
   fix(snipper): 修复高分屏下选区偏移
   feat(paster): 新增贴图分组快捷键
   docs(readme): 补充 OCR 模型路径说明
   ```

4. 提交前运行完整质量门禁，确认没有引入回归。
5. 发起 Pull Request，说明变更动机、实现方式、验证步骤与影响范围。

界面改动请在 PR 中附上改动前后的截图或录屏。涉及 GNOME Shell 扩展时，请注意：

- 修改 `extension/extension.js` 或 D-Bus 接口后，必须提升 `extension/metadata.json` 的 `version`。
- 扩展 JavaScript 无法热更新，验证前需要注销并重新登录。
- 未重新登录时截图会降级到 XDG Portal，这是预期行为，不是新引入的缺陷。

涉及打包脚本的改动，请至少执行一次 `./scripts/build-deb.sh`，并检查生成的 deb 内容：

```bash
dpkg-deb -c target/snapin_*_amd64.deb
```

## 许可证

Snapin 自有源代码以 Apache License 2.0 发布。提交 Pull Request 即表示你同意将贡献内容
按同一许可授权给本项目。

设置中心使用 Slint，依据 Slint Royalty-free 2.0 使用。贡献时请勿移除 `AboutSlint` 归属组件、
`LICENSE`、`NOTICE`、`THIRD_PARTY_NOTICES.md` 或 `LICENSES/` 下的许可文本。引入新的第三方依赖前，
请确认其许可与项目兼容，并在 `THIRD_PARTY_NOTICES.md` 中补充声明。

## 社区

参与本项目即表示你同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。
