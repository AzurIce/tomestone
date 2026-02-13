# FF14 染色系统

## 核心概念: Colorset

每个材质 (.mtrl) 包含一个 ColorTable，每行定义一组颜色属性:

- **Diffuse** — 基础颜色 (Half3, RGB)
- **Specular** — 高光色 (Half3, RGB)
- **Emissive** — 自发光色 (Half3, RGB)
- **Gloss** — 光泽度 (Half1, scalar)
- **Specular Power** — 高光强度 (Half1, scalar)

Legacy 材质 16 行，Dawntrail 材质 32 行。

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

## 两种渲染模式

### 传统 Diffuse 纹理

旧式装备（以及部分 Dawntrail 装备）有 `_d.tex` 或 `_base.tex` 作为 diffuse 纹理，
直接采样即可获得基础颜色。

### ColorTable + _id.tex 模式

Dawntrail 新式装备 (e0800+) 没有传统 diffuse，而是用 ColorTable 查表着色：

1. 从 MTRL 的 `texture_paths` 中找 `_id.tex`
2. 读取 `_id.tex` 每个像素的 R 通道
3. R 映射到 ColorTable 行号：
   - Legacy 16 行: `row = R / 17` (clamp 0..15)
   - Dawntrail 32 行: `row = R * 32 / 256` (clamp 0..31)
4. 取该行的 `diffuse_color`，做 linear → sRGB 转换
5. 输出为 RGBA 纹理

### CPU 端烘焙策略

`_id.tex` 通常只有 4x4 像素。我们在 CPU 端完成 ColorTable 查表，烘焙出一张小的伪 diffuse 纹理，
shader 完全不需要改动。染色时只需重新烘焙并上传 GPU，开销极小。

## Stain 染料数据

- 存储在 Stain EXD 表中，约 136 种染料
- 每种染料有 ID、名称、RGB 颜色值

## ColorDyeTable

每个 MTRL 可包含一个 ColorDyeTable，与 ColorTable 行数一一对应。
每行编码为位域：

### Legacy (u16)

| 位域      | 含义            |
|-----------|-----------------|
| [15:5]    | template_id     |
| bit 0     | diffuse 可染色  |
| bit 1     | specular 可染色 |
| bit 2     | emissive 可染色 |
| bit 3     | gloss 可染色    |
| bit 4     | specular_strength 可染色 |

### Dawntrail (u32)

| 位域      | 含义            |
|-----------|-----------------|
| [26:16]   | template_id     |
| [28:27]   | channel (0-3)   |
| bit 0     | diffuse         |
| bit 1     | specular        |
| bit 2     | emissive        |
| bit 3-11  | 其他 PBR 属性   |

## STM 文件二进制格式

有两个 STM 文件:
- `chara/base_material/stainingtemplate.stm` — Endwalker 格式，5 个子表
- `chara/base_material/stainingtemplate_gud.stm` — Dawntrail 格式，12 个子表

当前实现使用 Endwalker STM 文件（覆盖所有模板 ID）。

参考实现: [TexTools STM.cs](https://github.com/TexTools/xivModdingFramework/blob/master/xivModdingFramework/Materials/FileTypes/STM.cs)

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

| 子表           | 元素类型 | 起始偏移            | 字节大小              |
|----------------|----------|---------------------|-----------------------|
| diffuse        | Half3    | data_start          | ends[0] × 2          |
| specular       | Half3    | data_start+ends[0]×2| ends[1]×2 - ends[0]×2|
| emissive       | Half3    | data_start+ends[1]×2| ends[2]×2 - ends[1]×2|
| gloss          | Half1    | data_start+ends[2]×2| ends[3]×2 - ends[2]×2|
| specular_power | Half1    | data_start+ends[3]×2| ends[4]×2 - ends[3]×2|

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

### Dawntrail 模板 ID 映射

Dawntrail 材质中的 template_id >= 1000 (如 1200, 1500)，
映射方式: `template_id - 1000` → 对应 STM 中的 key。

例: template_id=1200 → 查找 STM key=200。

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

## 着色器

- `characterlegacy.shpk` — 旧版角色着色器
- `character.shpk` — Dawntrail 新版 (支持天球贴图/金属/玻璃/全息)

当前实现使用 CPU 烘焙，无需修改 shader。
