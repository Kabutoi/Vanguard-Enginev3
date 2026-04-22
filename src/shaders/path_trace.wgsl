// Vanguard Engine v3 - Dynamic Path Tracer
// Hardened for Crimson Desert Baseline (Direct Vision Bypass)

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
@group(0) @binding(1) var current_radiance_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var box_tex: texture_2d<f32>;
@group(0) @binding(3) var box_sampler: sampler;
@group(0) @binding(4) var g_buffer_tex: texture_storage_2d<rgba16float, read_write>;

fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn get_blue_noise(id: vec2<u32>, frame: u32, dimension: u32) -> vec2<f32> {
    var state = pcg_hash(id.x + pcg_hash(id.y + pcg_hash(dimension)));
    state = pcg_hash(state); let shift_x = f32(state) / 4294967296.0;
    state = pcg_hash(state); let shift_y = f32(state) / 4294967296.0;
    let spatial_shift = vec2(shift_x, shift_y);
    
    let alpha = vec2<f32>(0.7548776662466927, 0.5698402909980532);
    let r2 = fract(f32(frame + 1u) * alpha);
    return fract(r2 + spatial_shift);
}

fn cosine_weighted_hemisphere(n: vec3<f32>, u: vec2<f32>) -> vec3<f32> {
    let phi = 6.283185307 * u.x; let r = sqrt(u.y);
    let x = r * cos(phi); let y = r * sin(phi); let z = sqrt(max(0.0, 1.0 - u.y));
    let up = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let tan = normalize(cross(up, n)); let bitan = cross(n, tan);
    return normalize(tan * x + bitan * y + n * z);
}

struct Hit { t: f32, pos: vec3<f32>, normal: vec3<f32>, color: vec3<f32>, emissive: vec3<f32>, uv: vec2<f32>, use_tex: f32 };

fn intersect_sphere(ray_orig: vec3<f32>, ray_dir: vec3<f32>, center: vec3<f32>, radius: f32) -> f32 {
    let oc = ray_orig - center; let b = dot(oc, ray_dir); let c = dot(oc, oc) - radius * radius;
    let h = b * b - c; if (h < 0.0) { return -1.0; }
    let sqrt_h = sqrt(h); 
    let t1 = -b - sqrt_h; if (t1 > 0.001) { return t1; }
    let t2 = -b + sqrt_h; if (t2 > 0.001) { return t2; }
    return -1.0;
}

fn intersect_plane(ray_orig: vec3<f32>, ray_dir: vec3<f32>, n: vec3<f32>, d: f32) -> f32 {
    let denom = dot(n, ray_dir); if (abs(denom) > 1e-6) {
        let t = (d - dot(n, ray_orig)) / denom; if (t > 0.001) { return t; }
    }
    return -1.0;
}

fn intersect_box(ray_orig: vec3<f32>, ray_dir: vec3<f32>, b_min: vec3<f32>, b_max: vec3<f32>) -> vec4<f32> {
    let t_min = (b_min - ray_orig) / ray_dir;
    let t_max = (b_max - ray_orig) / ray_dir;
    let t1 = min(t_min, t_max);
    let t2 = max(t_min, t_max);
    let t_near = max(max(t1.x, t1.y), t1.z);
    let t_far = min(min(t2.x, t2.y), t2.z);
    
    if (t_near > t_far || t_far < 0.001) { return vec4(-1.0); }
    
    let t_hit = select(t_near, t_far, t_near < 0.001);
    if (t_hit < 0.001) { return vec4(-1.0); }

    // Precise Normal Detection: Compare t_hit to each axis
    var normal = vec3(0.0);
    if (t_hit == t1.x) { normal = vec3(-sign(ray_dir.x), 0.0, 0.0); }
    else if (t_hit == t1.y) { normal = vec3(0.0, -sign(ray_dir.y), 0.0); }
    else if (t_hit == t1.z) { normal = vec3(0.0, 0.0, -sign(ray_dir.z)); }
    else if (t_hit == t2.x) { normal = vec3(sign(ray_dir.x), 0.0, 0.0); }
    else if (t_hit == t2.y) { normal = vec3(0.0, sign(ray_dir.y), 0.0); }
    else { normal = vec3(0.0, 0.0, sign(ray_dir.z)); }
    
    return vec4(t_hit, normal);
}

