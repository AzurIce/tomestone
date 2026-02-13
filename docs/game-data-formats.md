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

### 国服 SqPack 兼容性问题

SqPack 头部偏移 0x20 处有一个 `Region` 字段:
- `0xFFFFFFFF (-1)` = Global (国际服)
- `0x01` = KoreaChina
- `0x00` = 国服实际使用的值 (Unspecified)

physis 库的 Region 枚举原本缺少 `0` 值导致无法解析国服数据。ironworks 不解析此字段所以不受影响。

---

## EXD 数据表 — 装备信息提取

游戏结构化数据存储在 Excel 格式的二进制表中:
- `.exl` — 表名列表
- `.exh` — 表头 (列类型定义)
- `.exd` — 数据行

### 关键表

| 表 | 内容 |
|----|------|
| **Item** | 物品信息，装备包含模型 ID、槽位、职业限制等 |
| **Stain** | 染料 ID、名称、RGB 颜色值 (约 136 种) |
| **EquipSlotCategory** | 装备槽位类别 |

### Item 表关键列

| 列索引 | 类型 | 内容 |
|--------|------|------|
| 0 | String | 物品名 (如"猫魅坎肩"、"大码马夏文化衫") |
| 10 | u16 | 图标 ID |
| 17 | u8 | 装备槽位类别 (EquipSlotCategory) |
| 47 | u64 | **ModelMain** — 模型/变体复合值 |

### ModelMain 字段解析

ModelMain 是一个 u64，编码了装备 ID 和变体:

```
bits [0:15]  → set_id     (装备套装 ID，如 6234)
bits [16:31] → variant_id (变体 ID，如 9)
bits [32:63] → 未使用
```

```rust
let set_id = (model_main & 0xFFFF) as u16;
let variant_id = ((model_main >> 16) & 0xFFFF) as u16;
```

### 装备槽位

| EquipSlotCategory | 名称 | 缩写 | 说明 |
|-------------------|------|------|------|
| 3 | Head | `met` | 头部 |
| 4 | Body | `top` | 身体 |
| 5 | Gloves | `glv` | 手部 |
| 7 | Legs | `dwn` | 腿部 |
| 8 | Feet | `sho` | 脚部 |

---

## 种族码 (Race Code)

装备模型按种族区分。种族码格式: `c{raceId:02d}{bodyId:02d}`，bodyId 通常为 `01`。

### 种族码对照表

| 种族码 | 种族 | 性别 |
|--------|------|------|
| c0101 | Hyur Midlander (人族中原之民) | 男 |
| c0201 | Hyur Midlander (人族中原之民) | 女 |
| c0301 | Hyur Highlander (人族高地之民) | 男 |
| c0401 | Hyur Highlander (人族高地之民) | 女 |
| c0501 | Elezen (精灵族) | 男 |
| c0601 | Elezen (精灵族) | 女 |
| c0701 | Miqo'te (猫魅族) | 男 |
| c0801 | Miqo'te (猫魅族) | 女 |
| c0901 | Roegadyn (鲁加族) | 男 |
| c1001 | Roegadyn (鲁加族) | 女 |
| c1101 | Lalafell (拉拉菲尔族) | 男 |
| c1201 | Lalafell (拉拉菲尔族) | 女 |
| c1301 | Au Ra (敖龙族) | 男 |
| c1401 | Au Ra (敖龙族) | 女 |
| c1501 | Hrothgar (硌狮族) | 男 |
| c1701 | Viera (维埃拉族) | 男 |
| c1801 | Viera (维埃拉族) | 女 |

### 装备模型的种族分类

1. **通用模型**: 大多数装备只有 `c0201`（Hyur 女）和/或 `c0101`（Hyur 男）模型。其他种族通过骨骼变形共享这些模型。
2. **种族专属模型**: 部分装备只有特定种族的模型文件，如:
   - `c0701e0088_top.mdl` — 猫魅族男性坎肩 (e0088)
   - `c0801e0089_dwn.mdl` — 猫魅族女性下装 (e0089)
3. **身体材质 (b0001)**: 许多装备引用 `mt_c{race}b0001_a.mtrl`，这是角色身体皮肤材质，不在装备路径下存在。

---

## MDL 模型格式

### 路径格式

