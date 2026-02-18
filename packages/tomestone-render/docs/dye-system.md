# FF14 染色系统

## 总览

FF14 装备的染色能力由每件装备的**材质文件 (MTRL)** 决定，而非由游戏版本统一切换。
同一版本中可以同时存在不可染色、单染色和双染色的装备。

```
                    MTRL 文件
                       │
          ┌────────────┼────────────┐
          │            │            │
     无 ColorTable   有 ColorTable  有 ColorTable
     或仅传统 diffuse  无 ColorDyeTable  有 ColorDyeTable
          │            │            │
       不可染色       不可染色      ┌──┴──┐
                                Legacy  Dawntrail
                                (u16)    (u32)
                                 │        │
                               单染色   单染色/双染色
                                      (取决于 channel)
```

### 判定逻辑 (代码层面)

| MTRL 内容 | `uses_color_table` | `color_dye_table` | 染色能力 |
|---|---|---|---|
| 仅传统 `_d.tex` / `_base.tex` | `false` | — | **不可染色** — 跳过整个染色流程 |
| 有 ColorTable，无 ColorDyeTable | `true` | `None` | **不可染色** — 有颜色行但无染色标记 |
| LegacyColorTable + LegacyColorDyeTable | `true` | `Some(Legacy)` | **单染色** — 16 行，1 个染料槽 |
| DawntrailColorTable + DawntrailColorDyeTable | `true` | `Some(Dawntrail)` | **单/双染色** — 32 行，按 `channel` 字段区分 |

---

## ColorTable (色表)

每个材质 (.mtrl) 可包含一个 ColorTable，每行定义一组颜色属性:

| 属性 | 类型 | 说明 |
|------|------|------|
| Diffuse | Half3 (RGB) | 基础颜色 |
| Specular | Half3 (RGB) | 高光色 |
| Emissive | Half3 (RGB) | 自发光色 |
| Gloss | Half1 (scalar) | 光泽度 |
| Specular Power | Half1 (scalar) | 高光强度 |

两代格式:

| 格式 | 行数 | 对应版本 |
|------|------|----------|
| **LegacyColorTable** | 16 行 | Endwalker 及之前 |
| **DawntrailColorTable** | 32 行 | Dawntrail (7.0+) |

---

## 行号映射: 像素 → ColorTable 行

每个像素需要一个行号来索引 ColorTable。两代系统的行号来源不同:

### Legacy: 法线贴图 Alpha 通道

Endwalker 及之前，colorset 行号编码在 `_n.tex`（法线贴图）的 Alpha 通道中。

### Dawntrail: 专用 `_id.tex`

7.0+ 使用独立的 `_id.tex` 纹理:

| 通道 | 用途 |
|------|------|
| R | colorset 行号 |
| G | A/B 变体选择 (0x00=B, 0xFF=A) |

行号映射公式:

| 格式 | 映射规则 | 行号范围 |
|------|----------|----------|
| Legacy (16 行) | `row = R / 17` | 0 ~ 15 |
| Dawntrail (32 行) | `row = R * 32 / 256` | 0 ~ 31 |

---

## ColorDyeTable (染色配置表)

与 ColorTable 行数一一对应，标记每一行是否可被染料影响。**如果 MTRL 中没有 ColorDyeTable，装备就不可染色。**

### Legacy 格式 (u16 位域)

| 位域 | 含义 |
|------|------|
| [15:5] | `template_id` — 指向 STM 模板 |
| bit 0 | diffuse 可染色 |
| bit 1 | specular 可染色 |
| bit 2 | emissive 可染色 |
| bit 3 | gloss 可染色 |
| bit 4 | specular_strength 可染色 |

所有标记了 `diffuse: true` 的行会被同一种染料影响 → **只有一个染料槽**。

### Dawntrail 格式 (u32 位域)

| 位域 | 含义 |
|------|------|
| [26:16] | `template_id` — 指向 STM 模板 |
| **[28:27]** | **`channel` (0-3)** — 染色通道 |
| bit 0 | diffuse 可染色 |
| bit 1 | specular 可染色 |
| bit 2 | emissive 可染色 |
| bit 3-11 | 其他 PBR 属性 |

