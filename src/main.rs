mod font_atlas;

use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::path::{Path, PathBuf};
use crate::font_atlas::FontAtlas;
use winit::{
    event::*,
    event_loop::EventLoop,
    window::WindowBuilder,
    keyboard::{Key, NamedKey},
};
use serde::Deserialize;

// --- Race Discovery ---
#[derive(Debug, Clone, Deserialize)]
struct RaceMeta {
    id: String,
    name: String,
    color: [f32; 4],
}

#[derive(Debug, Clone)]
struct RaceEntry {
    meta: RaceMeta,
    profile_path: PathBuf,
    global_path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct RaceExaggeration {
    exaggeration_2d: f32,
    exaggeration_3d: f32,
}

impl Default for RaceExaggeration {
    fn default() -> Self {
        Self {
            exaggeration_2d: 1.0,
            exaggeration_3d: 1.0,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum DraggingSlider {
    None,
    Lissage,
    Exaggeration2d,
    Exaggeration3d,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Settings {
    use_metallic: bool,
    use_neon_green: bool,
    show_shadows: bool,
    use_brushed: bool,
    metallic_smoothing: u32,
    white_sky: bool,
    #[serde(default)]
    races: std::collections::HashMap<String, RaceExaggeration>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            use_metallic: false,
            use_neon_green: true,
            show_shadows: true,
            use_brushed: true,
            metallic_smoothing: 30,
            white_sky: false,
            races: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    fn load() -> Self {
        let path = find_data_dir().parent().map(|p| p.join("settings.json")).unwrap_or_else(|| PathBuf::from("settings.json"));
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<Self>(&content) {
                return settings;
            }
        }
        Self::default()
    }

    fn save(&self) {
        let path = find_data_dir().parent().map(|p| p.join("settings.json")).unwrap_or_else(|| PathBuf::from("settings.json"));
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, content);
        }
    }
}

fn discover_races(data_dir: &Path) -> Vec<RaceEntry> {
    let races_dir = data_dir.join("races");
    let mut entries = Vec::new();
    let Ok(dir) = std::fs::read_dir(&races_dir) else {
        eprintln!("[WARN] data/races/ directory not found at {:?}", races_dir);
        return entries;
    };
    let mut dirs: Vec<_> = dir.filter_map(|e| e.ok()).collect();
    dirs.sort_by_key(|e| e.file_name());
    for entry in dirs {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let meta_path = path.join("meta.json");
        let profile_path = path.join("profile.bin");
        let global_path = path.join("global.bin");
        if !meta_path.exists() || !profile_path.exists() || !global_path.exists() {
            eprintln!("[WARN] Skipping race {:?}: missing meta.json, profile.bin or global.bin", path.file_name().unwrap_or_default());
            continue;
        }
        let Ok(json) = std::fs::read_to_string(&meta_path) else { continue; };
        let Ok(meta) = serde_json::from_str::<RaceMeta>(&json) else {
            eprintln!("[WARN] Invalid meta.json in {:?}", path);
            continue;
        };
        entries.push(RaceEntry { meta, profile_path, global_path });
    }
    entries
}

fn find_data_dir() -> PathBuf {
    // 1. Current directory
    let cwd = std::env::current_dir().unwrap_or_default().join("data");
    if cwd.join("races").exists() { return cwd; }
    // 2. Executable directory
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("data");
            if p.join("races").exists() { return p; }
            // 3. One level up (for cargo run from project root — exe is in target/debug/)
            if let Some(gp) = parent.parent() {
                let p2 = gp.join("data");
                if p2.join("races").exists() { return p2; }
                if let Some(ggp) = gp.parent() {
                    let p3 = ggp.join("data");
                    if p3.join("races").exists() { return p3; }
                }
            }
        }
    }
    cwd // fallback
}

#[derive(Debug, Clone, PartialEq)]
enum AppPhase {
    Menu,
    Racing,
}

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
    depth: f32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    view_proj: glam::Mat4,            // 0-64
    light_space_matrix: glam::Mat4,   // 64-128
    translate: [f32; 2],              // 128-136
    scale: f32,                        // 136-140
    thickness: f32,                    // 140-144
    resolution: [f32; 2],             // 144-152
    y_stretch: f32,                    // 152-156
    morph: f32,                        // 156-160
    color: [f32; 4],                  // 160-176
    mouse_pos: [f32; 2],              // 176-184
    raw_mouse_x: f32,                  // 184-188
    max_dist: f32,                     // 188-192
    y_min: f32,                        // 192-196
    y_max: f32,                        // 196-200
    rel_scale: f32,                    // 200-204
    camera_tilt: f32,                  // 204-208
    camera_heading: f32,               // 208-212
    global_center_x: f32,              // 212-216
    global_center_y: f32,              // 216-220
    slope_x1: f32,                     // 220-224
    slope_x2: f32,                     // 224-228
    slope_y1: f32,                     // 228-232
    slope_y2: f32,                     // 232-236
    capped_rel_scale: f32,             // 236-240
    circle_thickness: f32,             // 240-244
    pad1: f32,                         // 244-248
    pad2: f32,                         // 248-252
    pad3: f32,                         // 252-256 (align to 16 bytes)
    pad4: f32,                         // 256-260
    pad5: f32,                         // 260-264
    pad6: f32,                         // 264-268
    pad7: f32,                         // 268-272
    pad8: f32,                         // 272-276
    y_stretch_3d: f32,                 // 276-280
    pad10: f32,                        // 280-284
    pad11: f32,                        // 284-288 (align to 16 bytes)
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

#[derive(Copy, Clone)]
struct SwitchAnimation {
    start_time: std::time::Instant,
    duration: std::time::Duration,
    start_t: f32,
    target_t: f32,
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
    text_3d_pipeline: wgpu::RenderPipeline,
    ui_render_pipeline: wgpu::RenderPipeline,
    selected_render_pipeline: wgpu::RenderPipeline,
    hover_render_pipeline: wgpu::RenderPipeline,
    sparkline_render_pipeline: wgpu::RenderPipeline,
    sparkline_stroke_pipeline: wgpu::RenderPipeline,
    sparkline_fill_render_pipeline: wgpu::RenderPipeline,
    reticule_render_pipeline: wgpu::RenderPipeline,
    axes_render_pipeline: wgpu::RenderPipeline,
    global_render_pipeline: wgpu::RenderPipeline,
    global_fill_render_pipeline: wgpu::RenderPipeline,
    dim_overlay_pipeline: wgpu::RenderPipeline,
    settings_card_pipeline: wgpu::RenderPipeline,

    // Buffers & Resources
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: wgpu::Buffer,
    depth_texture: wgpu::TextureView,
    shadow_texture_view: wgpu::TextureView,
    shadow_bind_group: wgpu::BindGroup,
    shadow_render_pipeline: wgpu::RenderPipeline,

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
    global_vertex_buffer: wgpu::Buffer,
    global_index_buffer: wgpu::Buffer,
    global_fill_vertex_buffer: wgpu::Buffer,
    global_fill_index_buffer: wgpu::Buffer,
    slope_text_buf: Option<wgpu::Buffer>,
    slope_text_count: usize,

    // Metadata
    num_indices: u32,
    num_poly_indices: u32,
    num_axes_indices: u32,
    num_static_text_vertices: u32,
    num_stage_border_vertices: u32,
    num_spark_vertices: u32,
    num_spark_fill_vertices: u32,
    num_spark_stroke_vertices: u32,
    num_sidebar_text_vertices: u32,
    num_header_text_vertices: u32,
    global_index_count: u32,
    global_fill_index_count: u32,

    // State
    profile_points: Vec<[f32; 2]>,
    smooth_normals: Vec<[f32; 2]>,
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

    // Multi-race
    race_color: [f32; 4],
    race_name: String,
    available_races: Vec<RaceEntry>,
    current_race_idx: usize,
    app_phase: AppPhase,
    hovered_menu_idx: Option<usize>,
    use_neon_green: bool,
    use_metallic: bool,
    show_shadows: bool,
    show_settings: bool,
    settings: Settings,
    settings_switch_t: f32,
    settings_switch_animation: Option<SwitchAnimation>,
    settings_neon_green_t: f32,
    settings_neon_green_animation: Option<SwitchAnimation>,
    settings_brushed_t: f32,
    settings_brushed_animation: Option<SwitchAnimation>,
    use_brushed: bool,
    dragging_slider: DraggingSlider,
    settings_white_sky_t: f32,
    settings_white_sky_animation: Option<SwitchAnimation>,
    use_white_sky: bool,
}



impl<'a> State<'a> {
    async fn new(window: Arc<winit::window::Window>, available_races: Vec<RaceEntry>, initial_race_idx: usize, app_phase: AppPhase) -> State<'a> {
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
        
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            surface_caps.present_modes[0]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        // --- Load race data dynamically ---
        let initial_race = available_races.get(initial_race_idx)
            .or_else(|| available_races.first())
            .expect("No races available — check data/races/ directory");
        let settings = Settings::load();
        settings.save();
        let use_neon_green = settings.use_neon_green;
        let race_color = if use_neon_green {
            [0.18, 1.0, 0.18, 1.0]
        } else {
            initial_race.meta.color
        };
        let race_name = initial_race.meta.name.clone();

