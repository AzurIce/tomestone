//! 渲染不同类型的房屋外装
//!
//! 用法: cargo run --example render_housing_types

use std::path::Path;
use tomestone::game::{
    compute_bounding_box, extract_mdl_paths_from_sgb, load_housing_mesh_textures, load_mdl,
    GameData, MeshData,
};
use tomestone_render::{Camera, ModelRenderer, ModelType, SceneSettings};

const INSTALL_DIR: &str = r"G:\最终幻想XIV";
const WIDTH: u32 = 512;
const HEIGHT: u32 = 512;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .expect("无法获取 GPU adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("无法获取 GPU device");

    let game = GameData::new(Path::new(INSTALL_DIR));
    let items = game.load_housing_exterior_list();

    // 每种类型渲染第一个
    let types = ["屋根", "外壁", "窓", "扉", "塀"];
    for type_name in &types {
        let item = match items.iter().find(|i| i.part_type.display_name() == *type_name) {
            Some(i) => i,
            None => continue,
        };

        println!("渲染 [{}] {}", type_name, item.name);

        let mut all_mdl_paths: Vec<String> = Vec::new();
        for sgb_path in &item.sgb_paths {
            if let Ok(data) = game.read_file(sgb_path) {
                for p in extract_mdl_paths_from_sgb(&data) {
                    if !all_mdl_paths.contains(&p) {
                        all_mdl_paths.push(p);
                    }
                }
            }
        }

        let mut all_meshes: Vec<MeshData> = Vec::new();
        let mut all_material_names: Vec<String> = Vec::new();
        let mut first_mdl_path: Option<String> = None;

        for mdl_path in &all_mdl_paths {
            if let Ok(result) = load_mdl(&game, mdl_path) {
                if !result.meshes.is_empty() {
                    if first_mdl_path.is_none() {
                        first_mdl_path = Some(mdl_path.clone());
                    }
                    let mat_offset = all_material_names.len() as u16;
                    for mut mesh in result.meshes {
                        mesh.material_index += mat_offset;
                        all_meshes.push(mesh);
                    }
                    all_material_names.extend(result.material_names);
                }
            }
        }

        if all_meshes.is_empty() {
            println!("  跳过: 无网格");
            continue;
        }

        let bbox = compute_bounding_box(&all_meshes);
        let mdl_path_ref = first_mdl_path.as_deref().unwrap_or("");
        let load_result =
            load_housing_mesh_textures(&game, &all_material_names, &all_meshes, mdl_path_ref);

        let geometry: Vec<(&[tomestone_render::Vertex], &[u16])> = all_meshes
            .iter()
            .map(|m| (m.vertices.as_slice(), m.indices.as_slice()))
            .collect();

        let mut renderer = ModelRenderer::new(&device);
        renderer.set_model_type(ModelType::Background);
        renderer.set_mesh_data(&device, &queue, &geometry, &load_result.mesh_textures);

        let mut camera = Camera::default();
        camera.focus_on(&bbox);

        renderer.render_offscreen(&device, &queue, WIDTH, HEIGHT, &camera, &SceneSettings::default());

        let pixels = read_pixels(&device, &queue, &renderer, WIDTH, HEIGHT).await;
        let safe_name = type_name.replace("/", "_");
        let filename = format!("housing_type_{}.png", safe_name);
        image::save_buffer(&filename, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8)
            .expect("保存 PNG 失败");
        println!("  保存: {}", filename);
    }
}

async fn read_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &ModelRenderer,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes_per_row = align_to(width * 4, 256);
    let buffer_size = (bytes_per_row * height) as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let texture = renderer.color_texture_ref().expect("无渲染纹理");
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            timeout: Some(std::time::Duration::from_secs(10)),
            submission_index: None,
        })
        .ok();
    rx.recv().unwrap().expect("map 失败");
    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&mapped[start..end]);
    }
    drop(mapped);
    staging.unmap();
    pixels
}

fn align_to(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}
