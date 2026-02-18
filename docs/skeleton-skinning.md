# CPU 端骨骼蒙皮实现

## 问题背景

FF14 的装备模型以种族码（race code）区分体型，例如 `c0201`（Hyur Midlander）、`c0401`（Hyur Highlander）等。每件装备不一定对所有种族码都有独立模型文件——游戏引擎通过 **骨骼蒙皮** 将少数几套模型适配到所有体型。

在幻化编辑器中，当多个槽位的装备来自不同种族码时（例如身体来自 `c0201`，手套只有 `c0401` 的模型），渲染时部件之间会产生错位。解决方案是在 CPU 端做一次性的骨骼姿态重映射。

## 数据流总览

```
┌─────────────┐     ┌──────────────┐     ┌────────────────┐
│  .mdl 文件   │     │  .sklb 文件   │     │  glamour_editor │
│             │     │              │     │                │
│ · 顶点位置   │     │ · 骨骼层级    │     │ 合并多槽位模型   │
│ · 法线/切线  │     │ · 父子关系    │     │ 检测种族码差异   │
│ · 混合权重   │     │ · 局部变换    │     │ 调用 apply_     │
│ · 混合索引   │     │   (pos/rot/  │     │   skinning()   │
│ · 骨骼名称表 │     │    scale)    │     │                │
│ · 骨骼表     │     └──────┬───────┘     └───────┬────────┘
└──────┬──────┘            │                      │
       │                   ▼                      │
       │         compute_bind_pose_matrices()     │
       │           → 世界空间绑定姿态矩阵           │
       │             (bone_name → Mat4)           │
       │                                          │
       └──────────────────────────────────────────┘
```

## 第一步：MDL 文件中的骨骼数据

MDL 文件包含三层骨骼相关数据：

### 1.1 每顶点蒙皮数据 (Vertex Skinning Data)

存储在顶点缓冲区中，每个顶点有：

- **BlendWeight** (`usage=1, format=ByteFloat4`)：4 个混合权重，每个为 `u8 / 255.0`，表示该顶点受 4 根骨骼影响的程度
- **BlendIndex** (`usage=2, format=Byte4`)：4 个骨骼索引（u8），指向该网格的 **骨骼表**

```
顶点 k:
  blend_weights = [0.7, 0.2, 0.1, 0.0]   // 受 3 根骨骼影响
  blend_indices = [0, 3, 5, 0]             // 指向骨骼表中的第 0、3、5 项
```

### 1.2 骨骼表 (Bone Table)

每个网格关联一个骨骼表（通过 `mesh.bone_table_index`）。骨骼表是一个 `u16` 索引数组，将顶点的本地骨骼索引映射到模型全局的骨骼名称数组索引：

```
bone_table[0] = 12   // 本地索引 0 → 全局骨骼 #12
bone_table[3] = 45   // 本地索引 3 → 全局骨骼 #45
bone_table[5] = 7    // 本地索引 5 → 全局骨骼 #7
```

骨骼表有两种格式：
- **V5**（`version ≤ 0x1000005`）：固定 132 字节 = `[u16; 64]` + `u8 count` + 3 字节 padding
- **V6**（`version ≥ 0x1000006`，Dawntrail）：可变长度，先读 `(offset, count)` 对，再按 count 读取 u16 索引，4 字节对齐

### 1.3 骨骼名称数组 (Bone Names)

MDL 文件的字符串块中存储了骨骼名称，通过 `bone_name_offsets` 数组按偏移查找。例如：

```
bone_names[12] = "j_kosi"        // 腰部
bone_names[45] = "j_ude_a_r"     // 右上臂
bone_names[7]  = "j_sebo_a"      // 脊柱 A
```

### 索引解析链

```
顶点 blend_index[i]
    → bone_table.bone_indices[blend_index]
    → bone_names[global_index]
    → "j_kosi" (骨骼名称)
```

## 第二步：骨骼文件与绑定姿态

### 2.1 SKLB 文件

骨骼文件路径格式：`chara/human/{race_code}/skeleton/base/b0001/skl_{race_code}b0001.sklb`

每个种族码有自己的骨骼文件。SKLB 文件内部封装了 Havok 二进制数据，physis 将其解析为 `Skeleton` 结构：

```rust
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

pub struct Bone {
    pub name: String,           // 例如 "j_kosi"
    pub parent_index: i32,      // 父骨骼索引（-1 = 根骨骼）
    pub position: [f32; 3],     // 局部平移
    pub rotation: [f32; 4],     // 局部旋转（四元数 xyzw）
    pub scale: [f32; 3],        // 局部缩放
}
```

### 2.2 计算世界空间绑定姿态矩阵

骨骼以层级结构存储，每根骨骼的变换是相对于父骨骼的 **局部变换**。要得到世界空间的绝对位置，需要沿层级逐级累乘：

