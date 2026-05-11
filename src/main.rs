mod font_atlas;

use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::{
    event::*,
    event_loop::EventLoop,
    window::WindowBuilder,
};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    prev: [f32; 2],
    next: [f32; 2],
    side: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PolyVertex {
    pos: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TextVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    anchor: [f32; 2],
    size: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    translate: [f32; 2],
    scale: f32,
    thickness: f32,
    resolution: [f32; 2],
    y_stretch: f32,
    _pad1: f32,
    color: [f32; 4],
    mouse_pos: [f32; 2],
    raw_mouse_x: f32,
    max_dist: f32,
}

struct ZoomAnimation {
    start_time: Instant,
    duration: Duration,
    start_scale: f64,
    target_scale: f64,
    start_translate: [f64; 2],
    target_translate: [f64; 2],
}

struct Stage {
    name: String,
    start: String,
    finish: String,
    date: String,
    max_dist: f32,
    max_ele: f32,
    min_ele: f32,
    sparkline: Vec<f32>,
    vertices: Vec<f32>, // raw floats
    indices: Vec<u32>,
    profile_points: Vec<[f32; 2]>,
}


struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<winit::window::Window>,

    render_pipeline: wgpu::RenderPipeline,
    poly_render_pipeline: wgpu::RenderPipeline,
    text_render_pipeline: wgpu::RenderPipeline,
    text_screen_pipeline: wgpu::RenderPipeline,
    text_ui_pipeline: wgpu::RenderPipeline,
    reticule_render_pipeline: wgpu::RenderPipeline,

    dot_render_pipeline: wgpu::RenderPipeline,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    poly_vertex_buffer: wgpu::Buffer,
    poly_index_buffer: wgpu::Buffer,
    axes_vertex_buffer: wgpu::Buffer,
    axes_index_buffer: wgpu::Buffer,
    static_text_buffer: wgpu::Buffer,

    num_indices: u32,
    num_poly_indices: u32,
    num_axes_indices: u32,
    num_static_text_vertices: u32,
    
    stages: Vec<Stage>,
    selected_stage_idx: usize,
    sidebar_text_buffer: wgpu::Buffer,
    num_sidebar_text_vertices: u32,
    
    max_dist: f32,
    max_ele: f32,
    profile_points: Vec<[f32; 2]>,

    pos_translate: [f64; 2],
    pos_scale: f64,
    initial_scale: f64,
    mouse_pos: [f32; 2],
    mouse_pressed: bool,
    last_mouse_pos: [f64; 2],

    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group: Option<wgpu::BindGroup>,
    
    sidebar_bg_buffer: wgpu::Buffer,
    selected_bg_buffer: wgpu::Buffer,
    hover_bg_buffer: wgpu::Buffer,
    sparkline_buffer: wgpu::Buffer,
    ui_render_pipeline: wgpu::RenderPipeline,
    selected_render_pipeline: wgpu::RenderPipeline,
    hover_render_pipeline: wgpu::RenderPipeline,
    sparkline_render_pipeline: wgpu::RenderPipeline,
    
    hover_stage_idx: Option<usize>,
    
    animation: Option<ZoomAnimation>,
    fa: Option<font_atlas::FontAtlas>,
    sidebar_scroll_y: f32,
    sidebar_target_scroll_y: f32,
}