`channel` 字段是双染色的关键:

| channel 值 | 含义 | 游戏中对应 |
|------------|------|------------|
| 0 | 染料通道 1 | 装备主要大面积区域 |
| 1 | 染料通道 2 | 次要细节 (纽扣、装饰等) |

**同一装备的不同 ColorDyeTable 行可以有不同的 `channel` 值**，意味着这些行可以被不同的染料独立影响。如果所有行的 channel 都是 0，那么即使使用 Dawntrail 格式，效果也等同于单染色。

---

## Stain、STM、ColorDyeTable 三者关系

染色系统涉及三个数据源，它们各自承担不同的职责：

```
Stain EXD                 ColorDyeTable (每件装备的 MTRL 内)       STM 文件
 "有哪些染料"               "这件装备怎么染"                        "染出来什么颜色"
┌──────────────┐          ┌──────────────────────┐              ┌────────────────────────┐
│ id: 1        │          │ 行[0] template=200   │              │ template=200:          │
│ name: 雪白   │          │       channel=0      │──┐           │   stain[0] → (0.80,    │
│ color: 白色块│          │       diffuse=true   │  │           │              0.78,     │
│ (仅 UI 预览) │          │ 行[1] template=200   │  │  查表     │              0.75)     │
├──────────────┤          │       channel=1      │  ├─────────→ │   stain[1] → (0.71,    │
│ id: 36       │          │       diffuse=true   │  │           │              0.70,     │
│ name: 煤黑   │          │ 行[2] template=100   │──┘           │              0.68)     │
│ color: 黑色块│          │       diffuse=false  │              │   ...                  │
│ (仅 UI 预览) │          │       (不可染色)      │              │                        │
├──────────────┤          └──────────────────────┘              │ template=100:          │
│ ...136 种    │                                                │   stain[0] → (0.90,    │
└──────────────┘                                                │              0.88,     │
       │                                                        │              0.92)     │
       │  提供 stain_id                                         │   ...                  │
       └──────────────────────────────────────────────────────→ │  (43 个 template       │
                                                                │   × 128 种染料)        │
                                                                └────────────────────────┘
```

关键点：**同一种染料在不同材质上产生不同的颜色**。

"雪白"(stain_id=1) 在布料材质 (template=200) 上可能偏暖白，在金属材质 (template=100) 上可能偏冷白。Stain EXD 只存一个 UI 预览色块，无法表达这种差异。STM 才是存储精确渲染参数的地方，它是一张 `template × stain → 颜色` 的二维查找表。

| 数据源 | 内容 | 数量级 | 用途 |
|--------|------|--------|------|
| **Stain EXD** | 染料 ID、名称、UI 预览色 | ~136 行 | 给玩家/UI 展示"有哪些染料可选" |
| **ColorDyeTable** | 每行的 template_id、channel、可染标志 | 16 或 32 行/材质 | 告诉引擎"这件装备的每个颜色区域用哪个模板、属于哪个染料通道" |
| **STM** | template × stain → diffuse/specular/emissive/gloss 的精确值 | 43 template × 128 stain | 查表得到最终渲染颜色 |

### 完整查询路径

```
用户选择 "雪白" (stain_id=1)
  ↓
装备 MTRL 的 ColorDyeTable 第 3 行: template_id=200, diffuse=true
  ↓
STM.get_dye_pack(template=200, stain_index=0)
  → DyePack { diffuse: (0.80, 0.78, 0.75), specular: ..., ... }
  ↓
用这个 diffuse 替换 ColorTable 第 3 行的颜色 → 重新烘焙纹理
```

---

## Stain 染料数据

存储在 Stain EXD 表中，约 136 种染料。仅提供染料的元信息和 UI 预览色，不包含实际渲染颜色。

