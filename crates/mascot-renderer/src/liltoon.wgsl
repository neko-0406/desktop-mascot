// lilToon reproducing shader with 2-step toon shading, rimlight, emission, and inverted hull outline.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_pos: vec3<f32>,
    fov_factor: f32,
};

struct LightUniform {
    direction: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    _pad1: f32,
    ambient_color: vec3<f32>,
    _pad2: f32,
};

struct LilToonUniform {
    base_color: vec4<f32>,
    shade_color: vec4<f32>,
    shade2_color: vec4<f32>,
    shade_border: f32,
    shade_blur: f32,
    shade2_border: f32,
    shade2_blur: f32,
    rim_color: vec4<f32>,
    rim_border: f32,
    rim_blur: f32,
    rim_fresnel_power: f32,
    _pad0: f32,
    emission_color: vec4<f32>,
    outline_color: vec4<f32>,
    outline_width: f32,
    outline_enable: u32,
    _pad1: vec2<f32>,
    matcap_color: vec4<f32>,
    matcap_border: f32,
    matcap_blur: f32,
    matcap_enable: u32,
    _pad2: f32,
};

struct ModelUniform {
    model_matrix: mat4x4<f32>,
    color: vec4<f32>,
};

// Group 0: Scene globals
@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> light: LightUniform;

// Group 1: Material (lilToon parameters and textures)
@group(1) @binding(0) var<uniform> mat: LilToonUniform;
@group(1) @binding(1) var t_base: texture_2d<f32>;
@group(1) @binding(2) var s_base: sampler;

// Group 2: Model & Bones
@group(2) @binding(0) var<uniform> model: ModelUniform;
@group(2) @binding(1) var<storage, read> bone_matrices: array<mat4x4<f32>>;

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
    @location(3) world_tangent: vec4<f32>,
};

fn get_bone_matrix(idx: u32) -> mat4x4<f32> {
    let count = arrayLength(&bone_matrices);
    if (idx >= count) {
        return mat4x4<f32>(
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        );
    }
    return bone_matrices[idx];
}

fn get_skin_matrix(indices: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    let w_sum = weights.x + weights.y + weights.z + weights.w;
    if (w_sum < 0.0001) {
        return mat4x4<f32>(
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        );
    }

    let m0 = get_bone_matrix(indices.x) * weights.x;
    let m1 = get_bone_matrix(indices.y) * weights.y;
    let m2 = get_bone_matrix(indices.z) * weights.z;
    let m3 = get_bone_matrix(indices.w) * weights.w;
    return m0 + m1 + m2 + m3;
}

