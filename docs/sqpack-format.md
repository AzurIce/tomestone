# SqPack 容器格式

FF14 所有游戏资源打包在 SqPack 容器中，位于游戏安装目录 `game/sqpack/` 下。

## 目录结构

```
game/sqpack/
├── ffxiv/          ← 基础游戏 (A Realm Reborn)
│   ├── 000000.win32.index
│   ├── 000000.win32.index2
│   ├── 000000.win32.dat0
│   ├── 040000.win32.index
│   ├── 040000.win32.dat0
│   ├── 040000.win32.dat1
│   ├── ...
│   └── 130000.win32.dat0
├── ex1/            ← Heavensward
│   ├── 020100.win32.index
│   ├── ...
│   └── ex1.ver
├── ex2/            ← Stormblood
├── ex3/            ← Shadowbringers
├── ex4/            ← Endwalker
└── ex5/            ← Dawntrail
    ├── 020500.win32.index
    ├── 020501.win32.index   ← 同分类的第 2 块
    ├── ...
    └── ex5.ver
```

每个仓库目录包含三种文件: `.index` / `.index2`（哈希索引）、`.dat0` `.dat1` ...（数据）、`.ver`（版本号）。

---

## 文件命名规则

所有文件遵循 `CCEERR.win32.{ext}` 的命名模式:

```
CC   = 分类 ID (2 位十六进制)
EE   = 扩展包 ID (00=基础, 01=ex1, 02=ex2, ..., 05=ex5)
RR   = 块号 (同一分类内的分片编号, 通常 00)
```

示例:
- `040000.win32.index` — 基础游戏 chara 分类, 块 0
- `020501.win32.dat0` — ex5 bg 分类, 块 1 的第一个数据文件

---

## 分类 ID 一览

| ID (hex) | 名称 | 内容 |
|----------|------|------|
| `00` | common | 公共资源 |
| `01` | bgcommon | 场景公共资源 |
| `02` | bg | 场景地图 |
| `03` | cut | 过场动画 |
| `04` | chara | 角色模型、装备、纹理 |
| `05` | shader | 着色器包 |
| `06` | ui | 用户界面 |
| `07` | sound | 音效 |
| `08` | vfx | 特效 |
| `09` | ui_script | UI 脚本 |
| `0a` | exd | Excel 数据表 |
| `0b` | game_script | 游戏脚本 |
| `0c` | music | 音乐 |
| `12` | _debug | 调试资源 |
| `13` | _sqpack_test | 测试数据 |

---

## .index 文件（哈希索引）

SqPack 使用 CRC32 哈希索引来定位文件。索引文件不存储原始文件路径，只存储路径的哈希值。

### 文件头 (SqPack Header)

每个 `.index` 文件以 1024 字节 (0x400) 的 SqPack 头开始:

```
偏移    大小    类型     说明
0x00    8       bytes   签名 "SqPack\0\0"
0x08    1       u8      平台 ID (0=PC)
0x09    3       bytes   填充
0x0C    4       u32     头大小 (= 1024)
0x10    4       u32     版本 (= 1)
0x14    4       u32     类型 (2=Index)
0x18    ...             保留/填充至 0x400
```

### 索引头 (Index Header)

紧跟 SqPack 头之后，偏移 0x400 处:

```
偏移    大小    类型     说明
0x00    4       u32     索引头大小 (= 1024)
0x04    4       u32     版本
0x08    4       u32     哈希表偏移 (文件内绝对偏移)
0x0C    4       u32     哈希表大小 (字节)
0x10    ...             保留
```

条目数 = `哈希表大小 / 16` (每条目 16 字节)

### 哈希表条目 (IndexHashTableEntry)

`.index` 文件的每个条目 16 字节:

```
偏移    大小    类型     说明
0x00    4       u32     文件名哈希 (CRC32 of filename)
0x04    4       u32     文件夹哈希 (CRC32 of folder path)
0x08    4       u32     数据定位 (bit-packed, 见下文)
0x0C    4       u32     填充
```

**数据定位字段 (u32) 的位布局:**

```
Bit 0       : 未知标志
Bit 1-3     : dat 文件编号 (0→dat0, 1→dat1, 2→dat2, ...)
Bit 4-31    : 偏移值 (实际字节偏移 = 该值 × 8)
```

---

## .index2 文件（全路径哈希索引）

`.index2` 与 `.index` 结构相同，但哈希方式不同:

- `.index`: 分别哈希文件夹路径和文件名 (两个 u32)
- `.index2`: 哈希完整路径 (一个 u32 存入 u64 字段)

`.index2` 每条目 8 字节:

```
偏移    大小    类型     说明
0x00    4       u32     完整路径哈希
0x04    4       u32     数据定位 (同 .index 的位布局)
```

条目数 = `哈希表大小 / 8`

两种索引文件引用相同的 `.dat` 数据文件，只是查找方式不同。

---

## 哈希算法

SqPack 使用 **CRC-32/JAMCRC** 变体:

| 参数 | 值 |
|------|-----|
| 多项式 | `0x04C11DB7` |
| 初始值 | `0xFFFFFFFF` |
| 输入反射 | 是 |
| 输出反射 | 是 |
| 最终异或 | `0x00000000` |