| 字段 | 类型 | 说明 |
|------|------|------|
| row_id | u32 | 染料 ID (1-based，0 = 无染料) |
| name | String | 染料名称 |
| color | [u8; 3] | RGB 预览色 (**仅用于 UI 色块显示，非实际渲染色**) |
| shade | u8 | 色调分类 |
| is_metallic | bool | 是否为金属染料 |

---

## STM (StainingTemplate) 染色模板

STM 是染色系统的核心数据：一张 **template_id × stain_index** 的二维查找表，存储每种材质模板在每种染料下的精确渲染参数。

### 为什么需要 STM？

同一种染料涂在不同材质上，颜色是不一样的。STM 文件为每种 (template, stain) 组合预计算了 diffuse、specular、emissive、gloss 等属性值。template_id 可以理解为"材质类型"（如布料、皮革、金属），由装备 MTRL 的 ColorDyeTable 每行指定。

### 两个 STM 文件

| 文件 | 格式 | 子表数 |
|------|------|--------|
| `chara/base_material/stainingtemplate.stm` | Endwalker | 5 (diffuse, specular, emissive, gloss, specular_power) |
| `chara/base_material/stainingtemplate_gud.stm` | Dawntrail | 12 (扩展 PBR 属性) |

当前实现使用 Endwalker STM 文件（覆盖所有模板 ID）。

