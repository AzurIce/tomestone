// ── Types ──

struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    light_dir: vec3<f32>,
    ambient_sky: vec3<f32>,
    ambient_ground: vec3<f32>,
    // bit0: 1=Equipment(顶点颜色遮罩+法线alpha裁剪), 0=Background
    model_flags: u32,
};

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

// ── Bindings ──

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_shared: sampler;
@group(1) @binding(2) var t_normal: texture_2d<f32>;
@group(1) @binding(3) var t_mask: texture_2d<f32>;
@group(1) @binding(4) var t_emissive: texture_2d<f32>;

// ── Vertex ──

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

// ── Fragment ──

@fragment fn fs_main(f: VsOut) -> @location(0) vec4<f32> {
    let is_equipment = (u.model_flags & 1u) != 0u;

    // 采样纹理
    let diffuse_sample = textureSample(t_diffuse, s_shared, f.uv);
    let normal_sample = textureSample(t_normal, s_shared, f.uv);
    let mask_sample = textureSample(t_mask, s_shared, f.uv);
    let emissive_sample = textureSample(t_emissive, s_shared, f.uv);

    // Alpha 裁剪: 仅装备模型使用法线贴图 alpha 通道裁剪
    if is_equipment && normal_sample.a < 0.5 {
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
    // 装备模型: 顶点颜色用于遮罩 (R=高光, G=粗糙度, B=漫反射)
    // BG 模型: 顶点颜色直接作为颜色调制
    var vc_spec_mask: f32;
    var vc_roughness: f32;
    var vc_diffuse_mask: f32;
    if is_equipment {
        vc_spec_mask = f.color.r;
        vc_roughness = f.color.g;
        vc_diffuse_mask = f.color.b;
    } else {
        vc_spec_mask = 1.0;
        vc_roughness = 1.0;
        vc_diffuse_mask = 1.0;
    }

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
