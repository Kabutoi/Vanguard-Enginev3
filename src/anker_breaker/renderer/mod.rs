use wgpu::{Instance, Backends, PowerPreference, RequestAdapterOptions, Surface, ComputePipeline, BindGroup, RenderPipeline, Buffer};
use winit::window::Window;
use tracing::{info};
use std::sync::Arc;
use std::thread;

pub mod reconstruction;
pub use reconstruction::FsrState;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DenoiseParams {
    pub step_width: i32,
    pub pad1: i32,
    pub pad2: i32,
    pub pad3: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneParams {
    pub player_pos: [f32; 4],
    pub camera_rot: [f32; 16],
    pub resolution: [f32; 2],
    pub time: f32,
    pub frame_index: f32,
    pub spp: f32,
    pub bounces: f32,
    pub denoise_radius: f32,
    pub sharpness: f32,
    pub prev_camera_rot: [f32; 16],
    pub prev_player_pos: [f32; 4],
    pub room_width: f32,
    pub room_height: f32,
    pub room_depth: f32,
    pub padding: f32,
    pub prev_view_proj: [f32; 16],
}

pub struct VanguardRenderer {
    pub instance: Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: Option<Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    pub compute_pipeline: ComputePipeline,
    pub compute_bind_group: BindGroup,
    pub current_radiance_texture: wgpu::Texture,
    pub temporal_pipeline: wgpu::ComputePipeline,
    pub temporal_bind_group_a: wgpu::BindGroup,
    pub temporal_bind_group_b: wgpu::BindGroup,
    pub temporal_bind_group_layout: wgpu::BindGroupLayout,
    pub output_texture: wgpu::Texture,
    pub accumulation_texture_a: wgpu::Texture,
    pub accumulation_texture_b: wgpu::Texture,
    pub params_buffer: Buffer,
    pub blit_pipeline: RenderPipeline,
    pub blit_bind_group: BindGroup,
    pub start_time: std::time::Instant,
    pub last_frame_time: std::time::Instant,
    pub frame_index: u32,
    pub fps: f32,
    pub camera_pos: [f32; 3],
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub egui_state: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,
    pub width: u32,
    pub height: u32,
    pub box_texture: wgpu::Texture,
    pub box_sampler: wgpu::Sampler,
    pub frame_times: std::collections::VecDeque<f32>,
    pub g_buffer_texture: wgpu::Texture,
    pub prev_g_buffer_texture: wgpu::Texture,
    pub denoise_texture_a: wgpu::Texture,
    pub denoise_texture_b: wgpu::Texture,
    pub denoise_pipeline: wgpu::ComputePipeline,
    pub denoise_bind_groups: Vec<wgpu::BindGroup>,
    pub denoise_bind_group_a: wgpu::BindGroup,
    pub denoise_bind_group_b: wgpu::BindGroup,
    pub denoise_param_buffers: Vec<wgpu::Buffer>,
    pub spp: u32,
    pub bounces: u32,
    pub denoise_radius: u32,
    pub prev_camera_rot: [f32; 16],
    pub prev_player_pos: [f32; 4],
    pub compute_bind_group_layout: wgpu::BindGroupLayout,
    pub denoise_bind_group_layout: wgpu::BindGroupLayout,
    pub blit_bind_group_layout: wgpu::BindGroupLayout,
    pub box_view: wgpu::TextureView,
    pub blit_sampler: wgpu::Sampler,
    pub sharpness: f32,
    pub render_scale: f32,
    pub upscale_pipeline: wgpu::ComputePipeline,
    pub upscale_bind_group: wgpu::BindGroup,
    pub upscale_bind_group_layout: wgpu::BindGroupLayout,
    pub upscale_sampler: wgpu::Sampler,
    pub pending_render_scale: Option<f32>,
    pub room_width: f32,
    pub room_height: f32,
    pub room_depth: f32,
    pub current_view_proj: [f32; 16],
    pub show_ui: bool,
}

impl VanguardRenderer {
    pub async fn new(window: Option<Arc<Window>>) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Initializing Vanguard GI Engine (Hardened Baseline 0.19.4)...");
        let instance = Instance::new(wgpu::InstanceDescriptor { backends: Backends::DX12, ..Default::default() });
        let surface = if let Some(ref win) = window { Some(instance.create_surface(win.clone())?) } else { None };
        let adapter = instance.request_adapter(&RequestAdapterOptions { power_preference: PowerPreference::HighPerformance, compatible_surface: surface.as_ref(), force_fallback_adapter: false }).await.ok_or("GPU FAIL")?;
        
        // CRITICAL: Required for ReadWrite storage textures in 0.19.4
        let mut required_features = wgpu::Features::empty();
        required_features |= wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features, required_limits: wgpu::Limits::default() }, None).await?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let shader = device.create_shader_module(wgpu::include_wgsl!("../../shaders/path_trace.wgsl"));
        let compute_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba16Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::ReadWrite, format: wgpu::TextureFormat::Rgba16Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
        ]});
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&compute_bind_group_layout], push_constant_ranges: &[] })), module: &shader, entry_point: "cp_main" });
        
        let temporal_shader = device.create_shader_module(wgpu::include_wgsl!("../../shaders/temporal.wgsl"));
        let temporal_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 5, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba16Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
        ]});
        let temporal_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&temporal_bind_group_layout], push_constant_ranges: &[] })), module: &temporal_shader, entry_point: "cp_main" });
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<SceneParams>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (width, height) = if let Some(ref win) = window { let size = win.inner_size(); (size.width.max(1), size.height.max(1)) } else { (1920, 1080) };
        let output_texture = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let current_radiance_texture = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
        let accumulation_texture_a = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
        let accumulation_texture_b = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
        let g_buffer_texture = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let prev_g_buffer_texture = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
        let denoise_texture_a = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let denoise_texture_b = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });

        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let current_rad_view = current_radiance_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let accumulation_view_a = accumulation_texture_a.create_view(&wgpu::TextureViewDescriptor::default());
        let accumulation_view_b = accumulation_texture_b.create_view(&wgpu::TextureViewDescriptor::default());
        let g_buffer_view = g_buffer_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let prev_g_buffer_view = prev_g_buffer_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let den_a_v = denoise_texture_a.create_view(&wgpu::TextureViewDescriptor::default());
        let den_b_v = denoise_texture_b.create_view(&wgpu::TextureViewDescriptor::default());

        // Load Snow Texture (Embedded)
        let snow_bytes = include_bytes!("../../../psx_snow_cube.png");
        let snow_img = image::load_from_memory(snow_bytes).map_err(|e| format!("Failed to load embedded texture: {}", e))?.to_rgba8();
        let (t_w, t_h) = snow_img.dimensions();
        let box_texture = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: t_w, height: t_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
        queue.write_texture(wgpu::ImageCopyTexture { texture: &box_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, &snow_img, wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * t_w), rows_per_image: Some(t_h) }, wgpu::Extent3d { width: t_w, height: t_h, depth_or_array_layers: 1 });
        let box_view = box_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let box_sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &compute_bind_group_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&current_rad_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&box_view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&box_sampler) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&g_buffer_view) },
        ]});

        let temporal_bind_group_a = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &temporal_bind_group_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&current_rad_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&accumulation_view_b) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&g_buffer_view) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&prev_g_buffer_view) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&accumulation_view_a) },
        ]});

        let temporal_bind_group_b = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &temporal_bind_group_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&current_rad_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&accumulation_view_a) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&g_buffer_view) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&prev_g_buffer_view) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&accumulation_view_b) },
        ]});

        // Denoise Pipeline
        let denoise_shader = device.create_shader_module(wgpu::include_wgsl!("../../shaders/denoise.wgsl"));
        let denoise_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba16Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ]});
        
        let denoise_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&denoise_layout], push_constant_ranges: &[] })), module: &denoise_shader, entry_point: "cp_main" });
        
        let mut denoise_param_buffers = vec![];
        for st in [1, 2, 4, 8] {
            let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<DenoiseParams>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: true });
            buf.slice(..).get_mapped_range_mut().copy_from_slice(bytemuck::bytes_of(&DenoiseParams { step_width: st, pad1: 0, pad2: 0, pad3: 0 }));
            buf.unmap();
            denoise_param_buffers.push(buf);
        }

        let denoise_bind_group_a = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &denoise_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&accumulation_view_a) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&g_buffer_view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) },
            wgpu::BindGroupEntry { binding: 4, resource: denoise_param_buffers[0].as_entire_binding() },
        ]});

        let denoise_bind_group_b = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &denoise_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&accumulation_view_b) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&g_buffer_view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) },
            wgpu::BindGroupEntry { binding: 4, resource: denoise_param_buffers[0].as_entire_binding() },
        ]});

        let mut denoise_bind_groups = vec![];
        denoise_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &denoise_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&accumulation_view_a) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&g_buffer_view) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 4, resource: denoise_param_buffers[0].as_entire_binding() } ]}));
        denoise_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &denoise_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&g_buffer_view) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_b_v) }, wgpu::BindGroupEntry { binding: 4, resource: denoise_param_buffers[1].as_entire_binding() } ]}));
        denoise_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &denoise_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_b_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&g_buffer_view) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 4, resource: denoise_param_buffers[2].as_entire_binding() } ]}));
        denoise_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &denoise_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&g_buffer_view) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_b_v) }, wgpu::BindGroupEntry { binding: 4, resource: denoise_param_buffers[3].as_entire_binding() } ]}));

        let blit_shader = device.create_shader_module(wgpu::include_wgsl!("../../shaders/blit.wgsl"));
        let blit_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ]});
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &blit_bind_group_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&den_b_v) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blit_sampler) },
        ]});
        
        // DLSS 4.5 Upscale Pipeline
        let upscale_shader = device.create_shader_module(wgpu::include_wgsl!("../../shaders/upscale.wgsl"));
        let upscale_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba8Unorm, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
        ]});
        let upscale_sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let upscale_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&upscale_bind_group_layout], push_constant_ranges: &[] })), module: &upscale_shader, entry_point: "cp_main" });
        let upscale_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &upscale_bind_group_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_b_v) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&upscale_sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&output_view) },
        ]});

        let mut config = None;
        let mut blit_pipeline = None;
        let mut format = wgpu::TextureFormat::Rgba8Unorm;
        if let (Some(ref s), Some(ref win)) = (surface.as_ref(), window.as_ref()) {
            let caps = s.get_capabilities(&adapter);
            format = caps.formats[0];
            let c = wgpu::SurfaceConfiguration { usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format, width, height, present_mode: wgpu::PresentMode::Fifo, alpha_mode: caps.alpha_modes[0], view_formats: vec![], desired_maximum_frame_latency: 2 };
            s.configure(&device, &c);
            config = Some(c);
            blit_pipeline = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&blit_bind_group_layout], push_constant_ranges: &[] })), vertex: wgpu::VertexState { module: &blit_shader, entry_point: "vs_main", buffers: &[] }, fragment: Some(wgpu::FragmentState { module: &blit_shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None }));
        }
        let egui_context = egui::Context::default();
        let egui_state = egui_winit::State::new(egui_context, egui::viewport::ViewportId::ROOT, window.as_ref().unwrap(), Some(window.as_ref().unwrap().scale_factor() as f32), None);
        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1);
        Ok(VanguardRenderer { 
            instance, adapter, device, queue, surface, config, compute_pipeline, compute_bind_group, 
            current_radiance_texture, temporal_pipeline, temporal_bind_group_a, temporal_bind_group_b, temporal_bind_group_layout,
            output_texture, accumulation_texture_a, accumulation_texture_b, params_buffer, blit_pipeline: blit_pipeline.ok_or("Fail")?, blit_bind_group, 
            start_time: std::time::Instant::now(), last_frame_time: std::time::Instant::now(), frame_index: 0, fps: 0.0,
            camera_pos: [0.0, 0.0, 4.0], camera_yaw: 0.0, camera_pitch: 0.0, egui_state, egui_renderer,
            width, height, box_texture, box_sampler,
            frame_times: std::collections::VecDeque::with_capacity(1001),
            g_buffer_texture, prev_g_buffer_texture, denoise_texture_a, denoise_texture_b, denoise_pipeline, denoise_bind_groups, denoise_bind_group_a, denoise_bind_group_b, denoise_param_buffers,
            spp: 1, bounces: 3, denoise_radius: 15,
            prev_camera_rot: [0.0; 16], prev_player_pos: [0.0; 4],
            compute_bind_group_layout, denoise_bind_group_layout: denoise_layout,
            blit_bind_group_layout, box_view, blit_sampler,
            sharpness: 0.5, render_scale: 0.5,
            upscale_pipeline, upscale_bind_group, upscale_bind_group_layout, upscale_sampler,
            pending_render_scale: None,
            room_width: 5.0, room_height: 5.0, room_depth: 5.0,
            current_view_proj: [0.0; 16],
            show_ui: true,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.width = new_size.width;
            self.height = new_size.height;
            if let (Some(ref mut config), Some(ref surface)) = (self.config.as_mut(), self.surface.as_ref()) {
                config.width = new_size.width;
                config.height = new_size.height;
                surface.configure(&self.device, config);
            }

            // Rehydrate Viewport Textures
            let (width, height) = (self.width, self.height);
            let internal_w = (width as f32 * self.render_scale) as u32;
            let internal_h = (height as f32 * self.render_scale) as u32;

            self.output_texture = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
            self.current_radiance_texture = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            self.accumulation_texture_a = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            self.accumulation_texture_b = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            self.g_buffer_texture = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
            self.prev_g_buffer_texture = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
            self.denoise_texture_a = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
            self.denoise_texture_b = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: internal_w, height: internal_h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });

            let out_v = self.output_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let current_rad_v = self.current_radiance_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let acc_a_v = self.accumulation_texture_a.create_view(&wgpu::TextureViewDescriptor::default());
            let acc_b_v = self.accumulation_texture_b.create_view(&wgpu::TextureViewDescriptor::default());
            let gbf_v = self.g_buffer_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let prev_gbf_v = self.prev_g_buffer_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let den_a_v = self.denoise_texture_a.create_view(&wgpu::TextureViewDescriptor::default());
            let den_b_v = self.denoise_texture_b.create_view(&wgpu::TextureViewDescriptor::default());

            // Update Bind Groups
            self.compute_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.compute_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&current_rad_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.box_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.box_sampler) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&gbf_v) },
            ]});

            self.temporal_bind_group_a = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.temporal_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&current_rad_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&acc_b_v) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&gbf_v) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&prev_gbf_v) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&acc_a_v) },
            ]});

            self.temporal_bind_group_b = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.temporal_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&current_rad_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&acc_a_v) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&gbf_v) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&prev_gbf_v) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&acc_b_v) },
            ]});

            self.denoise_bind_group_a = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.denoise_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&acc_a_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gbf_v) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) },
                wgpu::BindGroupEntry { binding: 4, resource: self.denoise_param_buffers[0].as_entire_binding() },
            ]});

            self.denoise_bind_group_b = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.denoise_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&acc_b_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gbf_v) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) },
                wgpu::BindGroupEntry { binding: 4, resource: self.denoise_param_buffers[0].as_entire_binding() },
            ]});

            self.denoise_bind_groups.clear();
            self.denoise_bind_groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.denoise_bind_group_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&acc_a_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gbf_v) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 4, resource: self.denoise_param_buffers[0].as_entire_binding() } ]}));
            self.denoise_bind_groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.denoise_bind_group_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gbf_v) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_b_v) }, wgpu::BindGroupEntry { binding: 4, resource: self.denoise_param_buffers[1].as_entire_binding() } ]}));
            self.denoise_bind_groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.denoise_bind_group_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_b_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gbf_v) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 4, resource: self.denoise_param_buffers[2].as_entire_binding() } ]}));
            self.denoise_bind_groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.denoise_bind_group_layout, entries: &[ wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_a_v) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&gbf_v) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&den_b_v) }, wgpu::BindGroupEntry { binding: 4, resource: self.denoise_param_buffers[3].as_entire_binding() } ]}));

            self.upscale_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.upscale_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&den_b_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.upscale_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&out_v) },
            ]});

            self.blit_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.blit_bind_group_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&out_v) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.blit_sampler) },
            ]});

            self.frame_index = 0;
        }
    }

    fn calculate_rotation_matrix(&self) -> [f32; 16] {
        let yaw = self.camera_yaw.to_radians(); let pitch = self.camera_pitch.to_radians();
        let cos_y = yaw.cos(); let sin_y = yaw.sin(); let cos_p = pitch.cos(); let sin_p = pitch.sin();
        let mut mat = [0.0; 16];
        mat[0] = cos_y; mat[2] = -sin_y; mat[4] = sin_y * sin_p; mat[5] = cos_p; mat[6] = cos_y * sin_p; mat[8] = sin_y * cos_p; mat[9] = -sin_p; mat[10] = cos_y * cos_p; mat[15] = 1.0;
        mat
    }

    fn calculate_view_proj_matrix(&self) -> [f32; 16] {
        let rot = self.calculate_rotation_matrix();
        let rot_mat = nalgebra::Matrix4::from_column_slice(&rot);
        let pos = nalgebra::Vector3::new(self.camera_pos[0], self.camera_pos[1], self.camera_pos[2]);
        let translate = nalgebra::Matrix4::new_translation(&pos);
        
        // World matrix is translation * rotation
        let world_cam = translate * rot_mat;
        // View matrix is the inverse
        let view_mat = world_cam.try_inverse().unwrap_or_else(nalgebra::Matrix4::identity);
        
        // Projection matrix mapping
        let aspect = self.width as f32 / self.height as f32;
        let mut proj = nalgebra::Matrix4::zeros();
        proj[(0,0)] = 3.0 / aspect;
        proj[(1,1)] = -3.0; // Negative 3 cleanly maps WGSL screen +Y!
        proj[(2,2)] = 1.0;
        proj[(3,2)] = -1.0;
        
        let view_proj = proj * view_mat;
        let mut out = [0.0; 16];
        out.copy_from_slice(view_proj.as_slice());
        out
    }

    pub fn render_frame(&mut self, window: &Window) {
        // DLSS 4.5 Vanguard Edition: Handle Deferred Resolution Scaling
        if let Some(scale) = self.pending_render_scale.take() {
            self.render_scale = scale;
            self.resize(winit::dpi::PhysicalSize::new(self.width, self.height));
        }

        let now = std::time::Instant::now(); let delta = now.duration_since(self.last_frame_time).as_secs_f32(); self.last_frame_time = now;
        self.frame_times.push_back(delta); if self.frame_times.len() > 1000 { self.frame_times.pop_front(); }
        
        let avg_dt: f32 = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        let max_fps = 1.0 / self.frame_times.iter().copied().fold(f32::INFINITY, f32::min).max(0.0001);
        
        // 0.1% Low
        let mut sorted = self.frame_times.iter().copied().collect::<Vec<f32>>();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let low_idx = (sorted.len() as f32 * 0.999) as usize;
        let low_01 = 1.0 / sorted.get(low_idx.min(sorted.len() - 1)).copied().unwrap_or(0.0001).max(0.0001);

        let current_rot = self.calculate_rotation_matrix();
        let params = SceneParams { 
            player_pos: [self.camera_pos[0], self.camera_pos[1], self.camera_pos[2], 0.0], 
            camera_rot: current_rot, 
            resolution: [self.accumulation_texture_a.width() as f32, self.accumulation_texture_a.height() as f32], 
            time: self.start_time.elapsed().as_secs_f32(), 
            frame_index: self.frame_index as f32,
            spp: self.spp as f32,
            bounces: self.bounces as f32,
            denoise_radius: self.denoise_radius as f32,
            sharpness: self.sharpness,
            prev_camera_rot: self.prev_camera_rot,
            prev_player_pos: self.prev_player_pos,
            room_width: self.room_width,
            room_height: self.room_height,
            room_depth: self.room_depth,
            padding: 0.0,
            prev_view_proj: self.current_view_proj,
        };
        self.prev_camera_rot = current_rot;
        self.prev_player_pos = [self.camera_pos[0], self.camera_pos[1], self.camera_pos[2], 0.0];
        self.current_view_proj = self.calculate_view_proj_matrix();

        self.queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        { 
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None }); 
            cpass.set_pipeline(&self.compute_pipeline); 
            cpass.set_bind_group(0, &self.compute_bind_group, &[]); 
            let (width, height) = (self.accumulation_texture_a.width(), self.accumulation_texture_a.height()); 
            cpass.dispatch_workgroups((width + 7) / 8, (height + 7) / 8, 1); 
        }

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            cpass.set_pipeline(&self.temporal_pipeline);
            let temporal_bg = if self.frame_index % 2 == 0 { &self.temporal_bind_group_a } else { &self.temporal_bind_group_b };
            cpass.set_bind_group(0, temporal_bg, &[]);
            let (width, height) = (self.current_radiance_texture.width(), self.current_radiance_texture.height());
            cpass.dispatch_workgroups((width + 7) / 8, (height + 7) / 8, 1);
        }
        
        let acc_curr = if self.frame_index % 2 == 0 { &self.accumulation_texture_a } else { &self.accumulation_texture_b };

        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture { texture: &self.g_buffer_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::ImageCopyTexture { texture: &self.prev_g_buffer_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: acc_curr.width(), height: acc_curr.height(), depth_or_array_layers: 1 },
        );

        let (width, height) = (self.accumulation_texture_a.width(), self.accumulation_texture_a.height()); 
        
        // SVGF À-trous loop bounds
        for iter in 0..4 {
            // Create a new pass for EVERY iteration to satisfy wgpu memory barriers
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { 
                label: Some("SVGF_Ping_Pong_Pass"), 
                timestamp_writes: None 
            }); 
            
            cpass.set_pipeline(&self.denoise_pipeline); 
            if iter == 0 {
                let den_bg = if self.frame_index % 2 == 0 { &self.denoise_bind_group_a } else { &self.denoise_bind_group_b };
                cpass.set_bind_group(0, den_bg, &[]); 
            } else {
                cpass.set_bind_group(0, &self.denoise_bind_groups[iter], &[]); 
            }
            cpass.dispatch_workgroups((width + 7) / 8, (height + 7) / 8, 1); 
        }
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None }); 
            cpass.set_pipeline(&self.upscale_pipeline); 
            cpass.set_bind_group(0, &self.upscale_bind_group, &[]); 
            let (width, height) = (self.width, self.height); 
            cpass.dispatch_workgroups((width + 7) / 8, (height + 7) / 8, 1); 
        }
        if let Some(ref surface) = self.surface {
            let surface_texture = match surface.get_current_texture() { Ok(t) => t, Err(_) => return };
            let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
            { let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None }); rpass.set_pipeline(&self.blit_pipeline); rpass.set_bind_group(0, &self.blit_bind_group, &[]); rpass.draw(0..6, 0..1); }            let raw_input = self.egui_state.take_egui_input(window);
            let old_scale = self.render_scale;
            let full_output = self.egui_state.egui_ctx().run(raw_input, |ctx| {
                if self.show_ui {
                    egui::Window::new("Vanguard Diagnostics").anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10., 10.)).collapsible(false).title_bar(false).frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_black_alpha(180))).show(ctx, |ui| {
                        ui.label(egui::RichText::new(format!("AVG FPS: {:.1}", 1.0/avg_dt)).color(egui::Color32::GREEN).size(18.0).strong());
                        ui.label(egui::RichText::new(format!("MAX FPS: {:.1}", max_fps)).color(egui::Color32::LIGHT_BLUE).size(14.0).strong());
                        ui.label(egui::RichText::new(format!("0.1% LOW: {:.1}", low_01)).color(egui::Color32::RED).size(14.0).strong());
                        ui.add(egui::Slider::new(&mut self.spp, 1..=64).text("Rays/Pixel").text_color(egui::Color32::WHITE));
                        ui.add(egui::Slider::new(&mut self.bounces, 1..=8).text("Bounces"));
                        ui.add(egui::Slider::new(&mut self.denoise_radius, 1..=20).text("Denoise Radius"));
                        ui.add(egui::Slider::new(&mut self.sharpness, 0.0..=1.0).text("DLSS 4.5 (Sharpening)"));
                        ui.add(egui::Slider::new(&mut self.render_scale, 0.25..=1.0).text("DLSS Quality (Render Scale)"));
                        ui.add(egui::Slider::new(&mut self.room_width, 0.1..=100000.0).logarithmic(true).text("Room Width"));
                        ui.add(egui::Slider::new(&mut self.room_height, 0.1..=100000.0).logarithmic(true).text("Room Height"));
                        ui.add(egui::Slider::new(&mut self.room_depth, 0.1..=100000.0).logarithmic(true).text("Room Depth"));
                        if ui.button("Reset Accumulation").clicked() { self.frame_index = 0; }
                    });
                }
            });


            if self.render_scale != old_scale {
                self.pending_render_scale = Some(self.render_scale);
                self.render_scale = old_scale; // Hold current scale until frame boundary
            }

            let paint_jobs = self.egui_state.egui_ctx().tessellate(full_output.shapes, full_output.pixels_per_point);
            let screen_descriptor = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.as_ref().unwrap().width, self.config.as_ref().unwrap().height], pixels_per_point: window.scale_factor() as f32 };
            for (id, delta) in full_output.textures_delta.set { self.egui_renderer.update_texture(&self.device, &self.queue, id, &delta); }
            let _ = self.egui_renderer.update_buffers(&self.device, &self.queue, &mut encoder, &paint_jobs, &screen_descriptor);
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }, })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None });
                self.egui_renderer.render(&mut rpass, &paint_jobs, &screen_descriptor);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            surface_texture.present();
            for id in full_output.textures_delta.free { self.egui_renderer.free_texture(&id); }
        }
        self.frame_index += 1;
    }

    pub fn save_screenshot_async(&self, path: &str) {
        let device = self.device.clone(); let queue = self.queue.clone(); let path = path.to_string(); let width = self.output_texture.width(); let height = self.output_texture.height();
        let u32_size = std::mem::size_of::<u32>() as u32; let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; let unpadded_bytes_per_row = u32_size * width; let padding = (align - unpadded_bytes_per_row % align) % align; let padded_bytes_per_row = unpadded_bytes_per_row + padding;
        let output_buffer_desc = wgpu::BufferDescriptor { size: (padded_bytes_per_row * height) as wgpu::BufferAddress, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, label: None, mapped_at_creation: false };
        let output_buffer = device.create_buffer(&output_buffer_desc);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(wgpu::ImageCopyTexture { texture: &self.output_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::ImageCopyBuffer { buffer: &output_buffer, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(padded_bytes_per_row), rows_per_image: Some(height) } }, self.output_texture.size());
        queue.submit(Some(encoder.finish()));
        thread::spawn(move || {
            let buffer_slice = output_buffer.slice(..); let (tx, rx) = std::sync::mpsc::channel(); buffer_slice.map_async(wgpu::MapMode::Read, move |v| tx.send(v).unwrap()); device.poll(wgpu::Maintain::Wait);
            if let Ok(Ok(())) = rx.recv() {
                let data = buffer_slice.get_mapped_range(); let mut png_data = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
                for row in 0..height { let start = (row * padded_bytes_per_row) as usize; let end = start + unpadded_bytes_per_row as usize; png_data.extend_from_slice(&data[start..end]); }
                let _ = image::save_buffer(path, &png_data, width, height, image::ExtendedColorType::Rgba8);
                drop(data); output_buffer.unmap();
            }
        });
    }
}
