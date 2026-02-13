# FF14 MDL 顶点数据格式参考

## 概述

FF14 的 3D 模型使用 `.mdl` 文件格式。每个 mesh 通过 **顶点声明 (Vertex Declaration)** 描述其顶点布局，每个声明包含最多 17 个 **顶点元素 (Vertex Element)**。

## 顶点元素结构

每个元素 8 字节：

| 字段 | 大小 | 说明 |
|------|------|------|
| stream | u8 | 顶点缓冲区流索引 (0-2)，0xFF 表示结束 |
| offset | u8 | 在该流中的字节偏移 |
| format | u8 | 数据格式（见下表） |
| usage | u8 | 语义用途（见下表） |
| usage_index | u8 | 用途索引（区分同 usage 的多个元素） |
| padding | 3 bytes | 填充 |

## 数据格式 (format)

| 值 | 名称 | 大小 | 说明 |
|----|------|------|------|
| 1 | Single2 | 8B | 2x float32 |
| 2 | Single3 | 12B | 3x float32 |
| 3 | Single4 | 16B | 4x float32 |
| 5 | UByte4 | 4B | 4x uint8 (原始整数) |
| 8 | ByteFloat4 | 4B | 4x uint8 归一化到 [0,1]（除以 255） |
| 13 | Half2 | 4B | 2x float16 |
| 14 | Half4 | 8B | 4x float16 |

## 语义用途 (usage)

| 值 | 名称 | 典型格式 | 说明 |
|----|------|----------|------|
| 0 | Position | Single3 / Single4 / Half4 | 顶点位置 (XYZ) |
| 1 | BlendWeight | ByteFloat4 | 骨骼蒙皮权重 |
| 2 | BlendIndex | UByte4 | 骨骼索引 |
| 3 | Normal | Half4 / ByteFloat4 / Single3 | 法线方向 |
| 4 | UV | Half2 / Half4 / Single2 | 纹理坐标 |
| 5 | Tangent2 | Half4 / ByteFloat4 | 切线（副切线相关） |
| 6 | Tangent1 | Half4 / ByteFloat4 | 切线 |
| 7 | Color | ByteFloat4 | 顶点颜色（见下文详解） |

## 顶点颜色详解 (usage=7)

**顶点颜色不是可视颜色，而是着色器使用的元数据。** 含义取决于着色器类型和 shader key 配置。

### Character.shpk (装备模型着色器, Dawntrail)

受 shader key `F52CCF05` 控制，有两种模式：

#### MASK 模式（默认）— Vertex Color 1 (usage_index=0)

| 通道 | 用途 |
|------|------|
| R | Specular Mask（高光遮罩）|
| G | Roughness（粗糙度）|
| B | Diffuse Mask（漫反射遮罩）|
| A | Opacity（不透明度）|

#### COLOR 模式

Vertex Color 1 直接作为漫反射颜色使用。

#### Vertex Color 2 (usage_index=1)

| 通道 | 用途 |
|------|------|
| R | Faux-Wind Influence（伪风力影响）|
| G | Faux-Wind Multiplier（伪风力乘数）|
| B/A | 未知 |

### 其他着色器

| 着色器 | R | G | B | A |
|--------|---|---|---|---|
| Skin.shpk | Muscle Slider Influence | — | — | Shadow On/Off |
| Hair.shpk (VC2) | Faux-Wind + Anisotropy | — | — | Shadow On/Off |
| Iris.shpk | Left Eye Color | Right Eye Color | — | — |

## 典型装备顶点布局示例

Stream 0 (几何数据):
```
offset=0   Position    Half4     (8 bytes)
offset=8   BlendWeight ByteFloat4 (4 bytes)
offset=12  BlendIndex  UByte4    (4 bytes)
```

Stream 1 (属性数据):
```
offset=0   Normal      Half4     (8 bytes)
offset=8   UV          Half4     (8 bytes)
offset=16  Color       ByteFloat4 (4 bytes)
offset=20  Tangent1    Half4     (8 bytes)
```

> 实际布局因模型而异，必须读取顶点声明来确定。

## 纹理管线

模型外观主要由纹理决定，而非顶点颜色：

```
MDL (模型) → MTRL (材质) → TEX (纹理)
```

路径模式：
```
模型: chara/equipment/e{id}/model/c{race}e{id}_{slot}.mdl
材质: chara/equipment/e{id}/material/v{variant}/mt_c{race}e{id}_{letter}.mtrl
纹理: chara/equipment/e{id}/texture/...  (由 mtrl 中的采样器引用)
```

### 纹理类型 (Character.shpk)

| 后缀 | 类型 | R | G | B | A |
|------|------|---|---|---|---|
| `_d` | Diffuse | 颜色 R | 颜色 G | 颜色 B | — |
| `_n` | Normal | 法线 X | 法线 Y | Opacity | — |
| `_m` | Mask | Specular Power | Roughness | AO | — |
| `_s` | Specular | 颜色 R | 颜色 G | 颜色 B | — |
| `_id` | Index/ID | Colorset Pair | Even/Odd Blend | — | — |

### .tex 文件压缩格式

| 格式 | ID | 说明 |
|------|----|------|
| DXT1 (BC1) | 13344 | 4bpp, 无 Alpha, 常用于不透明 Diffuse |
| DXT3 (BC2) | 13360 | 8bpp, 带 Alpha |
| DXT5 (BC3) | 13361 | 8bpp, 常用于法线贴图 |
| BC5 | — | 双通道法线压缩（ironworks 不直接支持） |
| BC7 | — | 高质量新格式（Dawntrail 新资产可能使用） |
| ARGB8 | 5200 | 32-bit 未压缩 |
| RGBA8 | 17409 | 32-bit 未压缩 |
