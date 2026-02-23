//! 渲染装备模型验证 ambient 变更
//!
//! 用法: cargo run --example render_equipment

use std::path::Path;
use tomestone::game::{compute_bounding_box, load_mdl, load_mesh_textures, GameData};
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

    // 渲染几个装备模型
    let items: Vec<(&str, u16, u16)> = vec![
        ("chara/equipment/e6016/model/c0101e6016_top.mdl", 6016, 1), // 一件上衣
        ("chara/equipment/e6001/model/c0101e6001_top.mdl", 6001, 1), // 另一件
    ];

    for (mdl_path, set_id, variant_id) in &items {
        println!("渲染: {}", mdl_path);
        match load_mdl(&game, mdl_path) {
            Ok(result) => {
                if result.meshes.is_empty() {
                    println!("  跳过: 无网格");
                    continue;
                }

                let bbox = compute_bounding_box(&result.meshes);
                let load_result = load_mesh_textures(
                    &game,
                    &result.material_names,
                    &result.meshes,
                    *set_id,
                    *variant_id,
                );

                let geometry: Vec<(&[tomestone_render::Vertex], &[u16])> = result
                    .meshes
                    .iter()
                    .map(|m| (m.vertices.as_slice(), m.indices.as_slice()))
                    .collect();

                let mut renderer = ModelRenderer::new(&device);
                renderer.set_model_type(ModelType::Equipment);
                renderer.set_mesh_data(
                    &device,
                    &queue,
                    &geometry,
                    &load_result.mesh_textures,
                );

                let mut camera = Camera::default();
                camera.focus_on(&bbox);

                renderer.render_offscreen(&device, &queue, WIDTH, HEIGHT, &camera, &SceneSettings::default());

                let pixels = read_pixels(&device, &queue, &renderer, WIDTH, HEIGHT).await;
                let filename = format!("equip_{}.png", set_id);
                image::save_buffer(&filename, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8)
                    .expect("保存 PNG 失败");
                println!("  保存: {}", filename);
            }
            Err(e) => println!("  失败: {}", e),
        }
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