impl<'a> State<'a> {
    async fn new(window: Arc<winit::window::Window>) -> State<'a> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(Arc::clone(&window)).unwrap();
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
        }, None).await.unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(surface_caps.formats[0]);
        
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Mailbox,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let bin_data = std::fs::read("data/profile.bin").expect("Failed to read profile.bin");
        let mut offset = 0;
        let num_stages = u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap());
        offset += 4;

        let mut stages = Vec::new();
        for _ in 0..num_stages {
            let read_string = |off: &mut usize| {
                let len = u32::from_le_bytes(bin_data[*off..*off+4].try_into().unwrap()) as usize;
                *off += 4;
                let s = String::from_utf8_lossy(&bin_data[*off..*off+len]).to_string();
                *off += len;
                s
            };
            let name = read_string(&mut offset);
            let start = read_string(&mut offset);
            let finish = read_string(&mut offset);
            let date = read_string(&mut offset);
            
            let max_dist = f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()); offset += 4;
            let max_ele = f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()); offset += 4;
            let min_ele = f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()); offset += 4;
            
            let mut sparkline = Vec::with_capacity(60);
            for _ in 0..60 {
                sparkline.push(f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()));
                offset += 4;
            }

            let v_count = u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()) as usize; offset += 4;
            let i_count = u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()) as usize; offset += 4;

            let mut vertices = Vec::with_capacity(v_count);
            for _ in 0..v_count {
                vertices.push(f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()));
                offset += 4;
            }
            let mut indices = Vec::with_capacity(i_count);
            for _ in 0..i_count {
                indices.push(u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()));
                offset += 4;
            }

            let mut profile_points = Vec::new();
            for j in (0..v_count).step_by(14) {
                profile_points.push([vertices[j], vertices[j+1]]);
            }
            stages.push(Stage { name, start, finish, date, max_dist, max_ele, min_ele, sparkline, vertices, indices, profile_points });
        }

        let selected_stage_idx = 0;
        let active_stage = &stages[selected_stage_idx];
        let max_dist = active_stage.max_dist;
        let max_ele = active_stage.max_ele;
        let profile_points = active_stage.profile_points.clone();

        // Create buffers large enough for the biggest stage or just resize
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"), size: (active_stage.vertices.len() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&active_stage.vertices));

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"), size: (active_stage.indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&active_stage.indices));

        let mut poly_vertices = Vec::new();
        let mut poly_indices = Vec::new();
        for i in 0..profile_points.len() {
            let p = profile_points[i];
            poly_vertices.push(PolyVertex { pos: [p[0], p[1]] });
            poly_vertices.push(PolyVertex { pos: [p[0], 0.0] }); 
            if i < profile_points.len() - 1 {
                let b = (i * 2) as u32;
                poly_indices.extend_from_slice(&[b, b+2, b+1, b+1, b+2, b+3]);
            }
        }
        let poly_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Vertex Buffer"), size: (poly_vertices.len() * 8) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&poly_vertex_buffer, 0, bytemuck::cast_slice(&poly_vertices));
        let poly_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Index Buffer"), size: (poly_indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&poly_index_buffer, 0, bytemuck::cast_slice(&poly_indices));

        let fa = font_atlas::FontAtlas::from_file("data/fonts/font.ttf");
        
        // Sidebar text
        let mut sidebar_text_vertices = Vec::new();
        if let Some(ref font) = fa {
            // Title
            let title = "TOUR DE FRANCE 2026";
            let (pos, uvs) = font.get_text_geometry(title);
            let anchor = [80.0, size.height as f32 - 35.0];
            for i in 0..(pos.len() / 2) {
                sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.5 });
            }

            for (idx, stage) in stages.iter().enumerate() {
                let y_top = size.height as f32 - 60.0 - (idx as f32 * 135.0);
                
                // Name
                let name_txt = format!("{}. {}", idx + 1, stage.name);
                let (pos, uvs) = font.get_text_geometry(&name_txt);
                let anchor = [80.0, y_top - 20.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.4 });
                }

                // Cities
                let cities_txt = format!("{} > {}", stage.start, stage.finish);
                let (pos, uvs) = font.get_text_geometry(&cities_txt);
                let anchor = [80.0, y_top - 45.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.25 });
                }

                // Date & Dist
                let date_txt = format!("{}  |  {:.1} km", stage.date, stage.max_dist / 1000.0);
                let (pos, uvs) = font.get_text_geometry(&date_txt);
                let anchor = [80.0, y_top - 65.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.25 });
                }
            }


        }


        let sidebar_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (sidebar_text_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&sidebar_text_buffer, 0, bytemuck::cast_slice(&sidebar_text_vertices));

        let axes_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let axes_index_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let static_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }] });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { layout: &uniform_bind_group_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }], label: None });
        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }, wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None }] });
        let atlas_bind_group = if let Some(ref f) = fa {
            let tex = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: f.tex_size, height: f.tex_size, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
            queue.write_texture(tex.as_image_copy(), &f.rgba_data, wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(f.tex_size * 4), rows_per_image: None }, wgpu::Extent3d { width: f.tex_size, height: f.tex_size, depth_or_array_layers: 1 });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
            Some(device.create_bind_group(&wgpu::BindGroupDescriptor { layout: &atlas_bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&sampler) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) }], label: None }))
        } else { None };

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&uniform_bind_group_layout], push_constant_ranges: &[] });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&uniform_bind_group_layout, &atlas_bgl], push_constant_ranges: &[] });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let poly_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_poly", buffers: &[wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_poly", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_screen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text_screen", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        let reticule_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_reticule", buffers: &[] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_reticule", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let dot_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_dot", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_dot", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let sidebar_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 96, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let sidebar_bg_data = [
            PolyVertex { pos: [0.0, 0.0] }, PolyVertex { pos: [500.0, 0.0] }, PolyVertex { pos: [0.0, size.height as f32] },
            PolyVertex { pos: [0.0, size.height as f32] }, PolyVertex { pos: [500.0, 0.0] }, PolyVertex { pos: [500.0, size.height as f32] },
            // White border line (as a thin rectangle)
            PolyVertex { pos: [500.0, 0.0] }, PolyVertex { pos: [502.0, 0.0] }, PolyVertex { pos: [500.0, size.height as f32] },
            PolyVertex { pos: [500.0, size.height as f32] }, PolyVertex { pos: [502.0, 0.0] }, PolyVertex { pos: [502.0, size.height as f32] },
        ];
        queue.write_buffer(&sidebar_bg_buffer, 0, bytemuck::cast_slice(&sidebar_bg_data));

        let ui_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_sidebar_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let selected_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_selected_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let hover_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_sidebar_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None }); // Will use a different background logic? No, just a different color.
        
        // Sparkline pipeline
        let sparkline_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_white", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        let selected_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 48, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let hover_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 48, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        // Sparklines buffer (all stages)
        let mut spark_vertices = Vec::new();
        for (idx, stage) in stages.iter().enumerate() {
            let y_base = size.height as f32 - 60.0 - (idx as f32 * 135.0) - 125.0;
            let x_start = 80.0;
            let width = 340.0;
            let height = 45.0;


            let min = stage.min_ele;
            let max = stage.max_ele;
            let range = (max - min).max(1.0);
            
            for j in 0..59 {
                let x1 = x_start + (j as f32 / 59.0) * width;
                let x2 = x_start + ((j+1) as f32 / 59.0) * width;
                let y1 = y_base + ((stage.sparkline[j] - min) / range) * height;
                let y2 = y_base + ((stage.sparkline[j+1] - min) / range) * height;
                
                // Draw as a very thin rectangle (1px)
                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx*dx + dy*dy).sqrt();
                let ux = -dy / len;
                let uy = dx / len;
                let w = 1.0;
                
                spark_vertices.push(PolyVertex { pos: [x1 + ux*w, y1 + uy*w] });
                spark_vertices.push(PolyVertex { pos: [x1 - ux*w, y1 - uy*w] });
                spark_vertices.push(PolyVertex { pos: [x2 + ux*w, y2 + uy*w] });
                spark_vertices.push(PolyVertex { pos: [x1 - ux*w, y1 - uy*w] });
                spark_vertices.push(PolyVertex { pos: [x2 - ux*w, y2 - uy*w] });
                spark_vertices.push(PolyVertex { pos: [x2 + ux*w, y2 + uy*w] });
            }
        }
        let sparkline_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (spark_vertices.len() * 8) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&sparkline_buffer, 0, bytemuck::cast_slice(&spark_vertices));

        let mut state = State {
            surface, device, queue, config, size, window,
            render_pipeline, poly_render_pipeline, text_render_pipeline, text_screen_pipeline, text_ui_pipeline, reticule_render_pipeline, dot_render_pipeline,
            ui_render_pipeline, selected_render_pipeline, hover_render_pipeline, sparkline_render_pipeline,
            sidebar_bg_buffer, selected_bg_buffer, hover_bg_buffer, sparkline_buffer,
            hover_stage_idx: None,
            vertex_buffer, index_buffer, poly_vertex_buffer, poly_index_buffer, axes_vertex_buffer, axes_index_buffer, static_text_buffer,
            num_indices: active_stage.indices.len() as u32, num_poly_indices: poly_indices.len() as u32, num_axes_indices: 0, num_static_text_vertices: 0,
            stages, selected_stage_idx, sidebar_text_buffer, num_sidebar_text_vertices: 0,
            max_dist, max_ele, profile_points, pos_translate: [0.0, 0.0], pos_scale: 1.0, initial_scale: 1.0,
            mouse_pos: [0.0, 0.0], mouse_pressed: false, last_mouse_pos: [0.0, 0.0], uniform_buffer, uniform_bind_group, atlas_bind_group, animation: None, fa,
            sidebar_scroll_y: 0.0,
            sidebar_target_scroll_y: 0.0,
        };

        state.rebuild_ui();
        state.update_axes();
        state.select_stage(0);

        state
    }

    fn rebuild_ui(&mut self) {
        let size = self.size;
        let scroll = self.sidebar_scroll_y;
        let margin = 35.0; // Notre "CSS margin-left"
        let mut sidebar_text_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            let title = "TOUR DE FRANCE 2026";
            let (pos, uvs) = font.get_text_geometry(title);
            let anchor = [margin, size.height as f32 - 35.0 + scroll];
            for i in 0..(pos.len() / 2) {
                sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.5 });
            }

            for (idx, stage) in self.stages.iter().enumerate() {
                let y_top = size.height as f32 - 60.0 - (idx as f32 * 135.0) + scroll;
                let anchor_x = margin;
                
                let name_txt = format!("{}. {}", idx + 1, stage.name);
                let (pos, uvs) = font.get_text_geometry(&name_txt);
                let a = [anchor_x, y_top - 20.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: a, size: 0.4 });
                }

                let cities_txt = format!("{} > {}", stage.start, stage.finish);
                let (pos, uvs) = font.get_text_geometry(&cities_txt);
                let a = [anchor_x, y_top - 45.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: a, size: 0.25 });
                }

                let date_txt = format!("{}  |  {:.1} km", stage.date, stage.max_dist / 1000.0);
                let (pos, uvs) = font.get_text_geometry(&date_txt);
                let a = [anchor_x, y_top - 65.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: a, size: 0.25 });
                }
            }
        }
        self.num_sidebar_text_vertices = sidebar_text_vertices.len() as u32;
        self.sidebar_text_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (sidebar_text_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.sidebar_text_buffer, 0, bytemuck::cast_slice(&sidebar_text_vertices));

        // Background + Scrollbar handle
        let mut sidebar_bg_data = vec![
            PolyVertex { pos: [0.0, 0.0] }, PolyVertex { pos: [350.0, 0.0] }, PolyVertex { pos: [0.0, size.height as f32] },
            PolyVertex { pos: [0.0, size.height as f32] }, PolyVertex { pos: [350.0, 0.0] }, PolyVertex { pos: [350.0, size.height as f32] },
            PolyVertex { pos: [350.0, 0.0] }, PolyVertex { pos: [352.0, 0.0] }, PolyVertex { pos: [350.0, size.height as f32] },
            PolyVertex { pos: [350.0, size.height as f32] }, PolyVertex { pos: [352.0, 0.0] }, PolyVertex { pos: [352.0, size.height as f32] },
        ];

        // Visual Scrollbar Handle
        let total_h = self.stages.len() as f32 * 135.0;
        let view_h = size.height as f32 - 100.0;
        if total_h > view_h {
            let handle_h = (view_h / total_h) * view_h;
            let handle_y = (scroll / (total_h - view_h)) * (view_h - handle_h);
            let y0 = size.height as f32 - 100.0 - handle_y;
            let y1 = y0 - handle_h;
            sidebar_bg_data.extend_from_slice(&[
                PolyVertex { pos: [345.0, y1] }, PolyVertex { pos: [348.0, y1] }, PolyVertex { pos: [345.0, y0] },
                PolyVertex { pos: [345.0, y0] }, PolyVertex { pos: [348.0, y1] }, PolyVertex { pos: [348.0, y0] },
            ]);
        }

        self.sidebar_bg_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (sidebar_bg_data.len() * 8) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.sidebar_bg_buffer, 0, bytemuck::cast_slice(&sidebar_bg_data));

        let mut spark_vertices = Vec::new();
        for (idx, stage) in self.stages.iter().enumerate() {
            let y_base = size.height as f32 - 60.0 - (idx as f32 * 135.0) - 125.0 + scroll;
            let x_start = margin;
            let width = 250.0;
            let height = 45.0;
            let min = stage.min_ele;
            let max = stage.max_ele;
            let range = (max - min).max(1.0);
            for j in 0..59 {
                let x1 = x_start + (j as f32 / 59.0) * width;
                let x2 = x_start + ((j+1) as f32 / 59.0) * width;
                let y1 = y_base + ((stage.sparkline[j] - min) / range) * height;
                let y2 = y_base + ((stage.sparkline[j+1] - min) / range) * height;
                let dx = x2 - x1; let dy = y2 - y1; let len = (dx*dx + dy*dy).sqrt();
                let ux = -dy / len; let uy = dx / len; let w = 1.0;
                spark_vertices.push(PolyVertex { pos: [x1 + ux*w, y1 + uy*w] });
                spark_vertices.push(PolyVertex { pos: [x1 - ux*w, y1 - uy*w] });
                spark_vertices.push(PolyVertex { pos: [x2 + ux*w, y2 + uy*w] });
                spark_vertices.push(PolyVertex { pos: [x1 - ux*w, y1 - uy*w] });
                spark_vertices.push(PolyVertex { pos: [x2 - ux*w, y2 - uy*w] });
                spark_vertices.push(PolyVertex { pos: [x2 + ux*w, y2 + uy*w] });
            }
        }
        self.sparkline_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (spark_vertices.len() * 8) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.sparkline_buffer, 0, bytemuck::cast_slice(&spark_vertices));
        
        self.select_stage(self.selected_stage_idx);
    }

    fn update_axes(&mut self) {
        let mut axes_vertices = Vec::new();
        let mut axes_indices = Vec::new();
        let max_dist = self.max_dist;
        let ext_x = max_dist * 0.05;
        let ext_y = 2700.0 * 0.1;
        
        let mut add_line = |p1: [f32; 2], p2: [f32; 2]| {
            let base = axes_vertices.len() as u32;
            axes_vertices.push(Vertex { pos: p1, prev: p1, next: p2, side: 1.0 });
            axes_vertices.push(Vertex { pos: p1, prev: p1, next: p2, side: -1.0 });
            axes_vertices.push(Vertex { pos: p2, prev: p1, next: p2, side: 1.0 });
            axes_vertices.push(Vertex { pos: p2, prev: p1, next: p2, side: -1.0 });
            axes_indices.extend_from_slice(&[base, base+1, base+2, base+1, base+3, base+2]);
        };
        
        add_line([-ext_x, 0.0], [max_dist + ext_x, 0.0]);
        add_line([0.0, -ext_y], [0.0, 2700.0 + ext_y]);
        add_line([max_dist, -ext_y], [max_dist, 2700.0 + ext_y]);

        let mut static_text_vertices = Vec::new();
        let tick_len = max_dist * 0.01;
        for h in (0..=2700).step_by(200) {
            let y = h as f32;
            add_line([-tick_len, y], [0.0, y]);
            if let Some(ref font) = self.fa {
                let text = format!("{}m", h);
                let (pos, uvs) = font.get_text_geometry(&text);
                let anchor = [-tick_len * 3.0, y];
                let size = 0.3;
                for i in 0..(pos.len() / 2) {
                    static_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size });
                }
            }
        }

        self.axes_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.axes_vertex_buffer, 0, bytemuck::cast_slice(&axes_vertices));
        self.axes_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_indices.len() * 4) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.axes_index_buffer, 0, bytemuck::cast_slice(&axes_indices));
        self.static_text_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (static_text_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.static_text_buffer, 0, bytemuck::cast_slice(&static_text_vertices));
        self.num_axes_indices = axes_indices.len() as u32;
        self.num_static_text_vertices = static_text_vertices.len() as u32;
    }

    fn select_stage(&mut self, idx: usize) {
        if idx >= self.stages.len() { return; }
        self.selected_stage_idx = idx;
        let active_stage = &self.stages[idx];
        self.max_dist = active_stage.max_dist;
        self.max_ele = active_stage.max_ele;
        self.profile_points = active_stage.profile_points.clone();

        // Write to existing buffers (assuming they are large enough or recreated if needed)
        // For simplicity and to avoid size issues, we'll recreate them here if size changed
        self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"), size: (active_stage.vertices.len() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&active_stage.vertices));

        self.index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"), size: (active_stage.indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&active_stage.indices)); 
        self.num_indices = active_stage.indices.len() as u32;

        let mut poly_vertices = Vec::new();
        let mut poly_indices = Vec::new();
        for i in 0..self.profile_points.len() {
            let p = self.profile_points[i];
            poly_vertices.push(PolyVertex { pos: [p[0], p[1]] });
            poly_vertices.push(PolyVertex { pos: [p[0], 0.0] }); 
            if i < self.profile_points.len() - 1 {
                let b = (i * 2) as u32;
                poly_indices.extend_from_slice(&[b, b+2, b+1, b+1, b+2, b+3]);
            }
        }
        self.poly_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Vertex Buffer"), size: (poly_vertices.len() * 8) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        self.queue.write_buffer(&self.poly_vertex_buffer, 0, bytemuck::cast_slice(&poly_vertices));
        self.poly_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Index Buffer"), size: (poly_indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        self.queue.write_buffer(&self.poly_index_buffer, 0, bytemuck::cast_slice(&poly_indices)); 
        self.num_poly_indices = poly_indices.len() as u32;

        let y_top = self.size.height as f32 - 60.0 - (idx as f32 * 135.0) + self.sidebar_scroll_y;
        let sel_data = [
            PolyVertex { pos: [0.0, y_top - 135.0] }, PolyVertex { pos: [350.0, y_top - 135.0] }, PolyVertex { pos: [0.0, y_top] },
            PolyVertex { pos: [0.0, y_top] }, PolyVertex { pos: [350.0, y_top - 135.0] }, PolyVertex { pos: [350.0, y_top] },
        ];

        self.queue.write_buffer(&self.selected_bg_buffer, 0, bytemuck::cast_slice(&sel_data));

        self.update_axes();
 
        let margin_x = 400.0;
        let graph_width = (self.size.width as f64) - margin_x - 100.0;
        self.initial_scale = graph_width / (self.max_dist as f64);
        self.pos_scale = self.initial_scale;
        self.pos_translate = [margin_x, (self.size.height as f64) * 0.15];

    }

    fn update(&mut self) {
        // Sidebar scroll smoothing
        if (self.sidebar_target_scroll_y - self.sidebar_scroll_y).abs() > 0.1 {
            self.sidebar_scroll_y += (self.sidebar_target_scroll_y - self.sidebar_scroll_y) * 0.15;
            self.rebuild_ui();
        }

        if let Some(ref anim) = self.animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let duration = anim.duration.as_secs_f64();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0 - (1.0 - t).powi(3); 
            self.pos_scale = anim.start_scale + (anim.target_scale - anim.start_scale) * eased_t;
            self.pos_translate[0] = anim.start_translate[0] + (anim.target_translate[0] - anim.start_translate[0]) * eased_t;
            self.pos_translate[1] = anim.start_translate[1] + (anim.target_translate[1] - anim.start_translate[1]) * eased_t;
            if t >= 1.0 { self.animation = None; }
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.rebuild_ui();
        }
    }



    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let graph_height = (self.size.height as f64) * 0.7;
        let y_stretch = graph_height / (2700.0 * self.initial_scale); 
        let dyn_thickness = (1.8 * (self.pos_scale / self.initial_scale).powf(0.40)) as f32;
        let rel_scale = (self.pos_scale / self.initial_scale) as f32;
        let capped_rel_scale = rel_scale.min(10.0);
        
        let mouse_world_x = (self.mouse_pos[0] - self.pos_translate[0] as f32) / self.pos_scale as f32;
        let world_x = mouse_world_x.clamp(0.0, self.max_dist);
        let mut current_ele = 0.0;
        if !self.profile_points.is_empty() {
            if world_x <= 0.0 {
                current_ele = self.profile_points[0][1];
            } else if world_x >= self.max_dist {
                current_ele = self.profile_points.last().unwrap()[1];
            } else {
                for i in 0..self.profile_points.len()-1 {
                    let p1 = self.profile_points[i];
                    let p2 = self.profile_points[i+1];
                    if world_x >= p1[0] && world_x <= p2[0] {
                        let t = (world_x - p1[0]) / (p2[0] - p1[0]);
                        current_ele = p1[1] + (p2[1] - p1[1]) * t;
                        break;
                    }
                }
            }
        }
        let profile_x_screen = world_x * self.pos_scale as f32 + self.pos_translate[0] as f32;
        let profile_y_screen = (current_ele * y_stretch as f32 * self.pos_scale as f32 + self.pos_translate[1] as f32);
        let uniforms = Uniforms {
            translate: [self.pos_translate[0] as f32, self.pos_translate[1] as f32],
            scale: self.pos_scale as f32, thickness: dyn_thickness,
            resolution: [self.size.width as f32, self.size.height as f32],
            y_stretch: y_stretch as f32, _pad1: capped_rel_scale, color: [1.0, 1.0, 1.0, 1.0],
            mouse_pos: [profile_x_screen, profile_y_screen],
            raw_mouse_x: if mouse_world_x >= 0.0 && mouse_world_x <= self.max_dist && self.mouse_pos[0] > 350.0 { self.mouse_pos[0] } else { -1000.0 },
            max_dist: self.max_dist,
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut dyn_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            let gap = 15.0; let s = 0.4; let row_h = font.font_size * 1.4;
            let half_h = (row_h * s * capped_rel_scale) / 2.0;
            
            let alt_text = format!("{:.0} m", current_ele);
            let (pos_alt, uvs_alt) = font.get_text_geometry(&alt_text);
            let anchor_alt = [profile_x_screen + gap, profile_y_screen + half_h + 5.0];
            for i in 0..(pos_alt.len() / 2) { 
                dyn_vertices.push(TextVertex { pos: [pos_alt[i*2], pos_alt[i*2+1]], uv: [uvs_alt[i*2], uvs_alt[i*2+1]], anchor: anchor_alt, size: s }); 
            }

            let dist_text = format!("{:.2} km", world_x / 1000.0);
            let (pos_dist, uvs_dist) = font.get_text_geometry(&dist_text);
            let anchor_dist = [profile_x_screen + gap, profile_y_screen - half_h - 5.0];
            for i in 0..(pos_dist.len() / 2) { 
                dyn_vertices.push(TextVertex { pos: [pos_dist[i*2], pos_dist[i*2+1]], uv: [uvs_dist[i*2], uvs_dist[i*2+1]], anchor: anchor_dist, size: s }); 
            }
        }
        let dyn_buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (dyn_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&dyn_buf, 0, bytemuck::cast_slice(&dyn_vertices));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None });
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            
            // Draw sidebar (no clipping)
            pass.set_pipeline(&self.ui_render_pipeline);
            pass.set_vertex_buffer(0, self.sidebar_bg_buffer.slice(..));
            let num_bg = if self.stages.len() as f32 * 135.0 > self.size.height as f32 - 100.0 { 18 } else { 12 };
            pass.draw(0..num_bg, 0..1); // BG + Border
            
            pass.set_pipeline(&self.selected_render_pipeline);
            pass.set_vertex_buffer(0, self.selected_bg_buffer.slice(..));
            pass.draw(0..6, 0..1);

            // Hover
            if let Some(idx) = self.hover_stage_idx {
                let y_top = self.size.height as f32 - 60.0 - (idx as f32 * 135.0) + self.sidebar_scroll_y;
                let hover_data = [
                    PolyVertex { pos: [0.0, y_top - 135.0] }, PolyVertex { pos: [350.0, y_top - 135.0] }, PolyVertex { pos: [0.0, y_top] },
                    PolyVertex { pos: [0.0, y_top] }, PolyVertex { pos: [350.0, y_top - 135.0] }, PolyVertex { pos: [350.0, y_top] },
                ];


                self.queue.write_buffer(&self.hover_bg_buffer, 0, bytemuck::cast_slice(&hover_data));
                pass.set_pipeline(&self.hover_render_pipeline);
                pass.set_vertex_buffer(0, self.hover_bg_buffer.slice(..));
                pass.draw(0..6, 0..1);
            }

            pass.set_pipeline(&self.sparkline_render_pipeline);
            pass.set_vertex_buffer(0, self.sparkline_buffer.slice(..));
            pass.draw(0..(self.stages.len() as u32 * 59 * 6), 0..1);

            if let Some(ref bg) = self.atlas_bind_group {
                pass.set_pipeline(&self.text_ui_pipeline); 
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, self.sidebar_text_buffer.slice(..)); 
                pass.draw(0..self.num_sidebar_text_vertices, 0..1);
            }

            // Draw Graph (Scissored)
            pass.set_scissor_rect(352, 0, self.size.width - 352, self.size.height);


            pass.set_pipeline(&self.poly_render_pipeline);
            pass.set_vertex_buffer(0, self.poly_vertex_buffer.slice(..));
            pass.set_index_buffer(self.poly_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_poly_indices, 0, 0..1);

            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.axes_vertex_buffer.slice(..));
            pass.set_index_buffer(self.axes_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_axes_indices, 0, 0..1);
            
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);
            
            pass.set_pipeline(&self.reticule_render_pipeline);
            pass.draw(0..6, 0..1);
            
            if let Some(ref bg) = self.atlas_bind_group {
                pass.set_pipeline(&self.text_render_pipeline); 
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, self.static_text_buffer.slice(..)); 
                pass.draw(0..self.num_static_text_vertices, 0..1);
                
                pass.set_pipeline(&self.text_screen_pipeline); 
                pass.set_vertex_buffer(0, dyn_buf.slice(..));
                let num_dyn = dyn_vertices.len() as u32;
                pass.draw(0..num_dyn, 0..1); 
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(WindowBuilder::new().with_title("TDF 2026 - Profile").build(&event_loop).unwrap());
    let mut state = pollster::block_on(State::new(Arc::clone(&window)));
    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent { ref event, window_id } if window_id == state.window.id() => match event {
            WindowEvent::CloseRequested => elwt.exit(),
            WindowEvent::Resized(s) => {
                state.resize(*s);
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.mouse_pos = [position.x as f32, (state.size.height as f64 - position.y) as f32];
                if state.mouse_pos[0] < 350.0 {
                    let y_from_top = position.y as f32;
                    let idx = ((y_from_top - 60.0 + state.sidebar_scroll_y) / 135.0) as i32;
                    if idx >= 0 && (idx as usize) < state.stages.len() {
                        state.hover_stage_idx = Some(idx as usize);
                    } else {
                        state.hover_stage_idx = None;
                    }
                } else {
                    state.hover_stage_idx = None;
                }
                if state.mouse_pressed {
                    state.pos_translate[0] += position.x - state.last_mouse_pos[0];
                    state.pos_translate[1] -= position.y - state.last_mouse_pos[1];
                }
                state.last_mouse_pos = [position.x, position.y];
            }
            WindowEvent::MouseInput { state: s, button: MouseButton::Left, .. } => {
                state.mouse_pressed = *s == ElementState::Pressed;
                if state.mouse_pressed && state.mouse_pos[0] < 350.0 {
                    let y_from_top = state.size.height as f32 - state.mouse_pos[1];
                    let idx = ((y_from_top - 60.0 + state.sidebar_scroll_y) / 135.0) as i32;
                    if idx >= 0 && (idx as usize) < state.stages.len() {
                        state.select_stage(idx as usize);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if state.mouse_pos[0] < 350.0 {
                    let amount = match delta { MouseScrollDelta::LineDelta(_, y) => *y as f32 * 100.0, MouseScrollDelta::PixelDelta(p) => p.y as f32 };
                    state.sidebar_target_scroll_y = (state.sidebar_target_scroll_y - amount).max(0.0).min((state.stages.len() as f32 * 135.0) - (state.size.height as f32 - 200.0));
                    return;
                }
                let amount = match delta { MouseScrollDelta::LineDelta(_, y) => *y as f64, MouseScrollDelta::PixelDelta(p) => p.y / 60.0 };
                let target_scale = (if amount > 0.0 { state.pos_scale * 1.5 } else { state.pos_scale / 1.5 }).clamp(state.initial_scale, state.initial_scale * 500.0);
                let mut target_translate = state.pos_translate;
                if target_scale == state.initial_scale { 
                    target_translate = [580.0, (state.size.height as f64) * 0.15]; 
                } else {
                    let wx = (state.mouse_pos[0] as f64 - state.pos_translate[0]) / state.pos_scale;
                    let wy = (state.mouse_pos[1] as f64 - state.pos_translate[1]) / state.pos_scale;
                    target_translate = [state.mouse_pos[0] as f64 - wx * target_scale, state.mouse_pos[1] as f64 - wy * target_scale];
                }
                state.animation = Some(ZoomAnimation { start_time: Instant::now(), duration: Duration::from_millis(350), start_scale: state.pos_scale, target_scale, start_translate: state.pos_translate, target_translate });
            }
            WindowEvent::RedrawRequested => {
                state.update();
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            _ => {}
        }
        Event::AboutToWait => {
            state.window.request_redraw();
        }
        _ => {}
    }).unwrap();
}