```rust
fn compute_bind_pose_matrices(skeleton: &Skeleton) -> HashMap<String, Mat4> {
    for (i, bone) in skeleton.bones.iter().enumerate() {
        // 1. 从 position/rotation/scale 构建局部变换矩阵
        let local = Mat4::from_scale_rotation_translation(scale, quat, position);

        // 2. 乘以父骨骼的世界矩阵得到当前骨骼的世界矩阵
        world[i] = if has_parent {
            world[parent_index] * local
        } else {
            local  // 根骨骼
        };

        // 3. 按骨骼名称存储
        result.insert(bone.name, world[i]);
    }
}
```

由于 physis 保证父骨骼索引总是小于子骨骼索引，所以一次正序遍历就够了。

**关键点**：不同种族码的同名骨骼（如 `j_kosi`）世界空间位置不同——这正是体型差异的来源。

## 第三步：CPU 蒙皮算法 (Linear Blend Skinning)

### 3.1 原理

给定：
- **Source bind pose**：模型原始种族码的骨骼绑定姿态 `S[bone_name]`
- **Target bind pose**：目标种族码的骨骼绑定姿态 `T[bone_name]`
- 每顶点 4 组 `(weight, bone_index)`

对每个顶点，计算重映射矩阵并混合：

```
remap[bone] = T[bone] × S[bone]⁻¹
blended = Σ (weight_i × remap[bone_i])
new_position = blended × old_position
```

`S[bone]⁻¹` 将顶点从世界空间变换回骨骼局部空间，`T[bone]` 再将其变换到目标种族的世界空间。效果是：顶点"跟随"骨骼从源体型移动到目标体型。

### 3.2 实现

```rust
pub fn apply_skinning(
    meshes: &mut [MeshData],
    bone_names: &[String],
    bone_tables: &[MdlBoneTable],
    source_bind: &HashMap<String, Mat4>,
    target_bind: &HashMap<String, Mat4>,
) {
    for each vertex:
        // 1. 归一化权重（防止权重和不为 1.0）
        inv_total = 1.0 / sum(weights)

        // 2. 累加混合矩阵
        blended_mat = Σ weight_i * (target[name] * inverse(source[name]))

        // 3. 变换位置（齐次坐标，带平移）
        new_pos = blended_mat.transform_point3(old_pos)

        // 4. 变换法线和切线（3x3 子矩阵，无平移，归一化）
        new_normal = normalize(Mat3(blended_mat) * old_normal)
        new_tangent.xyz = normalize(Mat3(blended_mat) * old_tangent.xyz)
        new_tangent.w = old_tangent.w  // 保留 handedness
}
```

**处理细节**：
- 权重总和 < ε 时跳过（无蒙皮数据的顶点不变换）
- 缺失骨骼名称时使用单位矩阵（保持原位）
- UV 和颜色不变换

## 第四步：集成到幻化编辑器

### 4.1 统一种族码选择

```
遍历 RACE_CODES 优先级列表
找到第一个所有已装备部件都有模型文件的种族码 → unified_race
```

### 4.2 每槽位加载逻辑

```
对每个槽位:
  1. 尝试用 unified_race 加载 → 成功则 actual_race = unified_race
  2. 失败则逐个种族码尝试 → actual_race = 实际命中的种族码
  3. 如果 actual_race ≠ unified_race:
     a. 加载源骨骼: source_bind = skeleton_cache.get(actual_race)
     b. 加载目标骨骼: target_bind = skeleton_cache.get(unified_race)
     c. apply_skinning(meshes, source_bind, target_bind)
  4. 继续纹理加载和合并流程
```

### 4.3 SkeletonCache

骨骼加载开销不小（需要解析 Havok 二进制格式），所以用 `HashMap<String, HashMap<String, Mat4>>` 缓存每个种族码的绑定姿态。编辑器生命周期内，同一种族码只加载一次。

## MDL 元数据字节布局参考

Model Header 之后，各段依次排列（与 physis `ModelData` 一致）：

```
1. Element IDs:              element_id_count × 32 字节
2. LODs:                     3 × 60 字节
3. Extra LODs (可选):         3 × 32 字节
4. Meshes:                   mesh_count × 36 字节
5. Attribute name offsets:   attribute_count × 4 字节
6. Terrain shadow meshes:    terrain_shadow_mesh_count × 20 字节
7. Submeshes:                submesh_count × 16 字节
8. Terrain shadow submeshes: terrain_shadow_submesh_count × 12 字节
9. Material name offsets:    material_count × 4 字节       ← 读取
10. Bone name offsets:       bone_count × 4 字节           ← 读取
11. Bone tables:             取决于版本 (V5: 132B/表, V6: 可变) ← 读取
12. Shapes / Shape meshes / Shape values / ...              ← 不需要
```
