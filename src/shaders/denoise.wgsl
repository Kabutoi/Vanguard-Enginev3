struct SceneParams {
    player_pos: vec4<f32>,
    camera_rot: mat4x4<f32>,
    resolution: vec2<f32>,
    time: f32,
    frame_index: f32,
    spp: f32,
    bounces: f32,
    denoise_radius: f32,
    sharpness: f32,
    prev_camera_rot: mat4x4<f32>,
    prev_player_pos: vec4<f32>,
    room_width: f32,
    room_height: f32,
    room_depth: f32,
    padding: f32,
    prev_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> params: SceneParams;
@group(0) @binding(1) var input_color_tex: texture_2d<f32>;
@group(0) @binding(2) var g_buffer_tex: texture_2d<f32>; 
@group(0) @binding(3) var output_color_tex: texture_storage_2d<rgba16float, write>;

struct DenoiseParams { step_width: i32, pad1: i32, pad2: i32, pad3: i32 }
@group(0) @binding(4) var<uniform> d_params: DenoiseParams;

fn get_luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

@compute @workgroup_size(8, 8)
fn cp_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    var kernel_weights = array<f32, 5>(1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0);
    
    let screen = vec2<u32>(params.resolution);
    if (global_id.x >= screen.x || global_id.y >= screen.y) { return; }
    
    let center_coord = vec2<i32>(global_id.xy);
    let center_color = textureLoad(input_color_tex, center_coord, 0).rgb;
    let center_gbuf = textureLoad(g_buffer_tex, center_coord, 0);
    let center_norm = center_gbuf.xyz;
    let center_depth = center_gbuf.w;

    // HDR DIRECT LIGHT PROTECTION (Unconditional Bypass)
    let l_center = get_luma(center_color);
    if (l_center > 5.0 || center_depth > 9999.0) {
        textureStore(output_color_tex, center_coord, vec4(center_color, 1.0));
        return;
    }

    // NEIGHBORHOOD FIREFLY CLAMPING
    var local_max = vec3<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            if (dx == 0 && dy == 0) { continue; }
            let neighbor_coord = clamp(center_coord + vec2<i32>(dx, dy), vec2<i32>(0), vec2<i32>(screen) - vec2<i32>(1));
            let neighbor_color = textureLoad(input_color_tex, neighbor_coord, 0).rgb;
            local_max = max(local_max, neighbor_color);
        }
    }

    let center_lum = get_luma(center_color);
    let max_lum = get_luma(local_max);
    var safe_center_color = center_color;

    if (center_lum > max_lum * 2.0) { 
        safe_center_color = local_max;
    }

    var sum_color = vec3<f32>(0.0);
    var sum_weight = 0.0;

    // 5x5 À-Trous Grid
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let offset = vec2<i32>(x, y) * d_params.step_width; 
            let sample_coord = clamp(center_coord + offset, vec2<i32>(0), vec2<i32>(screen) - vec2<i32>(1));
            
            var sample_color = textureLoad(input_color_tex, sample_coord, 0).rgb;
            // Note: Since we don't want to re-run neighborhood max extraction for all 25 taps natively due to perf, 
            // the center pixel replacement alone heavily isolates the central blown-out convolution footprint.
            if(x == 0 && y == 0) { sample_color = safe_center_color; }
            
            let sample_gbuf = textureLoad(g_buffer_tex, sample_coord, 0);
            let sample_norm = sample_gbuf.xyz;
            let sample_depth = sample_gbuf.w;
            
            // 1. Spatial Gaussian Weight
            let w_spatial = kernel_weights[x + 2] * kernel_weights[y + 2];
            
            // 2. Normal Edge-Stopping Weight
            let w_normal = pow(max(0.0, dot(center_norm, sample_norm)), 128.0);
            
            // 3. Depth Edge-Stopping Weight
            let depth_diff = abs(center_depth - sample_depth);
            let w_depth = exp(-depth_diff / 0.1); 
            
            // Combine weights
            let final_weight = w_spatial * w_normal * w_depth;
            
            sum_color += sample_color * final_weight;
            sum_weight += final_weight;
        }
    }

    let denoised_color = sum_color / max(sum_weight, 0.0001);
    textureStore(output_color_tex, center_coord, vec4<f32>(denoised_color, 1.0));
}