JAMCRC = `NOT(标准 CRC32)`, 即标准 CRC32 取反。

### 路径哈希计算

**.index 查找:**

```
路径: "chara/equipment/e0005/model/c0201e0005_top.mdl"
  ↓ 在最后一个 "/" 处分割
文件夹: "chara/equipment/e0005/model"  → folder_hash = CRC32("chara/equipment/e0005/model")
文件名: "c0201e0005_top.mdl"          → file_hash   = CRC32("c0201e0005_top.mdl")
```

**.index2 查找:**

```
路径: "chara/equipment/e0005/model/c0201e0005_top.mdl"
  → full_hash = CRC32("chara/equipment/e0005/model/c0201e0005_top.mdl")
```

**重要限制**: CRC32 哈希不可逆，因此无法从索引反向枚举出文件路径列表。

---

## .dat 文件（数据）

`.dat0`, `.dat1`, `.dat2` ... 存储实际的文件数据。当单个 `.dat` 文件超过约 2GB 时自动拆分为多个文件，由索引条目中的 dat 文件编号 (bit 1-3) 指定使用哪个。

### 数据块 (Data Block)

给定索引条目的偏移值，在 `.dat` 文件中读取:

```
偏移    大小    类型     说明
0x00    4       u32     头大小 (通常 0x80 = 128)
0x04    4       u32     内容类型
0x08    4       u32     未压缩大小
0x0C    4       u32     _未知
0x10    4       u32     块数量
0x14    4       u32     _未知
0x18    ...             各数据块的尺寸表
```

当只有 1 个块时 (小文件), 紧跟头部之后就是压缩数据。
当有多个块时, 每块独立 zlib 压缩, 块大小记录在头部的尺寸表中。

### 块级压缩

每个数据块有自己的子头:

```
偏移    大小    类型     说明
0x00    4       u32     子头大小 (= 0x10)
0x04    4       u32     _未知
0x08    4       u32     压缩后大小 (= 32000 表示未压缩)
0x0C    4       u32     未压缩大小
```

- 如果 `压缩后大小 == 32000`: 数据未压缩, 直接读取 `未压缩大小` 字节
- 否则: 数据使用 zlib (raw deflate) 压缩, 读取 `压缩后大小` 字节后解压

### 读取流程

```
1. 从 .index 查找到条目 → 得到 dat_id 和 offset
2. 打开对应的 .dat{dat_id} 文件
3. Seek 到 offset 位置
4. 读取数据块头 (128 字节)
5. 按块数量依次读取和解压每个块
6. 拼接所有块的解压数据 → 得到原始文件
```

---

## .ver 文件（版本号）

每个扩展包仓库 (ex1-ex5) 包含一个 `.ver` 文件, 记录该仓库的版本号。
基础游戏 (ffxiv) 目录下没有 `.ver` 文件。

格式为纯文本, 内容是形如 `YYYY.MM.DD.XXXX.XXXX` 的版本字符串:

```
ex5.ver → "2026.01.30.0000.0000"
ex1.ver → "2026.01.13.0000.0000"
```

游戏客户端使用此版本号判断补丁状态, 决定是否需要下载更新。

---

## 完整的文件访问路径

```
用户请求: "chara/equipment/e0005/model/c0201e0005_top.mdl"
     │
     ▼
① 解析路径 → 确定仓库 (ffxiv) 和分类 (chara = 0x04)
     │
     ▼
② 定位索引文件 → ffxiv/040000.win32.index
     │
     ▼
③ 计算哈希:
   folder_hash = CRC32("chara/equipment/e0005/model")
   file_hash   = CRC32("c0201e0005_top.mdl")
     │
     ▼
④ 在哈希表中查找匹配条目
     │
     ▼
⑤ 解析数据定位: dat_id=2, offset=0x00FD21D0
     │
     ▼
⑥ 打开 ffxiv/040000.win32.dat2, Seek 到 offset
     │
     ▼
⑦ 读取数据块头 + 解压各块
     │
     ▼
⑧ 返回原始 .mdl 文件数据
```

---

## EXD 数据表系统 (Excel Data)

游戏的结构化数据 (物品、技能、地图名等) 使用类似电子表格的二进制格式存储，统称 Excel / EXD 系统。存放在 SqPack 的 `exd` 分类 (0x0a) 中。

由三种文件组成：

```
exd/root.exl          ← 表名列表 (纯文本)
exd/item.exh          ← Item 表头 (列定义、分页、语言)
exd/item_0_chs.exd    ← Item 表数据 (第 0 页, 简体中文)
exd/item_10000_chs.exd
exd/stain.exh
exd/stain_0_chs.exd
...
```

### .exl — 表名列表

纯文本文件，位于 `exd/root.exl`。第一行是固定头 `EXLT,2`，之后每行一个表：

```
EXLT,2
Achievement,209
Action,4
Item,36
Stain,83
content/DeepDungeon2Achievement,-1
```

格式: `表名,ID`。ID 为不可变编号，`-1` 表示无固定 ID。
表名转小写后加 `.exh` 后缀即为表头文件路径: `Item` → `exd/item.exh`