```
chara/equipment/e{set_id:04d}/model/{race_code}e{set_id:04d}_{slot}.mdl
```

**示例:**
- `chara/equipment/e0005/model/c0201e0005_met.mdl` — Hyur 女性 e0005 头盔
- `chara/equipment/e6234/model/c0201e6234_top.mdl` — Hyur 女性 e6234 上衣

### MDL 文件头 (68 bytes)

```
偏移   类型    说明
0x00   u32    version
0x04   u32    stack_size
0x08   u32    runtime_size
0x0C   u16    vertex_decl_count   (顶点声明数量)
0x0E   u16    material_count      (材质数量)
0x10   u32×3  vertex_offset[3]    (每 LOD 的顶点偏移)
0x1C   u32×3  index_offset[3]     (每 LOD 的索引偏移)
0x28   u32×3  vertex_buffer_size[3]
0x34   u32×3  index_buffer_size[3]
0x40   u32    lod_count + padding
```

### 顶点声明

每个声明包含最多 17 个顶点元素，每个元素 8 字节:

```
u8   stream     (0-2, 0xFF=终止符)
u8   offset     (流内字节偏移)
u8   format     (2=Single3, 3=Single4, 8=ByteFloat4, 13=Half2, 14=Half4)
u8   usage      (0=Position, 3=Normal, 4=UV, 7=Color)
u32  padding
```

### 字符串表 (String Table)

紧跟顶点声明之后:

```
u16  string_count
u16  padding
u32  string_size (字节)
[null-terminated strings]
```

字符串表包含材质名称引用（过滤 `.mtrl` 结尾），如:
- `/mt_c0201e6234_top_a.mtrl`
- `/mt_c0201e6234_top_b.mtrl`
- `/mt_c0201b0001_a.mtrl` (身体材质)

### 模型头 (Model Header)

```
f32  radius
u16  mesh_count
u16  attribute_count
u16  submesh_count
u16  material_count
u16  bone_count
...
u8   lod_count
u8   flags1
...
```

### LOD 数据 (每级 ~52 bytes)

```
u16  mesh_index, mesh_count
f32  lod_range_min, lod_range_max
...
u32  vertex_data_offset  (绝对文件偏移)
u32  index_data_offset   (绝对文件偏移)
```

### Mesh 数据

```
u16  vertex_count
u16  padding
u32  index_count
u16  material_index   ← 指向字符串表中的材质
u16  submesh_index
u16  submesh_count
u16  bone_table_index
u32  start_index      (索引缓冲区字节偏移)
u32  vbo0_offset      (顶点流 0 偏移)
u32  vbo1_offset      (顶点流 1 偏移)
u32  vbo2_offset      (顶点流 2 偏移)
u8   vbs0, vbs1, vbs2 (每流步长)
u8   stream_count
```

### 顶点/索引数据定位

```
vertex_abs = lod.vertex_data_offset + mesh.vbo{n}_offset + element.offset + stride * vertex_idx
index_abs  = lod.index_data_offset + mesh.start_index  (u16 little-endian)
```

### MDL 版本

- **v5**: Endwalker 及之前，ironworks 可直接解析
- **v6**: Dawntrail 新版本，ironworks 的 FileKind 无法识别 (0xc6f3ff04)，需要 physis 或手动解析

---

## MTRL 材质格式

### 路径格式

```
chara/equipment/e{set_id:04d}/material/v{variant_id:04d}/{material_name}
```

**示例:**
- `chara/equipment/e6234/material/v0009/mt_c0201e6234_top_b.mtrl`
- `chara/equipment/e0005/material/v0001/mt_c0201e0005_top_a.mtrl`

### 材质名命名规则

```
mt_c{race_code}e{set_id:04d}_{slot}_{letter}.mtrl
```

- `{letter}`: a, b, c... 同一槽位可有多个材质（对应不同 mesh 部件）
- 特殊: `mt_c{race}b0001_a.mtrl` 为身体皮肤材质

### MTRL 容器头 (16 bytes)

```
偏移   类型    说明
0x00   u32    version
0x04   u16    file_size
0x06   u16    data_set_size
0x08   u16    string_table_size   ← 字符串表大小
0x0A   u16    shader_name_offset
0x0C   u8     texture_count       ← 纹理数量
0x0D   u8     uv_set_count
0x0E   u8     color_set_count
0x0F   u8     additional_data_size
```

