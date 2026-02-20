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

## 参考资料

- [xiv.dev](https://xiv.dev) - FF14 数据格式文档
- [ffxiv-datamining](https://github.com/xivapi/ffxiv-datamining/blob/master/docs/README.md) - 数据挖掘文档
- [ffxiv-datamining research](https://github.com/xivapi/ffxiv-datamining/blob/master/research/README.md) - 数据挖掘研究报告
- [XIVAPI v2](https://v2.xivapi.com/docs/welcome/) - API 文档

## 开发约定

- 使用中文进行对话和注释
- 逐功能迭代，每个功能点完成后编写 commit
- 优先实现最小可验证功能，再逐步扩展
- 游戏数据路径需要用户配置 (FF14 安装目录)

## 注意事项

- 注意 Windows 平台下不要错误地将 nul 被当成普通文件创建出来
