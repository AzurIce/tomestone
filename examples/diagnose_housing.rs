//! 诊断房屋外装模型管线
//!
//! 用法: cargo run --example diagnose_housing

use std::path::Path;
use tomestone::game::{extract_mdl_paths_from_sgb, load_housing_mesh_textures, load_mdl, GameData};

const INSTALL_DIR: &str = r"G:\最终幻想XIV";

fn main() {
    let game = GameData::new(Path::new(INSTALL_DIR));

    for model_key in [1u16, 2, 3, 10, 20, 50] {
        let id = format!("{:04}", model_key);
        println!("\n====== model_key={} ======", id);

        let sgb_path = format!(
            "bgcommon/hou/outdoor/general/{}/asset/gar_b0_m{}.sgb",
            id, id
        );

        let mdl_paths = match game.read_file(&sgb_path) {
            Ok(data) => {
                let paths = extract_mdl_paths_from_sgb(&data);
                println!("SGB -> MDL: {:?}", paths);
                paths
            }
            Err(_) => {
                vec![format!(
                    "bgcommon/hou/outdoor/general/{}/bgparts/gar_b0_m{}.mdl",
                    id, id
                )]
            }
        };

        for mdl_path in &mdl_paths {
            match load_mdl(&game, mdl_path) {
                Ok(result) => {
                    println!(
                        "  MDL: {} mesh={} mat={} bone={}",
                        mdl_path,
                        result.meshes.len(),
                        result.material_names.len(),
                        result.bone_names.len(),
                    );
                    for (i, name) in result.material_names.iter().enumerate() {
                        println!("    材质[{}]: {}", i, name);
                    }

                    // 顶点数据诊断
                    for (mi, mesh) in result.meshes.iter().enumerate() {
                        let vc = mesh.vertices.len();
                        let ic = mesh.indices.len();
                        if vc == 0 {
                            println!("    mesh[{}]: 空", mi);
                            continue;
                        }

                        // 位置范围
                        let (mut pmin, mut pmax) = ([f32::MAX; 3], [f32::MIN; 3]);
                        // UV 范围
                        let (mut umin, mut umax) = ([f32::MAX; 2], [f32::MIN; 2]);
                        // 法线长度范围
                        let (mut nmin, mut nmax) = (f32::MAX, f32::MIN);
                        // 顶点颜色范围
                        let (mut cmin, mut cmax) = ([f32::MAX; 4], [f32::MIN; 4]);
                        // 统计全零法线
                        let mut zero_normal_count = 0u32;
                        // 统计全零 UV
                        let mut zero_uv_count = 0u32;

                        for v in &mesh.vertices {
                            for i in 0..3 {
                                pmin[i] = pmin[i].min(v.position[i]);
                                pmax[i] = pmax[i].max(v.position[i]);
                            }
                            for i in 0..2 {
                                umin[i] = umin[i].min(v.uv[i]);
                                umax[i] = umax[i].max(v.uv[i]);
                            }
                            let nlen = (v.normal[0] * v.normal[0]
                                + v.normal[1] * v.normal[1]
                                + v.normal[2] * v.normal[2])
                                .sqrt();
                            nmin = nmin.min(nlen);
                            nmax = nmax.max(nlen);
                            if nlen < 0.001 {
                                zero_normal_count += 1;
                            }
                            if v.uv[0].abs() < 0.0001 && v.uv[1].abs() < 0.0001 {
                                zero_uv_count += 1;
                            }
                            for i in 0..4 {
                                cmin[i] = cmin[i].min(v.color[i]);
                                cmax[i] = cmax[i].max(v.color[i]);
                            }
                        }

                        println!(
                            "    mesh[{}]: verts={} idx={} mat_idx={}",
                            mi, vc, ic, mesh.material_index
                        );
                        println!(
                            "      pos: [{:.2},{:.2},{:.2}] ~ [{:.2},{:.2},{:.2}]",
                            pmin[0], pmin[1], pmin[2], pmax[0], pmax[1], pmax[2]
                        );
                        println!(
                            "      uv: [{:.3},{:.3}] ~ [{:.3},{:.3}] (zero={})",
                            umin[0], umin[1], umax[0], umax[1], zero_uv_count
                        );
                        println!(
                            "      normal_len: {:.3} ~ {:.3} (zero={})",
                            nmin, nmax, zero_normal_count
                        );
                        println!(
                            "      color: [{:.3},{:.3},{:.3},{:.3}] ~ [{:.3},{:.3},{:.3},{:.3}]",
                            cmin[0], cmin[1], cmin[2], cmin[3], cmax[0], cmax[1], cmax[2], cmax[3]
                        );

                        // 打印前 3 个顶点的详细数据
                        for vi in 0..3.min(vc) {
                            let v = &mesh.vertices[vi];
                            println!(
                                "      v[{}]: pos=[{:.3},{:.3},{:.3}] n=[{:.3},{:.3},{:.3}] uv=[{:.4},{:.4}] c=[{:.3},{:.3},{:.3},{:.3}] t=[{:.3},{:.3},{:.3},{:.3}]",
                                vi,
                                v.position[0], v.position[1], v.position[2],
                                v.normal[0], v.normal[1], v.normal[2],
                                v.uv[0], v.uv[1],
                                v.color[0], v.color[1], v.color[2], v.color[3],
                                v.tangent[0], v.tangent[1], v.tangent[2], v.tangent[3],
                            );
                        }
                    }

                    // 纹理加载
                    let tex_result = load_housing_mesh_textures(
                        &game,
                        &result.material_names,
                        &result.meshes,
                        mdl_path,
                    );
                    for (i, mt) in tex_result.mesh_textures.iter().enumerate() {
                        println!(
                            "    tex[{}]: diffuse={}x{} normal={} mask={} emissive={}",
                            i,
                            mt.diffuse.width,
                            mt.diffuse.height,
                            mt.normal.is_some(),
                            mt.mask.is_some(),
                            mt.emissive.is_some(),
                        );
                    }
                }
                Err(e) => {
                    println!("  MDL 失败: {} -> {}", mdl_path, e);
                }
            }
        }
    }
}
