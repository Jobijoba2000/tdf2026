import re

def inject():
    with open('src/main.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    # 1. Structures
    structs = """
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
}

struct GlobalZoomAnimation {
    start_time: std::time::Instant,
    duration: std::time::Duration,
    start_scale: f64,
    target_scale: f64,
    start_center: [f32; 2],
    target_center: [f32; 2],
}
"""
    content = content.replace("struct Stage {", structs + "\nstruct Stage {")

    # 2. State fields
    content = content.replace(
        "axes_render_pipeline: wgpu::RenderPipeline,",
        "axes_render_pipeline: wgpu::RenderPipeline,\n    global_render_pipeline: wgpu::RenderPipeline,"
    )
    content = content.replace(
        "header_border_buffer: wgpu::Buffer,",
        "header_border_buffer: wgpu::Buffer,\n    global_vertex_buffer: wgpu::Buffer,\n    global_index_buffer: wgpu::Buffer,"
    )
    content = content.replace(
        "num_header_border_vertices: u32,",
        "num_header_border_vertices: u32,\n    global_index_count: u32,"
    )
    content = content.replace(
        "hover_stage_idx: Option<usize>,",
        "hover_stage_idx: Option<usize>,\n    global_view_state: GlobalViewState,\n    global_zoom_animation: Option<GlobalZoomAnimation>,"
    )

    # 3. State::new
    content = content.replace(
        "let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());",
        """let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        use wgpu::util::DeviceExt;

        let global_data = std::fs::read("data/vue_globale.bin").expect("Failed to load vue_globale.bin");
        let mut global_offset = 0;
        let global_num_vertices = u32::from_le_bytes(global_data[global_offset..global_offset+4].try_into().unwrap()); global_offset += 4;
        let global_index_count = u32::from_le_bytes(global_data[global_offset..global_offset+4].try_into().unwrap()); global_offset += 4;
        let global_vertices_size = (global_num_vertices * 32) as usize;
        let global_vertices = &global_data[global_offset..global_offset+global_vertices_size]; global_offset += global_vertices_size;
        let global_indices_size = (global_index_count * 4) as usize;
        let global_indices = &global_data[global_offset..global_offset+global_indices_size];
        
        let global_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Vertex Buffer"),
            contents: global_vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let global_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Index Buffer"),
            contents: global_indices,
            usage: wgpu::BufferUsages::INDEX,
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
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });"""
    )
    content = content.replace(
        "dot_render_pipeline, header_render_pipeline, axes_render_pipeline,",
        "dot_render_pipeline, header_render_pipeline, axes_render_pipeline, global_render_pipeline,"
    )
    content = content.replace(
        "selected_bg_buffer, hover_bg_buffer, header_text_buffer, header_bg_buffer, header_border_buffer,",
        "selected_bg_buffer, hover_bg_buffer, header_text_buffer, header_bg_buffer, header_border_buffer, global_vertex_buffer, global_index_buffer,"
    )
    content = content.replace(
        "num_stage_border_vertices: 0, num_spark_vertices: 0, num_sidebar_text_vertices: 0, num_header_text_vertices: 0, num_header_border_vertices: 0,",
        "num_stage_border_vertices: 0, num_spark_vertices: 0, num_sidebar_text_vertices: 0, num_header_text_vertices: 0, num_header_border_vertices: 0, global_index_count,"
    )
    content = content.replace(
        "hover_stage_idx: None,",
        "hover_stage_idx: None, global_view_state: GlobalViewState::Inactive, global_zoom_animation: None,"
    )

    # 4. State::update
    update_injection = """
        if self.global_view_state == GlobalViewState::MorphingToTopDown {
            if self.morph_animation.is_none() {
                self.global_view_state = GlobalViewState::Swapped;
                let active_stage = &self.stages[self.selected_stage_idx];
                let france_width = 1_200_000.0; 
                let rpw = (self.size.width as f64) - 350.0;
                let target_scale = rpw * 0.9 / france_width;
                self.global_zoom_animation = Some(GlobalZoomAnimation {
                    start_time: std::time::Instant::now(),
                    duration: std::time::Duration::from_millis(2500),
                    start_scale: self.pos_scale,
                    target_scale,
                    start_center: [active_stage.global_lx, active_stage.global_ly],
                    target_center: [0.0, 0.0],
                });
                self.global_view_state = GlobalViewState::ZoomingOut;
            }
        }
        if let Some(anim) = &self.global_zoom_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let duration = anim.duration.as_secs_f64();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0_f64 - (1.0_f64 - t).powi(3);
            self.pos_scale = anim.start_scale + (anim.target_scale - anim.start_scale) * eased_t;
            self.stage_center[0] = anim.start_center[0] + (anim.target_center[0] - anim.start_center[0]) * (eased_t as f32);
            self.stage_center[1] = anim.start_center[1] + (anim.target_center[1] - anim.start_center[1]) * (eased_t as f32);
            if t >= 1.0 {
                self.global_zoom_animation = None;
                self.global_view_state = GlobalViewState::FullyGlobal;
            }
            self.window.request_redraw();
        }
    }

    pub fn get_profile_at_mouse"""
    content = content.replace("    }\n\n    pub fn get_profile_at_mouse", update_injection)

    # 5. State::render
    render_injection = """
            if self.global_view_state == GlobalViewState::Inactive || self.global_view_state == GlobalViewState::MorphingToTopDown {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }
            if self.global_view_state == GlobalViewState::Swapped || self.global_view_state == GlobalViewState::ZoomingOut || self.global_view_state == GlobalViewState::FullyGlobal {
                pass.set_pipeline(&self.global_render_pipeline);
                pass.set_vertex_buffer(0, self.global_vertex_buffer.slice(..));
                pass.set_index_buffer(self.global_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.global_index_count, 0, 0..1);
            }
"""
    content = content.replace(
        "pass.set_pipeline(&self.render_pipeline);\n            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));\n            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);\n            pass.draw_indexed(0..self.num_indices, 0, 0..1);",
        render_injection
    )

    # 6. Event loop (Pan / Zoom / Key)
    content = content.replace(
        "if state.view_mode == 1 {",
        "if state.view_mode == 1 || state.global_view_state == GlobalViewState::FullyGlobal {"
    )
    
    key_injection = """
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
                            state.camera_angle = [0.0, 0.0];
                            let active_stage = &state.stages[state.selected_stage_idx];
                            state.stage_center = [active_stage.global_lx, active_stage.global_ly];
                            let rpw = (state.size.width as f32) - 352.0;
                            state.camera_offset = [352.0 + rpw * 0.5, state.size.height as f32 * 0.5];
                            state.global_view_state = GlobalViewState::MorphingToTopDown;
                            state.rebuild_ui();
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        if state.global_view_state != GlobalViewState::Inactive {
                            state.global_view_state = GlobalViewState::Inactive;
                            state.global_zoom_animation = None;
                            let active_stage = &state.stages[state.selected_stage_idx];
                            state.stage_center = [active_stage.global_lx, active_stage.global_ly];
                            state.view_mode = 0;
                            state.target_morph = 0.0;
                            state.camera_angle = [0.5, 0.0];
                            let rpw = (state.size.width as f64) - 350.0;
                            let graph_width = rpw * 0.8;
                            state.initial_scale = graph_width / (state.max_dist as f64);
                            state.pos_scale = state.initial_scale;
                            state.pos_translate = [350.0 + rpw * 0.1, (state.size.height as f64 - 260.0) * 0.2];
                            state.morph_animation = Some(MorphAnimation {
                                start_time: std::time::Instant::now(),
                                duration: std::time::Duration::from_millis(1400),
                                start_morph: state.current_morph,
                                target_morph: 0.0,
                            });
                            state.rebuild_ui();
                            return;
                        }
"""
    content = content.replace("Key::Named(NamedKey::Space) => {", key_injection)

    with open('src/main.rs', 'w', encoding='utf-8') as f:
        f.write(content)

inject()
