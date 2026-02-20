# FFXIV 资源管线：从 EXD 表到模型文件

本文档梳理 FFXIV 游戏数据中，装备/配饰等资源如何从 EXD 表出发，经过表间引用，最终定位到 MDL 模型、MTRL 材质、TEX 纹理等实际文件。

## 1. 总览

```
┌─────────────────────────────────────────────────────────────────┐
│                        EXD 表层（数据定义）                       │
│                                                                 │
│  Item ──→ EquipSlotCategory（槽位）                              │
│    │                                                            │
│    ├── ModelMain (u64) ─→ 编码了 set_id + variant_id            │
│    ├── ModelSub  (u64) ─→ 副手/盾牌的模型信息                    │
│    ├── Icon ─→ ui/icon/{group:06}/{id:06}.tex                   │
│    └── 其他字段（名称、等级、职业限制等）                          │
│                                                                 │
│  Stain ──→ 染料颜色 + Shade 分组                                 │
│  StainingTemplate ──→ 染色矩阵 (.stm)                           │
│                                                                 │
│  Mount      ──→ ModelChara ──→ 怪物/坐骑模型路径                 │
│  Companion  ──→ ModelChara                                      │
│  Ornament   ──→ Model (直接值)                                   │
│  BNpcBase   ──→ ModelChara                                      │
└────────────────────────┬────────────────────────────────────────┘
                         │ 路径构建规则
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                     SqPack 文件层（实际资源）                     │
│                                                                 │
│  chara/equipment/e{SSSS}/model/c{RRRR}e{SSSS}_{slot}.mdl       │
│  chara/accessory/a{SSSS}/model/c{RRRR}a{SSSS}_{slot}.mdl       │
│  chara/weapon/w{SSSS}/obj/body/b{VVVV}/model/w{SSSS}b{VVVV}.mdl│
│       │                                                         │
│       └──→ MDL 内嵌材质名 ──→ MTRL ──→ TEX                      │
└─────────────────────────────────────────────────────────────────┘
```

## 2. Item 表（核心装备/配饰表）

所有可装备物品（防具、配饰、武器）都在同一张 `Item` 表中。

### 2.1 关键列