// =========================================================================
// Main Toon Shading Pass
// =========================================================================

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let skin_matrix = get_skin_matrix(in.bone_indices, in.bone_weights);
    let world_pos = model.model_matrix * (skin_matrix * vec4<f32>(in.position, 1.0));
    let world_norm = normalize((model.model_matrix * (skin_matrix * vec4<f32>(in.normal, 0.0))).xyz);
    let world_tang = normalize((model.model_matrix * (skin_matrix * vec4<f32>(in.tangent.xyz, 0.0))).xyz);

    out.clip_position = camera.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    out.world_normal = world_norm;
    out.uv0 = in.uv0;
    out.world_tangent = vec4<f32>(world_tang, in.tangent.w);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base_tex = textureSample(t_base, s_base, in.uv0);
    let base_col = base_tex * mat.base_color;

    let n = normalize(in.world_normal);
    let l = normalize(light.direction);
    let v = normalize(camera.view_pos - in.world_pos);

    // Half-Lambert lighting calculation
    let n_dot_l = dot(n, l) * 0.5 + 0.5;

    // 2-step toon shading ramps with smoothstep
    let b1 = mat.shade_border;
    let w1 = max(mat.shade_blur, 0.001);
    let shade_factor1 = smoothstep(b1 - w1, b1 + w1, n_dot_l);

    let b2 = mat.shade2_border;
    let w2 = max(mat.shade2_blur, 0.001);
    let shade_factor2 = smoothstep(b2 - w2, b2 + w2, n_dot_l);

    let shade1_col = mat.shade_color.rgb * base_col.rgb;
    let shade2_col = mat.shade2_color.rgb * base_col.rgb;
    let toon_col = mix(shade2_col, mix(shade1_col, base_col.rgb, shade_factor1), shade_factor2);

    // Rimlight calculation with Fresnel
    let n_dot_v = max(dot(v, n), 0.0);
    let fresnel = 1.0 - n_dot_v;
    let rim_p = max(mat.rim_fresnel_power, 0.1);
    let rim_fresnel = pow(fresnel, rim_p);
    let rb = mat.rim_border;
    let rw = max(mat.rim_blur, 0.001);
    let rim_factor = smoothstep(rb - rw, rb + rw, rim_fresnel);
    let rim = mat.rim_color.rgb * (rim_factor * mat.rim_color.a);

    // Emission
    let emission = mat.emission_color.rgb;

    // MatCap (Sphere Mapping) based on view-space normal with procedural fallback
    var matcap_rgb = vec3<f32>(0.0);
    if (mat.matcap_enable != 0u || mat.matcap_color.a > 0.0) {
        let cam_up = vec3<f32>(0.0, 1.0, 0.0);
        let cam_right = cross(cam_up, v);
        let r_len = length(cam_right);
        let right = select(vec3<f32>(1.0, 0.0, 0.0), cam_right / r_len, r_len > 0.001);
        let up = cross(v, right);

        // View-space normal (-1.0 to 1.0)
        let vn = vec2<f32>(dot(n, right), dot(n, up));
        let matcap_uv = vn * 0.5 + 0.5;

        // Fallback procedural sphere reflection calculation
        let sphere_dist = clamp(length(vn), 0.0, 1.0);
        let sphere_falloff = 1.0 - sphere_dist * sphere_dist;
        let sphere_hl = max(0.0, dot(vn, normalize(vec2<f32>(0.5, 0.7))));
        let matcap_factor = pow(sphere_hl, 6.0) * 0.7 + sphere_falloff * 0.3;
        matcap_rgb = mat.matcap_color.rgb * (matcap_factor * mat.matcap_color.a);
    }

    // Direct and ambient lighting combination
    let lit_rgb = toon_col * light.color + light.ambient_color * base_col.rgb + rim + emission + matcap_rgb;
    let final_alpha = base_col.a;

    // PreMultiplied Alpha: RGB multiplied by alpha for DWM desktop composition
    let clamped_rgb = clamp(lit_rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(clamped_rgb * final_alpha, final_alpha);
}

// =========================================================================
// Inverted Hull Outline Pass
// =========================================================================

@vertex
fn vs_outline(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let skin_matrix = get_skin_matrix(in.bone_indices, in.bone_weights);
    let world_pos = model.model_matrix * (skin_matrix * vec4<f32>(in.position, 1.0));
    let world_norm = normalize((model.model_matrix * (skin_matrix * vec4<f32>(in.normal, 0.0))).xyz);
    let world_tang = normalize((model.model_matrix * (skin_matrix * vec4<f32>(in.tangent.xyz, 0.0))).xyz);

    var clip_pos = camera.view_proj * world_pos;
    let clip_norm = camera.view_proj * vec4<f32>(world_norm, 0.0);

    // Maintain screen-space outline thickness scaled by depth (clip_pos.w)
    let norm_len = length(clip_norm.xy);
    let offset_dir = select(vec2<f32>(0.0), clip_norm.xy / norm_len, norm_len > 0.0001);
    let offset = offset_dir * mat.outline_width * clip_pos.w * 0.0015;
    clip_pos.x += offset.x;
    clip_pos.y += offset.y;

    // Small depth bias to prevent Z-fighting with main mesh
    clip_pos.z += 0.0001 * clip_pos.w;

    out.clip_position = clip_pos;
    out.world_pos = world_pos.xyz;
    out.world_normal = world_norm;
    out.uv0 = in.uv0;
    out.world_tangent = vec4<f32>(world_tang, in.tangent.w);
    return out;
}

@fragment
fn fs_outline(in: VertexOutput) -> @location(0) vec4<f32> {
    if (mat.outline_enable == 0u || mat.outline_width <= 0.0) {
        discard;
    }
    let col = mat.outline_color;
    // PreMultiplied Alpha for outline
    return vec4<f32>(col.rgb * col.a, col.a);
}
