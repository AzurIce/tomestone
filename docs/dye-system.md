# FF14 染色系统

## 核心概念: Colorset

每个材质 (.mtrl) 包含 16 行 colorset，每行定义一组颜色属性:

- **Diffuse** — 基础颜色
- **Specular** — 高光色 (越亮越有光泽)
- **Emissive** — 自发光色
- **Gloss** — 光泽度

## 两代系统

### Legacy (Endwalker 及之前)

Colorset 行号信息存储在法线贴图的 Alpha 通道中。

### Dawntrail (7.0+)

使用专用 `_id.tex` 纹理:
- 红色通道 → colorset 行号 (0x00=行1, 0x11=行2, ... 0xFF=行16)
- 绿色通道 → A/B 变体选择 (0x00=B, 0xFF=A)

引入双染色通道:
- 通道 1: 装备主要大面积区域
- 通道 2: 次要细节 (纽扣、装饰)

## Stain 染料数据

- 存储在 Stain EXD 表中，约 136 种染料
- 每种染料有 ID、名称、RGB 颜色值

## STM 染色模板

`.stm` 文件定义染料如何影响 colorset 行。43 种模板，编号规则:

| 编号模式 | 含义 |
|----------|------|
| `(#)#00` | 默认染色 |
| `(#)#01` | 偏暗 |
| `(#)#02` | 偏亮 |
| `(#)#2#` | 强制黑色 |
| `(1)540` | 强制银色 |
| `(1)550` | 强制金色 |

## 着色器

- `characterlegacy.shpk` — 旧版角色着色器
- `character.shpk` — Dawntrail 新版 (支持天球贴图/金属/玻璃/全息)

## 染色渲染流程

1. 从 Item 表获取装备信息
2. 加载 MDL 模型和 MTRL 材质
3. 从材质中读取 ColorTable (16 行 colorset)
4. 从材质中读取 ColorDyeTable (染色配置)
5. 用户选择染料 → 查 Stain 表获取颜色
6. 根据 STM 模板计算染料对 colorset 各属性的影响
7. 在 fragment shader 中混合最终颜色