        let compressed_data = std::fs::read(&initial_race.profile_path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", initial_race.profile_path, e));
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

        let sidebar_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let axes_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let axes_index_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let static_text_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 1024 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<Uniforms>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
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

        // --- SHADOW MAP RESOURCES ---
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Texture"),
            size: wgpu::Extent3d { width: 2048, height: 2048, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_texture_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let shadow_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
            ],
            label: Some("Shadow Bind Group"),
        });

        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &shadow_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shadow_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_shadow",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32, 2 => Float32, 3 => Float32x2],
                }],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor { label: Some("Depth Texture"), size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        use wgpu::util::DeviceExt;

        let global_data = std::fs::read(&initial_race.global_path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", initial_race.global_path, e));
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
        let poly_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&shadow_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_poly", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32, 2 => Float32, 3 => Float32x2] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_poly", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }), multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text_graph", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_screen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text_screen", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text_graph", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&text_pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_text_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_text_graph", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let text_3d_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text 3D Depth Pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_text_screen",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_text_graph",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let reticule_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_reticule", buffers: &[] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_reticule", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
        let sidebar_bg_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 8192, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let sidebar_bg_data = [
            PolyVertex::new([0.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([0.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([0.0, size.height as f32, 0.0, 0.0], 0.0), PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([350.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([352.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0),
            PolyVertex::new([350.0, size.height as f32, 0.0, 0.0], 0.0), PolyVertex::new([352.0, 0.0, 0.0, 0.0], 0.0), PolyVertex::new([352.0, size.height as f32, 0.0, 0.0], 0.0),
        ];
        queue.write_buffer(&sidebar_bg_buffer, 0, bytemuck::cast_slice(&sidebar_bg_data));
        let ui_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_sidebar_bg", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
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

        // Sparkline stroke pipeline (white strokes)
        let sparkline_stroke_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: Some("Sparkline Stroke Pipeline"), layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        // Sparkline fill pipeline
        let sparkline_fill_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: Some("Sparkline Fill Pipeline"), layout: Some(&pipeline_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_sparkline_fill", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() }, depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });

        let dim_overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Dim Overlay Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_dim_overlay", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None
        });

        let settings_card_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Settings Card Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_ui", buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x4] }] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_settings_card", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None
        });

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
        let mut state = State {
            surface, device, queue, config, size, window,
            render_pipeline, poly_render_pipeline, text_render_pipeline, text_ui_pipeline, text_screen_pipeline, text_3d_pipeline,
            ui_render_pipeline, selected_render_pipeline, hover_render_pipeline, sparkline_render_pipeline, sparkline_stroke_pipeline, sparkline_fill_render_pipeline, reticule_render_pipeline,
            axes_render_pipeline, global_render_pipeline, global_fill_render_pipeline,
            dim_overlay_pipeline, settings_card_pipeline,
            uniform_bind_group, atlas_bind_group, uniform_buffer, depth_texture: depth_view,
            shadow_texture_view, shadow_bind_group, shadow_render_pipeline,
            vertex_buffer, index_buffer, poly_vertex_buffer, poly_index_buffer, axes_vertex_buffer, axes_index_buffer, static_text_buffer,
            sidebar_bg_buffer, sidebar_text_buffer, sparkline_buffer, stage_borders_buffer,
            selected_bg_buffer, hover_bg_buffer, header_text_buffer, global_vertex_buffer, global_index_buffer,
            global_fill_vertex_buffer, global_fill_index_buffer,
            slope_text_buf: None,
            slope_text_count: 0,
            num_indices: active_stage.indices.len() as u32, num_poly_indices: 0, num_axes_indices: 0, num_static_text_vertices: 0,
            num_stage_border_vertices: 0, num_spark_vertices: 0, num_spark_fill_vertices: 0, num_spark_stroke_vertices: 0, num_sidebar_text_vertices: 0, num_header_text_vertices: 0, global_index_count: line_index_count, global_fill_index_count: fill_index_count,
            profile_points, smooth_normals: Vec::new(), max_dist, min_ele, max_ele, global_max_dist, global_max_ele, global_max_ratio_diff,
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
            race_color, race_name, available_races, current_race_idx: initial_race_idx,
            app_phase, hovered_menu_idx: None,
            use_neon_green,
            use_metallic: settings.use_metallic,
            show_shadows: settings.show_shadows,
            show_settings: false,
            settings_switch_t: if settings.use_metallic { 1.0 } else { 0.0 },
            settings_switch_animation: None,
            settings_neon_green_t: if settings.use_neon_green { 1.0 } else { 0.0 },
            settings_neon_green_animation: None,
            settings_brushed_t: if settings.use_brushed { 1.0 } else { 0.0 },
            settings_brushed_animation: None,
            use_brushed: settings.use_brushed,
            settings_white_sky_t: if settings.white_sky { 1.0 } else { 0.0 },
            settings_white_sky_animation: None,
            use_white_sky: settings.white_sky,
            settings,
            dragging_slider: DraggingSlider::None,
        };

        state.rebuild_ui();
        state.select_stage(0);
        state
    }

    fn load_race(&mut self, race_idx: usize) {
        if race_idx >= self.available_races.len() { return; }
        self.current_race_idx = race_idx;
        let race = self.available_races[race_idx].clone();
        self.race_color = if self.use_neon_green {
            [0.18, 1.0, 0.18, 1.0]
        } else {
            race.meta.color
        };
        self.race_name = race.meta.name.clone();

        // 1. Load profile.bin
        let compressed_data = std::fs::read(&race.profile_path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", race.profile_path, e));
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
                let s = String::from_utf8(bin_data[*off..*off+len].to_vec()).unwrap();
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

            let num_vertices = u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()) as usize; offset += 4;
            let num_indices = u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()) as usize; offset += 4;

            let mut vertices = Vec::with_capacity(num_vertices);
            for _ in 0..num_vertices {
                vertices.push(f32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()));
                offset += 4;
            }
            let mut indices = Vec::with_capacity(num_indices);
            for _ in 0..num_indices {
                indices.push(u32::from_le_bytes(bin_data[offset..offset+4].try_into().unwrap()));
                offset += 4;
            }

            let mut profile_points = Vec::new();
            for j in (0..num_vertices).step_by(26) { // 2 vertices per point, 13 floats per vertex
                profile_points.push([vertices[j], vertices[j+1]]);
            }

            stages.push(Stage {
                name, start, finish, date, max_dist, max_ele, min_ele, global_lx, global_ly,
                sparkline, vertices, indices, profile_points
            });
        }
        
        let global_max_dist = stages.iter().map(|s| s.max_dist).fold(0.0, f32::max);
        let global_max_ele = stages.iter().map(|s| s.max_ele).fold(0.0, f32::max);
        let global_max_ratio_diff = stages.iter().map(|s| (s.max_ele - s.min_ele) / s.max_dist).fold(0.0f32, f32::max) * 1.2;

        self.stages = stages;
        self.global_max_dist = global_max_dist;
        self.global_max_ele = global_max_ele;
        self.global_max_ratio_diff = global_max_ratio_diff;

        // 2. Load global.bin
        let global_data = std::fs::read(&race.global_path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", race.global_path, e));
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

        use wgpu::util::DeviceExt;
        self.global_fill_vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Fill Vertex Buffer"), contents: fill_vertices, usage: wgpu::BufferUsages::VERTEX
        });
        self.global_fill_index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Fill Index Buffer"), contents: fill_indices, usage: wgpu::BufferUsages::INDEX
        });
        self.global_vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Line Vertex Buffer"), contents: line_vertices, usage: wgpu::BufferUsages::VERTEX
        });
        self.global_index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Line Index Buffer"), contents: line_indices, usage: wgpu::BufferUsages::INDEX
        });
        self.global_fill_index_count = fill_index_count;
        self.global_index_count = line_index_count;

        // Reset UI indices & state
        self.select_stage(0);
        self.sidebar_scroll_y = 0.0;
        self.sidebar_target_scroll_y = 0.0;
        self.slope_start = None;
        self.slope_end = None;
        self.slope_result = None;
        self.hover_stage_idx = None;
        self.global_view_state = GlobalViewState::Inactive;
        self.global_zoom_animation = None;
        self.camera_animation = None;
        self.animation = None;
        self.morph_animation = None;
        self.sidebar_animation = None;

        self.rebuild_ui();
    }

    fn current_race_id(&self) -> String {
        self.available_races[self.current_race_idx].meta.id.clone()
    }

    fn get_current_race_exaggeration(&self) -> (f32, f32) {
        let race_id = self.current_race_id();
        if let Some(re) = self.settings.races.get(&race_id) {
            (re.exaggeration_2d, re.exaggeration_3d)
        } else {
            (1.0, 1.0)
        }
    }

    fn set_current_race_exaggeration(&mut self, ex_2d: f32, ex_3d: f32) {
        let race_id = self.current_race_id();
        self.settings.races.insert(race_id, RaceExaggeration {
            exaggeration_2d: ex_2d,
            exaggeration_3d: ex_3d,
        });
        self.settings.save();
    }

    fn get_hovered_menu_card(&self) -> Option<usize> {
        let cx = self.size.width as f32 / 2.0;
        let cy = self.size.height as f32 / 2.0;
        let n = self.available_races.len() as f32;
        let start_y = cy + (n - 1.0) * 60.0;
        
        for i in 0..self.available_races.len() {
            let y_center = start_y - (i as f32) * 120.0;
            let x_min = cx - 250.0;
            let x_max = cx + 250.0;
            let y_min = y_center - 50.0;
            let y_max = y_center + 50.0;
            
            if self.mouse_pos[0] >= x_min && self.mouse_pos[0] <= x_max
                && self.mouse_pos[1] >= y_min && self.mouse_pos[1] <= y_max
            {
                return Some(i);
            }
        }
        None
    }

    fn rebuild_ui(&mut self) {
        let size = self.size;
        let scroll = self.sidebar_scroll_y;
        let margin_side = 5.0;
        let mut sidebar_text_vertices = Vec::new();
        let mut spark_vertices = Vec::new();
        let mut border_vertices = Vec::new();
        let mut spark_fill_count = 0;
        let mut spark_stroke_count = 0;

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
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor, size: 0.55, depth: 0.0 });
                }

                // 2. Villes (Départ > Arrivée)
                let cities = format!("{} > {}", stage.start, stage.finish);
                let (pos, uvs): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&cities);
                let anchor_c = [x_start, y_top - 62.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: anchor_c, size: 0.33, depth: 0.0 });
                }

                // 3. Date | Distance
                let info = format!("{}  |  {:.1} km", stage.date, stage.max_dist / 1000.0);
                let (pos, uvs): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&info);
                let anchor_i = [x_start, y_top - 86.0];
                for i in 0..(pos.len() / 2) {
                    sidebar_text_vertices.push(TextVertex { pos: [pos[i*2], pos[i*2+1]], uv: [uvs[i*2], uvs[i*2+1]], anchor: anchor_i, size: 0.33, depth: 0.0 });
                }

                // 4. Sparklines (Profil simplifié)
            }

            spark_fill_count = 0;
            spark_stroke_count = 0;

            // Remplissage sous la courbe (Première passe)
            for (idx, stage) in self.stages.iter().enumerate() {
                let y_top = size.height as f32 - 40.0 - (idx as f32 * 260.0) + scroll;
                let card_h = 230.0;
                let x_left = margin_side;
                let x_start = x_left + 15.0;
                let width = 310.0;
                
                let graph_width = self.size.width as f32 - 500.0;
                let graph_height = self.size.height as f32 * 0.5;
                let height = (width * graph_height / graph_width).min(120.0);
                
                let padding_bottom = 20.0;
                let y_bottom = (y_top - card_h) + padding_bottom;
                
                let min = stage.min_ele;
                let max = stage.max_ele;
                
                let delta_e = stage.max_dist * self.global_max_ratio_diff;
                let display_min = if max <= delta_e {
                    0.0
                } else {
                    let padding = delta_e * 0.1;
                    (min - padding).max(0.0)
                };
                let display_range = delta_e.max(1.0);
                
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
                    spark_fill_count += 6;
                }
            }

            // Ligne du profil épaisse (Deuxième passe)
            for (idx, stage) in self.stages.iter().enumerate() {
                let y_top = size.height as f32 - 40.0 - (idx as f32 * 260.0) + scroll;
                let card_h = 230.0;
                let x_left = margin_side;
                let x_start = x_left + 15.0;
                let width = 310.0;
                
                let graph_width = self.size.width as f32 - 500.0;
                let graph_height = self.size.height as f32 * 0.5;
                let height = (width * graph_height / graph_width).min(120.0);
                
                let padding_bottom = 20.0;
                let y_bottom = (y_top - card_h) + padding_bottom;
                
                let min = stage.min_ele;
                let max = stage.max_ele;
                
                let delta_e = stage.max_dist * self.global_max_ratio_diff;
                let display_min = if max <= delta_e {
                    0.0
                } else {
                    let padding = delta_e * 0.1;
                    (min - padding).max(0.0)
                };
                let display_range = delta_e.max(1.0);
                
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
                    spark_stroke_count += 6;
                }
            }
        }
        self.num_spark_fill_vertices = spark_fill_count;
        self.num_spark_stroke_vertices = spark_stroke_count;
        self.num_spark_vertices = spark_vertices.len() as u32;
        self.sparkline_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (spark_vertices.len() * std::mem::size_of::<PolyVertex>()).max(32) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.sparkline_buffer, 0, bytemuck::cast_slice(&spark_vertices));
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

        // Mise à jour du texte du header (format identique aux cartes de la sidebar)
        let mut header_text_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            let stage = &self.stages[self.selected_stage_idx];
            
            // Ligne 0: Nom de la course pour changer de course
            let race_line = self.race_name.clone();
            let (pos_r, uvs_r): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&race_line);
            let anchor_r = [370.0, self.size.height as f32 - 35.0];
            for i in 0..(pos_r.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos_r[i*2], pos_r[i*2+1]], uv: [uvs_r[i*2], uvs_r[i*2+1]], anchor: anchor_r, size: 0.52, depth: 0.0 });
            }

            // Ligne 1: Etape N
            let line1 = format!("Etape {}", self.selected_stage_idx + 1);
            let (pos1, uvs1): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&line1);
            let anchor1 = [370.0, self.size.height as f32 - 82.0];
            for i in 0..(pos1.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos1[i*2], pos1[i*2+1]], uv: [uvs1[i*2], uvs1[i*2+1]], anchor: anchor1, size: 1.32, depth: 0.0 });
            }

            // Ligne 2: Départ > Arrivée
            let line2 = format!("{} > {}", stage.start, stage.finish);
            let (pos2, uvs2): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&line2);
            let anchor2 = [370.0, self.size.height as f32 - 130.0];
            for i in 0..(pos2.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos2[i*2], pos2[i*2+1]], uv: [uvs2[i*2], uvs2[i*2+1]], anchor: anchor2, size: 0.66, depth: 0.0 });
            }

            // Ligne 3: Date | Distance
            let line3 = format!("{}  |  {:.1} km", stage.date, stage.max_dist / 1000.0);
            let (pos3, uvs3): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&line3);
            let anchor3 = [370.0, self.size.height as f32 - 162.0];
            for i in 0..(pos3.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos3[i*2], pos3[i*2+1]], uv: [uvs3[i*2], uvs3[i*2+1]], anchor: anchor3, size: 0.48, depth: 0.0 });
            }

            // Aide contextuelle en haut à droite
            let help_text = if self.view_mode == 0 { "[Espace] Voir le tracé 3D" } else { "[Espace] Retour au profil 2D" };
            let (pos_h, uvs_h): (Vec<f32>, Vec<f32>) = font.get_text_geometry(help_text);
            let anchor_h = [self.size.width as f32 - 300.0, self.size.height as f32 - 45.0];
            for i in 0..(pos_h.len() / 2) {
                header_text_vertices.push(TextVertex { pos: [pos_h[i*2], pos_h[i*2+1]], uv: [uvs_h[i*2], uvs_h[i*2+1]], anchor: anchor_h, size: 0.4, depth: 0.0 });
            }

            let show_enter_help = !matches!(self.global_view_state, GlobalViewState::MorphingToTopDown | GlobalViewState::Swapped | GlobalViewState::ZoomingOut | GlobalViewState::FullyGlobal);
            if show_enter_help {
                let help_enter = "[Entrée] Voir la carte globale";
                let (pos_e, uvs_e): (Vec<f32>, Vec<f32>) = font.get_text_geometry(help_enter);
                let anchor_e = [self.size.width as f32 - 300.0, self.size.height as f32 - 75.0];
                for i in 0..(pos_e.len() / 2) {
                    header_text_vertices.push(TextVertex { pos: [pos_e[i*2], pos_e[i*2+1]], uv: [uvs_e[i*2], uvs_e[i*2+1]], anchor: anchor_e, size: 0.4, depth: 0.0 });
                }
            }

            if self.view_mode == 0 && show_enter_help {
                let help_slope = if self.slope_start.is_none() {
                    "[Ctrl+Clic G.] Calculer la pente"
                } else {
                    "[Clic Droit] Sortir du calcul"
                };
                let (pos_s, uvs_s): (Vec<f32>, Vec<f32>) = font.get_text_geometry(help_slope);
                let anchor_s = [self.size.width as f32 - 300.0, self.size.height as f32 - 105.0];
                for i in 0..(pos_s.len() / 2) {
                    header_text_vertices.push(TextVertex { pos: [pos_s[i*2], pos_s[i*2+1]], uv: [uvs_s[i*2], uvs_s[i*2+1]], anchor: anchor_s, size: 0.4, depth: 0.0 });
                }
            }
        }
        self.num_header_text_vertices = header_text_vertices.len() as u32;
        self.queue.write_buffer(&self.header_text_buffer, 0, bytemuck::cast_slice(&header_text_vertices));
        self.num_stage_border_vertices = border_vertices.len() as u32;
        self.queue.write_buffer(&self.stage_borders_buffer, 0, bytemuck::cast_slice(&border_vertices));
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
                    static_text_vertices.push(TextVertex { pos: [pos_ax[i*2], pos_ax[i*2+1]], uv: [uvs_ax[i*2], uvs_ax[i*2+1]], anchor, size, depth: 0.0 });
                }
            }
        }

        self.axes_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_vertices.len() * 52) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.axes_vertex_buffer, 0, bytemuck::cast_slice(&axes_vertices));
        self.axes_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (axes_indices.len() * 4) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.axes_index_buffer, 0, bytemuck::cast_slice(&axes_indices));
        self.static_text_buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (static_text_vertices.len() * 32) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&self.static_text_buffer, 0, bytemuck::cast_slice(&static_text_vertices));
        self.num_axes_indices = axes_indices.len() as u32;
        self.num_static_text_vertices = static_text_vertices.len() as u32;
    }

    fn select_stage(&mut self, idx: usize) {
        if idx >= self.stages.len() { return; }
        self.selected_stage_idx = idx;
        self.slope_start = None;
        self.slope_end = None;
        self.slope_result = None;
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

        self.pos_translate = [350.0 + (rpw * 0.1) as f64, (self.size.height as f64 - 260.0) * 0.2];

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

        self.rebuild_poly_buffers();

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

    fn rebuild_poly_buffers(&mut self) {
        let active_stage = &self.stages[self.selected_stage_idx];
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

        // Smoothing pass: dynamic window size for metallic mode, 3 points (window 1) for standard mode
        self.smooth_normals = Vec::with_capacity(normals.len());
        let window_size = if self.use_metallic { self.settings.metallic_smoothing as i32 } else { 1 }; 
        for i in 0..normals.len() {
            let mut sn = [0.0, 0.0];
            for j in -window_size..=window_size {
                let idx = (i as i32 + j).clamp(0, normals.len() as i32 - 1) as usize;
                sn[0] += normals[idx][0];
                sn[1] += normals[idx][1];
            }
            let slen = (sn[0]*sn[0] + sn[1]*sn[1]).sqrt().max(1e-6);
            self.smooth_normals.push([sn[0]/slen, sn[1]/slen]);
        }

        let mut count = 0;
        for i in 0..self.profile_points.len() {
            let p = self.profile_points[i];
            let lx = active_stage.vertices[i * 26 + 2];
            let ly = active_stage.vertices[i * 26 + 3];
            let n = self.smooth_normals[i];

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

        if let Some(anim) = self.settings_switch_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f32();
            let duration = anim.duration.as_secs_f32();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 3.0 * t * t - 2.0 * t * t * t; // Smoothstep easing
            self.settings_switch_t = anim.start_t + (anim.target_t - anim.start_t) * eased_t;
            
            if t >= 1.0 {
                self.settings_switch_t = anim.target_t;
                self.settings_switch_animation = None;
            }
            self.window.request_redraw();
        } else {
            self.settings_switch_t = if self.use_metallic { 1.0 } else { 0.0 };
        }

        if let Some(anim) = self.settings_neon_green_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f32();
            let duration = anim.duration.as_secs_f32();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 3.0 * t * t - 2.0 * t * t * t; // Smoothstep easing
            self.settings_neon_green_t = anim.start_t + (anim.target_t - anim.start_t) * eased_t;
            
            if t >= 1.0 {
                self.settings_neon_green_t = anim.target_t;
                self.settings_neon_green_animation = None;
            }
            self.window.request_redraw();
        } else {
            self.settings_neon_green_t = if self.use_neon_green { 1.0 } else { 0.0 };
        }

        if let Some(anim) = self.settings_brushed_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f32();
            let duration = anim.duration.as_secs_f32();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 3.0 * t * t - 2.0 * t * t * t; // Smoothstep easing
            self.settings_brushed_t = anim.start_t + (anim.target_t - anim.start_t) * eased_t;
            
            if t >= 1.0 {
                self.settings_brushed_t = anim.target_t;
                self.settings_brushed_animation = None;
            }
            self.window.request_redraw();
        } else {
            self.settings_brushed_t = if self.use_brushed { 1.0 } else { 0.0 };
        }

        if let Some(anim) = self.settings_white_sky_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f32();
            let duration = anim.duration.as_secs_f32();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 3.0 * t * t - 2.0 * t * t * t; // Smoothstep easing
            self.settings_white_sky_t = anim.start_t + (anim.target_t - anim.start_t) * eased_t;
            
            if t >= 1.0 {
                self.settings_white_sky_t = anim.target_t;
                self.settings_white_sky_animation = None;
            }
            self.window.request_redraw();
        } else {
            self.settings_white_sky_t = if self.use_white_sky { 1.0 } else { 0.0 };
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
                self.rebuild_ui();
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
            self.camera_offset[0] = anim.start_center[0] + (anim.target_center[0] - anim.start_center[0]) * (eased_t as f32);
            self.camera_offset[1] = anim.start_center[1] + (anim.target_center[1] - anim.start_center[1]) * (eased_t as f32);
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

        if self.app_phase == AppPhase::Menu {
            let uniforms = Uniforms {
                view_proj: glam::Mat4::IDENTITY,
                light_space_matrix: glam::Mat4::IDENTITY,
                translate: [0.0, 0.0],
                scale: 1.0,
                thickness: 1.0,
                resolution: [self.size.width as f32, self.size.height as f32],
                y_stretch: 1.0,
                morph: 0.0,
                color: self.race_color, // Use currently selected race color for active accents
                mouse_pos: [0.0, 0.0],
                raw_mouse_x: -1000.0,
                max_dist: 1.0,
                y_min: 0.0,
                y_max: 1.0,
                rel_scale: 1.0,
                camera_tilt: 0.0,
                camera_heading: 0.0,
                global_center_x: 0.0,
                global_center_y: 0.0,
                slope_x1: -1000.0,
                slope_x2: -1000.0,
                slope_y1: -1000.0,
                slope_y2: -1000.0,
                capped_rel_scale: 1.0,
                circle_thickness: 1.0,
                pad1: if self.use_metallic { 1.0 } else { 0.0 },
                pad2: if self.show_shadows { 1.0 } else { 0.0 },
                pad3: self.settings_switch_t,
                pad4: self.settings_neon_green_t,
                pad5: if self.use_brushed { 1.0 } else { 0.0 },
                pad6: self.settings_brushed_t,
                pad7: (self.settings.metallic_smoothing.clamp(1, 1000) - 1) as f32 / 999.0,
                pad8: self.settings_white_sky_t,
                y_stretch_3d: 0.0,
                pad10: 0.0,
                pad11: 0.0,
            };
            self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            let mut bg_vertices = Vec::new();
            let add_rect = |x0: f32, y0: f32, x1: f32, y1: f32, list: &mut Vec<PolyVertex>| {
                list.push(PolyVertex::new([x0, y0, 0.0, 0.0], 0.0));
                list.push(PolyVertex::new([x1, y0, 0.0, 0.0], 0.0));
                list.push(PolyVertex::new([x0, y1, 0.0, 0.0], 0.0));
                list.push(PolyVertex::new([x0, y1, 0.0, 0.0], 0.0));
                list.push(PolyVertex::new([x1, y0, 0.0, 0.0], 0.0));
                list.push(PolyVertex::new([x1, y1, 0.0, 0.0], 0.0));
            };

            // Fullscreen backdrop
            add_rect(0.0, 0.0, self.size.width as f32, self.size.height as f32, &mut bg_vertices);

            // Cards background and borders
            let cx = self.size.width as f32 / 2.0;
            let cy = self.size.height as f32 / 2.0;
            let n = self.available_races.len() as f32;
            let start_y = cy + (n - 1.0) * 60.0;

            let mut card_vertices = Vec::new();
            let mut border_vertices = Vec::new();

            for i in 0..self.available_races.len() {
                let y_center = start_y - (i as f32) * 120.0;
                let x_min = cx - 250.0;
                let x_max = cx + 250.0;
                let y_min = y_center - 50.0;
                let y_max = y_center + 50.0;

                // Card background
                add_rect(x_min, y_min, x_max, y_max, &mut card_vertices);

                // Card border (thickness 2.0)
                let thick = 2.0f32;
                let border_list = if self.hovered_menu_idx == Some(i) {
                    &mut border_vertices // will be drawn in color (sparkline_render_pipeline)
                } else {
                    &mut card_vertices // will be drawn in dark grey (ui_render_pipeline)
                };
                
                // Top
                add_rect(x_min, y_max - thick, x_max, y_max, border_list);
                // Bottom
                add_rect(x_min, y_min, x_max, y_min + thick, border_list);
                // Left
                add_rect(x_min, y_min, x_min + thick, y_max, border_list);
                // Right
                add_rect(x_max - thick, y_min, x_max, y_max, border_list);
            }

            use wgpu::util::DeviceExt;
            let backdrop_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&bg_vertices), usage: wgpu::BufferUsages::VERTEX
            });
            let card_bg_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&card_vertices), usage: wgpu::BufferUsages::VERTEX
            });
            let border_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&border_vertices), usage: wgpu::BufferUsages::VERTEX
            });

            // Text
            let mut menu_text_vertices = Vec::new();
            if let Some(ref font) = self.fa {
                // Title
                let title = "CHOIX DE LA COURSE";
                let (pos, uvs) = font.get_text_geometry(title);
                let mut title_w = 0.0f32;
                for c in title.chars() {
                    if let Some(m) = font.metrics.get(&c).or_else(|| font.metrics.get(&' ')) {
                        title_w += m.advance;
                    }
                }
                let title_size = 1.0f32;
                let anchor = [cx - (title_w * title_size) / 2.0, cy + (n * 60.0) + 80.0];
                for k in 0..(pos.len() / 2) {
                    menu_text_vertices.push(TextVertex { pos: [pos[k*2], pos[k*2+1]], uv: [uvs[k*2], uvs[k*2+1]], anchor, size: title_size, depth: 0.0 });
                }

                // Subtitle
                let subtitle = "Sélectionnez une course à charger";
                let (pos_sub, uvs_sub) = font.get_text_geometry(subtitle);
                let mut sub_w = 0.0f32;
                for c in subtitle.chars() {
                    if let Some(m) = font.metrics.get(&c).or_else(|| font.metrics.get(&' ')) {
                        sub_w += m.advance;
                    }
                }
                let sub_size = 0.45f32;
                let anchor_sub = [cx - (sub_w * sub_size) / 2.0, anchor[1] - 40.0];
                for k in 0..(pos_sub.len() / 2) {
                    menu_text_vertices.push(TextVertex { pos: [pos_sub[k*2], pos_sub[k*2+1]], uv: [uvs_sub[k*2], uvs_sub[k*2+1]], anchor: anchor_sub, size: sub_size, depth: 0.0 });
                }

                // Races
                for (i, entry) in self.available_races.iter().enumerate() {
                    let y_center = start_y - (i as f32) * 120.0;
                    
                    let name = &entry.meta.name;
                    let (pos_n, uvs_n) = font.get_text_geometry(name);
                    let anchor_n = [cx - 230.0, y_center + 10.0];
                    for k in 0..(pos_n.len() / 2) {
                        menu_text_vertices.push(TextVertex { pos: [pos_n[k*2], pos_n[k*2+1]], uv: [uvs_n[k*2], uvs_n[k*2+1]], anchor: anchor_n, size: 0.65, depth: 0.0 });
                    }

                    let info = if entry.meta.id == "tdf" {
                        "Tour de France 2026  |  20 étapes"
                    } else if entry.meta.id == "giro" {
                        "Giro d'Italia 2026  |  21 étapes"
                    } else {
                        "Course cycliste  |  Fichiers de course détectés"
                    };
                    let (pos_i, uvs_i) = font.get_text_geometry(info);
                    let anchor_i = [cx - 230.0, y_center - 25.0];
                    for k in 0..(pos_i.len() / 2) {
                        menu_text_vertices.push(TextVertex { pos: [pos_i[k*2], pos_i[k*2+1]], uv: [uvs_i[k*2], uvs_i[k*2+1]], anchor: anchor_i, size: 0.40, depth: 0.0 });
                    }

                    if self.hovered_menu_idx == Some(i) {
                        let hint = "Charger ->";
                        let (pos_h, uvs_h) = font.get_text_geometry(hint);
                        let anchor_h = [cx + 140.0, y_center - 5.0];
                        for k in 0..(pos_h.len() / 2) {
                            menu_text_vertices.push(TextVertex { pos: [pos_h[k*2], pos_h[k*2+1]], uv: [uvs_h[k*2], uvs_h[k*2+1]], anchor: anchor_h, size: 0.45, depth: 0.0 });
                        }
                    }
                }
            }

            let menu_text_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&menu_text_vertices), usage: wgpu::BufferUsages::VERTEX
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Menu Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.06, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                // Backdrop
                pass.set_pipeline(&self.ui_render_pipeline);
                pass.set_vertex_buffer(0, backdrop_buf.slice(..));
                pass.draw(0..6, 0..1);

                // Cards (Background + Neutral borders)
                pass.set_vertex_buffer(0, card_bg_buf.slice(..));
                pass.draw(0..(card_vertices.len() as u32), 0..1);

                // Hovered borders (colored)
                if !border_vertices.is_empty() {
                    pass.set_pipeline(&self.sparkline_render_pipeline);
                    pass.set_vertex_buffer(0, border_buf.slice(..));
                    pass.draw(0..(border_vertices.len() as u32), 0..1);
                }

                // Texts
                if let Some(ref bg) = self.atlas_bind_group {
                    pass.set_pipeline(&self.text_screen_pipeline);
                    pass.set_bind_group(1, bg, &[]);
                    pass.set_vertex_buffer(0, menu_text_buf.slice(..));
                    pass.draw(0..(menu_text_vertices.len() as u32), 0..1);
                }
            }

            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            return Ok(());
        }

        let graph_height = (self.size.height as f64 - 260.0) * 0.5;
        let delta_e_displayed = (self.max_dist as f64) * (self.global_max_ratio_diff as f64);
        let y_stretch_max = graph_height / (delta_e_displayed * self.initial_scale); 
        let (ex_2d, ex_3d) = self.get_current_race_exaggeration();
        let y_stretch = 1.0 + (y_stretch_max - 1.0) * ex_2d as f64;
        let y_stretch_3d = 1.0 + (y_stretch_max * 0.5 - 1.0) * ex_3d as f64;
        
        // Commencer à 0m si max_ele rentre dans la plage affichée
        let delta_e_displayed = self.max_dist as f64 * self.global_max_ratio_diff as f64;
        let y_min = if self.max_ele <= delta_e_displayed as f32 {
            0.0
        } else {
            let padding = delta_e_displayed as f32 * 0.1;
            (self.min_ele - padding).max(0.0)
        };

        // No limit on height translation for the graph

        let dyn_thickness = (1.8 * (self.pos_scale / self.initial_scale).powf(0.20)) as f32;
        let is_global = self.global_view_state != GlobalViewState::Inactive;
        let circle_thickness_base = if is_global {
            dyn_thickness * 1.8
        } else {
            dyn_thickness
        };
        let s_circle = 8.0 * circle_thickness_base;
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
        let ortho = glam::Mat4::orthographic_rh(0.0, self.size.width as f32, 0.0, self.size.height as f32, -20000.0, 20000.0);
        // Adaptation for WGPU Z range [0, 1] instead of [-1, 1]
        let mut wgpu_fix = glam::Mat4::IDENTITY;
        wgpu_fix.z_axis.z = 0.5;
        wgpu_fix.w_axis.z = 0.5;
        let view_proj = wgpu_fix * ortho * model_view;

        // --- SHADOW MATRIX CALCULATION ---
        // La direction de la lumière d'ombre doit correspondre à light_dir1 dans shader.wgsl: vec3(-0.8, 0.4, 0.5).
        // Comme la caméra regarde le profil tourné par "rotation", la lumière d'ombre doit être fixe 
        // par rapport à l'écran (espace caméra) pour rester alignée avec le shader de fragment.
        // On place donc la caméra de la lumière dans l'espace après rotation.
        let light_dir = glam::vec3(-0.8, 0.4, 0.5).normalize();

        let active_stage = &self.stages[self.selected_stage_idx];
        let mut max_r_sq = 100.0f32;
        let num_pts = active_stage.vertices.len() / 26;
        for i in 0..num_pts {
            let lx = active_stage.vertices[i * 26 + 2];
            let ly = active_stage.vertices[i * 26 + 3];
            let dx = lx - self.stage_center[0];
            let dy = ly - self.stage_center[1];
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > max_r_sq {
                max_r_sq = dist_sq;
            }
        }
        let r = max_r_sq.sqrt();

        // La caméra de la lumière est placée à light_dir * (r * 2.0) dans l'espace de la caméra,
        // et regarde vers l'origine (0, 0, 0) (où le modèle est translaté par center_offset).
        let light_pos = light_dir * r * 2.0;
        let light_view = glam::Mat4::look_at_rh(light_pos, glam::Vec3::ZERO, glam::vec3(0.0, 0.0, 1.0));
        let light_proj = glam::Mat4::orthographic_rh(-r, r, -r, r, 0.1, r * 4.0);

        let mut wgpu_fix = glam::Mat4::IDENTITY;
        wgpu_fix.z_axis.z = 0.5;
        wgpu_fix.w_axis.z = 0.5;

        // La matrice complète intègre la translation vers l'origine et la rotation de la caméra principale.
        let light_space_matrix = wgpu_fix * light_proj * light_view * rotation * center_offset;

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

        let slope_x1 = -1000.0f32;
        let slope_x2 = -1000.0f32;
        let slope_y1 = -1000.0f32;
        let slope_y2 = -1000.0f32;

        let uniforms = Uniforms {
            view_proj,
            light_space_matrix,
            translate: [self.pos_translate[0] as f32, self.pos_translate[1] as f32 - (y_min * y_stretch as f32 * self.pos_scale as f32)],
            scale: self.pos_scale as f32, thickness: dyn_thickness,
            resolution: [self.size.width as f32, self.size.height as f32],
            y_stretch: y_stretch as f32,
            morph: self.current_morph,
            color: self.race_color, // Couleur dynamique selon la course
            mouse_pos: [profile_x_screen, profile_y_screen],
            raw_mouse_x: if mouse_world_x >= 0.0 && mouse_world_x <= self.max_dist && self.mouse_pos[0] > 350.0 { self.mouse_pos[0] } else { -1000.0 },
            max_dist: self.max_dist,
            y_min,
            y_max: y_min + delta_e_displayed as f32,
            rel_scale,
            camera_tilt: self.camera_angle[0],
            camera_heading: self.camera_angle[1],
            global_center_x: self.stages[self.selected_stage_idx].global_lx,
            global_center_y: self.stages[self.selected_stage_idx].global_ly,
            slope_x1,
            slope_x2,
            slope_y1,
            slope_y2,
            capped_rel_scale,
            circle_thickness: s_circle,
            pad1: if self.use_metallic { 1.0 } else { 0.0 },
            pad2: if self.show_shadows { 1.0 } else { 0.0 },
            pad3: self.settings_switch_t,
            pad4: self.settings_neon_green_t,
            pad5: if self.use_brushed { 1.0 } else { 0.0 },
            pad6: self.settings_brushed_t,
            pad7: (self.settings.metallic_smoothing.clamp(1, 1000) - 1) as f32 / 999.0,
            pad8: self.settings_white_sky_t,
            y_stretch_3d: y_stretch_3d as f32,
            pad10: ex_2d as f32,
            pad11: ex_3d as f32,
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut red_vertices = Vec::new();
        let mut red_indices = Vec::new();
        let mut slope_line_vertices = Vec::new();
        let mut slope_line_indices = Vec::new();

        if !self.profile_points.is_empty() && !self.smooth_normals.is_empty() {
            let active_stage = &self.stages[self.selected_stage_idx];

            // Helper to interpolate 3D position and normal along profile
            let get_profile_point_3d = |x: f32, stage: &Stage| -> (f32, f32, f32, [f32; 2]) {
                let mut idx = 0;
                for i in 0..self.profile_points.len() - 1 {
                    if x >= self.profile_points[i][0] && x <= self.profile_points[i+1][0] {
                        idx = i;
                        break;
                    }
                }
                if idx + 1 < self.profile_points.len() {
                    let t = if self.profile_points[idx+1][0] == self.profile_points[idx][0] { 0.0 } else {
                        (x - self.profile_points[idx][0]) / (self.profile_points[idx+1][0] - self.profile_points[idx][0])
                    };
                    let y = self.profile_points[idx][1] + (self.profile_points[idx+1][1] - self.profile_points[idx][1]) * t;
                    let lx = stage.vertices[idx * 26 + 2] + (stage.vertices[(idx+1) * 26 + 2] - stage.vertices[idx * 26 + 2]) * t;
                    let ly = stage.vertices[idx * 26 + 3] + (stage.vertices[(idx+1) * 26 + 3] - stage.vertices[idx * 26 + 3]) * t;
                    
                    let n_start = self.smooth_normals[idx];
                    let n_next = self.smooth_normals[idx+1];
                    let nx = n_start[0] + (n_next[0] - n_start[0]) * t;
                    let ny = n_start[1] + (n_next[1] - n_start[1]) * t;
                    
                    (y, lx, ly, [nx, ny])
                } else {
                    (0.0, 0.0, 0.0, [0.0, 0.0])
                }
            };

            // Generate vertical lines if we have start and/or end
            let add_thick_line = |p1: [f32; 4], p2: [f32; 4], vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>| {
                let dx = p2[0] - p1[0];
                let dy = p2[1] - p1[1];
                let dz = p2[2] - p1[2];
                let dw = p2[3] - p1[3];
                let len_sq = dx*dx + dy*dy + dz*dz + dw*dw;
                if len_sq > 1e-4 {
                    let base = vertices.len() as u32;
                    vertices.push(Vertex { pos: p1, prev: p1, next: p2, side: -1.0 });
                    vertices.push(Vertex { pos: p1, prev: p1, next: p2, side: 1.0 });
                    vertices.push(Vertex { pos: p2, prev: p1, next: p2, side: -1.0 });
                    vertices.push(Vertex { pos: p2, prev: p1, next: p2, side: 1.0 });

                    indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
                }
            };

            if let Some(start) = self.slope_start {
                let start_x = start[0];
                let (start_y, start_lx, start_ly, _) = get_profile_point_3d(start_x, active_stage);
                add_thick_line(
                    [start_x, y_min, start_lx, start_ly],
                    [start_x, start_y, start_lx, start_ly],
                    &mut slope_line_vertices,
                    &mut slope_line_indices,
                );
            }

            if let Some(end) = self.slope_end {
                let end_x = end[0];
                let (end_y, end_lx, end_ly, _) = get_profile_point_3d(end_x, active_stage);
                add_thick_line(
                    [end_x, y_min, end_lx, end_ly],
                    [end_x, end_y, end_lx, end_ly],
                    &mut slope_line_vertices,
                    &mut slope_line_indices,
                );
            }

            // Generate red selection fill if we have a finished result
            if self.slope_result.is_some() {
                if let (Some(start), Some(end)) = (self.slope_start, self.slope_end) {
                    let start_x = start[0].min(end[0]);
                    let end_x = start[0].max(end[0]);
                    
                    let mut pts_info = Vec::new();
                    
                    let mut start_idx = 0;
                    for i in 0..self.profile_points.len() - 1 {
                        if start_x >= self.profile_points[i][0] && start_x <= self.profile_points[i+1][0] {
                            start_idx = i;
                            break;
                        }
                    }
                    
                    let mut end_idx = 0;
                    for i in 0..self.profile_points.len() - 1 {
                        if end_x >= self.profile_points[i][0] && end_x <= self.profile_points[i+1][0] {
                            end_idx = i;
                            break;
                        }
                    }
                    
                    if start_idx + 1 < self.profile_points.len() && end_idx + 1 < self.profile_points.len() {
                        let (start_y, start_lx, start_ly, start_n) = get_profile_point_3d(start_x, active_stage);
                        pts_info.push((start_x, start_y, start_lx, start_ly, start_n));
                        
                        for i in (start_idx+1)..=end_idx {
                            let p = self.profile_points[i];
                            let lx = active_stage.vertices[i * 26 + 2];
                            let ly = active_stage.vertices[i * 26 + 3];
                            let n = self.smooth_normals[i];
                            pts_info.push((p[0], p[1] as f32, lx, ly, n));
                        }
                        
                        let (end_y, end_lx, end_ly, end_n) = get_profile_point_3d(end_x, active_stage);
                        pts_info.push((end_x, end_y, end_lx, end_ly, end_n));
                        
                        let mut count = 0;
                        for (x, y, lx, ly, n) in pts_info {
                            red_vertices.push(PolyVertex { pos: [x, y, lx, ly], side: 1.0, flag: 1.0, normal: n });
                            red_vertices.push(PolyVertex { pos: [x, y_min, lx, ly], side: 0.0, flag: 1.0, normal: n });
                            count += 1;
                        }
                        
                        for i in 0..count - 1 {
                            let b = (i * 2) as u32;
                            red_indices.extend_from_slice(&[b, b+2, b+1, b+1, b+2, b+3]);
                        }
                    }
                }
            }
        }

        let mut dyn_vertices = Vec::new();
        if let Some(ref font) = self.fa {
            // Helper to interpolate 3D position along profile for circles
            let get_profile_point_3d = |x: f32, stage: &Stage| -> (f32, f32, f32) {
                let mut idx = 0;
                for i in 0..self.profile_points.len() - 1 {
                    if x >= self.profile_points[i][0] && x <= self.profile_points[i+1][0] {
                        idx = i;
                        break;
                    }
                }
                if idx + 1 < self.profile_points.len() {
                    let t = if self.profile_points[idx+1][0] == self.profile_points[idx][0] { 0.0 } else {
                        (x - self.profile_points[idx][0]) / (self.profile_points[idx+1][0] - self.profile_points[idx][0])
                    };
                    let y = self.profile_points[idx][1] + (self.profile_points[idx+1][1] - self.profile_points[idx][1]) * t;
                    let lx = stage.vertices[idx * 26 + 2] + (stage.vertices[(idx+1) * 26 + 2] - stage.vertices[idx * 26 + 2]) * t;
                    let ly = stage.vertices[idx * 26 + 3] + (stage.vertices[(idx+1) * 26 + 3] - stage.vertices[idx * 26 + 3]) * t;
                    (y, lx, ly)
                } else {
                    (0.0, 0.0, 0.0)
                }
            };

            let get_screen_pos_at_distance = |x: f32, y: f32, lx: f32, ly: f32| -> [f32; 3] {
                let px_2d = (x as f64 * self.pos_scale + self.pos_translate[0]) as f32;
                let py_2d = (((y - y_min) as f64 * y_stretch * self.pos_scale) + self.pos_translate[1]) as f32;

                let model_pos = glam::vec4(lx, ly, (y - y_min) * y_stretch_3d as f32 + 0.1, 1.0);
                let clip_pos = view_proj * model_pos;
                let ndc = glam::vec3(clip_pos.x, clip_pos.y, clip_pos.z) / clip_pos.w.max(1e-6);
                let px_3d = (ndc.x * 0.5 + 0.5) * self.size.width as f32;
                let py_3d = (ndc.y * 0.5 + 0.5) * self.size.height as f32;

                let final_x = px_2d + (px_3d - px_2d) * self.current_morph;
                let final_y = py_2d + (py_3d - py_2d) * self.current_morph;
                let final_z = 0.6 + (ndc.z - 0.6) * self.current_morph;

                [final_x, final_y, final_z]
            };

            let mut slope_text_vertices = Vec::new();

            // Helper to push a circle unit quad centered on [cx, cy]
            let push_circle = |cx: f32, cy: f32, cz: f32, vertices: &mut Vec<TextVertex>| {
                let anchor = [cx, cy];
                // Quad vertices: pos in [-1, 1], uv in [-1, 1]
                let quad = [
                    // Triangle 1
                    ([-1.0, -1.0], [-1.0, -1.0]),
                    ([ 1.0, -1.0], [ 1.0, -1.0]),
                    ([-1.0,  1.0], [-1.0,  1.0]),
                    // Triangle 2
                    ([-1.0,  1.0], [-1.0,  1.0]),
                    ([ 1.0, -1.0], [ 1.0, -1.0]),
                    ([ 1.0,  1.0], [ 1.0,  1.0]),
                ];
                for (p, uv) in quad {
                    vertices.push(TextVertex {
                        pos: p,
                        uv,
                        anchor,
                        size: -999.0, // Special flag for circle
                        depth: cz,
                    });
                }
            };

            // Draw white circles at the top of vertical boundary lines if active
            let active_stage = &self.stages[self.selected_stage_idx];
            if let Some(start) = self.slope_start {
                let start_x = start[0];
                let (start_y, start_lx, start_ly) = get_profile_point_3d(start_x, active_stage);
                let pos = get_screen_pos_at_distance(start_x, start_y, start_lx, start_ly);
                push_circle(pos[0], pos[1], pos[2], &mut slope_text_vertices);
            }

            if let Some(end) = self.slope_end {
                let end_x = end[0];
                let (end_y, end_lx, end_ly) = get_profile_point_3d(end_x, active_stage);
                let pos = get_screen_pos_at_distance(end_x, end_y, end_lx, end_ly);
                push_circle(pos[0], pos[1], pos[2], &mut slope_text_vertices);
            }

            let gap = 15.0; let s = 0.4; let row_h = font.font_size * 1.4;
            let half_h = (row_h * s * capped_rel_scale) / 2.0;

            if self.current_morph < 0.5 {
                let alt_text = format!("{:.0} m", current_ele);
                let (pos_alt, uvs_alt): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&alt_text);
                let anchor_alt = [profile_x_screen + gap, profile_y_screen + half_h + 5.0];
                for i in 0..(pos_alt.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_alt[i*2], pos_alt[i*2+1]], uv: [uvs_alt[i*2], uvs_alt[i*2+1]], anchor: anchor_alt, size: -s, depth: 0.0 }); 
                }

                let dist_text = format!("{:.2} km", world_x / 1000.0);
                let (pos_dist, uvs_dist): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&dist_text);
                let anchor_dist = [profile_x_screen + gap, profile_y_screen - half_h - 5.0];
                for i in 0..(pos_dist.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_dist[i*2], pos_dist[i*2+1]], uv: [uvs_dist[i*2], uvs_dist[i*2+1]], anchor: anchor_dist, size: -s, depth: 0.0 }); 
                }
            }

            // Affichage de la pente (Slope)
            if let Some(res) = self.slope_result {
                if let Some(start) = self.slope_start {
                    if let Some(end) = self.slope_end {
                        let sign = if res.2 >= 0.0 { "+" } else { "" };
                        let text_pct = format!("{:.2}%", res.0);
                        let text_sub = format!("{}{:.0} m  •  {:.2} km", sign, res.2, res.1 / 1000.0);
                        
                        let (pos_pct, uvs_pct): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&text_pct);
                        let (pos_sub, uvs_sub): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&text_sub);
                        
                        let size_mult = 1.0 - 0.25 * self.current_morph;
                        let s_size_pct = 0.38f32 * size_mult;
                        let s_size_sub = 0.285f32 * size_mult;
                        
                        let mut text_width_pct = 0.0f32;
                        for c in text_pct.chars() {
                            if let Some(m) = font.metrics.get(&c).or_else(|| font.metrics.get(&' ')) {
                                text_width_pct += m.advance;
                            }
                        }
                        let scaled_width_pct = text_width_pct * s_size_pct * rel_scale;
                        
                        let mut text_width_sub = 0.0f32;
                        for c in text_sub.chars() {
                            if let Some(m) = font.metrics.get(&c).or_else(|| font.metrics.get(&' ')) {
                                text_width_sub += m.advance;
                            }
                        }
                        let scaled_width_sub = text_width_sub * s_size_sub * rel_scale;
                        
                        // 2D position
                        let sx1 = start[0] * self.pos_scale as f32 + self.pos_translate[0] as f32;
                        let sx2 = end[0] * self.pos_scale as f32 + self.pos_translate[0] as f32;
                        let mid_x_2d = (sx1 + sx2) * 0.5;
                        let profile_bottom_2d = self.pos_translate[1] as f32;
                        
                        // 3D projected position of the midpoint
                        let start_x = start[0];
                        let end_x = end[0];
                        let active_stage = &self.stages[self.selected_stage_idx];
                        
                        let mut start_idx = 0;
                        for i in 0..self.profile_points.len() - 1 {
                            if start_x >= self.profile_points[i][0] && start_x <= self.profile_points[i+1][0] {
                                start_idx = i;
                                break;
                            }
                        }
                        let mut end_idx = 0;
                        for i in 0..self.profile_points.len() - 1 {
                            if end_x >= self.profile_points[i][0] && end_x <= self.profile_points[i+1][0] {
                                end_idx = i;
                                break;
                            }
                        }
                        
                        let mut mid_x_3d = mid_x_2d;
                        let mut mid_y_3d = profile_bottom_2d;
                        let mut depth = 0.0f32;
                        if start_idx + 1 < self.profile_points.len() && end_idx + 1 < self.profile_points.len() {
                            let t_start = if self.profile_points[start_idx+1][0] == self.profile_points[start_idx][0] { 0.0 } else {
                                (start_x - self.profile_points[start_idx][0]) / (self.profile_points[start_idx+1][0] - self.profile_points[start_idx][0])
                            };
                            let start_lx = active_stage.vertices[start_idx * 26 + 2] + (active_stage.vertices[(start_idx+1) * 26 + 2] - active_stage.vertices[start_idx * 26 + 2]) * t_start as f32;
                            let start_ly = active_stage.vertices[start_idx * 26 + 3] + (active_stage.vertices[(start_idx+1) * 26 + 3] - active_stage.vertices[start_idx * 26 + 3]) * t_start as f32;
                            let start_ele = self.profile_points[start_idx][1] + (self.profile_points[start_idx+1][1] - self.profile_points[start_idx][1]) * t_start;

                            let t_end = if self.profile_points[end_idx+1][0] == self.profile_points[end_idx][0] { 0.0 } else {
                                (end_x - self.profile_points[end_idx][0]) / (self.profile_points[end_idx+1][0] - self.profile_points[end_idx][0])
                            };
                            let end_lx = active_stage.vertices[end_idx * 26 + 2] + (active_stage.vertices[(end_idx+1) * 26 + 2] - active_stage.vertices[end_idx * 26 + 2]) * t_end as f32;
                            let end_ly = active_stage.vertices[end_idx * 26 + 3] + (active_stage.vertices[(end_idx+1) * 26 + 3] - active_stage.vertices[end_idx * 26 + 3]) * t_end as f32;
                            let end_ele = self.profile_points[end_idx][1] + (self.profile_points[end_idx+1][1] - self.profile_points[end_idx][1]) * t_end;

                            let mid_lx = (start_lx + end_lx) * 0.5;
                            let mid_ly = (start_ly + end_ly) * 0.5;
                            let mid_ele = (start_ele + end_ele) * 0.5;
                            let model_pos = glam::vec4(mid_lx, mid_ly, 0.0, 1.0);
                            let clip_pos = view_proj * model_pos;
                            let ndc = glam::vec3(clip_pos.x, clip_pos.y, clip_pos.z) / clip_pos.w.max(1e-6);
                            mid_x_3d = (ndc.x * 0.5 + 0.5) * self.size.width as f32;
                            mid_y_3d = (ndc.y * 0.5 + 0.5) * self.size.height as f32;
                            depth = 0.6 + (ndc.z - 0.6) * self.current_morph;
                        }

                        // Interpolate between 2D and 3D screen space coordinates
                        let mid_x = mid_x_2d + (mid_x_3d - mid_x_2d) * self.current_morph;
                        let profile_bottom_y = profile_bottom_2d + (mid_y_3d - profile_bottom_2d) * self.current_morph;

                        let tilt_factor = self.camera_angle[0].sin();
                        let y_scale = 1.0 + (tilt_factor - 1.0) * self.current_morph;

                        let h1 = row_h * s_size_pct * rel_scale;
                        
                        // We use a constant proportional offset from the profile bottom so it scales perfectly with zoom!
                        let d1 = 0.75 * h1 * y_scale;
                        let anchor_y_pct = profile_bottom_y - d1;
                        
                        // We set a smaller line spacing (e.g. 0.65 * h1) to avoid a large gap between lines.
                        let anchor_y_sub = anchor_y_pct - 0.65 * h1 * y_scale;
                        
                        let anchor_x_pct = mid_x - scaled_width_pct * 0.5;
                        let anchor_pct = [anchor_x_pct, anchor_y_pct];
                        
                        let anchor_x_sub = mid_x - scaled_width_sub * 0.5;
                        let anchor_sub = [anchor_x_sub, anchor_y_sub];
                        
                        for i in 0..(pos_pct.len() / 2) { 
                            slope_text_vertices.push(TextVertex { pos: [pos_pct[i*2], pos_pct[i*2+1] * y_scale], uv: [uvs_pct[i*2], uvs_pct[i*2+1]], anchor: anchor_pct, size: s_size_pct, depth }); 
                        }
                        for i in 0..(pos_sub.len() / 2) { 
                            slope_text_vertices.push(TextVertex { pos: [pos_sub[i*2], pos_sub[i*2+1] * y_scale], uv: [uvs_sub[i*2], uvs_sub[i*2+1]], anchor: anchor_sub, size: s_size_sub, depth }); 
                        }
                    }
                }
            } else if self.current_morph < 0.5 && self.slope_start.is_some() {
                let text = "Cliquer sur le second point (Ctrl+clic)";
                let (pos_help, uvs_help): (Vec<f32>, Vec<f32>) = font.get_text_geometry(&text);
                
                let mut text_width = 0.0f32;
                for c in text.chars() {
                    if let Some(m) = font.metrics.get(&c).or_else(|| font.metrics.get(&' ')) {
                        text_width += m.advance;
                    }
                }
                let s_size = 0.35f32;
                let scaled_width = text_width * s_size * capped_rel_scale;
                
                let anchor_x = (self.mouse_pos[0] - 15.0 - scaled_width).max(355.0);
                let text_half_h = (row_h * s_size * capped_rel_scale) / 2.0;
                let anchor_y = self.pos_translate[1] as f32 - 30.0 - text_half_h;
                let anchor = [anchor_x, anchor_y];
                
                for i in 0..(pos_help.len() / 2) { 
                    dyn_vertices.push(TextVertex { pos: [pos_help[i*2], pos_help[i*2+1]], uv: [uvs_help[i*2], uvs_help[i*2+1]], anchor, size: -s_size, depth: 0.0 }); 
                }
            }

            let slope_text_buf = if !slope_text_vertices.is_empty() {
                let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Slope Text Vertex Buffer"),
                    size: (slope_text_vertices.len() * 32) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.queue.write_buffer(&buf, 0, bytemuck::cast_slice(&slope_text_vertices));
                Some(buf)
            } else {
                None
            };
            self.slope_text_buf = slope_text_buf;
            self.slope_text_count = slope_text_vertices.len();
        }
        let dyn_buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (dyn_vertices.len() * 32) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&dyn_buf, 0, bytemuck::cast_slice(&dyn_vertices));

        let mut red_v_buf = None;
        let mut red_i_buf = None;
        if !red_indices.is_empty() {
            let v_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Red Poly Vertex Buffer"),
                size: (red_vertices.len() * 32) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&v_buf, 0, bytemuck::cast_slice(&red_vertices));
            
            let i_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Red Poly Index Buffer"),
                size: (red_indices.len() * 4) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&i_buf, 0, bytemuck::cast_slice(&red_indices));
            
            red_v_buf = Some(v_buf);
            red_i_buf = Some(i_buf);
        }

        let mut slope_line_v_buf = None;
        let mut slope_line_i_buf = None;
        let mut num_slope_line_indices = 0;
        if !slope_line_indices.is_empty() {
            let v_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Slope Boundary Line Vertex Buffer"),
                size: (slope_line_vertices.len() * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&v_buf, 0, bytemuck::cast_slice(&slope_line_vertices));
            
            let i_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Slope Boundary Line Index Buffer"),
                size: (slope_line_indices.len() * 4) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&i_buf, 0, bytemuck::cast_slice(&slope_line_indices));
            
            slope_line_v_buf = Some(v_buf);
            slope_line_i_buf = Some(i_buf);
            num_slope_line_indices = slope_line_indices.len() as u32;
        }

        // --- SHADOW MAP PASS ---
        {
            let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shadow Map Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            shadow_pass.set_pipeline(&self.shadow_render_pipeline);
            shadow_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            shadow_pass.set_vertex_buffer(0, self.poly_vertex_buffer.slice(..));
            shadow_pass.set_index_buffer(self.poly_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            shadow_pass.draw_indexed(0..self.num_poly_indices, 0, 0..1);
        }

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
            let scissor_left = if self.current_morph < 0.5 {
                let rpw = (self.size.width as f32) - 350.0;
                (350.0 + rpw * 0.1) as u32
            } else {
                352
            };
            let scissor_width = if self.current_morph < 0.5 {
                let rpw = (self.size.width as f32) - 350.0;
                (rpw * 0.8) as u32
            } else {
                self.size.width - 352
            };
            pass.set_scissor_rect(scissor_left, 0, scissor_width, self.size.height);

            pass.set_pipeline(&self.poly_render_pipeline);
            pass.set_bind_group(1, &self.shadow_bind_group, &[]);
            pass.set_vertex_buffer(0, self.poly_vertex_buffer.slice(..));
            pass.set_index_buffer(self.poly_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_poly_indices, 0, 0..1);

            // Draw Red Selection Fill if active!
            if let (Some(ref v_buf), Some(ref i_buf)) = (&red_v_buf, &red_i_buf) {
                pass.set_vertex_buffer(0, v_buf.slice(..));
                pass.set_index_buffer(i_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..(red_indices.len() as u32), 0, 0..1);
            }

            // Draw Profile/Trace Stroke (always needed)
            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);

            // Draw Slope Boundary Lines if active!
            if let (Some(ref v_buf), Some(ref i_buf), num_indices) = (&slope_line_v_buf, &slope_line_i_buf, num_slope_line_indices) {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, v_buf.slice(..));
                pass.set_index_buffer(i_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..num_indices, 0, 0..1);
            }

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

            // Draw Slope Text and Circles (depth-tested, drawn in Pass 1)
            if let Some(ref buf) = self.slope_text_buf {
                if self.slope_text_count > 0 {
                    if let Some(ref bg) = self.atlas_bind_group {
                        pass.set_pipeline(&self.text_3d_pipeline);
                        pass.set_bind_group(1, bg, &[]);
                        pass.set_vertex_buffer(0, buf.slice(..));
                        pass.draw(0..self.slope_text_count as u32, 0..1);
                    }
                }
            }
        }

        #[allow(unused_assignments)]
        let mut settings_bg_buf = None;
        #[allow(unused_assignments)]
        let mut settings_card_buf = None;
        #[allow(unused_assignments)]
        let mut settings_text_buf = None;

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

            // 5. Sparklines Fills
            pass.set_pipeline(&self.sparkline_fill_render_pipeline);
            pass.set_vertex_buffer(0, self.sparkline_buffer.slice(..));
            pass.draw(0..self.num_spark_fill_vertices, 0..1);

            // 6. Sparklines Strokes
            pass.set_pipeline(&self.sparkline_stroke_pipeline);
            pass.set_vertex_buffer(0, self.sparkline_buffer.slice(..));
            pass.draw(self.num_spark_fill_vertices..(self.num_spark_fill_vertices + self.num_spark_stroke_vertices), 0..1);

            // 6. Sidebar text
            if let Some(ref bg) = self.atlas_bind_group {
                pass.set_pipeline(&self.text_ui_pipeline);
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, self.sidebar_text_buffer.slice(..));
                pass.draw(0..self.num_sidebar_text_vertices, 0..1);
            }

            // 7. Reticule + graph text (scissored to graph area)
            let scissor_left = if self.current_morph < 0.5 {
                let rpw = (self.size.width as f32) - 350.0;
                (350.0 + rpw * 0.1) as u32
            } else {
                352
            };
            let scissor_width = if self.current_morph < 0.5 {
                let rpw = (self.size.width as f32) - 350.0;
                (rpw * 0.8) as u32
            } else {
                self.size.width - 352
            };
            pass.set_scissor_rect(scissor_left, 0, scissor_width, self.size.height);
            if self.current_morph < 0.5 {
                pass.set_pipeline(&self.reticule_render_pipeline);
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

            // 9. Settings Overlay
            if self.show_settings {
                pass.set_scissor_rect(0, 0, self.size.width, self.size.height);

                let cx = self.size.width as f32 / 2.0;
                let cy = self.size.height as f32 / 2.0;

                // 9.1 backdrop vertices
                let mut bg_verts = Vec::new();
                let add_rect = |x0: f32, y0: f32, x1: f32, y1: f32, list: &mut Vec<PolyVertex>| {
                    list.push(PolyVertex::new([x0, y0, 0.0, 0.0], 0.0));
                    list.push(PolyVertex::new([x1, y0, 0.0, 0.0], 0.0));
                    list.push(PolyVertex::new([x0, y1, 0.0, 0.0], 0.0));
                    list.push(PolyVertex::new([x0, y1, 0.0, 0.0], 0.0));
                    list.push(PolyVertex::new([x1, y0, 0.0, 0.0], 0.0));
                    list.push(PolyVertex::new([x1, y1, 0.0, 0.0], 0.0));
                };

                // Fullscreen dim overlay
                add_rect(0.0, 0.0, self.size.width as f32, self.size.height as f32, &mut bg_verts);
                
                // Settings Card backdrop (width 420, height 560) with local UV in pos.z/w
                let card_w = 420.0f32;
                let card_h = 560.0f32;
                let x_min = cx - card_w * 0.5;
                let x_max = cx + card_w * 0.5;
                let y_min = cy - card_h * 0.5;
                let y_max = cy + card_h * 0.5;
                
                let mut card_verts = Vec::new();
                let add_card_rect = |x0: f32, y0: f32, x1: f32, y1: f32, list: &mut Vec<PolyVertex>| {
                    list.push(PolyVertex::new([x0, y0, -1.0, -1.0], 0.0));
                    list.push(PolyVertex::new([x1, y0,  1.0, -1.0], 0.0));
                    list.push(PolyVertex::new([x0, y1, -1.0,  1.0], 0.0));
                    list.push(PolyVertex::new([x0, y1, -1.0,  1.0], 0.0));
                    list.push(PolyVertex::new([x1, y0,  1.0, -1.0], 0.0));
                    list.push(PolyVertex::new([x1, y1,  1.0,  1.0], 0.0));
                };
                add_card_rect(x_min, y_min, x_max, y_max, &mut card_verts);
                
                use wgpu::util::DeviceExt;
                settings_bg_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Settings Dim BG Buf"), contents: bytemuck::cast_slice(&bg_verts), usage: wgpu::BufferUsages::VERTEX
                }));
                settings_card_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Settings Card BG Buf"), contents: bytemuck::cast_slice(&card_verts), usage: wgpu::BufferUsages::VERTEX
                }));

                // Draw fullscreen backdrop
                pass.set_pipeline(&self.dim_overlay_pipeline);
                pass.set_vertex_buffer(0, settings_bg_buf.as_ref().unwrap().slice(..));
                pass.draw(0..6, 0..1);

                // Draw card backdrop
                pass.set_pipeline(&self.settings_card_pipeline);
                pass.set_vertex_buffer(0, settings_card_buf.as_ref().unwrap().slice(..));
                pass.draw(0..6, 0..1);

                // Draw text
                if let Some(ref font) = self.fa {
                    let mut settings_text_vertices = Vec::new();
                    
                    // Title "PARAMÈTRES"
                    let title = "PARAMÈTRES";
                    let (pos, uvs) = font.get_text_geometry(title);
                    let mut title_w = 0.0f32;
                    for c in title.chars() {
                        if let Some(m) = font.metrics.get(&c).or_else(|| font.metrics.get(&' ')) {
                            title_w += m.advance;
                        }
                    }
                    let title_size = 0.7f32;
                    let anchor_title = [cx - (title_w * title_size) / 2.0, y_max - 55.0];
                    for k in 0..(pos.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos[k*2], pos[k*2+1]], uv: [uvs[k*2], uvs[k*2+1]], anchor: anchor_title, size: title_size, depth: 0.0 });
                    }

                    let lbl_size = 0.5f32;
                    let metallic_depth = if self.use_metallic { 0.0 } else { 1.0 };

                    // Render mode selection (local Y = 110.0)
                    let label1 = "Rendu métallisé";
                    let (pos_lbl1, uvs_lbl1) = font.get_text_geometry(label1);
                    let anchor_lbl1 = [x_min + 40.0, cy + 110.0 - 8.0];
                    for k in 0..(pos_lbl1.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl1[k*2], pos_lbl1[k*2+1]], uv: [uvs_lbl1[k*2], uvs_lbl1[k*2+1]], anchor: anchor_lbl1, size: lbl_size, depth: 0.0 });
                    }

                    // Render mode shortcut indicator [T]
                    let sc1 = "[T]";
                    let (pos_sc1, uvs_sc1) = font.get_text_geometry(sc1);
                    let anchor_sc1 = [cx + 160.0, cy + 110.0 - 8.0];
                    for k in 0..(pos_sc1.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_sc1[k*2], pos_sc1[k*2+1]], uv: [uvs_sc1[k*2], uvs_sc1[k*2+1]], anchor: anchor_sc1, size: lbl_size, depth: 0.0 });
                    }

                    // Brushed Metal selection (local Y = 70.0)
                    let label_brushed = "Effet brossé";
                    let (pos_lbl_b, uvs_lbl_b) = font.get_text_geometry(label_brushed);
                    let anchor_lbl_b = [x_min + 40.0, cy + 70.0 - 8.0];
                    let brushed_depth = if self.use_metallic { 0.0 } else { 1.0 };
                    for k in 0..(pos_lbl_b.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl_b[k*2], pos_lbl_b[k*2+1]], uv: [uvs_lbl_b[k*2], uvs_lbl_b[k*2+1]], anchor: anchor_lbl_b, size: lbl_size, depth: brushed_depth });
                    }

                    // Brushed Metal shortcut indicator [B]
                    let sc_brushed = "[B]";
                    let (pos_sc_b, uvs_sc_b) = font.get_text_geometry(sc_brushed);
                    let anchor_sc_b = [cx + 160.0, cy + 70.0 - 8.0];
                    for k in 0..(pos_sc_b.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_sc_b[k*2], pos_sc_b[k*2+1]], uv: [uvs_sc_b[k*2], uvs_sc_b[k*2+1]], anchor: anchor_sc_b, size: lbl_size, depth: brushed_depth });
                    }

                    // White Sky selection (local Y = 30.0)
                    let label_sky = "Ciel blanc";
                    let (pos_lbl_sky, uvs_lbl_sky) = font.get_text_geometry(label_sky);
                    let anchor_lbl_sky = [x_min + 40.0, cy + 30.0 - 8.0];
                    for k in 0..(pos_lbl_sky.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl_sky[k*2], pos_lbl_sky[k*2+1]], uv: [uvs_lbl_sky[k*2], uvs_lbl_sky[k*2+1]], anchor: anchor_lbl_sky, size: lbl_size, depth: metallic_depth });
                    }

                    // White Sky shortcut indicator [W]
                    let sc_sky = "[W]";
                    let (pos_sc_sky, uvs_sc_sky) = font.get_text_geometry(sc_sky);
                    let anchor_sc_sky = [cx + 160.0, cy + 30.0 - 8.0];
                    for k in 0..(pos_sc_sky.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_sc_sky[k*2], pos_sc_sky[k*2+1]], uv: [uvs_sc_sky[k*2], uvs_sc_sky[k*2+1]], anchor: anchor_sc_sky, size: lbl_size, depth: metallic_depth });
                    }

                    // Lissage selection (local Y = -20.0)
                    let label_smooth = format!("Lissage: {}", self.settings.metallic_smoothing);
                    let (pos_lbl_s, uvs_lbl_s) = font.get_text_geometry(&label_smooth);
                    let anchor_lbl_s = [x_min + 40.0, cy - 20.0 - 8.0];
                    for k in 0..(pos_lbl_s.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl_s[k*2], pos_lbl_s[k*2+1]], uv: [uvs_lbl_s[k*2], uvs_lbl_s[k*2+1]], anchor: anchor_lbl_s, size: lbl_size, depth: metallic_depth });
                    }

                    // Neon Green selection (local Y = -100.0)
                    let label2 = "Vert néon";
                    let (pos_lbl2, uvs_lbl2) = font.get_text_geometry(label2);
                    let anchor_lbl2 = [x_min + 40.0, cy - 100.0 - 8.0];
                    for k in 0..(pos_lbl2.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl2[k*2], pos_lbl2[k*2+1]], uv: [uvs_lbl2[k*2], uvs_lbl2[k*2+1]], anchor: anchor_lbl2, size: lbl_size, depth: 0.0 });
                    }

                    // Neon Green shortcut indicator [C]
                    let sc2 = "[C]";
                    let (pos_sc2, uvs_sc2) = font.get_text_geometry(sc2);
                    let anchor_sc2 = [cx + 160.0, cy - 100.0 - 8.0];
                    for k in 0..(pos_sc2.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_sc2[k*2], pos_sc2[k*2+1]], uv: [uvs_sc2[k*2], uvs_sc2[k*2+1]], anchor: anchor_sc2, size: lbl_size, depth: 0.0 });
                    }

                    // Exagération 2D selection (local Y = -150.0)
                    let (ex_2d, ex_3d) = self.get_current_race_exaggeration();
                    let label_ex2d = format!("Exagération 2D: {:.0}%", ex_2d * 100.0);
                    let (pos_lbl_ex2d, uvs_lbl_ex2d) = font.get_text_geometry(&label_ex2d);
                    let anchor_lbl_ex2d = [x_min + 40.0, cy - 150.0 - 8.0];
                    for k in 0..(pos_lbl_ex2d.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl_ex2d[k*2], pos_lbl_ex2d[k*2+1]], uv: [uvs_lbl_ex2d[k*2], uvs_lbl_ex2d[k*2+1]], anchor: anchor_lbl_ex2d, size: lbl_size, depth: 0.0 });
                    }

                    // Exagération 3D selection (local Y = -210.0)
                    let label_ex3d = format!("Exagération 3D: {:.0}%", ex_3d * 100.0);
                    let (pos_lbl_ex3d, uvs_lbl_ex3d) = font.get_text_geometry(&label_ex3d);
                    let anchor_lbl_ex3d = [x_min + 40.0, cy - 210.0 - 8.0];
                    for k in 0..(pos_lbl_ex3d.len() / 2) {
                        settings_text_vertices.push(TextVertex { pos: [pos_lbl_ex3d[k*2], pos_lbl_ex3d[k*2+1]], uv: [uvs_lbl_ex3d[k*2], uvs_lbl_ex3d[k*2+1]], anchor: anchor_lbl_ex3d, size: lbl_size, depth: 0.0 });
                    }

                    settings_text_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Settings Text Buf"), contents: bytemuck::cast_slice(&settings_text_vertices), usage: wgpu::BufferUsages::VERTEX
                    }));

                    if let Some(ref bg) = self.atlas_bind_group {
                        pass.set_pipeline(&self.text_ui_pipeline);
                        pass.set_bind_group(1, bg, &[]);
                        pass.set_vertex_buffer(0, settings_text_buf.as_ref().unwrap().slice(..));
                        pass.draw(0..(settings_text_vertices.len() as u32), 0..1);
                    }
                }
            }
        }



        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let initial_race_id = args.windows(2)
        .find(|w| w[0] == "--race")
        .map(|w| w[1].clone());

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(WindowBuilder::new()
        .with_title("Cycling Visualizer")
        .with_maximized(true)
        .build(&event_loop).unwrap());
    window.set_cursor_visible(true);

    let data_dir = find_data_dir();
    let available_races = discover_races(&data_dir);
    if available_races.is_empty() {
        panic!("No races found in data/races/! Did you run the preprocessing scripts?");
    }

    let mut initial_race_idx = 0;
    let mut start_in_menu = true;
    if let Some(ref id) = initial_race_id {
        if let Some(idx) = available_races.iter().position(|r| &r.meta.id == id) {
            initial_race_idx = idx;
            start_in_menu = false;
        } else {
            eprintln!("[WARN] Requested race '{}' not found, starting in menu", id);
        }
    }

    let app_phase = if start_in_menu { AppPhase::Menu } else { AppPhase::Racing };

    let mut state = pollster::block_on(State::new(Arc::clone(&window), available_races, initial_race_idx, app_phase));
    
    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent { ref event, window_id } if window_id == state.window.id() => match event {
            WindowEvent::CloseRequested => elwt.exit(),
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key: key, state: ElementState::Pressed, .. }, .. } => {
                if state.show_settings {
                    match key {
                        Key::Named(NamedKey::Escape) => {
                            state.show_settings = false;
                            state.window.request_redraw();
                        }
                        Key::Character(ref s) if s.eq_ignore_ascii_case("p") => {
                            state.show_settings = false;
                            state.window.request_redraw();
                        }
                        Key::Character(ref s) if s.eq_ignore_ascii_case("t") => {
                            state.use_metallic = !state.use_metallic;
                            state.settings.use_metallic = state.use_metallic;
                            state.settings.save();
                            state.rebuild_poly_buffers();
                            let target_t = if state.use_metallic { 1.0 } else { 0.0 };
                            state.settings_switch_animation = Some(SwitchAnimation {
                                start_time: std::time::Instant::now(),
                                duration: std::time::Duration::from_millis(250),
                                start_t: state.settings_switch_t,
                                target_t,
                            });
                            state.window.request_redraw();
                        }
                        Key::Character(ref s) if s.eq_ignore_ascii_case("c") => {
                            state.use_neon_green = !state.use_neon_green;
                            state.settings.use_neon_green = state.use_neon_green;
                            state.settings.save();
                            if state.use_neon_green {
                                state.race_color = [0.18, 1.0, 0.18, 1.0];
                            } else {
                                let race = state.available_races[state.current_race_idx].clone();
                                state.race_color = race.meta.color;
                            }
                            state.rebuild_ui();
                            let target_t = if state.use_neon_green { 1.0 } else { 0.0 };
                            state.settings_neon_green_animation = Some(SwitchAnimation {
                                start_time: std::time::Instant::now(),
                                duration: std::time::Duration::from_millis(250),
                                start_t: state.settings_neon_green_t,
                                target_t,
                            });
                            state.window.request_redraw();
                        }
                        Key::Character(ref s) if s.eq_ignore_ascii_case("b") => {
                            if state.use_metallic {
                                state.use_brushed = !state.use_brushed;
                                state.settings.use_brushed = state.use_brushed;
                                state.settings.save();
                                let target_t = if state.use_brushed { 1.0 } else { 0.0 };
                                state.settings_brushed_animation = Some(SwitchAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(250),
                                    start_t: state.settings_brushed_t,
                                    target_t,
                                });
                                state.window.request_redraw();
                            }
                        }
                        Key::Character(ref s) if s.eq_ignore_ascii_case("w") => {
                            if state.use_metallic {
                                state.use_white_sky = !state.use_white_sky;
                                state.settings.white_sky = state.use_white_sky;
                                state.settings.save();
                                let target_t = if state.use_white_sky { 1.0 } else { 0.0 };
                                state.settings_white_sky_animation = Some(SwitchAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(250),
                                    start_t: state.settings_white_sky_t,
                                    target_t,
                                });
                                state.window.request_redraw();
                            }
                        }
                        _ => {}
                    }
                    return;
                }

                match key {
                    Key::Named(NamedKey::Escape) => elwt.exit(),
                    Key::Named(NamedKey::F11) => {
                        let is_fullscreen = state.window.fullscreen().is_some();
                        if is_fullscreen {
                            state.window.set_fullscreen(None);
                        } else {
                            state.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("p") => {
                        state.show_settings = true;
                        state.window.request_redraw();
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("m") => {
                        state.app_phase = match state.app_phase {
                            AppPhase::Menu => AppPhase::Racing,
                            AppPhase::Racing => AppPhase::Menu,
                        };
                        state.rebuild_ui();
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("c") => {
                        state.use_neon_green = !state.use_neon_green;
                        state.settings.use_neon_green = state.use_neon_green;
                        state.settings.save();
                        if state.use_neon_green {
                            state.race_color = [0.18, 1.0, 0.18, 1.0];
                        } else {
                            let race = state.available_races[state.current_race_idx].clone();
                            state.race_color = race.meta.color;
                        }
                        state.rebuild_ui();
                        let target_t = if state.use_neon_green { 1.0 } else { 0.0 };
                        state.settings_neon_green_animation = Some(SwitchAnimation {
                            start_time: std::time::Instant::now(),
                            duration: std::time::Duration::from_millis(250),
                            start_t: state.settings_neon_green_t,
                            target_t,
                        });
                        state.window.request_redraw();
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("t") => {
                        state.use_metallic = !state.use_metallic;
                        state.settings.use_metallic = state.use_metallic;
                        state.settings.save();
                        state.rebuild_poly_buffers();
                        let target_t = if state.use_metallic { 1.0 } else { 0.0 };
                        state.settings_switch_animation = Some(SwitchAnimation {
                            start_time: std::time::Instant::now(),
                            duration: std::time::Duration::from_millis(250),
                            start_t: state.settings_switch_t,
                            target_t,
                        });
                        state.window.request_redraw();
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("o") => {
                        state.show_shadows = !state.show_shadows;
                        state.settings.show_shadows = state.show_shadows;
                        state.settings.save();
                        state.window.request_redraw();
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("b") => {
                        if state.use_metallic {
                            state.use_brushed = !state.use_brushed;
                            state.settings.use_brushed = state.use_brushed;
                            state.settings.save();
                            let target_t = if state.use_brushed { 1.0 } else { 0.0 };
                            state.settings_brushed_animation = Some(SwitchAnimation {
                                start_time: std::time::Instant::now(),
                                duration: std::time::Duration::from_millis(250),
                                start_t: state.settings_brushed_t,
                                target_t,
                            });
                            state.window.request_redraw();
                        }
                    }
                    Key::Character(ref s) if s.eq_ignore_ascii_case("w") => {
                        if state.use_metallic {
                            state.use_white_sky = !state.use_white_sky;
                            state.settings.white_sky = state.use_white_sky;
                            state.settings.save();
                            let target_t = if state.use_white_sky { 1.0 } else { 0.0 };
                            state.settings_white_sky_animation = Some(SwitchAnimation {
                                start_time: std::time::Instant::now(),
                                duration: std::time::Duration::from_millis(250),
                                start_t: state.settings_white_sky_t,
                                target_t,
                            });
                            state.window.request_redraw();
                        }
                    }
                    _ => {
                        if state.app_phase == AppPhase::Racing {
                            match key {
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
                    }
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
                if state.show_settings {
                    if state.dragging_slider != DraggingSlider::None {
                        let cx = state.size.width as f32 / 2.0;
                        let mx = state.mouse_pos[0];
                        let ratio = ((mx - (cx - 170.0)) / 340.0).clamp(0.0, 1.0);
                        match state.dragging_slider {
                            DraggingSlider::Lissage => {
                                let val = 1 + (ratio * 999.0).round() as u32;
                                if val != state.settings.metallic_smoothing {
                                    state.settings.metallic_smoothing = val;
                                    state.settings.save();
                                    state.rebuild_poly_buffers();
                                }
                            }
                            DraggingSlider::Exaggeration2d => {
                                let (old_2d, old_3d) = state.get_current_race_exaggeration();
                                if ratio != old_2d {
                                    state.set_current_race_exaggeration(ratio, old_3d);
                                }
                            }
                            DraggingSlider::Exaggeration3d => {
                                let (old_2d, old_3d) = state.get_current_race_exaggeration();
                                if ratio != old_3d {
                                    state.set_current_race_exaggeration(old_2d, ratio);
                                }
                            }
                            DraggingSlider::None => {}
                        }
                        state.window.request_redraw();
                    }
                    state.last_mouse_pos = [position.x as f32, position.y as f32];
                    return;
                }
                if state.app_phase == AppPhase::Menu {
                    state.hovered_menu_idx = state.get_hovered_menu_card();
                    state.hover_stage_idx = None;
                } else {
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
                }
                state.last_mouse_pos = [position.x as f32, position.y as f32];
            }
            WindowEvent::MouseInput { state: s, button, .. } => {
                if *button == MouseButton::Left && *s == ElementState::Released {
                    state.dragging_slider = DraggingSlider::None;
                }
                if state.show_settings {
                    if *button == MouseButton::Left && *s == ElementState::Pressed {
                        let cx = state.size.width as f32 / 2.0;
                        let cy = state.size.height as f32 / 2.0;
                        let mx = state.mouse_pos[0];
                        let my = state.mouse_pos[1];
                        
                        if mx >= cx - 210.0 && mx <= cx + 210.0 && my >= cy - 280.0 && my <= cy + 280.0 {
                            if my >= cy + 90.0 {
                                // Switch 1: Rendu métallisé (Y center +110.0, band >= +90)
                                state.use_metallic = !state.use_metallic;
                                state.settings.use_metallic = state.use_metallic;
                                state.settings.save();
                                state.rebuild_poly_buffers();
                                let target_t = if state.use_metallic { 1.0 } else { 0.0 };
                                state.settings_switch_animation = Some(SwitchAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(250),
                                    start_t: state.settings_switch_t,
                                    target_t,
                                });
                            } else if my >= cy + 50.0 {
                                // Switch 2: Effet brossé (Y center +70.0, band [+50, +90])
                                if state.use_metallic {
                                    state.use_brushed = !state.use_brushed;
                                    state.settings.use_brushed = state.use_brushed;
                                    state.settings.save();
                                    let target_t = if state.use_brushed { 1.0 } else { 0.0 };
                                    state.settings_brushed_animation = Some(SwitchAnimation {
                                        start_time: std::time::Instant::now(),
                                        duration: std::time::Duration::from_millis(250),
                                        start_t: state.settings_brushed_t,
                                        target_t,
                                    });
                                }
                            } else if my >= cy + 10.0 {
                                // Switch 3: Ciel blanc (Y center +30.0, band [+10, +50])
                                if state.use_metallic {
                                    state.use_white_sky = !state.use_white_sky;
                                    state.settings.white_sky = state.use_white_sky;
                                    state.settings.save();
                                    let target_t = if state.use_white_sky { 1.0 } else { 0.0 };
                                    state.settings_white_sky_animation = Some(SwitchAnimation {
                                        start_time: std::time::Instant::now(),
                                        duration: std::time::Duration::from_millis(250),
                                        start_t: state.settings_white_sky_t,
                                        target_t,
                                    });
                                }
                            } else if my >= cy - 75.0 {
                                // Slider 1: Lissage (Y center -50.0, band [-75, +10])
                                if state.use_metallic {
                                    state.dragging_slider = DraggingSlider::Lissage;
                                    let ratio = ((mx - (cx - 170.0)) / 340.0).clamp(0.0, 1.0);
                                    state.settings.metallic_smoothing = 1 + (ratio * 999.0).round() as u32;
                                    state.settings.save();
                                    state.rebuild_poly_buffers();
                                }
                            } else if my >= cy - 125.0 {
                                // Switch 4: Vert néon (Y center -100.0, band [-125, -75])
                                state.use_neon_green = !state.use_neon_green;
                                state.settings.use_neon_green = state.use_neon_green;
                                state.settings.save();
                                if state.use_neon_green {
                                    state.race_color = [0.18, 1.0, 0.18, 1.0];
                                } else {
                                    let race = state.available_races[state.current_race_idx].clone();
                                    state.race_color = race.meta.color;
                                }
                                state.rebuild_ui();
                                let target_t = if state.use_neon_green { 1.0 } else { 0.0 };
                                state.settings_neon_green_animation = Some(SwitchAnimation {
                                    start_time: std::time::Instant::now(),
                                    duration: std::time::Duration::from_millis(250),
                                    start_t: state.settings_neon_green_t,
                                    target_t,
                                });
                            } else if my >= cy - 210.0 {
                                // Slider 2: Exagération 2D (Y center -180.0, band [-210, -125])
                                state.dragging_slider = DraggingSlider::Exaggeration2d;
                                let ratio = ((mx - (cx - 170.0)) / 340.0).clamp(0.0, 1.0);
                                let (_, old_3d) = state.get_current_race_exaggeration();
                                state.set_current_race_exaggeration(ratio, old_3d);
                            } else {
                                // Slider 3: Exagération 3D (Y center -240.0, band [-280, -210])
                                state.dragging_slider = DraggingSlider::Exaggeration3d;
                                let ratio = ((mx - (cx - 170.0)) / 340.0).clamp(0.0, 1.0);
                                let (old_2d, _) = state.get_current_race_exaggeration();
                                state.set_current_race_exaggeration(old_2d, ratio);
                            }
                            state.window.request_redraw();
                        } else {
                            state.show_settings = false;
                            state.window.request_redraw();
                        }
                    }
                    return;
                }

                if *button == MouseButton::Left {
                    state.mouse_pressed = *s == ElementState::Pressed;
                    
                    if state.mouse_pressed {
                        if state.app_phase == AppPhase::Menu {
                            if let Some(idx) = state.hovered_menu_idx {
                                state.load_race(idx);
                                state.app_phase = AppPhase::Racing;
                                state.mouse_pressed = false; // reset
                            }
                        } else {
                            if state.view_mode == 0 && state.global_view_state == GlobalViewState::Inactive && state.ctrl_pressed && state.mouse_pos[0] >= 352.0 {
                                // Slope Calculation with Ctrl + Left Click
                                let p = state.get_profile_at_mouse();
                                if state.slope_result.is_some() {
                                    state.slope_start = Some(p);
                                    state.slope_end = None;
                                    state.slope_result = None;
                                } else if let Some(start) = state.slope_start {
                                    let dist_diff = (p[0] - start[0]).abs();
                                    let ele_diff = p[1] - start[1];
                                    if dist_diff > 0.1 {
                                        let slope = (ele_diff / dist_diff) * 100.0;
                                        state.slope_result = Some((slope, dist_diff, ele_diff));
                                        state.slope_end = Some(p);
                                    } else {
                                        state.slope_start = None;
                                        state.slope_end = None;
                                        state.slope_result = None;
                                    }
                                } else {
                                    state.slope_start = Some(p);
                                    state.slope_end = None;
                                    state.slope_result = None;
                                }
                                state.rebuild_ui();
                            } else if state.mouse_pos[0] < 350.0 {
                                // Sidebar selection
                                let y_from_top = state.size.height as f32 - state.mouse_pos[1];
                                let idx = ((y_from_top - 40.0 + state.sidebar_scroll_y) / 260.0) as i32;
                                if idx >= 0 && (idx as usize) < state.stages.len() {
                                    state.select_stage(idx as usize);
                                    state.slope_start = None;
                                    state.slope_end = None;
                                    state.slope_result = None;
                                }
                            }
                        }
                    }
                } else if *button == MouseButton::Right {
                    state.right_mouse_pressed = *s == ElementState::Pressed;
                    if state.right_mouse_pressed && state.view_mode == 0 && state.global_view_state == GlobalViewState::Inactive {
                        state.slope_start = None;
                        state.slope_end = None;
                        state.slope_result = None;
                        state.rebuild_ui();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if state.show_settings {
                    return;
                }
                if state.app_phase == AppPhase::Menu {
                    return;
                }
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
                    let mx = state.mouse_pos[0];
                    let my = state.mouse_pos[1];
                    let scale_factor = (target_scale / state.pos_scale) as f32;
                    let target_offset = [
                        mx + (state.camera_offset[0] - mx) * scale_factor,
                        my + (state.camera_offset[1] - my) * scale_factor,
                    ];
                    state.animation = Some(ZoomAnimation {
                        start_time: Instant::now(),
                        duration: Duration::from_millis(300),
                        start_scale: state.pos_scale,
                        target_scale,
                        start_translate: [state.camera_offset[0] as f64, state.camera_offset[1] as f64],
                        target_translate: [target_offset[0] as f64, target_offset[1] as f64],
                    });
                } else {
                    let target_translate = if target_scale == state.initial_scale {
                        let rpw = (state.size.width as f64) - 350.0;
                        [350.0 + rpw * 0.1, (state.size.height as f64 - 260.0) * 0.2]
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
