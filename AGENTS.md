# AGENTS.md

## 项目概述

FF14 本地工具软件，使用 Rust 实现。当前功能: 浏览游戏内服装并预览 3D 模型及染色效果。

## 技术栈

- **数据层**: physis (SqPack/MDL/TEX/MTRL/STM/EXD 解析)
- **GUI**: eframe + egui
- **渲染**: wgpu (自定义 WGSL shader)

## docs/ 目录说明

| 文件 | 内容 | 何时参考 |
|------|------|----------|
| `docs/game-data-formats.md` | SqPack/MDL/TEX/MTRL/EXD 格式说明 | 实现数据读取、模型加载、纹理解析时 |
| `docs/dye-system.md` | 染色系统技术细节 (colorset/stain/STM) | 实现染色预览功能时 |
| `docs/tech-stack.md` | 技术选型依据和参考项目 | 选择依赖库、查找参考实现时 |

## 开发约定

- 使用中文进行对话和注释
- 逐功能迭代，每个功能点完成后编写 commit
- 优先实现最小可验证功能，再逐步扩展
- 游戏数据路径需要用户配置 (FF14 安装目录)