fn world_hit(ray_orig: vec3<f32>, ray_dir: vec3<f32>) -> Hit {
    var hit = Hit(1e20, vec3(0.), vec3(0.), vec3(0.), vec3(0.), vec2(0.), 0.0);
    
    let t_f = intersect_plane(ray_orig, ray_dir, vec3(0.,1.,0.), -params.room_height);
    if (t_f > 0.0 && t_f < hit.t) { hit = Hit(t_f, ray_orig+ray_dir*t_f, vec3(0.,1.,0.), vec3(0.7), vec3(0.), vec2(0.), 0.0); }
    let t_c = intersect_plane(ray_orig, ray_dir, vec3(0.,-1.,0.), -params.room_height);
    if (t_c > 0.0 && t_c < hit.t) { hit = Hit(t_c, ray_orig+ray_dir*t_c, vec3(0.,-1.,0.), vec3(0.7), vec3(0.), vec2(0.), 0.0); }
    let t_b = intersect_plane(ray_orig, ray_dir, vec3(0.,0.,1.), -params.room_depth);
    if (t_b > 0.0 && t_b < hit.t) { hit = Hit(t_b, ray_orig+ray_dir*t_b, vec3(0.,0.,1.), vec3(0.7), vec3(0.), vec2(0.), 0.0); }
    let t_front = intersect_plane(ray_orig, ray_dir, vec3(0.,0.,-1.), -params.room_depth);
    if (t_front > 0.0 && t_front < hit.t) { hit = Hit(t_front, ray_orig+ray_dir*t_front, vec3(0.,0.,-1.), vec3(0.7), vec3(0.), vec2(0.), 0.0); }
    let t_l = intersect_plane(ray_orig, ray_dir, vec3(1.,0.,0.), -params.room_width);
    if (t_l > 0.0 && t_l < hit.t) { hit = Hit(t_l, ray_orig+ray_dir*t_l, vec3(1.,0.,0.), vec3(0.6,0.1,0.1), vec3(0.), vec2(0.), 0.0); }
    let t_r = intersect_plane(ray_orig, ray_dir, vec3(-1.,0.,0.), -params.room_width);
    if (t_r > 0.0 && t_r < hit.t) { hit = Hit(t_r, ray_orig+ray_dir*t_r, vec3(-1.,0.,0.), vec3(0.1,0.6,0.1), vec3(0.), vec2(0.), 0.0); }
    let box_res = intersect_box(ray_orig, ray_dir, vec3(-3.8, -5.0, -1.0), vec3(-0.8, -2.0, 2.0));
    if (box_res.x > 0.0 && box_res.x < hit.t) {
        let p = ray_orig + ray_dir * box_res.x;
        let n = box_res.yzw;
        let b_min = vec3(-3.8, -5.0, -1.0);
        let b_max = vec3(-0.8, -2.0, 2.0);
        let p_rel = (p - b_min) / (b_max - b_min);
        var uv = vec2(0.0);
        if (abs(n.x) > 0.5) { uv = p_rel.zy; } else if (abs(n.y) > 0.5) { uv = p_rel.xz; } else { uv = p_rel.xy; }
        hit = Hit(box_res.x, p, n, vec3(1.0), vec3(0.0), uv, 1.0);
    }
    let t_s2 = intersect_sphere(ray_orig, ray_dir, vec3(1.8, -3.0, 1.0), 2.0);
    if (t_s2 > 0.0 && t_s2 < hit.t) { 
        let p = ray_orig+ray_dir*t_s2;
        hit = Hit(t_s2, p, normalize(p - vec3(1.8, -3.0, 1.0)), vec3(0.9), vec3(0.), vec2(0.), 0.0); 
    }
    let t_light = intersect_plane(ray_orig, ray_dir, vec3(0.,-1.,0.), -(params.room_height - 0.05));
    let p_light = ray_orig + ray_dir * t_light;
    if (t_light > 0.0 && t_light < hit.t && abs(p_light.x) < params.room_width && abs(p_light.z) < params.room_depth) { 
        hit = Hit(t_light, p_light, vec3(0.,-1.,0.), vec3(1.0), vec3(25.0), vec2(0.), 0.0); 
    }
    // Force Double-Sided Rendering: Normal must face the ray
    if (dot(hit.normal, ray_dir) > 0.0) {
        hit.normal = -hit.normal;
    }
    
    return hit;
}