### 纹理偏移表

```
for i in 0..texture_count:
  u16  offset    (字符串表内偏移)
  u16  flags
```

### UV / ColorSet 偏移

```
for i in 0..uv_set_count:    u32 data
for i in 0..color_set_count: u32 offset
```

### 字符串表

紧跟偏移表之后，包含 null-terminated 纹理路径字符串。

### 材质包含的内容

- 纹理引用列表 (由字符串表给出)
- ColorTable (16 行 colorset，用于染色系统)
- ColorDyeTable (染色配置)
- 着色器引用 (SHPK)

---

## TEX 纹理格式

### 路径格式

```
chara/equipment/e{set_id:04d}/texture/v{variant:02d}_c{race}e{set_id:04d}_{slot}_{type}.tex
```

**示例:**
- `chara/equipment/e0005/texture/v20_c0201e0005_top_d.tex`
- `chara/equipment/e6234/texture/v09_c0201e6234_top_b_base.tex`
- `chara/common/texture/common_id.tex` (共用纹理)

### TEX 文件头 (80 bytes, little-endian)

```
偏移    类型    说明
0x00    u32    attribute (标志位)
0x04    u32    format_id (压缩格式)
0x08    u16    width
0x0A    u16    height
0x0C    u16    depth (通常 1)
0x0E    u16    padding
0x10    u32    padding
0x14    u32    lod0_offset
0x18    u32    lod1_offset
0x1C    u32    lod2_offset
0x20-0x4F     其他元数据/padding
```

**像素数据从偏移 0x50 (80) 开始。** 仅使用 mip level 0。

### 纹理类型

#### 旧式 (pre-Dawntrail)

| 后缀 | 用途 | 说明 |
|------|------|------|
| `_d.tex` | Diffuse | 漫反射/颜色贴图 |
| `_n.tex` | Normal | 法线贴图 (显示为蓝紫色) |
| `_s.tex` | Specular | 高光/Multi 贴图 |
| `_m.tex` | Mask | 遮罩贴图 |

#### 新式 (Dawntrail 7.0+)

| 后缀 | 用途 | 说明 |
|------|------|------|
| `_base.tex` | Base Color | 新漫反射 (替代 `_d.tex`) |
| `_norm.tex` | Normal | 新法线贴图 (替代 `_n.tex`) |
| `_mask.tex` | Mask | 新遮罩贴图 (替代 `_m.tex`) |
| `_id.tex` | Color ID | 染色索引 (colorset 行查找) |

### 压缩格式

| format_id | 名称 | 每像素/每块 | 说明 |
|-----------|------|-------------|------|
| `0x1450` | ARGB8 | 32bpp | 未压缩，BGRA 字节序 (需 swizzle → RGBA) |
| `0x1451` | RGBA8 | 32bpp | 未压缩，原生 RGBA |
| `0x1452` | RGBX8 | 32bpp | 未压缩，Alpha 通道忽略 (补 255) |
| `0x3420` | DXT1/BC1 | 8B/4×4 block | 4:1 压缩，无/1-bit Alpha |
| `0x3430` | DXT3/BC2 | 16B/4×4 block | 显式 Alpha |
| `0x3431` | DXT5/BC3 | 16B/4×4 block | 插值 Alpha |
| `0x6230` | BC7 | 16B/4×4 block | 高质量压缩 (Dawntrail 常用) |

### Mip0 数据大小计算

```
BC1:       ((width+3)/4) × ((height+3)/4) × 8   字节
BC2/BC3:   ((width+3)/4) × ((height+3)/4) × 16  字节
BC7:       ((width+3)/4) × ((height+3)/4) × 16  字节
未压缩:     width × height × 4                   字节
```

---

## 两种材质模式

### 模式一: 传统纹理贴图 (旧式装备)

```
MTRL → 引用 _d.tex (漫反射颜色)
     → 引用 _n.tex (法线细节)
     → 引用 _s.tex (高光)
```

**特征:** 装备外观主要由 diffuse 纹理决定。染色通过修改纹理颜色实现。

**实例:** e0001~e0500 等早期装备，set_id 较小的装备。

### 模式二: ColorSet 着色 (Dawntrail 新式)