参考实现: [TexTools STM.cs](https://github.com/TexTools/xivModdingFramework/blob/master/xivModdingFramework/Materials/FileTypes/STM.cs)

### 查询接口

```
stm.get_dye_pack(template_id, stain_index) → Option<DyePack>
```

DyePack 包含:
- `diffuse: [f32; 3]` — 染色后的 diffuse 颜色 (linear RGB)
- `specular: [f32; 3]` — 染色后的高光色
- `emissive: [f32; 3]` — 染色后的自发光色
- `gloss: f32` — 染色后的光泽度
- `specular_power: f32` — 染色后的高光强度

### Dawntrail 模板 ID 映射

Dawntrail 材质中的 `template_id >= 1000` (如 1200, 1500)，需要减去 1000 再查 STM:

```
template_id=1200 → 查找 STM key=200
```

### 文件头

```
偏移  大小  类型   说明
0x00  2     u16    magic
0x02  2     u16    version (0x0101 = new format)
0x04  2     u16    entry_count — 模板条目数 (实测 43)
0x06  2     u16    unknown
```

### 新旧格式检测

检查 `data[0x0A]` 和 `data[0x0B]`（第一个 key 条目偏移 +2 的位置）:
- 若 `data[0x0A] != 0 || data[0x0B] != 0` → **旧格式** (u16 keys/offsets, numDyes=128)
- 否则 → **新格式** (u32 keys/offsets, numDyes=254)

当前游戏文件为**新格式**。

### Keys 和 Offsets

旧格式:
```
0x08        u16 × N    template_ids
0x08+2N     u16 × N    offsets (半字单位)
```

新格式:
```
0x08        u32 × N    template_ids
0x08+4N     u32 × N    offsets (半字单位)
```

数据区起始: `data_base = 8 + (key_size + offset_size) * entry_count`

### 条目定位

```
entry_absolute_offset = data_base + offsets[i] * 2
```

### 条目结构

每个条目开头是 5 个 u16 累积端点偏移 (各 ×2 转为字节偏移):

```
偏移           大小  说明
entry+0x00     2     ends[0] — diffuse 子表结束偏移 (半字)
entry+0x02     2     ends[1] — specular 子表结束偏移
entry+0x04     2     ends[2] — emissive 子表结束偏移
entry+0x06     2     ends[3] — gloss 子表结束偏移
entry+0x08     2     ends[4] — specular_power 子表结束偏移
entry+0x0A     ...   子表数据开始 (data_start)
```

5 个子表的字节范围:

| 子表 | 元素类型 | 起始偏移 | 字节大小 |
|------|----------|----------|----------|
| diffuse | Half3 | data_start | ends[0] × 2 |
| specular | Half3 | data_start+ends[0]×2 | ends[1]×2 - ends[0]×2 |
| emissive | Half3 | data_start+ends[1]×2 | ends[2]×2 - ends[1]×2 |
| gloss | Half1 | data_start+ends[2]×2 | ends[3]×2 - ends[2]×2 |
| specular_power | Half1 | data_start+ends[3]×2 | ends[4]×2 - ends[3]×2 |

### 子表编码

三种编码模式 (令 `array_size = sub_size / sizeof(T)`):

**1. Singleton** (`array_size == 1`):
单个值，复制给所有 numDyes 个染料。

**2. OneToOne** (`array_size >= numDyes`):
直接存储 numDyes 个值，一一对应。

**3. Indexed** (`1 < array_size < numDyes`):
```
[palette: T × P]           — P 个调色板值
[marker: u8 × 1]           — 标记字节 (0xFF)，跳过
[indices: u8 × (numDyes-1)]— 索引，1-based
```

其中 `P = (sub_size - numDyes) / sizeof(T)`

索引解读 (1-based):
- index == 0 或 index == 255 → 使用默认值 (零)
- 否则 → palette[index - 1]
- 最后一个染料条目强制为默认值

---

## 渲染: 两种 Diffuse 来源

### 传统 Diffuse 纹理

旧式装备有 `_d.tex` 或 `_base.tex` 作为 diffuse 纹理，直接采样获得基础颜色。
这类装备的 `uses_color_table = false`，染色流程完全不经过 ColorTable。

### ColorTable + _id.tex 烘焙

没有传统 diffuse 的装备 (常见于 e0800+) 用 ColorTable 查表着色:

```
1. 从 MTRL 的 texture_paths 中找 _id.tex
2. 读取 _id.tex 每个像素的 R 通道
3. R 映射到 ColorTable 行号 (映射规则见上文)
4. 取该行的 diffuse_color
5. linear → sRGB 转换
6. 输出为 RGBA 纹理
```

### CPU 端烘焙策略

`_id.tex` 通常只有 4x4 像素。在 CPU 端完成 ColorTable 查表，烘焙出一张小的伪 diffuse 纹理，shader 完全不需要改动。染色时只需重新烘焙并上传 GPU，开销极小。

---

## 染色应用流程

```
用户选择 stain_id (1-based, Stain EXD 表的 row_id)

对 ColorTable 每一行 i:
  1. 读取 ColorDyeTable[i]
  2. 检查 diffuse 标志是否为 true
  3. 若可染色:
     template_id = ColorDyeTable[i].template
     // Dawntrail 模板映射: >= 1000 时减去 1000
     stm_key = template_id >= 1000 ? template_id - 1000 : template_id
     stain_index = stain_id - 1   (转为 0-based)
     dyed_color = STM.entries[stm_key].diffuse[stain_index]
  4. 若不可染色:
     dyed_color = ColorTable[i].diffuse_color (保持原色)

用 dyed_color 数组重新烘焙 _id.tex → 伪 diffuse 纹理 → 上传 GPU
```

### 双染色的应用 (尚未实现)

当前实现忽略了 Dawntrail ColorDyeTable 的 `channel` 字段，所有行使用同一个 `stain_id`。

完整的双染色支持需要:

```
用户选择 stain_id_1 (通道 1 的染料) 和 stain_id_2 (通道 2 的染料)

对 ColorTable 每一行 i:
  1. 读取 ColorDyeTable[i]
  2. 若 diffuse == true:
     根据 channel 选择对应的 stain_id:
       channel == 0 → 使用 stain_id_1
       channel == 1 → 使用 stain_id_2
     用选定的 stain_id 查 STM 获取颜色
  3. 若 diffuse == false:
     保持原色
```

---

## 着色器

| 着色器包 | 版本 | 说明 |
|----------|------|------|
| `characterlegacy.shpk` | Endwalker 及之前 | 旧版角色着色器 |
| `character.shpk` | Dawntrail (7.0+) | 新版，支持天球贴图/金属/玻璃/全息 |

当前实现使用 CPU 烘焙，无需修改 shader。
