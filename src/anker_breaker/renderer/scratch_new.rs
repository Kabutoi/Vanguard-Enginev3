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
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::SamplerBindingType::Filtering.into(), count: None }, // Fixed this line
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba8Unorm, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
        ]});
        // Fixed Sampler binding type in previous line
