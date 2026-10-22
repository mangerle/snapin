## 变更说明

说明这次改动解决了什么问题、为什么采用当前方案。

## 关联 Issue

Closes #

## 变更类型

- [ ] 缺陷修复
- [ ] 新功能
- [ ] 重构或性能优化
- [ ] 文档
- [ ] 构建、打包或 CI
- [ ] 其他

## 验证方式

说明你如何验证改动，包括关键操作路径、边界条件与错误分支。

## 检查清单

- [ ] 一次提交只解决一个问题，提交信息遵循 Conventional Commits
- [ ] `cargo fmt --all` 通过
- [ ] `cargo clippy --all-targets -- -D warnings` 零警告
- [ ] `cargo check --all-targets` 通过
- [ ] `cargo test` 全部通过
- [ ] 新增或修改的核心逻辑包含单元测试
- [ ] 相关注释、README 或第三方许可声明已同步更新
- [ ] 未提交密钥、令牌、用户配置或含敏感内容的截图
- [ ] 如修改了 GNOME Shell 扩展，已提升 `extension/metadata.json` 的 `version`
- [ ] 如修改了打包脚本，已执行 `./scripts/build-deb.sh` 并检查 deb 内容
- [ ] 界面改动已附改动前后的截图或录屏（如适用）
