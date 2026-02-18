use std::collections::HashMap;
use glam::{Mat4, Quat, Vec3, Mat3};
use physis::skeleton::Skeleton;

use crate::game_data::GameData;
use crate::mdl_loader::{MdlBoneTable, MeshData};

/// 从骨骼层级计算世界空间绑定姿态矩阵
pub fn compute_bind_pose_matrices(skeleton: &Skeleton) -> HashMap<String, Mat4> {
    let bone_count = skeleton.bones.len();
    let mut world_matrices = vec![Mat4::IDENTITY; bone_count];
    let mut result = HashMap::with_capacity(bone_count);

    for (i, bone) in skeleton.bones.iter().enumerate() {
        let position = Vec3::new(bone.position[0], bone.position[1], bone.position[2]);
        let rotation = Quat::from_xyzw(
            bone.rotation[0],
            bone.rotation[1],
            bone.rotation[2],
            bone.rotation[3],
        );
        let scale = Vec3::new(bone.scale[0], bone.scale[1], bone.scale[2]);
        let local = Mat4::from_scale_rotation_translation(scale, rotation, position);

        let world = if bone.parent_index >= 0 && (bone.parent_index as usize) < bone_count {
            world_matrices[bone.parent_index as usize] * local
        } else {
            local
        };

        world_matrices[i] = world;
        result.insert(bone.name.clone(), world);
    }

    result
}

/// 骨骼绑定姿态缓存 (race_code → bone_name → world matrix)
pub struct SkeletonCache {
    cache: HashMap<String, HashMap<String, Mat4>>,
}

impl SkeletonCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// 获取指定种族码的绑定姿态矩阵，加载并缓存
    pub fn get_bind_pose(
        &mut self,
        race_code: &str,
        game: &GameData,
    ) -> Option<&HashMap<String, Mat4>> {
        if !self.cache.contains_key(race_code) {
            let skeleton = game.load_skeleton(race_code)?;
            let matrices = compute_bind_pose_matrices(&skeleton);
            self.cache.insert(race_code.to_string(), matrices);
        }
        self.cache.get(race_code)
    }
}

/// 将模型从 source 种族绑定姿态变换到 target 种族绑定姿态 (CPU 端线性混合蒙皮)
pub fn apply_skinning(
    meshes: &mut [MeshData],
    bone_names: &[String],
    bone_tables: &[MdlBoneTable],
    source_bind: &HashMap<String, Mat4>,
    target_bind: &HashMap<String, Mat4>,
) {
    for mesh in meshes.iter_mut() {
        let table = match bone_tables.get(mesh.bone_table_index as usize) {
            Some(t) => t,
            None => continue,
        };

        for (vi, skin) in mesh.skin_vertices.iter().enumerate() {
            // 计算混合权重总和
            let total_weight: f32 = skin.blend_weights.iter().sum();
            if total_weight < 1e-6 {
                continue; // 无蒙皮数据，跳过
            }

            let inv_total = 1.0 / total_weight;
            let mut blended_mat = Mat4::ZERO;

            for i in 0..4 {
                let w = skin.blend_weights[i] * inv_total;
                if w < 1e-6 {
                    continue;
                }

                let local_bone_idx = skin.blend_indices[i] as usize;

                // blend_index → bone_table → global bone index → bone name
                let global_bone_idx = match table.bone_indices.get(local_bone_idx) {
                    Some(&idx) => idx as usize,
                    None => continue,
                };

                let bone_name = match bone_names.get(global_bone_idx) {
                    Some(name) => name,
                    None => continue,
                };

                // remap = target_bind[name] * inverse(source_bind[name])
                let source_mat = source_bind.get(bone_name).copied().unwrap_or(Mat4::IDENTITY);
                let target_mat = target_bind.get(bone_name).copied().unwrap_or(Mat4::IDENTITY);
                let remap = target_mat * source_mat.inverse();

                blended_mat += remap * w;
            }

            // 变换 position
            let pos = Vec3::from(mesh.vertices[vi].position);
            let new_pos = blended_mat.transform_point3(pos);
            mesh.vertices[vi].position = new_pos.into();

            // 变换 normal (用上 3x3 矩阵, 归一化)
            let mat3 = Mat3::from_mat4(blended_mat);
            let normal = Vec3::from(mesh.vertices[vi].normal);
            let new_normal = (mat3 * normal).normalize_or_zero();
            mesh.vertices[vi].normal = new_normal.into();

            // 变换 tangent (保留 w 分量 = handedness)
            let tangent_xyz = Vec3::new(
                mesh.vertices[vi].tangent[0],
                mesh.vertices[vi].tangent[1],
                mesh.vertices[vi].tangent[2],
            );
            let new_tangent = (mat3 * tangent_xyz).normalize_or_zero();
            mesh.vertices[vi].tangent = [
                new_tangent.x,
                new_tangent.y,
                new_tangent.z,
                mesh.vertices[vi].tangent[3], // 保留 handedness
            ];
        }
    }
}
