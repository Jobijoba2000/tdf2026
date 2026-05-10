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
    _pad2: [f32; 2],
}

struct ZoomAnimation {
    start_time: Instant,
    duration: Duration,
    start_scale: f64,
    target_scale: f64,
    start_translate: [f64; 2],
    target_translate: [f64; 2],
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
    
    animation: Option<ZoomAnimation>,
    fa: Option<font_atlas::FontAtlas>,
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
        let max_dist = f32::from_le_bytes(bin_data[0..4].try_into().unwrap());
        let max_ele = f32::from_le_bytes(bin_data[4..8].try_into().unwrap());
        let num_floats = u32::from_le_bytes(bin_data[8..12].try_into().unwrap()) as usize;
        let num_indices = u32::from_le_bytes(bin_data[12..16].try_into().unwrap()) as usize;

        let vertices_raw: &[f32] = bytemuck::cast_slice(&bin_data[16..16 + num_floats * 4]);
        let indices: &[u32] = bytemuck::cast_slice(&bin_data[16 + num_floats * 4..16 + num_floats * 4 + num_indices * 4]);

        let mut profile_points = Vec::new();
        for i in (0..num_floats).step_by(7) {
            if vertices_raw[i+6] == 1.0 {
                profile_points.push([vertices_raw[i], vertices_raw[i+1]]);
            }
        }

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"), size: (vertices_raw.len() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(vertices_raw));

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"), size: (indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(indices));

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

        let mut axes_vertices = Vec::new();
        let mut axes_indices = Vec::new();
        let ext_x = max_dist * 0.05;
        let ext_y = 2700.0 * 0.1;
        let add_line = |p1: [f32; 2], p2: [f32; 2], idx_vec: &mut Vec<u32>, v_vec: &mut Vec<Vertex>| {
            let base = v_vec.len() as u32;
            v_vec.push(Vertex { pos: p1, prev: p1, next: p2, side: 1.0 });
            v_vec.push(Vertex { pos: p1, prev: p1, next: p2, side: -1.0 });
            v_vec.push(Vertex { pos: p2, prev: p1, next: p2, side: 1.0 });
            v_vec.push(Vertex { pos: p2, prev: p1, next: p2, side: -1.0 });
            idx_vec.extend_from_slice(&[base, base+1, base+2, base+1, base+3, base+2]);
        };
        add_line([-ext_x, 0.0], [max_dist + ext_x, 0.0], &mut axes_indices, &mut axes_vertices);
        add_line([0.0, -ext_y], [0.0, 2700.0 + ext_y], &mut axes_indices, &mut axes_vertices);
        add_line([max_dist, -ext_y], [max_dist, 2700.0 + ext_y], &mut axes_indices, &mut axes_vertices);