```
MTRL → 引用 _id.tex (色表索引, 4×4 很小)
     → 引用 _norm.tex (法线)
     → 引用 _mask.tex (遮罩)
     → 内嵌 ColorTable (16 行颜色/材质参数)
```

**特征:** 装备外观由 ColorTable + _id.tex 查表决定，不再需要大尺寸 diffuse 纹理。_id.tex 通常很小 (4×4)，指示每个像素使用 ColorTable 的哪一行。

**实例:** e0800+ 高版本装备、e6200+ Dawntrail 装备。这些装备在当前浏览器中显示为白色（因为未实现 ColorSet 渲染）。

### 模式三: 混合模式 (Dawntrail _base.tex)

```
MTRL → 引用 _base.tex (基色纹理)
     → 引用 _norm.tex (法线)
     → 引用 _mask.tex (遮罩)
```

**特征:** 使用新后缀命名但仍有独立的漫反射纹理 (`_base.tex`)。常见于 Dawntrail 中有复杂图案的装备。

**实例:** e6234 的部分材质有 `_base.tex` (如 `v09_c0201e6234_top_b_base.tex`, 1024×1024 DXT1)。

---

## 变体系统 (Variant)

### 变体 ID

从 Item 表 ModelMain 字段提取的 `variant_id` 决定材质使用的变体文件夹:

```
chara/equipment/e{set_id}/material/v{variant_id:04d}/...
```

### 变体含义

- 不同变体代表**同一装备的不同外观** (如不同染色预设、不同图案)
- 纹理路径中的 `v{xx}` 前缀也可能因变体而异:
  - `v01_c0201e6234_top_norm.tex` (变体无关，共用法线)
  - `v09_c0201e6234_top_b_base.tex` (变体 9 的漫反射)

### 变体回退

若 `v{variant_id}` 路径不存在，回退到 `v0001` (默认变体):

```rust
let candidates = if variant_id != 1 {
    vec![v{variant_id} 路径, v0001 路径]
} else {
    vec![v0001 路径]
};
```

---

## 身体材质 (b0001)

许多装备 MDL 的字符串表中包含 `mt_c{race}b0001_a.mtrl`，这是**角色身体皮肤材质**。

- 路径: `chara/human/c{race}/obj/body/b0001/material/v0001/mt_c{race}b0001_a.mtrl`
- **不在** `chara/equipment/` 路径下 — 按装备路径拼接会找不到
- 对应角色身体露出部分的皮肤渲染
- 在装备浏览器中使用白色回退是合理的

---

## 装备文件层级总览

```
Item EXD (装备列表)
 │  set_id, variant_id, slot
 │
 ▼
MDL  chara/equipment/e{set_id}/model/{race}e{set_id}_{slot}.mdl
 │   包含: 顶点数据, 索引数据, 材质名引用, LOD
 │
 ├── MTRL [0]  .../material/v{variant}/{material_name_a}.mtrl
 │   ├── TEX  _d.tex 或 _base.tex (漫反射)    ← 优先查找
 │   ├── TEX  _n.tex 或 _norm.tex (法线)       ← 非 diffuse，跳过
 │   ├── TEX  _s.tex 或 _mask.tex (遮罩)       ← 非 diffuse，跳过
 │   ├── TEX  _id.tex (染色索引)                ← 非 diffuse，跳过
 │   ├── ColorTable (16 行)
 │   └── SHPK (着色器包)
 │
 ├── MTRL [1]  .../material/v{variant}/{material_name_b}.mtrl
 │   └── ...
 │
 ├── MTRL [2]  mt_c{race}b0001_a.mtrl (身体皮肤，装备路径下不存在)
 │
 └── MTRL [3]  ...
```

### Diffuse 纹理查找优先级

```
1. 优先: 路径以 _d.tex 结尾 (旧式漫反射)
2. 次选: 路径包含 _base.tex (Dawntrail 漫反射)
3. 回退: 第一个非 _n/_s/_m/_norm/_mask/_id 的纹理
4. 失败: 使用 1×1 白色回退纹理
```

### 数据访问双库策略

```
读取请求
 ├── ironworks (优先) — 支持国服, 但不认识 Dawntrail v6 MDL
 └── physis (回退) — 支持 v6 格式, 但需修补 Region 枚举
```