### .exh — 表头 (Excel Header)

定义表的列类型、分页方式和可用语言。**整个文件使用大端序 (Big Endian)**。

#### 文件头

```
偏移    大小    类型     说明
0x00    4       bytes   签名 "EXHF"
0x04    2       u16     版本
0x06    2       u16     dataOffset (固定长度数据区大小, 用于定位字符串)
0x08    2       u16     columnCount (列数)
0x0A    2       u16     pageCount (分页数)
0x0C    2       u16     languageCount (语言数)
0x0E    2       u16     _保留
0x10    1       u8      _保留
0x11    1       u8      variant (1=Default, 2=SubRows)
0x12    2       u16     _保留
0x14    4       u32     rowCount (总行数)
0x18    8       bytes   _保留
```

#### 列定义 (紧跟文件头, 共 columnCount 个)

每个 4 字节:

```
偏移    大小    类型     说明
0x00    2       u16     type (列数据类型)
0x02    2       u16     offset (该列在行数据中的字节偏移)
```

列类型:

| 值 | 类型 | 说明 |
|----|------|------|
| 0x00 | String | 字符串 (存储为 u32 偏移量) |
| 0x01 | Bool | 布尔 |
| 0x02 | Int8 | |
| 0x03 | UInt8 | |
| 0x04 | Int16 | |
| 0x05 | UInt16 | |
| 0x06 | Int32 | |
| 0x07 | UInt32 | |
| 0x09 | Float32 | |
| 0x0A | Int64 | |
| 0x0B | UInt64 | |
| 0x19-0x20 | PackedBool0-7 | 位压缩布尔 (bit 0-7) |

#### 分页定义 (紧跟列定义, 共 pageCount 个)

每个 8 字节:

```
偏移    大小    类型     说明
0x00    4       u32     startId (该页起始行 ID)
0x04    4       u32     rowCount (该页行数)
```

#### 语言列表 (紧跟分页定义, 共 languageCount 个)

每个 2 字节 (u16):

| 值 | 语言 | 后缀 |
|----|------|------|
| 0 | None | (无后缀) |
| 1 | Japanese | `ja` |
| 2 | English | `en` |
| 3 | German | `de` |
| 4 | French | `fr` |
| 5 | ChineseSimplified | `chs` |
| 6 | ChineseTraditional | `cht` |
| 7 | Korean | `ko` |

### 数据文件路径生成

根据表头中的分页和语言信息构造 `.exd` 文件路径:

- 有语言: `exd/<name>_<startId>_<lang>.exd`
- 无语言: `exd/<name>_<startId>.exd`

示例:

| 表 | 起始 ID | 语言 | 路径 |
|----|---------|------|------|
| Item | 0 | chs | `exd/item_0_chs.exd` |
| Item | 10000 | en | `exd/item_10000_en.exd` |
| Stain | 0 | chs | `exd/stain_0_chs.exd` |

### .exd — 表数据 (Excel Data)

存储一个分页的实际行数据。**同样使用大端序**。

#### 文件头

```
偏移    大小    类型     说明
0x00    4       bytes   签名 "EXDF"
0x04    2       u16     版本
0x06    2       u16     _保留
0x08    4       u32     indexSize (行偏移表的总字节大小)
0x0C    20      bytes   _保留
```

#### 行偏移表 (紧跟文件头, 共 indexSize/8 个)

每个 8 字节:

```
偏移    大小    类型     说明
0x00    4       u32     rowId (行 ID, 绝对值)
0x04    4       u32     offset (该行数据在文件中的绝对偏移)
```

#### 行头 (在 offset 位置)

```
偏移    大小    类型     说明
0x00    4       u32     dataSize (行数据总大小)
0x02    2       u16     rowCount (子行数, variant=Default 时固定为 1)
```

行头之后紧跟实际的列数据，按 .exh 中定义的列偏移读取。

#### 字符串列的读取

字符串不内联存储。列位置存的是一个 u32 偏移量，实际字符串位于:

```
字符串地址 = 行数据起始 + exh.dataOffset + 列中的u32值
```

字符串以 null 结尾。

### 路径发现机制

EXD 系统是 SqPack 中**唯一能主动发现文件路径**的入口:

```
root.exl (表名列表, 纯文本)
  → 枚举所有已知的表名
    → 读取 .exh 获取列定义
      → 读取 .exd 获取行数据
        → 从特定列提取路径信息:
           Item 表 → set_id + slot → 构造 MDL/MTRL/TEX 路径
           ModelChara 表 → 怪物/NPC 模型路径
           ...
```

这就是为什么当前 tomestone 能显示文件路径: 从 EXD 表获取元数据，按已知模式拼接路径，再通过 SqPack 哈希查找访问实际文件。

---

## 国服兼容性

SqPack 头部偏移 0x20 处有 `Region` 字段:
- `0xFFFFFFFF` = 国际服 (Global)
- `0x01` = KoreaChina
- `0x00` = 国服实际使用的值

某些解析库 (如早期 physis) 的 Region 枚举缺少 `0` 值导致无法解析国服数据。
