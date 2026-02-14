use egui_wgpu::wgpu;

use crate::mdl_loader::{BoundingBox, MeshData, Vertex};
use crate::tex_loader::{MeshTextures, TextureData};

/// 相机参数
pub struct Camera {
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub target: [f32; 3],
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            distance: 3.0,
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.3,
            target: [0.0, 0.8, 0.0],
        }
    }
}

impl Camera {
    pub fn eye_position(&self) -> [f32; 3] {
        [
            self.target[0] + self.distance * self.yaw.cos() * self.pitch.cos(),
            self.target[1] + self.distance * self.pitch.sin(),
            self.target[2] + self.distance * self.yaw.sin() * self.pitch.cos(),
        ]
    }

    pub fn view_proj(&self, aspect: f32) -> [[f32; 4]; 4] {
        let eye = self.eye_position();
        let view = look_at(eye, self.target, [0.0, 1.0, 0.0]);
        let proj = perspective(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        mat4_mul(proj, view)
    }

    /// 根据包围盒自动对焦相机
    pub fn focus_on(&mut self, bbox: &BoundingBox) {
        self.target = bbox.center();
        let size = bbox.size();
        self.distance = if size > 0.01 { size * 1.2 } else { 3.0 };
        self.yaw = std::f32::consts::FRAC_PI_2;
        self.pitch = 0.15;
    }

    /// 右键拖拽平移
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = [self.yaw.sin(), 0.0, -self.yaw.cos()];
        let up = [0.0, 1.0, 0.0];
        let scale = self.distance * 0.002;
        for i in 0..3 {
            self.target[i] += -right[i] * dx * scale + up[i] * dy * scale;
        }
    }
}

/// Uniform buffer 数据 (128 bytes, 16-byte aligned fields)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],   // 64 bytes
    camera_pos: [f32; 3],       // 12 bytes
    _pad0: f32,                  // 4 bytes
    light_dir: [f32; 3],        // 12 bytes
    _pad1: f32,                  // 4 bytes
    ambient_sky: [f32; 3],      // 12 bytes
    _pad2: f32,                  // 4 bytes
    ambient_ground: [f32; 3],   // 12 bytes
    _pad3: f32,                  // 4 bytes
}

/// 存储在 egui_wgpu CallbackResources 中的渲染资源
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
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    texture_bind_group: wgpu::BindGroup,
    // 存储非 diffuse 纹理，防止 GPU 资源被释放；view 用于染色重烘焙时重建 bind group
    _normal_tex: wgpu::Texture,
    normal_view: wgpu::TextureView,
    _mask_tex: wgpu::Texture,
    mask_view: wgpu::TextureView,
    _emissive_tex: wgpu::Texture,
    emissive_view: wgpu::TextureView,
}

const SHADER_SRC: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    light_dir: vec3<f32>,
    ambient_sky: vec3<f32>,
    ambient_ground: vec3<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_shared: sampler;
@group(1) @binding(2) var t_normal: texture_2d<f32>;
@group(1) @binding(3) var t_mask: texture_2d<f32>;
@group(1) @binding(4) var t_emissive: texture_2d<f32>;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) tangent: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_pos: vec3<f32>,
    @location(4) world_tangent: vec3<f32>,
    @location(5) tangent_w: f32,
};

@vertex fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    let world_pos = v.position;
    out.clip = u.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_normal = v.normal;
    out.color = v.color;
    out.uv = v.uv;
    out.world_pos = world_pos;
    out.world_tangent = v.tangent.xyz;
    out.tangent_w = v.tangent.w;
    return out;
}

