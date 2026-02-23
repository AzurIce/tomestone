use crate::camera::Camera;
use crate::math::{normalize, sub};
use crate::types::{MeshTextures, ModelType, TextureData, Vertex};

/// Uniform buffer 数据 (16-byte aligned fields)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    _pad0: f32,
    light_dir: [f32; 3],
    _pad1: f32,
    ambient_sky: [f32; 3],
    _pad2: f32,
    ambient_ground: [f32; 3],
    /// bit0: 1=Equipment(使用顶点颜色遮罩+法线alpha裁剪), 0=Background
    model_flags: u32,
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    texture_bind_group: wgpu::BindGroup,
    _normal_tex: wgpu::Texture,
    normal_view: wgpu::TextureView,
    _mask_tex: wgpu::Texture,
    mask_view: wgpu::TextureView,
    _emissive_tex: wgpu::Texture,
    emissive_view: wgpu::TextureView,
}

/// 1×1 默认法线贴图 (flat normal)
const DEFAULT_NORMAL: [u8; 4] = [128, 128, 255, 255];
/// 1×1 默认遮罩贴图
const DEFAULT_MASK: [u8; 4] = [0, 128, 255, 255];
/// 1×1 默认自发光贴图 (黑)
const DEFAULT_EMISSIVE: [u8; 4] = [0, 0, 0, 255];

/// 离屏模型渲染器
pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    gpu_sampler: wgpu::Sampler,
    color_texture: Option<(wgpu::Texture, wgpu::TextureView)>,
    depth_texture: Option<(wgpu::Texture, wgpu::TextureView)>,
    target_size: [u32; 2],
    meshes: Vec<GpuMesh>,
    model_type: ModelType,
}

