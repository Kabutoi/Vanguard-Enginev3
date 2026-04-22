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
@group(0) @binding(1) var current_radiance_tex: texture_2d<f32>;
@group(0) @binding(2) var history_tex: texture_2d<f32>;
@group(0) @binding(3) var g_buffer_tex: texture_2d<f32>;
@group(0) @binding(4) var prev_g_buffer_tex: texture_2d<f32>;
@group(0) @binding(5) var accumulation_tex: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn cp_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let screen = params.resolution;
    if (f32(id.x) >= screen.x || f32(id.y) >= screen.y) { return; }

    let center_coord = vec2<i32>(id.xy);
    let current_data = textureLoad(current_radiance_tex, center_coord, 0);
    let final_radiance = current_data.rgb;
    let c_depth = current_data.w;
    let gbuf_data = textureLoad(g_buffer_tex, center_coord, 0);
    let c_norm = gbuf_data.xyz;

    // NEIGHBORHOOD STATISTICAL CLAMPING (Mean + Variance)
    var m1 = vec3(0.0);
    var m2 = vec3(0.0);
    let count = 9.0;

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let neighbor_coord = clamp(center_coord + vec2<i32>(x, y), vec2<i32>(0), vec2<i32>(screen) - vec2<i32>(1));
            let val = textureLoad(current_radiance_tex, neighbor_coord, 0).rgb;
            m1 += val;
            m2 += val * val;
        }
    }

    let mean = m1 / count;
    let sigma = sqrt(max(vec3(0.0), (m2 / count) - (mean * mean)));
    
    // We use a 2.0 or 3.0 multiplier for 'sigma' to define the safe color range
    var current_min = mean - 2.0 * sigma;
    var current_max = mean + 2.0 * sigma;

    let epsilon = vec3<f32>(0.001);
    
    // TEMPORAL REPROJECTION
    let uv_rep = (vec2<f32>(id.xy) + 0.5) / screen * 2.0 - 1.0;
    let local_dir_rep = normalize(vec3(uv_rep.x * (screen.x / screen.y), -uv_rep.y, -3.0));
    
    var prev_clip = vec4<f32>(0.0);
    if (c_depth < 10000.0) {
        let world_pos = (params.camera_rot * vec4(local_dir_rep * c_depth, 0.0)).xyz + params.player_pos.xyz;
        prev_clip = params.prev_view_proj * vec4<f32>(world_pos, 1.0);
    } else {
        // Sky Reprojection: Directional only, ignore translation to avoid precision collapse
        let world_dir = (params.camera_rot * vec4(local_dir_rep, 0.0)).xyz;
        prev_clip = params.prev_view_proj * vec4<f32>(world_dir, 0.0);
    }

    let prev_ndc = prev_clip.xyz / (abs(prev_clip.w) + 1e-6);
    let prev_screen_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, 1.0 - (prev_ndc.y * 0.5 + 0.5));
    let prev_coords_f = prev_screen_uv * vec2<f32>(textureDimensions(prev_g_buffer_tex));
    
    var history = final_radiance;
    var alpha = 1.0; 
    
    if (params.frame_index > 0.0 && prev_screen_uv.x >= 0.0 && prev_screen_uv.x <= 1.0 && prev_screen_uv.y >= 0.0 && prev_screen_uv.y <= 1.0 && (c_depth > 10000.0 || prev_clip.w > 0.1)) {
        let pc0 = vec2<u32>(floor(prev_coords_f));
        let f = fract(prev_coords_f);
        
        let tex_dims = vec2<u32>(textureDimensions(prev_g_buffer_tex));
        let max_coord = tex_dims - vec2(1u);
        
        let pc00 = clamp(pc0, vec2(0u), max_coord);
        let pc10 = clamp(pc0 + vec2(1u, 0u), vec2(0u), max_coord);
        let pc01 = clamp(pc0 + vec2(0u, 1u), vec2(0u), max_coord);
        let pc11 = clamp(pc0 + vec2(1u, 1u), vec2(0u), max_coord);

        let h00 = textureLoad(history_tex, pc00, 0);
        let h10 = textureLoad(history_tex, pc10, 0);
        let h01 = textureLoad(history_tex, pc01, 0);
        let h11 = textureLoad(history_tex, pc11, 0);
        
        var found_valid_history = false;
        let tex_dims_i = vec2<i32>(tex_dims);
        
        var world_pos = params.player_pos.xyz + local_dir_rep * c_depth;
        if (c_depth < 10000.0) {
            world_pos = (params.camera_rot * vec4(local_dir_rep * c_depth, 0.0)).xyz + params.player_pos.xyz;
        }
        let view_dir = normalize(params.player_pos.xyz - world_pos);
        let NdotV = max(0.05, abs(dot(c_norm, view_dir)));
        let adaptive_depth_tolerance = clamp(0.1 / NdotV, 0.02, 0.25);

        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let target_coord = clamp(vec2<i32>(pc0) + vec2<i32>(x, y), vec2<i32>(0), tex_dims_i - vec2<i32>(1));
                let p_gbuf = textureLoad(prev_g_buffer_tex, target_coord, 0);
                let h_norm = p_gbuf.xyz;
                let h_depth = p_gbuf.w;
                
                if (dot(c_norm, h_norm) >= 0.85 && abs(c_depth - h_depth) <= adaptive_depth_tolerance) {
                    found_valid_history = true;
                    break;   
                }
            }
            if (found_valid_history) { break; } 
        }
        
        if (found_valid_history) {
            history = mix(mix(h00.rgb, h10.rgb, f.x), mix(h01.rgb, h11.rgb, f.x), f.y);
            
            // APPLY NEIGHBORHOOD CLAMPING TO HISTORY
            history = clamp(history, current_min - epsilon, current_max + epsilon);
            
            alpha = select(0.05, 0.02, c_depth > 10000.0); 
        }
    }
    
    let combined = mix(history, clamp(final_radiance, vec3(0.0), vec3(100.0)), alpha);
    textureStore(accumulation_tex, id.xy, vec4(combined, c_depth));
}