@compute @workgroup_size(8, 8)
fn cp_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let screen = params.resolution;
    if (f32(id.x) >= screen.x || f32(id.y) >= screen.y) { return; }
    
    let r_pos = params.player_pos.xyz;
    var final_radiance = vec3(0.0);
    
    let sample_count = i32(params.spp);
    for (var s = 0; s < sample_count; s++) {
        let uv = ((vec2<f32>(id.xy) + 0.5) / screen) * 2.0 - 1.0;
        
        let local_dir = normalize(vec3(uv.x * (screen.x / screen.y), -uv.y, -3.0));
        let ray_dir_init = (params.camera_rot * vec4(local_dir, 0.0)).xyz;
        
        var radiance = vec3(0.0);
        var throughput = vec3(1.0);
        var ray_orig = r_pos;
        var ray_dir = ray_dir_init;

        for (var i = 0; i < i32(params.bounces); i++) {
            let hit = world_hit(ray_orig, ray_dir);
            
            if (i == 0) { 
                textureStore(g_buffer_tex, id.xy, vec4(hit.normal, hit.t));
            }

            if (hit.t > 1e10) { break; }
            
            if (i == 0) { 
                radiance += throughput * hit.emissive;
            }
            
            var albedo = hit.color;
            if (hit.use_tex > 0.5) { albedo *= textureSampleLevel(box_tex, box_sampler, hit.uv, 0.0).rgb; }
            
            // NEXT EVENT ESTIMATION (NEE)
            let u_light = get_blue_noise(id.xy, u32(params.frame_index), u32(i * 10 + s * 100) + 1u);
            let light_sample_pos = vec3(
                (u_light.x * 2.0 - 1.0) * (params.room_width - 0.1), 
                params.room_height - 0.05, 
                (u_light.y * 2.0 - 1.0) * (params.room_depth - 0.1)
            );
            let light_vec = light_sample_pos - hit.pos;
            let light_dist = length(light_vec);
            let light_dir = light_vec / light_dist;
            
            let cos_l = dot(vec3(0.0, -1.0, 0.0), -light_dir);
            let cos_s = dot(hit.normal, light_dir);
            
            if (cos_l > 0.0 && cos_s > 0.0 && length(hit.emissive) < 1.0) {
                let shadow_ray_origin = hit.pos + (hit.normal * 0.01);
                let shadow_hit = world_hit(shadow_ray_origin, light_dir);
                if (shadow_hit.t >= light_dist - 0.01 || length(shadow_hit.emissive) > 0.5) {
                    let light_area = (params.room_width * 2.0) * (params.room_depth * 2.0);
                    let cos_l_safe = max(0.05, cos_l);
                    let solid_angle_pdf = max(0.01, (light_dist * light_dist) / (cos_l_safe * light_area));
                    let brdf = albedo / 3.14159265;
                    radiance += throughput * (brdf * vec3(5.0) * cos_s / solid_angle_pdf);
                }
            }
            
            throughput *= albedo;
            
            if (max(throughput.x, max(throughput.y, throughput.z)) < 0.005) { break; }
            let u_hemi = get_blue_noise(id.xy, u32(params.frame_index), u32(i * 10 + s * 100) + 2u);
            ray_dir = cosine_weighted_hemisphere(hit.normal, u_hemi);
            ray_orig = hit.pos + (hit.normal * 0.01);
        }
        
        // Strip out NaNs via select, guaranteeing pure rendering arrays
        let is_valid = !any(radiance != radiance); 
        radiance = select(vec3(0.0), clamp(radiance, vec3(0.0), vec3(25.0)), is_valid);
        final_radiance += radiance;
    }
    
    final_radiance /= f32(sample_count);
    
    // Store raw per-frame radiance for the decoupled temporal pass
    let gbuf_data = textureLoad(g_buffer_tex, vec2<i32>(id.xy));
    let c_depth = gbuf_data.w;
    textureStore(current_radiance_tex, id.xy, vec4(final_radiance, c_depth));
}