impl ModelRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("model_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/model.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buf"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bgl"),
                entries: &[
                    tex_entry(0), // diffuse
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    tex_entry(2), // normal
                    tex_entry(3), // mask
                    tex_entry(4), // emissive
                ],
            });

        let gpu_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shared_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_bgl, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 48,
                            shader_location: 4,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
            gpu_sampler,
            color_texture: None,
            depth_texture: None,
            target_size: [0, 0],
            meshes: Vec::new(),
            model_type: ModelType::Equipment,
        }
    }

    // ---- 纹理上传 ----

    fn upload_gpu_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        (texture, view)
    }

    fn create_texture_bind_group(
        &self,
        device: &wgpu::Device,
        diffuse_view: &wgpu::TextureView,
        normal_view: &wgpu::TextureView,
        mask_view: &wgpu::TextureView,
        emissive_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_bg"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(diffuse_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.gpu_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(mask_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(emissive_view),
                },
            ],
        })
    }

    // ---- 公开 API ----

    /// 上传网格几何体和纹理到 GPU
    pub fn set_mesh_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh_geometry: &[(&[Vertex], &[u16])],
        mesh_textures: &[MeshTextures],
    ) {
        self.meshes.clear();
        let white = TextureData {
            rgba: std::sync::Arc::new(vec![255, 255, 255, 255]),
            width: 1,
            height: 1,
        };

        for (i, (vertices, indices)) in mesh_geometry.iter().enumerate() {
            if vertices.is_empty() || indices.is_empty() {
                continue;
            }
            use wgpu::util::DeviceExt;
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex_buf"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("index_buf"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            let mt = mesh_textures.get(i);
            let diffuse_data = mt.map(|m| &m.diffuse).unwrap_or(&white);

            let (_, diffuse_view) = Self::upload_gpu_texture(
                device,
                queue,
                &diffuse_data.rgba,
                diffuse_data.width,
                diffuse_data.height,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            );

            let (normal_tex, normal_view) = match mt.and_then(|m| m.normal.as_ref()) {
                Some(nd) => Self::upload_gpu_texture(
                    device,
                    queue,
                    &nd.rgba,
                    nd.width,
                    nd.height,
                    wgpu::TextureFormat::Rgba8Unorm,
                ),
                None => Self::upload_gpu_texture(
                    device,
                    queue,
                    &DEFAULT_NORMAL,
                    1,
                    1,
                    wgpu::TextureFormat::Rgba8Unorm,
                ),
            };

            let (mask_tex, mask_view) = match mt.and_then(|m| m.mask.as_ref()) {
                Some(md) => Self::upload_gpu_texture(
                    device,
                    queue,
                    &md.rgba,
                    md.width,
                    md.height,
                    wgpu::TextureFormat::Rgba8Unorm,
                ),
                None => Self::upload_gpu_texture(
                    device,
                    queue,
                    &DEFAULT_MASK,
                    1,
                    1,
                    wgpu::TextureFormat::Rgba8Unorm,
                ),
            };

            let (emissive_tex, emissive_view) = match mt.and_then(|m| m.emissive.as_ref()) {
                Some(ed) => Self::upload_gpu_texture(
                    device,
                    queue,
                    &ed.rgba,
                    ed.width,
                    ed.height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                ),
                None => Self::upload_gpu_texture(
                    device,
                    queue,
                    &DEFAULT_EMISSIVE,
                    1,
                    1,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                ),
            };

            let texture_bind_group = self.create_texture_bind_group(
                device,
                &diffuse_view,
                &normal_view,
                &mask_view,
                &emissive_view,
            );

            self.meshes.push(GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: indices.len() as u32,
                texture_bind_group,
                _normal_tex: normal_tex,
                normal_view,
                _mask_tex: mask_tex,
                mask_view,
                _emissive_tex: emissive_tex,
                emissive_view,
            });
        }
    }

    /// 仅更新指定 mesh 的 diffuse 纹理（染色重烘焙），保留 normal/mask/emissive。
    /// `textures[i] == None` 表示不更新该 mesh。
    pub fn update_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &[Option<TextureData>],
    ) {
        for (i, gpu_mesh) in self.meshes.iter_mut().enumerate() {
            if let Some(Some(tex)) = textures.get(i) {
                let (_, diffuse_view) = Self::upload_gpu_texture(
                    device,
                    queue,
                    &tex.rgba,
                    tex.width,
                    tex.height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                );
                gpu_mesh.texture_bind_group =
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("texture_bg"),
                        layout: &self.texture_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&diffuse_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.gpu_sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&gpu_mesh.normal_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&gpu_mesh.mask_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(
                                    &gpu_mesh.emissive_view,
                                ),
                            },
                        ],
                    });
            }
        }
    }

    /// 离屏渲染模型
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        camera: &Camera,
    ) {
        if self.meshes.is_empty() || width == 0 || height == 0 {
            return;
        }
        self.ensure_targets(device, width, height);

        let aspect = width as f32 / height as f32;
        let vp = camera.view_proj(aspect);
        let eye = camera.eye_position();

        let to_target = normalize(sub(camera.target, eye));
        let light_dir = normalize([to_target[0] + 0.3, to_target[1] + 0.5, to_target[2] + 0.2]);

        let model_flags = match self.model_type {
            ModelType::Equipment => 1u32,
            ModelType::Background => 0u32,
        };

        let uniforms = Uniforms {
            view_proj: vp,
            camera_pos: eye,
            _pad0: 0.0,
            light_dir,
            _pad1: 0.0,
            ambient_sky: [0.45, 0.47, 0.55],
            _pad2: 0.0,
            ambient_ground: [0.25, 0.22, 0.20],
            model_flags,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let color_view = &self.color_texture.as_ref().unwrap().1;
            let depth_view = &self.depth_texture.as_ref().unwrap().1;

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("model_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.12,
                            b: 0.14,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            for mesh in &self.meshes {
                pass.set_bind_group(1, &mesh.texture_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// 获取离屏渲染结果的 TextureView
    pub fn color_view(&self) -> Option<&wgpu::TextureView> {
        self.color_texture.as_ref().map(|(_, v)| v)
    }

    /// 获取离屏渲染结果的 Texture 引用（用于 copy 操作）
    pub fn color_texture_ref(&self) -> Option<&wgpu::Texture> {
        self.color_texture.as_ref().map(|(t, _)| t)
    }

    /// 设置模型类型，影响 shader 中的光照和材质处理方式
    pub fn set_model_type(&mut self, model_type: ModelType) {
        self.model_type = model_type;
    }

    pub fn has_mesh(&self) -> bool {
        !self.meshes.is_empty()
    }

    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    // ---- 内部 ----

    fn ensure_targets(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if self.target_size == [w, h] && self.color_texture.is_some() {
            return;
        }
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_color"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        self.color_texture = Some((color, color_view));
        self.depth_texture = Some((depth, depth_view));
        self.target_size = [w, h];
    }
}