@fragment fn fs_main(f: VsOut) -> @location(0) vec4<f32> {
    // 采样纹理
    let diffuse_sample = textureSample(t_diffuse, s_shared, f.uv);
    let normal_sample = textureSample(t_normal, s_shared, f.uv);
    let mask_sample = textureSample(t_mask, s_shared, f.uv);
    let emissive_sample = textureSample(t_emissive, s_shared, f.uv);

    // Alpha 裁剪 (法线贴图 alpha 通道)
    if normal_sample.a < 0.5 {
        discard;
    }

    // ---- 法线贴图 ----
    let N = normalize(f.world_normal);
    let T = normalize(f.world_tangent - N * dot(f.world_tangent, N)); // Gram-Schmidt 正交化
    let B = cross(N, T) * f.tangent_w;
    let TBN = mat3x3<f32>(T, B, N);

    // 从法线贴图解码 (RG 通道, 重建 Z)
    var tn: vec3<f32>;
    tn.x = normal_sample.r * 2.0 - 1.0;
    tn.y = normal_sample.g * 2.0 - 1.0;
    tn.z = sqrt(max(1.0 - tn.x * tn.x - tn.y * tn.y, 0.0));
    let n = normalize(TBN * tn);

    // ---- 遮罩贴图: R=specular_power, G=roughness, B=ao ----
    let mask_spec = mask_sample.r;
    let mask_rough = mask_sample.g;
    let mask_ao = mask_sample.b;

    // ---- 顶点颜色材质属性 ----
    let vc_spec_mask = f.color.r;   // 高光遮罩
    let vc_roughness = f.color.g;   // 粗糙度调制
    let vc_diffuse_mask = f.color.b; // 漫反射遮罩

    // ---- 光照计算 ----
    let light_dir = normalize(u.light_dir);
    let view_dir = normalize(u.camera_pos - f.world_pos);

    // 漫反射 - Lambert
    let ndl = max(dot(n, light_dir), 0.0);

    // 半球环境光
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let ambient = mix(u.ambient_ground, u.ambient_sky, (dot(n, up) + 1.0) * 0.5);

    // Blinn-Phong 高光
    let half_dir = normalize(light_dir + view_dir);
    let ndh = max(dot(n, half_dir), 0.0);
    let spec_intensity = mask_spec * vc_spec_mask;
    let shininess = mix(8.0, 128.0, (1.0 - mask_rough * vc_roughness));
    let spec = pow(ndh, shininess) * spec_intensity;

    // 菲涅尔边缘光
    let ndv = max(dot(n, view_dir), 0.0);
    let fresnel = pow(1.0 - ndv, 5.0) * 0.15;

    // 最终合成
    let base_color = diffuse_sample.rgb * vc_diffuse_mask;
    let lit = base_color * mask_ao * (ambient + vec3<f32>(ndl)) + vec3<f32>(spec) + vec3<f32>(fresnel) * base_color;
    let final_color = lit + emissive_sample.rgb;

    return vec4<f32>(final_color, diffuse_sample.a);
}
"#;

/// 1x1 默认法线贴图 (flat normal: X=0, Y=0, Z=1)
/// R=128 → (128/255)*2-1 ≈ 0, G=128 → 0, 重建 Z=1, A=255 (不裁剪)
const DEFAULT_NORMAL: [u8; 4] = [128, 128, 255, 255];

/// 1x1 默认遮罩贴图 (无高光, 中等粗糙度, 无遮蔽)
/// R=0 (no spec), G=128 (mid rough), B=255 (full AO = no occlusion)
const DEFAULT_MASK: [u8; 4] = [0, 128, 255, 255];

/// 1x1 默认 emissive (黑色, 无自发光)
const DEFAULT_EMISSIVE: [u8; 4] = [0, 0, 0, 255];

