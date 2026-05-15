import re

# 1. Modify shader.wgsl
with open('src/shader.wgsl', 'r', encoding='utf-8') as f:
    shader = f.read()

shader = shader.replace("""    camera_heading: f32,       // 144-148
    _pad0: f32,                // 148-152
    _pad1: f32,                // 152-156
    _pad2: f32,                // 156-160""", """    camera_heading: f32,       // 144-148
    global_center: vec2<f32>,  // 148-156
    _pad2: f32,                // 156-160""")

vs_global_old = """@vertex
fn vs_global(model: GlobalVertexInput) -> GlobalVertexOutput {
    var out: GlobalVertexOutput;
    
    let p_clip = uniforms.view_proj * vec4<f32>(model.pos.x, model.pos.y, -0.5, 1.0);
    let prev_clip = uniforms.view_proj * vec4<f32>(model.prev.x, model.prev.y, -0.5, 1.0);
    let next_clip = uniforms.view_proj * vec4<f32>(model.next.x, model.next.y, -0.5, 1.0);"""

vs_global_new = """@vertex
fn vs_global(model: GlobalVertexInput) -> GlobalVertexOutput {
    var out: GlobalVertexOutput;
    
    let p_model = vec4<f32>(model.pos.x - uniforms.global_center.x, model.pos.y - uniforms.global_center.y, 0.5, 1.0);
    let prev_model = vec4<f32>(model.prev.x - uniforms.global_center.x, model.prev.y - uniforms.global_center.y, 0.5, 1.0);
    let next_model = vec4<f32>(model.next.x - uniforms.global_center.x, model.next.y - uniforms.global_center.y, 0.5, 1.0);
    
    let p_clip = uniforms.view_proj * p_model;
    let prev_clip = uniforms.view_proj * prev_model;
    let next_clip = uniforms.view_proj * next_model;"""

shader = shader.replace(vs_global_old, vs_global_new)

with open('src/shader.wgsl', 'w', encoding='utf-8') as f:
    f.write(shader)

# 2. Modify main.rs
with open('src/main.rs', 'r', encoding='utf-8') as f:
    main_rs = f.read()

main_rs = main_rs.replace("""    camera_heading: f32,       // 144-148
    _pad0: f32,                // 148-152
    _pad1: f32,                // 152-156
    _pad2: f32,                // 156-160""", """    camera_heading: f32,       // 144-148
    global_center: [f32; 2],   // 148-156
    _pad2: f32,                // 156-160""")

# Update uniforms struct creation
main_rs = main_rs.replace("""            camera_tilt: tilt,
            camera_heading: heading,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,""", """            camera_tilt: tilt,
            camera_heading: heading,
            global_center: [self.stages[self.selected_stage_idx].global_lx, self.stages[self.selected_stage_idx].global_ly],
            _pad2: 0.0,""")

# Fix ZoomAnimation logic
old_zoom = """        if let Some(anim) = &self.global_zoom_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let duration = anim.duration.as_secs_f64();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0 - (1.0 - t).powi(3);
            
            self.pos_scale = anim.start_scale + (anim.target_scale - anim.start_scale) * eased_t;
            self.stage_center[0] = anim.start_center[0] + (anim.target_center[0] - anim.start_center[0]) * (eased_t as f32);
            self.stage_center[1] = anim.start_center[1] + (anim.target_center[1] - anim.start_center[1]) * (eased_t as f32);
            if t >= 1.0 {
                self.global_zoom_animation = None;
                self.global_view_state = GlobalViewState::FullyGlobal;
            }
            self.window.request_redraw();
        }"""

new_zoom = """        if let Some(anim) = &self.global_zoom_animation {
            let elapsed = anim.start_time.elapsed().as_secs_f64();
            let duration = anim.duration.as_secs_f64();
            let t = (elapsed / duration).min(1.0);
            let eased_t = 1.0 - (1.0 - t).powi(3);
            
            self.pos_scale = anim.start_scale + (anim.target_scale - anim.start_scale) * eased_t;
            self.camera_offset[0] = (anim.start_center[0] + (anim.target_center[0] - anim.start_center[0]) * eased_t) as f32;
            self.camera_offset[1] = (anim.start_center[1] + (anim.target_center[1] - anim.start_center[1]) * eased_t) as f32;
            if t >= 1.0 {
                self.global_zoom_animation = None;
                self.global_view_state = GlobalViewState::FullyGlobal;
            }
            self.window.request_redraw();
        }"""
main_rs = main_rs.replace(old_zoom, new_zoom)

# Fix ZoomAnimation initialization
old_anim_init = """                let target_scale = rpw * 0.9 / france_width;
                self.global_zoom_animation = Some(GlobalZoomAnimation {
                    start_time: std::time::Instant::now(),
                    duration: std::time::Duration::from_millis(2500),
                    start_scale: self.pos_scale,
                    target_scale,
                    start_center: [active_stage.global_lx, active_stage.global_ly],
                    target_center: [0.0, 0.0],
                });"""

new_anim_init = """                let target_scale = rpw * 0.9 / france_width;
                
                let c_x = 352.0 + (rpw as f32) * 0.5;
                let c_y = self.size.height as f32 * 0.5;
                
                let p_france_x = -active_stage.global_lx;
                let p_france_y = -active_stage.global_ly;
                
                let target_offset_x = c_x - (target_scale as f32) * p_france_x;
                let target_offset_y = c_y - (target_scale as f32) * p_france_y;
                
                self.global_zoom_animation = Some(GlobalZoomAnimation {
                    start_time: std::time::Instant::now(),
                    duration: std::time::Duration::from_millis(2500),
                    start_scale: self.pos_scale,
                    target_scale,
                    start_center: [self.camera_offset[0] as f64, self.camera_offset[1] as f64],
                    target_center: [target_offset_x as f64, target_offset_y as f64],
                });"""
main_rs = main_rs.replace(old_anim_init, new_anim_init)

# Fix rendering block
old_render = """            if self.global_view_state == GlobalViewState::Inactive || self.global_view_state == GlobalViewState::MorphingToTopDown {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }
            if self.global_view_state == GlobalViewState::Swapped || self.global_view_state == GlobalViewState::ZoomingOut || self.global_view_state == GlobalViewState::FullyGlobal {
                pass.set_pipeline(&self.global_render_pipeline);"""

new_render = """            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);

            if self.global_view_state == GlobalViewState::Swapped || self.global_view_state == GlobalViewState::ZoomingOut || self.global_view_state == GlobalViewState::FullyGlobal {
                pass.set_pipeline(&self.global_render_pipeline);"""
main_rs = main_rs.replace(old_render, new_render)

with open('src/main.rs', 'w', encoding='utf-8') as f:
    f.write(main_rs)
