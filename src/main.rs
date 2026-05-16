mod font_atlas;

use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::font_atlas::FontAtlas;
use winit::{
    event::*,
    event_loop::EventLoop,
    window::WindowBuilder,
    keyboard::{Key, NamedKey},
};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 4],   // dist, ele, lx, ly
    prev: [f32; 4],
    next: [f32; 4],
    side: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PolyVertex {
    pos: [f32; 4], // x, y, lx, ly
    side: f32,     // 1.0 for top, 0.0 for bottom
    flag: f32,
    normal: [f32; 2], // Normal in LX/LY plane
}

impl PolyVertex {
    fn new(pos: [f32; 4], side: f32) -> Self {
        Self { pos, side, flag: 0.0, normal: [0.0, 0.0] }
    }
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
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    view_proj: glam::Mat4,   // 0-64
    translate: [f32; 2],     // 64-72
    scale: f32,              // 72-76
    thickness: f32,          // 76-80
    resolution: [f32; 2],    // 80-88
    y_stretch: f32,          // 88-92
    morph: f32,              // 92-96
    color: [f32; 4],         // 96-112
    mouse_pos: [f32; 2],     // 112-120
    raw_mouse_x: f32,        // 120-124
    max_dist: f32,           // 124-128
    y_min: f32,              // 128-132
    y_max: f32,              // 132-136
    rel_scale: f32,          // 136-140
    camera_tilt: f32,        // 140-144
    camera_heading: f32,     // 144-148
    global_center_x: f32,    // 148-152
    global_center_y: f32,    // 152-156
    _pad: f32,               // 156-160 (align to 16 bytes)
}

struct ZoomAnimation {
    start_time: Instant,
    duration: Duration,
    start_scale: f64,
    target_scale: f64,
    start_translate: [f64; 2],
    target_translate: [f64; 2],
}

#[derive(Copy, Clone)]
struct ScrollAnimation {
    start_time: Instant,
    duration: Duration,
    start_y: f32,
    target_y: f32,
}


#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobalVertex {
    pos: [f32; 2],
    prev: [f32; 2],
    next: [f32; 2],
    side: f32,
    color: f32,
}
impl GlobalVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlobalVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 24, shader_location: 3, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 28, shader_location: 4, format: wgpu::VertexFormat::Float32 },
            ],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum GlobalViewState {
    Inactive,
    MorphingToTopDown,
    Swapped,
    ZoomingOut,
    FullyGlobal,
    ZoomingIn,
    MorphingTo2D,
}

struct CameraAnimation {
    start_time: std::time::Instant,
    duration: std::time::Duration,
    start_angle: [f32; 2],
    target_angle: [f32; 2],
    start_offset: [f32; 2],
    target_offset: [f32; 2],
}
struct GlobalZoomAnimation {
    start_time: std::time::Instant,
    duration: std::time::Duration,
    start_scale: f64,
    target_scale: f64,
    start_center: [f32; 2],
    target_center: [f32; 2],
}

struct Stage {
    name: String,
    start: String,
    finish: String,
    date: String,
    max_dist: f32,
    max_ele: f32,
    min_ele: f32,
    global_lx: f32,
    global_ly: f32,
    sparkline: Vec<f32>,
    vertices: Vec<f32>, // raw floats
    indices: Vec<u32>,
    profile_points: Vec<[f32; 2]>,
}

struct MorphAnimation {
    start_time: Instant,
    duration: Duration,
    start_morph: f32,
    target_morph: f32,
}

struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<winit::window::Window>,

    // Pipelines
    render_pipeline: wgpu::RenderPipeline,
    poly_render_pipeline: wgpu::RenderPipeline,
    text_render_pipeline: wgpu::RenderPipeline,
    text_ui_pipeline: wgpu::RenderPipeline,
    text_screen_pipeline: wgpu::RenderPipeline,
    ui_render_pipeline: wgpu::RenderPipeline,
    selected_render_pipeline: wgpu::RenderPipeline,
    hover_render_pipeline: wgpu::RenderPipeline,
    sparkline_render_pipeline: wgpu::RenderPipeline,
    reticule_render_pipeline: wgpu::RenderPipeline,
    dot_render_pipeline: wgpu::RenderPipeline,
    header_render_pipeline: wgpu::RenderPipeline,
    axes_render_pipeline: wgpu::RenderPipeline,
    global_render_pipeline: wgpu::RenderPipeline,
    global_fill_render_pipeline: wgpu::RenderPipeline,

    // Buffers & Resources
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: wgpu::Buffer,
    depth_texture: wgpu::TextureView,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    poly_vertex_buffer: wgpu::Buffer,
    poly_index_buffer: wgpu::Buffer,
    axes_vertex_buffer: wgpu::Buffer,
    axes_index_buffer: wgpu::Buffer,
    static_text_buffer: wgpu::Buffer,
    
    sidebar_bg_buffer: wgpu::Buffer,
    sidebar_text_buffer: wgpu::Buffer,
    sparkline_buffer: wgpu::Buffer,
    stage_borders_buffer: wgpu::Buffer,
    selected_bg_buffer: wgpu::Buffer,
    hover_bg_buffer: wgpu::Buffer,
    header_text_buffer: wgpu::Buffer,
    header_bg_buffer: wgpu::Buffer,
    header_border_buffer: wgpu::Buffer,
    global_vertex_buffer: wgpu::Buffer,
    global_index_buffer: wgpu::Buffer,
    global_fill_vertex_buffer: wgpu::Buffer,
    global_fill_index_buffer: wgpu::Buffer,

    // Metadata
    num_indices: u32,
    num_poly_indices: u32,
    num_axes_indices: u32,
    num_axes_vertices: u32,
    num_static_text_vertices: u32,
    num_stage_border_vertices: u32,
    num_spark_vertices: u32,
    num_sidebar_text_vertices: u32,
    num_header_text_vertices: u32,
    num_header_border_vertices: u32,
    global_index_count: u32,
    global_fill_index_count: u32,

    // State
    profile_points: Vec<[f32; 2]>,
    max_dist: f32,
    min_ele: f32,
    max_ele: f32,
    global_max_dist: f32,
    global_max_ele: f32,
    global_max_ratio_diff: f32,
    
    mouse_pos: [f32; 2],
    mouse_pressed: bool,
    right_mouse_pressed: bool,
    last_mouse_pos: [f32; 2],
    
    pos_translate: [f64; 2],
    pos_scale: f64,
    initial_scale: f64,
    current_morph: f32,
    target_morph: f32,
    view_mode: u32, // 0: Profile, 1: Trace 3D
    ctrl_pressed: bool,
    
    camera_angle: [f32; 2], // [tilt, heading]
    camera_offset: [f32; 2],
    stage_center: [f32; 2],
    
    animation: Option<ZoomAnimation>,
    morph_animation: Option<MorphAnimation>,
    sidebar_animation: Option<ScrollAnimation>,
    
    fa: Option<FontAtlas>,
    stages: Vec<Stage>,
    selected_stage_idx: usize,
    
    sidebar_scroll_y: f32,
    sidebar_target_scroll_y: f32,
    slope_start: Option<[f32; 2]>,
    slope_end: Option<[f32; 2]>,
    slope_result: Option<(f32, f32, f32)>,

    hover_stage_idx: Option<usize>,
    global_view_state: GlobalViewState,
    global_zoom_animation: Option<GlobalZoomAnimation>,
    camera_animation: Option<CameraAnimation>,
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

        let compressed_data = include_bytes!("../data/profile.bin");
        let mut decoder = flate2::read::GzDecoder::new(&compressed_data[..]);
        let mut bin_data = Vec::new();
        use std::io::Read;
        decoder.read_to_end(&mut bin_data).expect("Failed to decompress profile.bin");
        
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
            let global_lx = f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()); offset += 4;
            let global_ly = f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()); offset += 4;
            
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
            for j in (0..v_count).step_by(26) { // 2 vertices per point, 13 floats per vertex
                profile_points.push([vertices[j], vertices[j+1]]);
            }
            stages.push(Stage { name, start, finish, date, max_dist, max_ele, min_ele, global_lx, global_ly, sparkline, vertices, indices, profile_points });
        }

        let selected_stage_idx = 0;
        let active_stage = &stages[selected_stage_idx];
        let max_dist = active_stage.max_dist;
        let max_ele = active_stage.max_ele;
        let min_ele = active_stage.min_ele;
        let profile_points = active_stage.profile_points.clone();
        
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
            let lx = active_stage.vertices[i * 26 + 2];
            let ly = active_stage.vertices[i * 26 + 3];
            poly_vertices.push(PolyVertex::new([p[0], p[1], lx, ly], 1.0));
            poly_vertices.push(PolyVertex::new([p[0], 0.0, lx, ly], 0.0)); 
            if i < profile_points.len() - 1 {
                let b = (i * 2) as u32;
                poly_indices.extend_from_slice(&[b, b+2, b+1, b+1, b+2, b+3]);
            }
        }
        let poly_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Vertex Buffer"), size: (poly_vertices.len() * std::mem::size_of::<PolyVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&poly_vertex_buffer, 0, bytemuck::cast_slice(&poly_vertices));
        let poly_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Index Buffer"), size: (poly_indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        queue.write_buffer(&poly_index_buffer, 0, bytemuck::cast_slice(&poly_indices));

        let fa = font_atlas::FontAtlas::from_bytes(include_bytes!("../data/fonts/font.ttf"));
        
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
                let (_pos, _uvs) = font.get_text_geometry(&date_txt);
                let _anchor = [80.0, y_top - 65.0];
                for _i in 0..(_pos.len() / 2) {
                }
            }
        }

        let sidebar_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let axes_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let axes_index_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let static_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 160, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let _sidebar_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 4096, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let stage_borders_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 65536, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
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

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor { label: Some("Depth Texture"), size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        use wgpu::util::DeviceExt;

        let global_data = include_bytes!("../data/vue_globale.bin");
        let mut global_offset = 0;
        let fill_num_vertices = u32::from_le_bytes(global_data[global_offset..global_offset+4].try_into().unwrap()); global_offset += 4;
        let fill_index_count = u32::from_le_bytes(global_data[global_offset..global_offset+4].try_into().unwrap()); global_offset += 4;
        let line_num_vertices = u32::from_le_bytes(global_data[global_offset..global_offset+4].try_into().unwrap()); global_offset += 4;
        let line_index_count = u32::from_le_bytes(global_data[global_offset..global_offset+4].try_into().unwrap()); global_offset += 4;

        let fill_vertices_size = (fill_num_vertices * 8) as usize;
        let fill_vertices = &global_data[global_offset..global_offset+fill_vertices_size]; global_offset += fill_vertices_size;
        let fill_indices_size = (fill_index_count * 4) as usize;
        let fill_indices = &global_data[global_offset..global_offset+fill_indices_size]; global_offset += fill_indices_size;

        let line_vertices_size = (line_num_vertices * 32) as usize;
        let line_vertices = &global_data[global_offset..global_offset+line_vertices_size]; global_offset += line_vertices_size;
        let line_indices_size = (line_index_count * 4) as usize;
        let line_indices = &global_data[global_offset..global_offset+line_indices_size];
        
        let global_fill_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Fill Vertex Buffer"),
            contents: fill_vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let global_fill_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Fill Index Buffer"),
            contents: fill_indices,
            usage: wgpu::BufferUsages::INDEX,
        });

        let global_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Vertex Buffer"),
            contents: line_vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let global_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Index Buffer"),
            contents: line_indices,
            usage: wgpu::BufferUsages::INDEX,
        });

        let global_fill_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Global Fill Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_global_fill",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_global_fill",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2000, // Pousse loin derrière
                    slope_scale: 1.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let global_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Global Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_global",
                buffers: &[GlobalVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_global",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 1000, // Pousse un peu moins loin que le remplissage
                    slope_scale: 1.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[wgpu::VertexBufferLayout { array_stride: 52, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState { constant: -1000, slope_scale: -1.0, clamp: 0.0 } }), multisample: wgpu::MultisampleState::default(), multiview: None });
        let poly_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_poly", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32, 2 => Float32, 3 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_poly", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }), multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text_graph", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_screen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text_screen", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text_graph", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text_std", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        let reticule_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_reticule", buffers: &[] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_reticule", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let dot_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_dot", buffers: &[wgpu::VertexBufferLayout { array_stride: 28, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_dot", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let sidebar_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 8192, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let sidebar_bg_data = [
            PolyVertex::new([0.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([0.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([0.0, size.height as f32, 0.0, 0.0], 0.0), PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([352.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0), PolyVertex::new([352.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([352.0, size.height as f32, 0.0, 0.0], 0.0),
        ];
        queue.write_buffer(&sidebar_bg_buffer, 0, bytemuck::cast_slice(&sidebar_bg_data));
        let ui_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_sidebar_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let header_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_header_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let selected_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_selected_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let hover_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_sidebar_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let axes_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { 
            label: Some("Axes Pipeline"), layout: Some(&pipeline_layout), 
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_axes", buffers: &[wgpu::VertexBufferLayout { array_stride: 52, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32] }] }, 
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_axes", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), 
            primitive: wgpu::PrimitiveState::default(), 
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::Always, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }), 
            multisample: wgpu::MultisampleState::default(), multiview: None 
        });
        
        // Sparkline pipeline
        let sparkline_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_yellow", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        let selected_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let hover_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

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
                
                spark_vertices.push(PolyVertex::new([x1 + ux*w, y1 + uy*w, 0.0, 0.0], 0.0));
                spark_vertices.push(PolyVertex::new([x1 - ux*w, y1 - uy*w, 0.0, 0.0], 0.0));
                spark_vertices.push(PolyVertex::new([x2 + ux*w, y2 + uy*w, 0.0, 0.0], 0.0));
                spark_vertices.push(PolyVertex::new([x1 - ux*w, y1 - uy*w, 0.0, 0.0], 0.0));
                spark_vertices.push(PolyVertex::new([x2 - ux*w, y2 - uy*w, 0.0, 0.0], 0.0));
                spark_vertices.push(PolyVertex::new([x2 + ux*w, y2 + uy*w, 0.0, 0.0], 0.0));
            }
        }
        let sparkline_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (spark_vertices.len() * std::mem::size_of::<PolyVertex>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&sparkline_buffer, 0, bytemuck::cast_slice(&spark_vertices));

        let global_max_dist = stages.iter().map(|s| s.max_dist).fold(0.0, f32::max);
        let global_max_ele = stages.iter().map(|s| s.max_ele).fold(0.0, f32::max);

        // K avec 20% de marge verticale pour que le profil ne touche jamais les axes
        let global_max_ratio_diff = stages.iter().map(|s| (s.max_ele - s.min_ele) / s.max_dist).fold(0.0f32, f32::max) * 1.2;

        let header_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let header_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 8192, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let header_border_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 8192, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut state = State {
            surface, device, queue, config, size, window,
            render_pipeline, poly_render_pipeline, text_render_pipeline, text_ui_pipeline, text_screen_pipeline,
            ui_render_pipeline, selected_render_pipeline, hover_render_pipeline, sparkline_render_pipeline, reticule_render_pipeline,
            dot_render_pipeline, header_render_pipeline, axes_render_pipeline, global_render_pipeline, global_fill_render_pipeline,
            uniform_bind_group, atlas_bind_group, uniform_buffer, depth_texture: depth_view,
            vertex_buffer, index_buffer, poly_vertex_buffer, poly_index_buffer, axes_vertex_buffer, axes_index_buffer, static_text_buffer,
            sidebar_bg_buffer, sidebar_text_buffer, sparkline_buffer, stage_borders_buffer,
            selected_bg_buffer, hover_bg_buffer, header_text_buffer, header_bg_buffer, header_border_buffer, global_vertex_buffer, global_index_buffer,
            global_fill_vertex_buffer, global_fill_index_buffer,
            num_indices: active_stage.indices.len() as u32, num_poly_indices: 0, num_axes_indices: 0, num_axes_vertices: 0, num_static_text_vertices: 0,
            num_stage_border_vertices: 0, num_spark_vertices: 0, num_sidebar_text_vertices: 0, num_header_text_vertices: 0, num_header_border_vertices: 0, global_index_count: line_index_count, global_fill_index_count: fill_index_count,
            profile_points, max_dist, min_ele, max_ele, global_max_dist, global_max_ele, global_max_ratio_diff,
            mouse_pos: [0.0, 0.0], mouse_pressed: false, right_mouse_pressed: false, last_mouse_pos: [0.0, 0.0],
            pos_translate: [0.0, 0.0], pos_scale: 1.0, initial_scale: 1.0,
            current_morph: 0.0, target_morph: 0.0, view_mode: 0, ctrl_pressed: false,
            camera_angle: [0.5, 0.0],
            camera_offset: [350.0 + ((size.width as f32 - 350.0) * 0.5), size.height as f32 * 0.5],
            stage_center: [0.0, 0.0],
            animation: None, morph_animation: None, sidebar_animation: None,
            fa, stages, selected_stage_idx: 0,
            sidebar_scroll_y: 0.0, sidebar_target_scroll_y: 0.0,
            slope_start: None, slope_end: None, slope_result: None,
            hover_stage_idx: None, global_view_state: GlobalViewState::Inactive, global_zoom_animation: None, camera_animation: None,
        };

        state.rebuild_ui();
        state.select_stage(0);
        state
    }

    fn rebuild_ui(&mut self) {
        let size = self.size;
        let scroll = self.sidebar_scroll_y;
        let margin_side = 5.0;
        let mut sidebar_text_vertices = Vec::new();
        let mut spark_vertices = Vec::new();
        let mut border_vertices = Vec::new();

        if let Some(ref font) = self.fa {
            for (idx, stage) in self.stages.iter().enumerate() {
                let y_top = size.height as f32 - 40.0 - (idx as f32 * 260.0) + scroll;
                let card_h = 230.0;
                let x_left = margin_side;
                let x_right = 345.0;
                
                // --- CADRES BLANCS (Juste les cadres) ---
                let b = 1.0; 
                let rects = [
                    [x_left, y_top - b, x_right, y_top], // top
                    [x_left, y_top - card_h, x_right, y_top - card_h + b], // bottom
                    [x_left, y_top - card_h, x_left + b, y_top], // left
                    [x_right - b, y_top - card_h, x_right, y_top], // right
                ];
                for r in rects {
                    border_vertices.push(PolyVertex::new([r[0], r[1], 0.0, 0.0], 0.0));
                    border_vertices.push(PolyVertex::new([r[2], r[1], 0.0, 0.0], 0.0));
                    border_vertices.push(PolyVertex::new([r[0], r[3], 0.0, 0.0], 0.0));
                    border_vertices.push(PolyVertex::new([r[0], r[3], 0.0, 0.0], 0.0));
                    border_vertices.push(PolyVertex::new([r[2], r[1], 0.0, 0.0], 0.0));
                    border_vertices.push(PolyVertex::new([r[2], r[3], 0.0, 0.0], 0.0));
                }

                // --- CONTENU RESTAURÉ ---
                let x_start = x_left + 15.0;
                
                // 1. Nom de l'étape
                let title = stage.name.clone();
                let (pos, uvs): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&title);
                let anchor = [x_start, y_top - 30.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.55 });
                }

                // 2. Villes (Départ > Arrivée)
                let cities = format!("{} > {}", stage.start, stage.finish);
                let (pos, uvs): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&cities);
                let anchor_c = [x_start, y_top - 62.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: anchor_c, size: 0.33 });
                }

                // 3. Date | Distance
                let info = format!("{}  |  {:.1} km", stage.date, stage.max_dist / 1000.0);
                let (pos, uvs): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&info);
                let anchor_i = [x_start, y_top - 86.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: anchor_i, size: 0.33 });
                }

                // 4. Sparklines (Profil simplifié) avec formule proportionnelle
                let width = 310.0;
                // Hauteur calculée pour que l'angle des pentes soit identique au graphique détaillé
                let graph_width = self.size.width as f32 - 500.0;
                let graph_height = self.size.height as f32 * 0.5;
                let height = (width * graph_height / graph_width).min(120.0); // légèrement réduit pour le padding
                
                let padding_bottom = 20.0;
                let y_bottom = (y_top - card_h) + padding_bottom;
                let _y_base = y_bottom; 
                
                let min = stage.min_ele;
                let max = stage.max_ele;
                
                // Même formule que le graphique principal
                let delta_e = stage.max_dist * self.global_max_ratio_diff;
                let display_min = if max <= delta_e {
                    0.0
                } else {
                    let padding = delta_e * 0.1;
                    (min - padding).max(0.0)
                };
                let display_range = delta_e.max(1.0);
                
                // Remplissage blanc semi-opaque sous la courbe
                for j in 0..59 {
                    let x1 = x_start + (j as f32 / 59.0) * width;
                    let x2 = x_start + ((j+1) as f32 / 59.0) * width;
                    let y1 = y_bottom + ((stage.sparkline[j] - display_min) / display_range) * height;
                    let y2 = y_bottom + ((stage.sparkline[j+1] - display_min) / display_range) * height;
                    
                    // Triangle fill sous la courbe
                    spark_vertices.push(PolyVertex::new([x1, y1, 0.0, 0.0], 1.0));
                    spark_vertices.push(PolyVertex::new([x2, y2, 0.0, 0.0], 1.0));
                    spark_vertices.push(PolyVertex::new([x1, y_bottom, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x1, y_bottom, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x2, y2, 0.0, 0.0], 1.0));
                    spark_vertices.push(PolyVertex::new([x2, y_bottom, 0.0, 0.0], 0.0));
                }
                
                // Ligne du profil (épaisse)
                for j in 0..59 {
                    let x1 = x_start + (j as f32 / 59.0) * width;
                    let x2 = x_start + ((j+1) as f32 / 59.0) * width;
                    let y1 = y_bottom + ((stage.sparkline[j] - display_min) / display_range) * height;
                    let y2 = y_bottom + ((stage.sparkline[j+1] - display_min) / display_range) * height;
                    let dx = x2 - x1; let dy = y2 - y1; let len = (dx*dx + dy*dy).sqrt();
                    let ux = -dy / len; let uy = dx / len; let w = 0.8;
                    spark_vertices.push(PolyVertex::new([x1 + ux*w, y1 + uy*w, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x1 - ux*w, y1 - uy*w, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x2 + ux*w, y2 + uy*w, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x2 + ux*w, y2 + uy*w, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x1 - ux*w, y1 - uy*w, 0.0, 0.0], 0.0));
                    spark_vertices.push(PolyVertex::new([x2 - ux*w, y2 - uy*w, 0.0, 0.0], 0.0));
                }
            }
        }
        self.num_spark_vertices = spark_vertices.len() as u32;
        self.sparkline_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (spark_vertices.len() * std::mem::size_of::<PolyVertex>()).max(32) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.sparkline_buffer, 0, bytemuck::cast_slice(&spark_vertices));
        self.num_stage_border_vertices = border_vertices.len() as u32;
        self.queue.write_buffer(&self.stage_borders_buffer, 0, bytemuck::cast_slice(&border_vertices));
        self.num_sidebar_text_vertices = sidebar_text_vertices.len() as u32;
        self.queue.write_buffer(&self.sidebar_text_buffer, 0, bytemuck::cast_slice(&sidebar_text_vertices));

        // Background + Scrollbar handle
        let mut sidebar_bg_data = vec![
            PolyVertex::new([0.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([0.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([0.0, size.height as f32, 0.0, 0.0], 0.0), PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([352.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0), PolyVertex::new([352.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([352.0, size.height as f32, 0.0, 0.0], 0.0),
        ];
        let total_h = self.stages.len() as f32 * 260.0;
        let view_h = size.height as f32 - 100.0;
        if total_h > view_h {
            let handle_h = (view_h / total_h) * view_h;
            let handle_y = (scroll / (total_h - view_h)) * (view_h - handle_h);
            let y0 = size.height as f32 - 100.0 - handle_y;
            let y1 = y0 - handle_h;
            sidebar_bg_data.extend_from_slice(&[
                PolyVertex::new([345.0, y1, 0.0, 0.0], 0.0), PolyVertex::new([348.0, y1, 0.0, 0.0], 0.0), PolyVertex::new([345.0, y0, 0.0, 0.0], 0.0),
                PolyVertex::new([345.0, y0, 0.0, 0.0], 0.0), PolyVertex::new([348.0, y1, 0.0, 0.0], 0.0), PolyVertex::new([348.0, y0, 0.0, 0.0], 0.0),
            ]);
        }
        self.queue.write_buffer(&self.sidebar_bg_buffer, 0, bytemuck::cast_slice(&sidebar_bg_data));

        // Mise à jour de la sélection dans la sidebar (doit suivre le scroll)
        let y_top_sel = size.height as f32 - 40.0 - (self.selected_stage_idx as f32 * 260.0) + self.sidebar_scroll_y;
        let sel_data = [
            PolyVertex::new([0.0, y_top_sel - 230.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, y_top_sel - 230.0, 0.0, 0.0], 0.0), PolyVertex::new([0.0, y_top_sel, 0.0, 0.0], 0.0),
            PolyVertex::new([0.0, y_top_sel, 0.0, 0.0], 0.0), PolyVertex::new([350.0, y_top_sel - 230.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, y_top_sel, 0.0, 0.0], 0.0),
        ];
        self.queue.write_buffer(&self.selected_bg_buffer, 0, bytemuck::cast_slice(&sel_data));

        // Header Background (adjusted for x1.2 and card-style layout)
        let rpw = (size.width as f32) - 352.0;
        let header_w = rpw * 0.5;
        let mut header_bg_data = vec![
            PolyVertex::new([355.0, size.height as f32 - 180.0, 0.0, 0.0], 0.0), PolyVertex::new([355.0 + header_w, size.height as f32 - 180.0, 0.0, 0.0], 0.0), PolyVertex::new([355.0, size.height as f32 - 5.0, 0.0, 0.0], 0.0),
            PolyVertex::new([355.0, size.height as f32 - 5.0, 0.0, 0.0], 0.0), PolyVertex::new([355.0 + header_w, size.height as f32 - 180.0, 0.0, 0.0], 0.0), PolyVertex::new([355.0 + header_w, size.height as f32 - 5.0, 0.0, 0.0], 0.0),
        ];

        // Right Header buttons (Profil / Tracé)
        let btn_w = 120.0;
        let btn_h = 45.0;
        let btn_gap = 10.0;
        let btn_x_start = size.width as f32 - (btn_w * 2.0 + btn_gap + 20.0);
        let btn_y = size.height as f32 - 60.0;

        for i in 0..2 {
            let bx = btn_x_start + (i as f32 * (btn_w + btn_gap));
            let is_selected = self.view_mode == i as u32;
            let b = if is_selected { 2.0 } else { 1.0 };
            
            // Button frame
            let rects = [
                [bx, btn_y - b, bx + btn_w, btn_y], // top
                [bx, btn_y - btn_h, bx + btn_w, btn_y - btn_h + b], // bottom
                [bx, btn_y - btn_h, bx + b, btn_y], // left
                [bx + btn_w - b, btn_y - btn_h, bx + btn_w, btn_y], // right
            ];
            for r in rects {
                border_vertices.push(PolyVertex::new([r[0], r[1], 0.0, 0.0], 0.0));
                border_vertices.push(PolyVertex::new([r[2], r[1], 0.0, 0.0], 0.0));
                border_vertices.push(PolyVertex::new([r[0], r[3], 0.0, 0.0], 0.0));
                border_vertices.push(PolyVertex::new([r[0], r[3], 0.0, 0.0], 0.0));
                border_vertices.push(PolyVertex::new([r[2], r[1], 0.0, 0.0], 0.0));
                border_vertices.push(PolyVertex::new([r[2], r[3], 0.0, 0.0], 0.0));
            }
            
        }

        self.queue.write_buffer(&self.header_bg_buffer, 0, bytemuck::cast_slice(&header_bg_data));

        // Mise à jour du texte du header (format identique aux cartes de la sidebar)
        let mut header_text_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            let stage = &self.stages[self.selected_stage_idx];
            
            // Ligne 1: Etape N
            let line1 = format!("Etape {}", self.selected_stage_idx + 1);
            let (pos1, uvs1): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&line1);
            let anchor1 = [370.0, self.size.height as f32 - 60.0];
            for i in 0..(pos1.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos1[i*2], pos1[i*2+1]], uv: [uvs1[i*2], uvs1[i*2+1]], anchor: anchor1, size: 1.32 });
            }

            // Ligne 2: Départ > Arrivée
            let line2 = format!("{} > {}", stage.start, stage.finish);
            let (pos2, uvs2): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&line2);
            let anchor2 = [370.0, self.size.height as f32 - 120.0];
            for i in 0..(pos2.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos2[i*2], pos2[i*2+1]], uv: [uvs2[i*2], uvs2[i*2+1]], anchor: anchor2, size: 0.66 });
            }

            // Ligne 3: Date | Distance
            let line3 = format!("{}  |  {:.1} km", stage.date, stage.max_dist / 1000.0);
            let (pos3, uvs3): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&line3);
            let anchor3 = [370.0, self.size.height as f32 - 160.0];
            for i in 0..(pos3.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos3[i*2], pos3[i*2+1]], uv: [uvs3[i*2], uvs3[i*2+1]], anchor: anchor3, size: 0.48 });
            }

            // Aide contextuelle en haut à droite
            let help_text = if self.view_mode == 0 { "[Espace] Voir le tracé 3D" } else { "[Espace] Retour au profil 2D" };
            let (pos_h, uvs_h): (Vec<f32>, Vec<f32>) = font.get_text_geometry(help_text);
            let anchor_h = [self.size.width as f32 - 250.0, self.size.height as f32 - 45.0];
            for i in 0..(pos_h.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos_h[i*2], pos_h[i*2+1]], uv: [uvs_h[i*2], uvs_h[i*2+1]], anchor: anchor_h, size: 0.4 });
            }
        }
        self.num_header_text_vertices = header_text_vertices.len() as u32;
        self.queue.write_buffer(&self.header_text_buffer, 0, bytemuck::cast_slice(&header_text_vertices));
    }

    fn update_axes(&mut self) {
        let mut axes_vertices = Vec::new();
        let mut axes_indices: Vec<u32> = Vec::new();
        let max_dist = self.max_dist;
        let ext_x = max_dist * 0.05;
        let delta_e_displayed = self.max_dist * self.global_max_ratio_diff;
        // Commencer à 0m si max_ele rentre dans la plage affichée
        let y_min = if self.max_ele <= delta_e_displayed {
            0.0
        } else {
            let padding = delta_e_displayed * 0.1;
            (self.min_ele - padding).max(0.0)
        };
        let y_max = y_min + delta_e_displayed;
        
        let ext_y = delta_e_displayed * 0.1;
        
        let mut add_line = |p1: [f32; 2], p2: [f32; 2]| {
            let base = axes_vertices.len() as u32;
            let n1 = [p1[0], p1[1], 0.0, 0.0];
            let n2 = [p2[0], p2[1], 0.0, 0.0];
            
            axes_vertices.push(Vertex { pos: n1, prev: n1, next: n2, side: -1.0 });
            axes_vertices.push(Vertex { pos: n1, prev: n1, next: n2, side: 1.0 });
            axes_vertices.push(Vertex { pos: n2, prev: n1, next: n2, side: -1.0 });
            axes_vertices.push(Vertex { pos: n2, prev: n1, next: n2, side: 1.0 });

            axes_indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
        };
        
        add_line([-ext_x, y_min], [max_dist + ext_x, y_min]);
        add_line([0.0, y_min - ext_y], [0.0, y_max + ext_y]);
        add_line([max_dist, y_min - ext_y], [max_dist, y_max + ext_y]);

        let mut static_text_vertices = Vec::new();
        
        let step = if delta_e_displayed > 4000.0 { 500 }
                   else if delta_e_displayed > 2000.0 { 200 }
                   else if delta_e_displayed > 1000.0 { 100 }
                   else { 50 };

        let mut start_h = (y_min / step as f32).floor() as i32 * step;
        if start_h < 0 { start_h = 0; }

        for h in (start_h..=(y_max as i32)).step_by(step as usize) {
            if h == 0 { continue; }
            let y = h as f32;
            if y < y_min { continue; }
            if let Some(ref font) = self.fa {
                let text = format!("{}m", h);
                let (pos_ax, uvs_ax): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&text);
                // Décalage fixe en coordonnées monde, cappé pour ne pas dériver au zoom
                let offset_x = -max_dist * 0.045;
                let anchor = [offset_x, y];
                let size = 0.3;
                for i in 0..(pos_ax.len() / 2) {
                    static_text_vertices.push(TextVertex { pos: [pos_ax[i*2], pos_ax[i*2+1]], uv: [uvs_ax[i*2], uvs_ax[i*2+1]], anchor, size });
                }
            }
        }

        self.axes_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_vertices.len() * 52) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
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
        self.min_ele = active_stage.min_ele;
        self.profile_points = active_stage.profile_points.clone();

        // Calculate geometric center of lx/ly
        let mut min_lx = f32::MAX; let mut max_lx = f32::MIN;
        let mut min_ly = f32::MAX; let mut max_ly = f32::MIN;
        for i in 0..(active_stage.vertices.len() / 13) {
            let lx = active_stage.vertices[i * 13 + 2];
            let ly = active_stage.vertices[i * 13 + 3];
            min_lx = min_lx.min(lx); max_lx = max_lx.max(lx);
            min_ly = min_ly.min(ly); max_ly = max_ly.max(ly);
        }
        self.stage_center = [(min_lx + max_lx) * 0.5, (min_ly + max_ly) * 0.5];
        
        // Auto-scale to fit stage in 3D
        let dx = max_lx - min_lx; let dy = max_ly - min_ly;
        let stage_size = dx.max(dy).max(1.0);
        let rpw = self.size.width as f32 - 350.0;
        let fit_scale = (rpw * 0.7) / stage_size;
        self.pos_scale = fit_scale as f64;
        self.initial_scale = fit_scale as f64;
        self.camera_offset = [350.0 + rpw * 0.5, self.size.height as f32 * 0.5];

        self.pos_translate = [350.0 + (rpw * 0.1) as f64, (self.size.height as f64) * 0.25];

        // Write to existing buffers
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

        let delta_e_displayed = self.max_dist * self.global_max_ratio_diff;
        let y_min = if self.max_ele <= delta_e_displayed {
            0.0
        } else {
            let padding = delta_e_displayed * 0.1;
            (self.min_ele - padding).max(0.0)
        };
        let poly_y_min = y_min;

        let mut poly_vertices = Vec::new();
        let mut poly_indices = Vec::new();

        // Pre-calculate smooth normals for the track
        let mut normals = Vec::with_capacity(self.profile_points.len());
        for i in 0..self.profile_points.len() {
            let lx = active_stage.vertices[i * 26 + 2];
            let ly = active_stage.vertices[i * 26 + 3];
            
            let mut n = [0.0, 0.0];
            let mut count = 0;
            
            if i > 0 {
                let lx_p = active_stage.vertices[(i-1) * 26 + 2];
                let ly_p = active_stage.vertices[(i-1) * 26 + 3];
                let dx = lx - lx_p;
                let dy = ly - ly_p;
                let len = (dx*dx + dy*dy).sqrt().max(1e-6);
                n[0] += -dy/len;
                n[1] += dx/len;
                count += 1;
            }
            if i < self.profile_points.len() - 1 {
                let lx_n = active_stage.vertices[(i+1) * 26 + 2];
                let ly_n = active_stage.vertices[(i+1) * 26 + 3];
                let dx = lx_n - lx;
                let dy = ly_n - ly;
                let len = (dx*dx + dy*dy).sqrt().max(1e-6);
                n[0] += -dy/len;
                n[1] += dx/len;
                count += 1;
            }
            
            if count > 0 {
                let mag = (n[0]*n[0] + n[1]*n[1]).sqrt().max(1e-6);
                n[0] /= mag;
                n[1] /= mag;
            } else {
                n = [1.0, 0.0];
            }
            normals.push(n);
        }

        // Smoothing pass ultra-large (window de 81 points) pour supprimer tout jitter de précision
        let mut smooth_normals = Vec::with_capacity(normals.len());
        let window_size = 1; 
        for i in 0..normals.len() {
            let mut sn = [0.0, 0.0];
            for j in -window_size..=window_size {
                let idx = (i as i32 + j).clamp(0, normals.len() as i32 - 1) as usize;
                sn[0] += normals[idx][0];
                sn[1] += normals[idx][1];
            }
            let slen = (sn[0]*sn[0] + sn[1]*sn[1]).sqrt().max(1e-6);
            smooth_normals.push([sn[0]/slen, sn[1]/slen]);
        }

        let mut count = 0;
        for i in 0..self.profile_points.len() {
            let p = self.profile_points[i];
            let lx = active_stage.vertices[i * 26 + 2];
            let ly = active_stage.vertices[i * 26 + 3];
            let n = smooth_normals[i];

            poly_vertices.push(PolyVertex { pos: [p[0], p[1], lx, ly], side: 1.0, flag: 0.0, normal: n });
            poly_vertices.push(PolyVertex { pos: [p[0], poly_y_min, lx, ly], side: 0.0, flag: 0.0, normal: n }); 
            count += 1;
        }


        for i in 0..count - 1 {
            let b = (i * 2) as u32;
            poly_indices.extend_from_slice(&[b, b+2, b+1, b+1, b+2, b+3]);
        }
        self.poly_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Vertex Buffer"), size: (poly_vertices.len() * 32) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        self.queue.write_buffer(&self.poly_vertex_buffer, 0, bytemuck::cast_slice(&poly_vertices));
        self.poly_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Poly Index Buffer"), size: (poly_indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        self.queue.write_buffer(&self.poly_index_buffer, 0, bytemuck::cast_slice(&poly_indices)); 
        self.num_poly_indices = poly_indices.len() as u32;

        self.update_axes();
 
        // Reset View
        let rpw = (self.size.width as f64) - 350.0;
        let graph_width = rpw * 0.8;
        let margin_x = 350.0 + rpw * 0.1;
        self.initial_scale = graph_width / (self.max_dist as f64);
        self.pos_scale = self.initial_scale;
        self.pos_translate = [margin_x, (self.size.height as f64 - 260.0) * 0.2];
        
        self.rebuild_ui();
    }

    fn update(&mut self) {
        // Sidebar scroll animation
        let mut scroll_finished = false;
        if let Some(anim) = self.sidebar_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f32();
            let duration = anim.duration.as_secs_f32();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0 - (1.0 - t).powi(3);
            self.sidebar_scroll_y = anim.start_y + (anim.target_y - anim.start_y) * eased_t;
            self.rebuild_ui();
            if t >= 1.0 { 
                self.sidebar_target_scroll_y = anim.target_y;
                scroll_finished = true;
            }
        }
        if scroll_finished {
            self.sidebar_animation = None;
        }

        // Zoom animation
        if let Some(ref anim) = self.animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let duration = anim.duration.as_secs_f64();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0 - (1.0 - t).powi(3);
            self.pos_scale = anim.start_scale + (anim.target_scale - anim.start_scale) * eased_t;
            if self.view_mode == 1 {
                // En 3D : l'animation interpole camera_offset
                self.camera_offset[0] = (anim.start_translate[0] + (anim.target_translate[0] - anim.start_translate[0]) * eased_t) as f32;
                self.camera_offset[1] = (anim.start_translate[1] + (anim.target_translate[1] - anim.start_translate[1]) * eased_t) as f32;
            } else {
                // En 2D : l'animation interpole pos_translate
                self.pos_translate[0] = anim.start_translate[0] + (anim.target_translate[0] - anim.start_translate[0]) * eased_t;
                self.pos_translate[1] = anim.start_translate[1] + (anim.target_translate[1] - anim.start_translate[1]) * eased_t;
            }
            if t >= 1.0 { self.animation = None; }
        }

        // Morphing animation
        if let Some(anim) = &self.morph_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f32();
            let duration = anim.duration.as_secs_f32();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 3.0 * t * t - 2.0 * t * t * t; // Smoothstep easing
            self.current_morph = anim.start_morph + (anim.target_morph - anim.start_morph) * eased_t;
            
            if t >= 1.0 {
                self.current_morph = anim.target_morph;
                self.morph_animation = None;
            }
            self.window.request_redraw();
        }


        if let Some(anim) = &self.camera_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let duration = anim.duration.as_secs_f64();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0_f64 - (1.0_f64 - t).powi(3);
            
            self.camera_angle[0] = anim.start_angle[0] + (anim.target_angle[0] - anim.start_angle[0]) * (eased_t as f32);
            self.camera_angle[1] = anim.start_angle[1] + (anim.target_angle[1] - anim.start_angle[1]) * (eased_t as f32);
            self.camera_offset[0] = anim.start_offset[0] + (anim.target_offset[0] - anim.start_offset[0]) * (eased_t as f32);
            self.camera_offset[1] = anim.start_offset[1] + (anim.target_offset[1] - anim.start_offset[1]) * (eased_t as f32);
            
            if t >= 1.0 {
                self.camera_animation = None;
            }
            self.window.request_redraw();
        }

        if self.global_view_state == GlobalViewState::MorphingTo2D {
            if self.morph_animation.is_none() {
                self.global_view_state = GlobalViewState::Inactive;
            }
        }

        if self.global_view_state == GlobalViewState::MorphingToTopDown {

            if self.morph_animation.is_none() {
                self.global_view_state = GlobalViewState::Swapped;
                let active_stage = &self.stages[self.selected_stage_idx];
                let france_width = 1_200_000.0; 
                let rpw = (self.size.width as f64) - 350.0;
                let target_scale = rpw * 0.60 / france_width;
                
                let c_x = 352.0 + (rpw as f32) * 0.5;
                let c_y = self.size.height as f32 * 0.5 - 50.0; // Un peu plus haut pour laisser de la place au texte
                
                let p_france_x = -active_stage.global_lx;
                let p_france_y = -active_stage.global_ly;
                
                let target_offset_x = c_x - (target_scale as f32) * p_france_x;
                let target_offset_y = c_y - (target_scale as f32) * p_france_y;
                
                self.global_zoom_animation = Some(GlobalZoomAnimation {
                    start_time: std::time::Instant::now(),
                    duration: std::time::Duration::from_millis(4000),
                    start_scale: self.pos_scale,
                    target_scale,
                    start_center: [self.camera_offset[0], self.camera_offset[1]],
                    target_center: [target_offset_x, target_offset_y],
                });
                self.global_view_state = GlobalViewState::ZoomingOut;
            }
        }
        if let Some(anim) = &self.global_zoom_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let delay = 0.3;
            let duration = anim.duration.as_secs_f64();
            let t_raw = (elapsed / duration).min(1.0);
            
            let eased_t = if elapsed < delay {
                0.0_f64
            } else {
                let t = ((elapsed - delay) / (duration - delay)).min(1.0);
                // Septic Ease In-Out (Puissance 7) : démarrage et fin ultra-doux
                if t < 0.5 { 64.0 * t.powi(7) } else { 1.0 - (-2.0 * t + 2.0).powi(7) / 2.0 }
            };
            
            self.pos_scale = anim.start_scale + (anim.target_scale - anim.start_scale) * eased_t;
            self.camera_offset[0] = (anim.start_center[0] + (anim.target_center[0] - anim.start_center[0]) * (eased_t as f32));
            self.camera_offset[1] = (anim.start_center[1] + (anim.target_center[1] - anim.start_center[1]) * (eased_t as f32));
            if t_raw >= 1.0 {
                self.global_zoom_animation = None;
                if self.global_view_state == GlobalViewState::ZoomingOut {
                    self.global_view_state = GlobalViewState::FullyGlobal;
                } else if self.global_view_state == GlobalViewState::ZoomingIn {
                    self.global_view_state = GlobalViewState::MorphingTo2D;
                    self.view_mode = 0;
                    self.target_morph = 0.0;
                    self.morph_animation = Some(MorphAnimation {
                        start_time: std::time::Instant::now(),
                        duration: std::time::Duration::from_millis(1400),
                        start_morph: self.current_morph,
                        target_morph: 0.0,
                    });
                    let rpw = (self.size.width as f64) - 350.0;
                    self.pos_translate = [350.0 + rpw * 0.1, (self.size.height as f64 - 260.0) * 0.2];
                }
            }
            self.window.request_redraw();
        }
    }

    pub fn get_profile_at_mouse(&self) -> [f32; 2] {
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
        [world_x, current_ele]
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor { label: Some("Depth Texture"), size: wgpu::Extent3d { width: new_size.width, height: new_size.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
            self.depth_texture = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.rebuild_ui();
            self.select_stage(self.selected_stage_idx);
        }
    }



    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let graph_height = (self.size.height as f64 - 260.0) * 0.5;
        let delta_e_displayed = (self.max_dist as f64) * (self.global_max_ratio_diff as f64);
        let y_stretch = graph_height / (delta_e_displayed * self.initial_scale); 
        
        // Commencer à 0m si max_ele rentre dans la plage affichée
        let delta_e_displayed = self.max_dist as f64 * self.global_max_ratio_diff as f64;
        let y_min = if self.max_ele <= delta_e_displayed as f32 {
            0.0
        } else {
            let padding = delta_e_displayed as f32 * 0.1;
            (self.min_ele - padding).max(0.0)
        };

        // Limiter le graphique pour qu'il ne monte pas dans le header (top - 260.0)
        if self.pos_translate[1] > (self.size.height as f64 - 260.0) {
            self.pos_translate[1] = self.size.height as f64 - 260.0;
        }

        let dyn_thickness = (1.8 * (self.pos_scale / self.initial_scale).powf(0.20)) as f32;
        let rel_scale = (self.pos_scale / self.initial_scale) as f32;
        let capped_rel_scale = rel_scale.min(10.0);
        
        // --- 3D VIEW-PROJ MATRIX CALCULATION ---
        let heading = self.camera_angle[1];
        let tilt = self.camera_angle[0];
        let rotation = glam::Mat4::from_rotation_x(-tilt) * glam::Mat4::from_rotation_z(heading);
        let s_3d = self.pos_scale as f32;
        let scale_mat = glam::Mat4::from_scale(glam::vec3(s_3d, s_3d, s_3d));
        let center_offset = glam::Mat4::from_translation(glam::vec3(-self.stage_center[0], -self.stage_center[1], 0.0));
        let screen_offset = glam::Mat4::from_translation(glam::vec3(self.camera_offset[0], self.camera_offset[1], 0.0));
        let model_view = screen_offset * scale_mat * rotation * center_offset;
        
        // Use Right-Handed orthographic projection
        let mut ortho = glam::Mat4::orthographic_rh(0.0, self.size.width as f32, 0.0, self.size.height as f32, -20000.0, 20000.0);
        // Adaptation for WGPU Z range [0, 1] instead of [-1, 1]
        let mut wgpu_fix = glam::Mat4::IDENTITY;
        wgpu_fix.z_axis.z = 0.5;
        wgpu_fix.w_axis.z = 0.5;
        let view_proj = wgpu_fix * ortho * model_view;

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
        let profile_y_screen = (current_ele - y_min) * y_stretch as f32 * self.pos_scale as f32 + self.pos_translate[1] as f32;

        let uniforms = Uniforms {
            view_proj,
            translate: [self.pos_translate[0] as f32, self.pos_translate[1] as f32 - (y_min * y_stretch as f32 * self.pos_scale as f32)],
            scale: self.pos_scale as f32, thickness: dyn_thickness,
            resolution: [self.size.width as f32, self.size.height as f32],
            y_stretch: y_stretch as f32,
            morph: self.current_morph,
            color: [1.0, 0.85, 0.0, 1.0], // Jaune TDF vif pour le stroke
            mouse_pos: [profile_x_screen, profile_y_screen],
            raw_mouse_x: if mouse_world_x >= 0.0 && mouse_world_x <= self.max_dist && self.mouse_pos[0] > 350.0 { self.mouse_pos[0] } else { -1000.0 },
            max_dist: self.max_dist,
            y_min,
            y_max: y_min + delta_e_displayed as f32,
            rel_scale: capped_rel_scale,
            camera_tilt: self.camera_angle[0],
            camera_heading: self.camera_angle[1],
            global_center_x: self.stages[self.selected_stage_idx].global_lx,
            global_center_y: self.stages[self.selected_stage_idx].global_ly,
            _pad: 0.0,
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut dyn_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            let gap = 15.0; let s = 0.4; let row_h = font.font_size * 1.4;
            let half_h = (row_h * s * capped_rel_scale) / 2.0;

            if self.current_morph < 0.5 {
                
                let alt_text = format!("{:.0} m", current_ele);
                let (pos_alt, uvs_alt): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&alt_text);
                let anchor_alt = [profile_x_screen + gap, profile_y_screen + half_h + 5.0];
                for i in 0..(pos_alt.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_alt[i*2], pos_alt[i*2+1]], uv: [uvs_alt[i*2], uvs_alt[i*2+1]], anchor: anchor_alt, size: s }); 
                }

                let dist_text = format!("{:.2} km", world_x / 1000.0);
                let (pos_dist, uvs_dist): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&dist_text);
                let anchor_dist = [profile_x_screen + gap, profile_y_screen - half_h - 5.0];
                for i in 0..(pos_dist.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_dist[i*2], pos_dist[i*2+1]], uv: [uvs_dist[i*2], uvs_dist[i*2+1]], anchor: anchor_dist, size: s }); 
                }
            }

            // Affichage de la pente (Slope)
            if let Some(res) = self.slope_result {
                let text = format!("Pente: {:.2}%  |  D+: {:.1}m  |  Dist: {:.2}km", res.0, res.2, res.1 / 1000.0);
                let (pos_slope, uvs_slope): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&text);
                let text_half_h = (row_h * 0.5 * capped_rel_scale) / 2.0;
                let anchor_y = (self.size.height as f32 - 190.0).min(self.size.height as f32 - text_half_h - 150.0);
                let anchor = [370.0, anchor_y];
                for i in 0..(pos_slope.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_slope[i*2], pos_slope[i*2+1]], uv: [uvs_slope[i*2], uvs_slope[i*2+1]], anchor, size: 0.5 }); 
                }
            } else if let Some(_) = self.slope_start {
                let text = "Cliquez sur le 2eme point (clic droit)";
                let (pos_help, uvs_help): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&text);
                let text_half_h = (row_h * 0.5 * capped_rel_scale) / 2.0;
                let anchor_y = (self.size.height as f32 - 190.0).min(self.size.height as f32 - text_half_h - 150.0);
                let anchor = [370.0, anchor_y];
                for i in 0..(pos_help.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_help[i*2], pos_help[i*2+1]], uv: [uvs_help[i*2], uvs_help[i*2+1]], anchor, size: 0.5 }); 
                }
            }
        }
        let dyn_buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (dyn_vertices.len() * 28) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&dyn_buf, 0, bytemuck::cast_slice(&dyn_vertices));

        // --- PASS 1: 3D geometry with depth buffer ---
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_scissor_rect(352, 0, self.size.width - 352, self.size.height);

            pass.set_pipeline(&self.poly_render_pipeline);
            pass.set_vertex_buffer(0, self.poly_vertex_buffer.slice(..));
            pass.set_index_buffer(self.poly_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_poly_indices, 0, 0..1);

            // Draw Profile/Trace Stroke (always needed)
            
            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);

            if self.global_view_state == GlobalViewState::Swapped || self.global_view_state == GlobalViewState::ZoomingOut || self.global_view_state == GlobalViewState::FullyGlobal || self.global_view_state == GlobalViewState::ZoomingIn {
                // 1. France Fill (#444)
                pass.set_pipeline(&self.global_fill_render_pipeline);
                pass.set_vertex_buffer(0, self.global_fill_vertex_buffer.slice(..));
                pass.set_index_buffer(self.global_fill_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.global_fill_index_count, 0, 0..1);

                // 2. Global Lines
                pass.set_pipeline(&self.global_render_pipeline);
                pass.set_vertex_buffer(0, self.global_vertex_buffer.slice(..));
                pass.set_index_buffer(self.global_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.global_index_count, 0, 0..1);
            }


            // Draw optional Axes (only in 2D Profile)
            if self.current_morph < 0.5 {
                pass.set_pipeline(&self.axes_render_pipeline);
                pass.set_vertex_buffer(0, self.axes_vertex_buffer.slice(..));
                pass.set_index_buffer(self.axes_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.num_axes_indices, 0, 0..1);
            }
        }

        // --- PASS 2: 2D UI without depth buffer ---
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear — composite on top of 3D
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // 1. Sidebar Background
            pass.set_pipeline(&self.ui_render_pipeline);
            pass.set_vertex_buffer(0, self.sidebar_bg_buffer.slice(..));
            let num_bg = if self.stages.len() as f32 * 260.0 > self.size.height as f32 - 100.0 { 18 } else { 12 };
            pass.draw(0..num_bg, 0..1);

            // 2. Selection Highlight
            pass.set_pipeline(&self.selected_render_pipeline);
            pass.set_vertex_buffer(0, self.selected_bg_buffer.slice(..));
            pass.draw(0..6, 0..1);

            // 3. Hover Highlight
            if let Some(idx) = self.hover_stage_idx {
                let y_top = self.size.height as f32 - 40.0 - (idx as f32 * 260.0) + self.sidebar_scroll_y;
                let hover_data = [
                    PolyVertex::new([0.0, y_top - 230.0, 0.0, 0.0], 1.0), PolyVertex::new([350.0, y_top - 230.0, 0.0, 0.0], 1.0), PolyVertex::new([0.0, y_top, 0.0, 0.0], 1.0),
                    PolyVertex::new([0.0, y_top, 0.0, 0.0], 1.0), PolyVertex::new([350.0, y_top - 230.0, 0.0, 0.0], 1.0), PolyVertex::new([350.0, y_top, 0.0, 0.0], 1.0),
                ];
                self.queue.write_buffer(&self.hover_bg_buffer, 0, bytemuck::cast_slice(&hover_data));
                pass.set_pipeline(&self.hover_render_pipeline);
                pass.set_vertex_buffer(0, self.hover_bg_buffer.slice(..));
                pass.draw(0..6, 0..1);
            }

            // 4. Card borders
            pass.set_pipeline(&self.sparkline_render_pipeline);
            pass.set_vertex_buffer(0, self.stage_borders_buffer.slice(..));
            pass.draw(0..self.num_stage_border_vertices, 0..1);

            // 5. Sparklines
            pass.set_pipeline(&self.sparkline_render_pipeline);
            pass.set_vertex_buffer(0, self.sparkline_buffer.slice(..));
            pass.draw(0..self.num_spark_vertices, 0..1);

            // 6. Sidebar text
            if let Some(ref bg) = self.atlas_bind_group {
                pass.set_pipeline(&self.text_ui_pipeline);
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, self.sidebar_text_buffer.slice(..));
                pass.draw(0..self.num_sidebar_text_vertices, 0..1);
            }

            // 7. Reticule + graph text (scissored to graph area)
            pass.set_scissor_rect(352, 0, self.size.width - 352, self.size.height);
            if self.current_morph < 0.5 {
                pass.set_pipeline(&self.reticule_render_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..)); // Use main vertex buffer for reticule pos
                pass.draw(0..6, 0..1);
            }

            if let Some(ref bg) = self.atlas_bind_group {
                if self.current_morph < 0.5 {
                    pass.set_pipeline(&self.text_render_pipeline);
                    pass.set_bind_group(1, bg, &[]);
                    pass.set_vertex_buffer(0, self.static_text_buffer.slice(..));
                    pass.draw(0..self.num_static_text_vertices, 0..1);
                }

                pass.set_pipeline(&self.text_screen_pipeline);
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, dyn_buf.slice(..));
                let num_dyn = dyn_vertices.len() as u32;
                pass.draw(0..num_dyn, 0..1);
            }

            // 8. Header text (full width)
            pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
            if let Some(ref bg) = self.atlas_bind_group {
                pass.set_pipeline(&self.text_ui_pipeline);
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, self.header_text_buffer.slice(..));
                pass.draw(0..self.num_header_text_vertices, 0..1);
            }
        }



        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(WindowBuilder::new()
        .with_title("TDF 2026 - Profile")
        .build(&event_loop).unwrap());
    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
    let mut state = pollster::block_on(State::new(Arc::clone(&window)));
    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent { ref event, window_id } if window_id == state.window.id() => match event {
            WindowEvent::CloseRequested => elwt.exit(),
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key: key, state: ElementState::Pressed, .. }, .. } => {
                match key {
                    Key::Named(NamedKey::Escape) => elwt.exit(),
                    
                    Key::Named(NamedKey::Enter) => {
                        if state.global_view_state == GlobalViewState::Inactive {
                            if state.view_mode == 0 {
                                state.view_mode = 1;
                                state.target_morph = 1.0;
                                state.morph_animation = Some(MorphAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(1400),
                                    start_morph: state.current_morph,
                                    target_morph: 1.0,
                                });
                            }
                            
                            let rpw = (state.size.width as f32) - 352.0;
                            let target_offset = [352.0 + rpw * 0.5, state.size.height as f32 * 0.5];
                            
                            state.camera_animation = Some(CameraAnimation {
                                start_time: std::time::Instant::now(),
                                duration: std::time::Duration::from_millis(1400),
                                start_angle: state.camera_angle,
                                target_angle: [0.0, 0.0],
                                start_offset: state.camera_offset,
                                target_offset,
                            });
                            
                            state.global_view_state = GlobalViewState::MorphingToTopDown;
                            state.rebuild_ui();
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        if state.global_view_state != GlobalViewState::Inactive {
                            if state.global_view_state == GlobalViewState::ZoomingIn || state.global_view_state == GlobalViewState::MorphingTo2D {
                                return; // Ignore space while already exiting
                            }
                            
                            if state.global_view_state == GlobalViewState::MorphingToTopDown {
                                state.global_view_state = GlobalViewState::Inactive;
                                state.camera_animation = None;
                                state.view_mode = 0;
                                state.target_morph = 0.0;
                                let rpw = (state.size.width as f64) - 350.0;
                                state.pos_translate = [350.0 + rpw * 0.1, (state.size.height as f64 - 260.0) * 0.2];
                                state.morph_animation = Some(MorphAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(1400),
                                    start_morph: state.current_morph,
                                    target_morph: 0.0,
                                });
                            } else {
                                state.global_view_state = GlobalViewState::ZoomingIn;
                                let rpw = (state.size.width as f32) - 352.0;
                                let graph_width = (rpw as f64) * 0.8;
                                let target_scale = graph_width / (state.max_dist as f64);
                                let c_x = 352.0 + rpw * 0.5;
                                let c_y = state.size.height as f32 * 0.5;
                                
                                state.global_zoom_animation = Some(GlobalZoomAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(2500),
                                    start_scale: state.pos_scale,
                                    target_scale,
                                    start_center: [state.camera_offset[0], state.camera_offset[1]],
                                    target_center: [c_x, c_y],
                                });
                            }
                            state.rebuild_ui();
                            return;
                        }

                        let target = if state.view_mode == 0 { 1.0 } else { 0.0 };
                        state.morph_animation = Some(MorphAnimation {
                            start_time: Instant::now(),
                            duration: Duration::from_millis(1400),
                            start_morph: state.current_morph,
                            target_morph: target,
                        });

                        if state.view_mode == 0 {
                            state.view_mode = 1;
                            state.target_morph = 1.0;
                            // Reset 3D camera to neutral top-down
                            state.camera_angle = [0.0, 0.0];
                            let rpw = (state.size.width as f32) - 352.0;
                            state.camera_offset = [352.0 + rpw * 0.5, state.size.height as f32 * 0.5];
                        } else {
                            state.view_mode = 0;
                            state.target_morph = 0.0;
                        }
                        state.rebuild_ui();
                    }
                    _ => {}
                }
            }
            WindowEvent::Resized(s) => {
                state.resize(*s);
            }
            WindowEvent::ModifiersChanged(m) => {
                state.ctrl_pressed = m.state().control_key();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.mouse_pos = [position.x as f32, (state.size.height as f64 - position.y) as f32];
                if state.mouse_pos[0] < 350.0 {
                    let y_from_top = position.y as f32;
                    let idx = ((y_from_top - 40.0 + state.sidebar_scroll_y) / 260.0) as i32;
                    if idx >= 0 && (idx as usize) < state.stages.len() {
                        state.hover_stage_idx = Some(idx as usize);
                    } else {
                        state.hover_stage_idx = None;
                    }
                } else {
                    state.hover_stage_idx = None;
                }
                if state.mouse_pressed || state.right_mouse_pressed {
                    let dx = position.x - state.last_mouse_pos[0] as f64;
                    let dy = position.y - state.last_mouse_pos[1] as f64;
                    
                    if state.view_mode == 1 || state.global_view_state == GlobalViewState::FullyGlobal {
                        if state.right_mouse_pressed {
                            // Rotation with Right Click
                            let rel_scale = (state.pos_scale / state.initial_scale) as f32;
                            let sensitivity = 0.005 / (rel_scale.max(1.0).sqrt());
                            state.camera_angle[1] += (dx as f32) * sensitivity; 
                            state.camera_angle[0] = (state.camera_angle[0] - (dy as f32) * sensitivity).clamp(0.0, 1.5708); 
                        } else if state.mouse_pressed {
                            // Panning in 3D with Left Click
                            state.camera_offset[0] += dx as f32;
                            state.camera_offset[1] -= dy as f32;
                        }
                    } else if state.mouse_pressed {
                        state.pos_translate[0] += dx;
                        state.pos_translate[1] -= dy;
                    }
                }
                state.last_mouse_pos = [position.x as f32, position.y as f32];
            }
            WindowEvent::MouseInput { state: s, button, .. } => {
                if *button == MouseButton::Left {
                    state.mouse_pressed = *s == ElementState::Pressed;
                    
                    if state.mouse_pressed {
                        if state.ctrl_pressed && state.mouse_pos[0] >= 352.0 {
                            // Slope Calculation with Ctrl + Left Click
                            let p = state.get_profile_at_mouse();
                            if let Some(start) = state.slope_start {
                                let dist_diff = (p[0] - start[0]).abs();
                                let ele_diff = p[1] - start[1];
                                if dist_diff > 0.1 {
                                    let slope = (ele_diff / dist_diff) * 100.0;
                                    state.slope_result = Some((slope, dist_diff, ele_diff));
                                }
                                state.slope_start = None;
                            } else {
                                state.slope_start = Some(p);
                                state.slope_result = None;
                            }
                        } else if state.mouse_pos[0] < 350.0 {
                            // Sidebar selection
                            let y_from_top = state.size.height as f32 - state.mouse_pos[1];
                            let idx = ((y_from_top - 40.0 + state.sidebar_scroll_y) / 260.0) as i32;
                            if idx >= 0 && (idx as usize) < state.stages.len() {
                                state.select_stage(idx as usize);
                                state.slope_start = None;
                                state.slope_result = None;
                            }
                        }
                    }
                } else if *button == MouseButton::Right {
                    state.right_mouse_pressed = *s == ElementState::Pressed;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if state.mouse_pos[0] < 350.0 {
                    let amount = match delta { MouseScrollDelta::LineDelta(_, y) => *y as f32 * 250.0, MouseScrollDelta::PixelDelta(p) => p.y as f32 * 2.0 };
                    let max_scroll = ((state.stages.len() as f32 * 260.0) - (state.size.height as f32 - 100.0)).max(0.0);
                    let new_target = (state.sidebar_target_scroll_y - amount).clamp(0.0, max_scroll);
                    
                    if new_target != state.sidebar_target_scroll_y {
                        state.sidebar_target_scroll_y = new_target;
                        state.sidebar_animation = Some(ScrollAnimation {
                            start_time: Instant::now(),
                            duration: Duration::from_millis(400),
                            start_y: state.sidebar_scroll_y,
                            target_y: new_target,
                        });
                    }
                    return;
                }
                let amount = match delta { MouseScrollDelta::LineDelta(_, y) => *y as f64, MouseScrollDelta::PixelDelta(p) => p.y / 60.0 };
                let zoom_in = amount > 0.0;
                let factor = if zoom_in { 1.5_f64 } else { 1.0 / 1.5_f64 };
                let rpw = (state.size.width as f64) - 350.0;
                let france_min_scale = rpw * 0.60 / 1_200_000.0;
                let min_scale = if state.global_view_state != GlobalViewState::Inactive { france_min_scale } else { state.initial_scale };
                let target_scale = (state.pos_scale * factor).clamp(min_scale, state.initial_scale * 500.0);

                if state.view_mode == 1 || state.global_view_state == GlobalViewState::FullyGlobal {
                    // === ZOOM 3D animé centré sur la souris ===
                    // screen_pt = world_rotated * scale + camera_offset
                    // Pour fixer le point sous la souris : camera_offset_new = mouse + (camera_offset - mouse) * new_scale/old_scale
                    let mx = state.mouse_pos[0];
                    let my = state.mouse_pos[1];
                    let scale_factor = (target_scale / state.pos_scale) as f32;
                    let target_offset = [
                        mx + (state.camera_offset[0] - mx) * scale_factor,
                        my + (state.camera_offset[1] - my) * scale_factor,
                    ];
                    // Animer scale + camera_offset simultanément avec easing
                    state.animation = Some(ZoomAnimation {
                        start_time: Instant::now(),
                        duration: Duration::from_millis(300),
                        start_scale: state.pos_scale,
                        target_scale,
                        start_translate: [state.camera_offset[0] as f64, state.camera_offset[1] as f64],
                        target_translate: [target_offset[0] as f64, target_offset[1] as f64],
                    });
                } else {
                    // === ZOOM 2D animé centré sur la souris ===
                    let target_translate = if target_scale == state.initial_scale {
                        let rpw = (state.size.width as f64) - 350.0;
                        [350.0 + rpw * 0.1, (state.size.height as f64) * 0.25]
                    } else {
                        let wx = (state.mouse_pos[0] as f64 - state.pos_translate[0]) / state.pos_scale;
                        let wy = (state.mouse_pos[1] as f64 - state.pos_translate[1]) / state.pos_scale;
                        [state.mouse_pos[0] as f64 - wx * target_scale, state.mouse_pos[1] as f64 - wy * target_scale]
                    };
                    state.animation = Some(ZoomAnimation {
                        start_time: Instant::now(),
                        duration: Duration::from_millis(300),
                        start_scale: state.pos_scale,
                        target_scale,
                        start_translate: state.pos_translate,
                        target_translate,
                    });
                }
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
