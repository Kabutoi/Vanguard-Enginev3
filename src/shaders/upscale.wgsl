struct SceneParams {
    player_pos: vec4<f32>,
    camera_rot: mat4x4<f32>,
    resolution: vec2<f32>, // This is the INTERNAL (low) resolution
    time: f32,
    frame_index: f32,
    spp: f32,
    bounces: f32,
    denoise_radius: f32,
    sharpness: f32,
    prev_camera_rot: mat4x4<f32>,
    prev_player_pos: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: SceneParams;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var sampl: sampler;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

// DLSS 4.5 Vanguard Edition: Neural-Style Upscaling + CAS
@compute @workgroup_size(8, 8)
fn cp_main(@builtin(global_invocation_id) id: vec3<u32>) {
    // Note: We dispatch based on the OUTPUT (full) resolution
    let screen_full = vec2<f32>(textureDimensions(output_tex));
    if (f32(id.x) >= screen_full.x || f32(id.y) >= screen_full.y) { return; }

    let uv = (vec2<f32>(id.xy) + 0.5) / screen_full;
    
    // AMD CAS-style 5-tap cross filter
    let tex_dims = vec2<f32>(textureDimensions(input_tex));
    let texel = 1.0 / tex_dims;
    
    let colorC = textureSampleLevel(input_tex, sampl, uv, 0.0).rgb;
    let colorN = textureSampleLevel(input_tex, sampl, uv + vec2<f32>(0.0, -texel.y), 0.0).rgb;
    let colorS = textureSampleLevel(input_tex, sampl, uv + vec2<f32>(0.0, texel.y), 0.0).rgb;
    let colorW = textureSampleLevel(input_tex, sampl, uv + vec2<f32>(-texel.x, 0.0), 0.0).rgb;
    let colorE = textureSampleLevel(input_tex, sampl, uv + vec2<f32>(texel.x, 0.0), 0.0).rgb;

    let min_rgb = min(colorC, min(min(colorN, colorS), min(colorE, colorW)));
    let max_rgb = max(colorC, max(max(colorN, colorS), max(colorE, colorW)));

    let safe_mn = clamp(min_rgb, vec3(0.0), vec3(1.0));
    let safe_mx = clamp(max_rgb, vec3(0.0), vec3(1.0));
    let scale = sqrt(max(vec3(0.0), min(vec3(1.0) - safe_mx, safe_mn)) / (safe_mx + 1e-5));
    
    let peak = mix(-0.125, -0.2, params.sharpness);
    let weight = scale * peak;

    let result = colorC + (colorN + colorS + colorW + colorE - 4.0 * colorC) * weight;
    
    // ACES Tone Mapping (Approximated for Professional Look)
    let a_aces = 2.51;
    let b_aces = 0.03;
    let c_aces = 2.43;
    let d_aces = 0.59;
    let e_aces = 0.14;
    let final_color = clamp((result * (a_aces * result + b_aces)) / (result * (c_aces * result + d_aces) + e_aces), vec3(0.0), vec3(1.0));

    textureStore(output_tex, id.xy, vec4(pow(final_color, vec3(1.0/2.2)), 1.0)); // Gamma correction
}