impl ModelRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("model_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buf"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // 纹理 bind group: diffuse(sRGB) + sampler + normal(linear) + mask(linear) + emissive(sRGB)
        let tex_entry = |binding: u32| -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }
        };

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
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
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },  // position
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 }, // normal
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 2 }, // uv
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 3 }, // color
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 4 }, // tangent
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
        }
    }

    /// 上传 RGBA 数据到 GPU 纹理
    fn upload_gpu_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
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

    /// 从纹理 views 创建 bind group
    fn create_bind_group(
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
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(diffuse_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.gpu_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(normal_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(mask_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(emissive_view) },
            ],
        })
    }

    pub fn set_mesh_data(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, meshes: &[MeshData], mesh_textures: &[MeshTextures]) {
        self.meshes.clear();
        let white = TextureData { rgba: vec![255, 255, 255, 255], width: 1, height: 1 };

        for (i, mesh) in meshes.iter().enumerate() {
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            use wgpu::util::DeviceExt;
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex_buf"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("index_buf"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            let mt = mesh_textures.get(i);
            let diffuse_data = mt.map(|m| &m.diffuse).unwrap_or(&white);

            // Diffuse + Emissive = sRGB, Normal + Mask = linear
            let (_, diffuse_view) = Self::upload_gpu_texture(
                device, queue, &diffuse_data.rgba, diffuse_data.width, diffuse_data.height,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            );

            let normal_data = mt.and_then(|m| m.normal.as_ref());
            let (normal_tex, normal_view) = if let Some(nd) = normal_data {
                Self::upload_gpu_texture(device, queue, &nd.rgba, nd.width, nd.height, wgpu::TextureFormat::Rgba8Unorm)
            } else {
                Self::upload_gpu_texture(device, queue, &DEFAULT_NORMAL, 1, 1, wgpu::TextureFormat::Rgba8Unorm)
            };

            let mask_data = mt.and_then(|m| m.mask.as_ref());
            let (mask_tex, mask_view) = if let Some(md) = mask_data {
                Self::upload_gpu_texture(device, queue, &md.rgba, md.width, md.height, wgpu::TextureFormat::Rgba8Unorm)
            } else {
                Self::upload_gpu_texture(device, queue, &DEFAULT_MASK, 1, 1, wgpu::TextureFormat::Rgba8Unorm)
            };

            let emissive_data = mt.and_then(|m| m.emissive.as_ref());
            let (emissive_tex, emissive_view) = if let Some(ed) = emissive_data {
                Self::upload_gpu_texture(device, queue, &ed.rgba, ed.width, ed.height, wgpu::TextureFormat::Rgba8UnormSrgb)
            } else {
                Self::upload_gpu_texture(device, queue, &DEFAULT_EMISSIVE, 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb)
            };

            let texture_bind_group = self.create_bind_group(device, &diffuse_view, &normal_view, &mask_view, &emissive_view);

            self.meshes.push(GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: mesh.indices.len() as u32,
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

    /// 仅更新指定 mesh 的 diffuse 纹理 (染色重烘焙)，保留 normal/mask/emissive
    /// textures[i] 为 None 表示不更新该 mesh
    pub fn update_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, textures: &[Option<TextureData>]) {
        for (i, gpu_mesh) in self.meshes.iter_mut().enumerate() {
            if let Some(Some(tex)) = textures.get(i) {
                let (_, diffuse_view) = Self::upload_gpu_texture(
                    device, queue, &tex.rgba, tex.width, tex.height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                );
                // 用新 diffuse + 已有 normal/mask/emissive 重建 bind group
                gpu_mesh.texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("texture_bg"),
                    layout: &self.texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&diffuse_view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.gpu_sampler) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gpu_mesh.normal_view) },
                        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&gpu_mesh.mask_view) },
                        wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&gpu_mesh.emissive_view) },
                    ],
                });
            }
        }
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if self.target_size == [w, h] && self.color_texture.is_some() {
            return;
        }
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_color"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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

    /// 离屏渲染模型，结果存储在内部 color texture 中
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

        // 光源方向跟随相机 (从相机方向偏移一点)
        let to_target = normalize(sub(camera.target, eye));
        let light_dir = normalize([
            to_target[0] + 0.3,
            to_target[1] + 0.5,
            to_target[2] + 0.2,
        ]);

        let uniforms = Uniforms {
            view_proj: vp,
            camera_pos: eye,
            _pad0: 0.0,
            light_dir,
            _pad1: 0.0,
            ambient_sky: [0.25, 0.27, 0.35],
            _pad2: 0.0,
            ambient_ground: [0.10, 0.08, 0.06],
            _pad3: 0.0,
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
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.12, g: 0.12, b: 0.14, a: 1.0 }),
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

    /// 获取离屏渲染结果的 TextureView，用于注册到 egui
    pub fn color_view(&self) -> Option<&wgpu::TextureView> {
        self.color_texture.as_ref().map(|(_, v)| v)
    }

    pub fn has_mesh(&self) -> bool {
        !self.meshes.is_empty()
    }
}

// ---- 数学工具 ----

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize(sub(target, eye));
    let r = normalize(cross(f, up));
    let u = cross(r, f);
    [
        [r[0], u[0], -f[0], 0.0],
        [r[1], u[1], -f[1], 0.0],
        [r[2], u[2], -f[2], 0.0],
        [-dot(r, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (fov_y / 2.0).tan();
    let nf = 1.0 / (near - far);
    // wgpu 深度范围 [0, 1]
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far * nf, -1.0],
        [0.0, 0.0, near * far * nf, 0.0],
    ]
}

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            out[i][j] = a[0][j] * b[i][0] + a[1][j] * b[i][1] + a[2][j] * b[i][2] + a[3][j] * b[i][3];
        }
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0]-b[0], a[1]-b[1], a[2]-b[2]] }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]] }
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0]*b[0] + a[1]*b[1] + a[2]*b[2] }
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len < 1e-10 { return [0.0; 3]; }
    [v[0]/len, v[1]/len, v[2]/len]
}
