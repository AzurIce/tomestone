# FF14 游戏数据格式

## SqPack 容器

FF14 所有游戏数据打包在 SqPack 容器中，位于游戏安装目录 `sqpack/` 下。

### 仓库结构

| 目录 | 内容 |
|------|------|
| `ffxiv/` | 基础游戏 (A Realm Reborn) |
| `ex1/` | Heavensward |
| `ex2/` | Stormblood |
| `ex3/` | Shadowbringers |
| `ex4/` | Endwalker |
| `ex5/` | Dawntrail |

### 分类 (Category)

| ID | 名称 | 内容 |
|----|------|------|
| 04 | chara | 角色模型、装备、纹理 |
| 06 | ui | 用户界面 |
| 0A | exd | Excel 数据表 |

### 文件组成

- `.index` / `.index2` — 哈希索引，CRC32 查找
- `.dat0` / `.dat1` ... — 实际数据，zlib 压缩，单文件最大 2GB

## MDL 模型格式

装备模型路径: `chara/equipment/e{NNNN}/model/c{RRRR}e{NNNN}_{slot}.mdl`

- slot: `met`(头) / `top`(身) / `glv`(手) / `dwn`(腿) / `sho`(脚)
- 最多 3 级 LOD
- 多流顶点布局 (最多 3 stream)
- 顶点属性: Position, Normal, UV, Tangent, Color, BlendWeight, BlendIndex
- 顶点索引 u16，每 mesh 最多 65535 顶点
- 每模型最多引用 4 个材质

## TEX 纹理格式

纹理路径: `chara/equipment/e{NNNN}/texture/v{VVVV}_c{RRRR}e{NNNN}_{slot}_{type}.tex`

### 纹理类型

| 后缀 | 用途 |
|------|------|
| `_d.tex` | Diffuse (漫反射/颜色) |
| `_n.tex` | Normal (法线) |
| `_s.tex` | Specular/Multi (高光) |
| `_id.tex` | Index (染色索引, 7.0+) |

### 压缩格式

| 格式 | 用途 |
|------|------|
| BC1 (DXT1) | 无/1-bit Alpha |
| BC3 (DXT5) | 完整 Alpha |
| BC5 (ATI2) | 法线贴图 |
| BC7 | 高质量压缩 |

宽高必须是 2 的幂次，允许非正方形。

## MTRL 材质格式

材质路径: `chara/equipment/e{NNNN}/material/v{VVVV}/mt_c{RRRR}e{NNNN}_{slot}_{id}.mtrl`

材质文件包含:
- 纹理引用列表
- ColorTable (16 行 colorset)
- ColorDyeTable (染色配置)
- 着色器引用 (SHPK)

## EXD 数据表

游戏结构化数据存储在 Excel 格式的二进制表中:
- `.exl` — 表名列表
- `.exh` — 表头 (列类型定义)
- `.exd` — 数据行

关键表:
- **Item** — 物品信息，装备包含模型 ID、槽位、职业限制等
- **Stain** — 染料 ID、名称、RGB 颜色值 (约 136 种)
- **EquipSlotCategory** — 装备槽位类别

## 装备文件层级总览

```
MDL (3D模型)
 └── MTRL (材质) × 最多4
      ├── TEX _d.tex (漫反射)
      ├── TEX _n.tex (法线)
      ├── TEX _s.tex (高光)
      ├── TEX _id.tex (染色索引, 7.0+)
      ├── ColorTable (16行 colorset)
      ├── ColorDyeTable (染色配置)
      └── SHPK (着色器包)
```
