# 技术选型

## 数据层: physis vs ironworks

| 维度 | ironworks | physis |
|------|-----------|--------|
| 许可证 | MIT | GPL-3.0 |
| 格式覆盖 | ~10 种 | 30+ 种 |
| 读写 | 只读 | 读写 |
| MDL/TEX/MTRL | 支持 | 支持 |
| STM 染色模板 | 不支持 | 支持 |
| BCn 纹理解码 | 不支持 | 内置 |
| 维护状态 | 活跃 | 活跃 |

**选择: ironworks (MIT)** — physis (GPL-3.0) 的 SqPack 解析器无法处理国服 region=0 的文件头。ironworks 不解析 region 字段，兼容性更好。STM/BCn 解码需要后续自行实现或引入 physis 作为补充。

## 渲染层

| 方案 | 优点 | 缺点 |
|------|------|------|
| three-d + egui | 最快出成果，内置相机/光照 | OpenGL，shader 灵活性低 |
| eframe/egui + wgpu | shader 完全可控 | 需自己实现渲染管线 |
| bevy | 功能最全 | 过度设计，编译慢 |

**选择: eframe/egui + wgpu** — 染色系统需要自定义 fragment shader，wgpu 提供完全控制权。egui 提供颜色选择器、列表等 UI 控件。

## 最终技术栈

```
ironworks 0.4.1  — SqPack 读取、EXD 数据表、MDL 解析 (features: sqpack, excel, ffxiv, mdl)
eframe 0.33      — 窗口管理 + UI (装备列表、染色选择器)
egui-wgpu 0.33   — egui 与 wgpu 集成
wgpu 27          — 3D 渲染 (自定义 WGSL shader, 离屏渲染)
bytemuck 1       — GPU 数据类型转换
```

## 已实现功能

1. **SqPack 数据读取** — ironworks + FsResource 读取国服游戏数据
2. **装备数据层** — 从 Item EXD 表加载 14937 件防具 (中文名称)
3. **egui 浏览界面** — 搜索、槽位过滤、虚拟滚动列表
4. **MDL 模型解析** — 提取 Position/Normal/UV 顶点属性 (支持 Vec3/Vec4)
5. **wgpu 3D 渲染** — 离屏渲染管线、方向光 WGSL 着色器、轨道相机、鼠标交互

## 待实现

- TEX 纹理加载 + 材质渲染
- MTRL 材质解析
- Colorset/STM 染色系统
- BCn 纹理解码

## 参考项目

| 项目 | 语言 | 参考价值 |
|------|------|----------|
| [Novus](https://github.com/redstrate/Novus) | C++/Qt | 使用 physis 的模型查看器 |
| [dlunch/FFXIVTools](https://github.com/dlunch/FFXIVTools) | Rust/WASM | Rust FF14 查看器 (已归档) |
| [TexTools](https://github.com/TexTools/FFXIV_TexTools_UI) | C#/WPF | 最成熟的染色编辑工具 |
| [Penumbra](https://github.com/xivdev/Penumbra) | C# | 染色系统实现参考 |

## 关键文档

- [xiv.dev/data-files/sqpack](https://xiv.dev/data-files/sqpack) — SqPack 格式规范
- [docs.xiv.zone](https://docs.xiv.zone) — MDL/TEX 格式文档 (physis 作者)
- [xivmodding.com](https://xivmodding.com) — Colorset/Dye 实践指南
- [EXDSchema](https://github.com/xivdev/EXDSchema) — EXD 表结构定义