根据 [EXDSchema](https://github.com/xivdev/EXDSchema) 的定义，Item 表的字段顺序如下（简化，仅列出与模型相关的）：

| Schema 字段名 | 列索引 | 类型 | 说明 |
|---|---|---|---|
| Name | 3 | String | 物品名（本地化） |
| ModelMain | 10 | UInt64 | 主模型编码 |
| ModelSub | 11 | UInt64 | 副模型编码（盾牌/副手） |
| EquipSlotCategory | 57 | UInt8 (link) | → EquipSlotCategory 表行 ID |
| Icon | 53 | UInt16 (icon) | 图标 ID |
| DyeCount | 63 | UInt8 | 可染色通道数（0/1/2） |

> **注意**：列索引取决于 EXH 的列定义顺序，与 Schema 字段顺序不同。
> 当前代码中实际使用的列索引（通过 EXD 浏览器确认）：
> - `COL_NAME = 0`（第一个 String 列，语言相关字段中的 Name）
> - `COL_EQUIP_SLOT_CATEGORY = 17`（u8）
> - `COL_MODEL_MAIN = 47`（u64）
>
> Schema 定义的字段顺序含有 array/link 展开，实际 EXH 列索引需要通过 EXD 浏览器或列偏移量确认。

### 2.2 ModelMain / ModelSub 编码

`ModelMain` 和 `ModelSub` 是 u64 值，编码方式：

```
ModelMain (u64):
  bits [0:15]   → set_id      (装备套装 ID，如 6234)
  bits [16:31]  → variant_id  (材质变体，如 9)
  bits [32:47]  → 未使用（装备）/ 武器第二参数
  bits [48:63]  → 未使用
```

解码：
```rust
let set_id     = (model_main & 0xFFFF) as u16;
let variant_id = ((model_main >> 16) & 0xFFFF) as u16;
```

当 `model_main == 0` 时表示该物品无模型（消耗品等）。

## 3. EquipSlotCategory 表（槽位定义）

EquipSlotCategory 表每行包含 14 个 bool 字段，表示该类别允许装备到哪些槽位：

| 字段名 | 含义 |
|--------|------|
| MainHand | 主手武器 |
| OffHand | 副手/盾牌 |
| Head | 头部 |
| Body | 身体 |
| Gloves | 手部 |
| Waist | 腰带（已废弃） |
| Legs | 腿部 |
| Feet | 脚部 |
| Ears | 耳饰 |
| Neck | 项链 |
| Wrists | 手镯 |
| FingerL | 左戒指 |
| FingerR | 右戒指 |
| SoulCrystal | 灵魂水晶 |

实际使用中，大部分物品只对应一个槽位。Item 表的 `EquipSlotCategory` 列的值就是该表的行 ID。

### 3.1 行 ID → 槽位映射

| 行 ID | 对应槽位 | 路径类型 | 缩写 |
|-------|---------|---------|------|
| 1 | MainHand | weapon | - |
| 2 | OffHand | weapon | - |
| 3 | Head | equipment | `met` |
| 4 | Body | equipment | `top` |
| 5 | Gloves | equipment | `glv` |
| 6 | Waist | (已废弃) | - |
| 7 | Legs | equipment | `dwn` |
| 8 | Feet | equipment | `sho` |
| 9 | Earring | accessory | `ear` |
| 10 | Neck | accessory | `nek` |
| 11 | Wrists | accessory | `wrs` |
| 12 | Ring (L+R) | accessory | `ril` |
| 13 | Ring (R) | accessory | `rir` |

> 行 ID 12 通常同时设置 FingerL=1 和 FingerR=1，表示戒指可装备到任一手指。
> 行 ID 13 极少使用。

## 4. 路径构建规则

### 4.1 三种模型路径格式

根据槽位类型，模型文件在不同目录下，使用不同命名规则：

#### 装备 (Equipment)：Head / Body / Gloves / Legs / Feet

```
chara/equipment/e{set_id:04}/model/c{race_code:04}e{set_id:04}_{slot_abbr}.mdl
```

示例：铁面具 (set_id=0005, Head)
```
chara/equipment/e0005/model/c0201e0005_met.mdl
                 ^^^^                ^^^^  ^^^
                 套装ID              套装ID 槽位缩写
```

#### 配饰 (Accessory)：Earring / Neck / Wrists / Ring

```
chara/accessory/a{set_id:04}/model/c{race_code:04}a{set_id:04}_{slot_abbr}.mdl
```

示例：耳坠 (set_id=0032, Earring)
```
chara/accessory/a0032/model/c0201a0032_ear.mdl
                ^                ^
            注意是 'a'        注意是 'a'
```

#### 武器 (Weapon)：MainHand / OffHand

```
chara/weapon/w{model_id:04}/obj/body/b{body_id:04}/model/w{model_id:04}b{body_id:04}.mdl
```

武器路径结构不同，不按种族区分，而是按 body 变体区分。

### 4.2 种族码 (Race Code)

格式：`c{race_id:02}{tribe_id:02}`

不同种族可能共用同一套模型，也可能有专属模型。加载时按优先级尝试：

| 种族码 | 种族 |
|-------|------|
| c0101 | Hyur Midlander ♂ |
| c0201 | Hyur Midlander ♀ |
| c0301 | Hyur Highlander ♂ |
| c0401 | Hyur Highlander ♀ |
| c0501 | Elezen ♂ |
| c0601 | Elezen ♀ |
| c0701 | Miqo'te ♂ |
| c0801 | Miqo'te ♀ |
| c0901 | Roegadyn ♂ |
| c1001 | Roegadyn ♀ |
| c1101 | Lalafell ♂ |
| c1201 | Lalafell ♀ |
| c1301 | Au Ra ♂ |
| c1401 | Au Ra ♀ |
| c1501 | Hrothgar ♂ |
| c1701 | Viera ♂ |
| c1801 | Viera ♀ |

当某种族码对应的模型文件不存在时，回退到其他种族码尝试。

### 4.3 槽位缩写对照

| 槽位 | 缩写 | 路径前缀 |
|------|------|---------|
| Head | `met` | equipment `e` |
| Body | `top` | equipment `e` |
| Gloves | `glv` | equipment `e` |
| Legs | `dwn` | equipment `e` |
| Feet | `sho` | equipment `e` |
| Earring | `ear` | accessory `a` |
| Neck | `nek` | accessory `a` |
| Wrists | `wrs` | accessory `a` |
| Ring Left | `ril` | accessory `a` |
| Ring Right | `rir` | accessory `a` |

## 5. 从 MDL 到材质和纹理

### 5.1 MDL → MTRL

MDL 文件内部包含一个材质名称字符串表，每个 mesh 通过 `material_index` 引用其中一个材质名。

材质名是短名，格式如 `/mt_c0201e6234_top_a.mtrl`，需要拼接完整路径：

**装备：**
```
chara/equipment/e{set_id:04}/material/v{variant_id:04}{material_short_name}
```

示例：
```
短名: /mt_c0201e6234_top_a.mtrl
完整: chara/equipment/e6234/material/v0009/mt_c0201e6234_top_a.mtrl
                      ^^^^          ^^^^
                      set_id        variant_id
```

**配饰：**
```
chara/accessory/a{set_id:04}/material/v{variant_id:04}{material_short_name}
```

> 当指定 variant 的材质不存在时，回退到 `v0001`。

### 5.2 MTRL → TEX

MTRL 文件解析后得到 `texture_paths: Vec<String>`，包含该材质引用的所有纹理完整路径。

纹理命名约定（两种体系共存）：

#### 旧式 (Endwalker 及之前)
| 后缀 | 用途 |
|------|------|
| `_d.tex` | Diffuse (漫反射贴图) |
| `_n.tex` | Normal (法线贴图) |
| `_s.tex` | Specular (高光贴图) |
| `_m.tex` | Mask (遮罩贴图) |

#### 新式 (Dawntrail)
| 后缀 | 用途 |
|------|------|
| `_base.tex` | Base Color (基础颜色) |
| `_norm.tex` | Normal (法线贴图) |
| `_mask.tex` | Mask (遮罩贴图) |
| `_id.tex` | ID Texture (ColorTable 索引图) |

### 5.3 ColorTable 着色系统

部分材质不使用传统 diffuse 贴图，而是使用 ColorTable 系统：

```
MTRL 包含:
  ├── ColorTable (16行 Legacy / 32行 Dawntrail)
  │   每行有 diffuse_color, specular_color, emissive_color 等
  ├── ColorDyeTable (可选，染色用)
  └── texture_paths 中的 _id.tex
```

渲染流程：
1. 读取 `_id.tex`，每个像素的 R 通道映射到 ColorTable 行号
2. 从 ColorTable 取该行的 diffuse_color
3. 用该颜色"烘焙"出最终 diffuse 纹理

染色时：
1. 从 Stain 表获取染料 ID
2. 从 `StainingTemplate` (.stm) 获取染色矩阵
3. 用 ColorDyeTable 决定哪些行受染料影响
4. 替换对应行的颜色后重新烘焙

## 6. 完整数据链路示例

以 "缎带蝴蝶结耳坠"（假设 row_id=38001）为例：

```
Step 1: 从 Item 表读取行
  ├── Name = "缎带蝴蝶结耳坠"
  ├── EquipSlotCategory = 9            ← 查 EquipSlotCategory 表: Ears=1
  ├── ModelMain = 0x0001_0020          ← set_id=32, variant_id=1
  └── DyeCount = 1

Step 2: 确定路径类型
  ├── EquipSlotCategory 9 → Earring → accessory 类型
  └── 槽位缩写: "ear"

Step 3: 构建 MDL 路径
  chara/accessory/a0032/model/c0201a0032_ear.mdl

Step 4: 解析 MDL → 得到 mesh + 材质短名
  材质: ["/mt_c0201a0032_ear_a.mtrl"]

Step 5: 构建 MTRL 路径
  chara/accessory/a0032/material/v0001/mt_c0201a0032_ear_a.mtrl

Step 6: 解析 MTRL → 得到纹理路径列表
  [
    "chara/accessory/a0032/texture/v01_c0201a0032_ear_d.tex",
    "chara/accessory/a0032/texture/v01_c0201a0032_ear_n.tex",
  ]

Step 7: 加载纹理 → 渲染
```

## 7. 其他资源类型的表结构

### 7.1 坐骑 (Mount)

```
Mount 表
  └── ModelChara (link) ──→ ModelChara 表
                               ├── Type (怪物类型)
                               ├── Model (模型 ID)
                               ├── Base
                               └── Variant
```

模型路径由 ModelChara 的 Type 决定：
- Type=1: `chara/demihuman/d{model:04}/obj/equipment/e{base:04}/model/...`
- Type=2: `chara/monster/m{model:04}/obj/body/b{base:04}/model/...`
- Type=3: `bg/...` (背景物体)

### 7.2 宠物/随从 (Companion/Minion)

```
Companion 表
  └── Model (link) ──→ ModelChara 表 ──→ 同上
```

### 7.3 时尚配饰/挂坠 (Ornament)

```
Ornament 表
  └── Model (直接值，非 link)
```

### 7.4 家具 (HousingFurniture)

```
HousingFurniture 表
  ├── Item (link) ──→ Item 表
  └── ModelKey ──→ 家具模型路径
```

家具模型在 `bgcommon/hou/` 目录下，路径规则不同于装备。

## 8. 染色相关表

### 8.1 Stain 表

| 字段 | 类型 | 说明 |
|------|------|------|
| Name | String | 染料名（本地化） |
| Color | UInt32 (color) | 0xRRGGBB 预览颜色 |
| Shade | UInt8 | 色调分组 (1=其他, 2=白灰黑, 4=红粉, 5=橙棕, 6=黄, 7=绿, 8=蓝, 9=紫, 10=特殊) |
| IsMetallic | Bool | 是否金属染料 |

### 8.2 StainingTemplate (.stm 文件)

非 EXD 表，是独立二进制文件：`chara/base_material/stainingtemplate.stm`

包含 128 个染色模板，每个模板定义了对各种基础颜色的变换矩阵。

### 8.3 染色流程

```
Item.DyeCount (0/1/2) → 是否可染 + 通道数
MTRL.ColorDyeTable     → 每行的模板 ID + 应用标志
Stain.rowId            → 选择的染料
StainingTemplate       → 模板 ID 对应的颜色变换
ColorTable             → 原始行颜色
  ↓
应用染色变换 → 修改后的行颜色 → 重新烘焙 _id.tex → 最终 diffuse
```

## 9. EXDSchema 与列索引的关系

EXH 中的列按 `(data_type, offset)` 定义，没有名称。EXDSchema 提供列名，但 Schema 字段顺序与 EXH 列索引不是简单的一一对应关系，因为：

1. Schema 中的 `array` 字段会展开为多个 EXH 列
2. Schema 中的 `link` 字段本质上就是一个整数列
3. 语言相关字段（Singular, Plural, Name 等多语言变体）只在特定语言 EXD 中出现

要准确确定列索引，需要：
- 用 EXD 浏览器查看 EXH 列定义（offset + type）
- 或按照 Schema 字段顺序展开 array，逐个对应 EXH 列

## 10. 当前 tomestone 的实现状态

| 资源类型 | 状态 | 路径前缀 |
|---------|------|---------|
| 装备 (Head/Body/Gloves/Legs/Feet) | ✅ 已实现 | `chara/equipment/` |
| 配饰 (Earring/Neck/Wrists/Ring) | ❌ 待实现 | `chara/accessory/` |
| 武器 (MainHand/OffHand) | ❌ 待实现 | `chara/weapon/` |
| 坐骑 (Mount) | ❌ 待实现 | `chara/monster/` 等 |
| 宠物 (Companion) | ❌ 待实现 | `chara/monster/` 等 |
| 时尚配饰 (Ornament) | ❌ 待实现 | 待调查 |
| 家具 (HousingFurniture) | ❌ 待实现 | `bgcommon/hou/` |
| 染色系统 | ✅ 已实现 | - |

添加配饰支持时需要修改的位置：
1. `game_data.rs` — 扩展 `EquipSlot` 枚举，添加 category 9-13 的映射
2. `game_data.rs` — `EquipmentItem::model_path()` 根据槽位类型选择 `equipment` 或 `accessory` 路径
3. `tex_loader.rs` — `resolve_material_path()` 同样需要区分 `equipment` / `accessory`
4. `main.rs` — `ALL_SLOTS` 常量需要包含新槽位