        let fa = font_atlas::FontAtlas::from_file("data/fonts/font.ttf");
        let mut static_text_vertices = Vec::new();
        let tick_len = max_dist * 0.01;
        for h in (0..=2700).step_by(100) {
            let y = h as f32;
            add_line([-tick_len, y], [0.0, y], &mut axes_indices, &mut axes_vertices);
            if let Some(ref font) = fa {
                let text = format!("{}m", h);
                let (pos, uvs) = font.get_text_geometry(&text);
                let anchor = [-tick_len * 3.0, y];
                let size = 0.3;
                for i in 0..(pos.len() / 2) {
                    static_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size });
                }
            }
        }

        let axes_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&axes_vertex_buffer, 0, bytemuck::cast_slice(&axes_vertices));
        let axes_index_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_indices.len() * 4) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&axes_index_buffer, 0, bytemuck::cast_slice(&axes_indices));
        let static_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (static_text_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&static_text_buffer, 0, bytemuck::cast_slice(&static_text_vertices));

        let margin_x = 100.0;
        let graph_width = (size.width as f64) - (margin_x * 2.0);
        let graph_height = (size.height as f64) * 0.7; 
        let initial_scale = graph_width / (max_dist as f64);
        let y_stretch = graph_height / (2700.0 * initial_scale); 
        let translate_y = (size.height as f64) * 0.15;
        
        let uniforms = Uniforms {
            translate: [margin_x as f32, translate_y as f32], scale: initial_scale as f32, thickness: 1.2,
            resolution: [size.width as f32, size.height as f32], y_stretch: y_stretch as f32, _pad1: 1.0,
            color: [1.0, 1.0, 1.0, 1.0], mouse_pos: [0.0, 0.0], _pad2: [0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
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
        let reticule_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_reticule", buffers: &[] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_reticule", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let dot_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_dot", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_dot", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        State {
            surface, device, queue, config, size, window,
            render_pipeline, poly_render_pipeline, text_render_pipeline, text_screen_pipeline, reticule_render_pipeline, dot_render_pipeline,
            vertex_buffer, index_buffer, poly_vertex_buffer, poly_index_buffer, axes_vertex_buffer, axes_index_buffer, static_text_buffer,
            num_indices: num_indices as u32, num_poly_indices: poly_indices.len() as u32, num_axes_indices: axes_indices.len() as u32, num_static_text_vertices: static_text_vertices.len() as u32,
            max_dist, max_ele, profile_points, pos_translate: [100.0, translate_y], pos_scale: initial_scale, initial_scale,
            mouse_pos: [0.0, 0.0], mouse_pressed: false, last_mouse_pos: [0.0, 0.0], uniform_buffer, uniform_bind_group, atlas_bind_group, animation: None, fa,
        }
    }

    fn update(&mut self) {
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

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let graph_height = (self.size.height as f64) * 0.7;
        let y_stretch = graph_height / (2700.0 * self.initial_scale); 
        let dyn_thickness = (1.2 * (self.pos_scale / self.initial_scale).powf(0.25)) as f32;
        let rel_scale = (self.pos_scale / self.initial_scale) as f32;
        
        let uniforms = Uniforms {
            translate: [self.pos_translate[0] as f32, self.pos_translate[1] as f32],
            scale: self.pos_scale as f32, thickness: dyn_thickness,
            resolution: [self.size.width as f32, self.size.height as f32],
            y_stretch: y_stretch as f32, _pad1: rel_scale, color: [1.0, 1.0, 1.0, 1.0],
            mouse_pos: self.mouse_pos, _pad2: [0.0, 0.0],
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let world_x = ((self.mouse_pos[0] - self.pos_translate[0] as f32) / self.pos_scale as f32).clamp(0.0, self.max_dist);
        let mut current_ele = 0.0;
        for i in 0..self.profile_points.len()-1 {
            let p1 = self.profile_points[i];
            let p2 = self.profile_points[i+1];
            if world_x >= p1[0] && world_x <= p2[0] {
                let t = (world_x - p1[0]) / (p2[0] - p1[0]);
                current_ele = p1[1] + (p2[1] - p1[1]) * t;
                break;
            }
        }

        let mut dyn_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            let gap = 21.0; let s = 0.5; let row_h = font.font_size * 1.4;
            let alt_text = format!("{:.0}m", current_ele);
            let (pos_alt, uvs_alt) = font.get_text_geometry(&alt_text);
            let mut width_alt = 0.0;
            for c in alt_text.chars() { width_alt += font.metrics.get(&c).map(|m| m.advance).unwrap_or(0.0); }
            let w_alt_px = width_alt * s;
            let anchor_alt = [self.mouse_pos[0] - gap - w_alt_px / 2.0, self.mouse_pos[1] + gap + (row_h * s) / 2.0];
            for i in 0..(pos_alt.len() / 2) { dyn_vertices.push(TextVertex { pos: [pos_alt[i*2], pos_alt[i*2+1]], uv: [uvs_alt[i*2], uvs_alt[i*2+1]], anchor: [anchor_alt[0], anchor_alt[1]], size: s }); }
            let dist_text = format!("{:.1}km", world_x / 1000.0);
            let (pos_dist, uvs_dist) = font.get_text_geometry(&dist_text);
            let mut width_dist = 0.0;
            for c in dist_text.chars() { width_dist += font.metrics.get(&c).map(|m| m.advance).unwrap_or(0.0); }
            let w_dist_px = width_dist * s;
            let anchor_dist = [self.mouse_pos[0] + gap + w_dist_px / 2.0, self.mouse_pos[1] + gap + (row_h * s) / 2.0];
            for i in 0..(pos_dist.len() / 2) { dyn_vertices.push(TextVertex { pos: [pos_dist[i*2], pos_dist[i*2+1]], uv: [uvs_dist[i*2], uvs_dist[i*2+1]], anchor: [anchor_dist[0], anchor_dist[1]], size: s }); }
            let dot_pos = [[-1.0,-1.0], [1.0,-1.0], [-1.0,1.0], [-1.0,1.0], [1.0,-1.0], [1.0,1.0]];
            for p in dot_pos { dyn_vertices.push(TextVertex { pos: p, uv: [0.0,0.0], anchor: [world_x, current_ele], size: 1.0 }); }
        }
        let dyn_buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (dyn_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&dyn_buf, 0, bytemuck::cast_slice(&dyn_vertices));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None });
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
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
                if dyn_vertices.len() >= 18 {
                    pass.draw(0..12, 0..1); 
                    pass.set_pipeline(&self.dot_render_pipeline); 
                    pass.draw(12..18, 0..1); 
                }
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
                state.size = *s; state.config.width = s.width; state.config.height = s.height; state.surface.configure(&state.device, &state.config);
                let margin_x = 100.0;
                let graph_width = (s.width as f64) - (margin_x * 2.0);
                let graph_height = (s.height as f64) * 0.7;
                state.initial_scale = graph_width / (state.max_dist as f64);
                let translate_y = (s.height as f64) * 0.15;
                if state.animation.is_none() { state.pos_scale = state.initial_scale; state.pos_translate = [margin_x, translate_y]; }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.mouse_pos = [position.x as f32, (state.size.height as f64 - position.y) as f32];
                if state.mouse_pressed {
                    state.pos_translate[0] += position.x - state.last_mouse_pos[0];
                    state.pos_translate[1] -= position.y - state.last_mouse_pos[1];
                }
                state.last_mouse_pos = [position.x, position.y];
            }
            WindowEvent::MouseInput { state: s, button: MouseButton::Left, .. } => state.mouse_pressed = *s == ElementState::Pressed,
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta { MouseScrollDelta::LineDelta(_, y) => *y as f64, MouseScrollDelta::PixelDelta(p) => p.y / 60.0 };
                let target_scale = (if amount > 0.0 { state.pos_scale * 1.5 } else { state.pos_scale / 1.5 }).clamp(state.initial_scale, state.initial_scale * 500.0);
                let mut target_translate = state.pos_translate;
                if target_scale == state.initial_scale { 
                    target_translate = [100.0, (state.size.height as f64) * 0.15]; 
                } else {
                    let wx = (state.mouse_pos[0] as f64 - state.pos_translate[0]) / state.pos_scale;
                    let wy = (state.mouse_pos[1] as f64 - state.pos_translate[1]) / state.pos_scale;
                    target_translate = [state.mouse_pos[0] as f64 - wx * target_scale, state.mouse_pos[1] as f64 - wy * target_scale];
                }
                state.animation = Some(ZoomAnimation { start_time: Instant::now(), duration: Duration::from_millis(350), start_scale: state.pos_scale, target_scale, start_translate: state.pos_translate, target_translate });
            }
            WindowEvent::RedrawRequested => { state.update(); if let Err(e) = state.render() { eprintln!("{:?}", e); } }
            _ => {}
        }
        Event::AboutToWait => { state.window.request_redraw(); }
        _ => {}
    }).unwrap();
}
