struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_pos: vec3<f32>,
    padding: f32,
};

struct ModelUniform {
    model_matrix: mat4x4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) bone_indices: vec4<u32>,
    @location(6) bone_weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv0: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = model.model_matrix * vec4<f32>(in.position, 1.0);
    let world_norm = normalize((model.model_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.clip_position = camera.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    out.world_normal = world_norm;
    out.uv0 = in.uv0;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.8));
    let n = normalize(in.world_normal);
    
    // Half-Lambert lighting term
    let n_dot_l = max(dot(n, light_dir), 0.0) * 0.5 + 0.5;
    
    // View direction and rim term
    let v = normalize(camera.view_pos - in.world_pos);
    let fresnel = 1.0 - max(dot(v, n), 0.0);
    let rim = pow(fresnel, 3.0) * 0.3;

    // Base color with lighting
    let base_col = model.color.rgb;
    let lit_col = base_col * (n_dot_l * 0.8 + 0.2) + vec3<f32>(rim);
    let alpha = model.color.a;

    // Pre-Multiplied Alpha output: RGB must be multiplied by Alpha for DWM composition
    return vec4<f32>(lit_col * alpha, alpha);
}
