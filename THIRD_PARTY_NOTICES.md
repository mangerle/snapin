# 第三方组件与许可声明

Snapin 的自有源代码以 [Apache License 2.0](LICENSE) 发布。本文件列出随项目分发、
或运行时所依赖的主要第三方组件及其许可信息。

分发本项目的源码或二进制包时，请保留本文件、`LICENSE`、`NOTICE` 与 `LICENSES/` 目录。

## Slint

- 组件：Slint 1.x 与 slint-build（设置中心图形界面及构建期代码生成）
- 版权：Copyright © SixtyFPS GmbH
- 上游：<https://slint.dev> / <https://github.com/slint-ui/slint>
- 上游许可：`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`
- 本项目采用：**Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0**
- 合规方式：设置中心“关于”页展示 `AboutSlint` 归属组件
- 许可全文：[LICENSES/LicenseRef-Slint-Royalty-free-2.0.md](LICENSES/LicenseRef-Slint-Royalty-free-2.0.md)

本项目将 Slint 作为设置中心的一部分使用，不单独分发 Slint 库文件，不将其用于嵌入式系统，
也不对外暴露 Slint 的 API。

## PP-OCRv6 离线识别模型

- 文件：`assets/models/ppocrv6/`（ONNX 模型、推理配置与字典）
- 来源：PaddleOCR（PaddlePaddle）
- 版权：Copyright PaddlePaddle Authors
- 许可：Apache License 2.0
- 上游：<https://github.com/PaddlePaddle/PaddleOCR>

模型文件仅用于本地文字检测与识别，不会上传用户截图。

## ONNX Runtime

- 组件：系统提供的 `libonnxruntime1.23`（deb 包运行时依赖，不随仓库捆绑）
- 版权：Copyright Microsoft Corporation
- 许可：MIT License
- 上游：<https://github.com/microsoft/onnxruntime>

## Rust 依赖

项目的主要直接依赖如下。完整依赖树（含间接依赖）以 `Cargo.lock` 为准；
各 crate 的许可全文随其源码包提供，可从 crates.io 获取对应版本。

| 组件 | 许可 |
| --- | --- |
| winit | Apache-2.0 |
| softbuffer | MIT OR Apache-2.0 |
| tiny-skia | BSD-3-Clause |
| image | MIT OR Apache-2.0 |
| cosmic-text | MIT OR Apache-2.0 |
| zbus | MIT |
| ashpd | MIT |
| reqwest | MIT OR Apache-2.0 |
| tokio | MIT |
| serde / serde_json / toml | MIT OR Apache-2.0 |
| clap | MIT OR Apache-2.0 |
| ort | MIT OR Apache-2.0 |
| arboard | MIT OR Apache-2.0 |
| rustix | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| bytemuck | Zlib OR Apache-2.0 OR MIT |
| raw-window-handle | MIT OR Apache-2.0 OR Zlib |
| thiserror / anyhow | MIT OR Apache-2.0 |
| tracing / tracing-subscriber | MIT |

除 Slint 外，上述依赖均提供了 MIT、Apache-2.0、BSD、ISC、Zlib 等宽松许可选项，
项目按这些许可选项使用。

## 图标

- `assets/icons/` 与 `extension/icons/` 中的应用图标、托盘图标与界面图标为项目资产，
  随项目以 Apache-2.0 发布。
- 设置中心的部分线性图标参考自 [Lucide](https://lucide.dev) 图标集，
  Copyright Lucide Contributors，ISC License。

## 系统库

Wayland、xkbcommon、fontconfig、FreeType、D-Bus 等系统库由发行版以动态链接方式提供，
不随本项目源码或 deb 包捆绑，其许可与版权归各发行版及上游项目所有。
