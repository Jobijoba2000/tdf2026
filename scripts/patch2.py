import re

with open('src/main.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Define CameraAnimation
camera_anim_struct = """struct CameraAnimation {
    start_time: std::time::Instant,
    duration: std::time::Duration,
    start_angle: [f32; 2],
    target_angle: [f32; 2],
    start_offset: [f32; 2],
    target_offset: [f32; 2],
}"""

content = content.replace("struct GlobalZoomAnimation {", camera_anim_struct + "\nstruct GlobalZoomAnimation {")

# 2. Add to State
content = content.replace(
    "global_zoom_animation: Option<GlobalZoomAnimation>,",
    "global_zoom_animation: Option<GlobalZoomAnimation>,\n    camera_animation: Option<CameraAnimation>,"
)

# 3. Add to State::new
content = content.replace(
    "global_zoom_animation: None,",
    "global_zoom_animation: None, camera_animation: None,"
)

# 4. Process CameraAnimation in update
update_code = """
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

        if self.global_view_state == GlobalViewState::MorphingToTopDown {
"""
content = content.replace("        if self.global_view_state == GlobalViewState::MorphingToTopDown {", update_code)

# 5. Enter Key Animation
old_enter = """                            state.camera_angle = [0.0, 0.0];
                            let active_stage = &state.stages[state.selected_stage_idx];
                            state.stage_center = [active_stage.global_lx, active_stage.global_ly];
                            let rpw = (state.size.width as f32) - 352.0;
                            state.camera_offset = [352.0 + rpw * 0.5, state.size.height as f32 * 0.5];
                            state.global_view_state = GlobalViewState::MorphingToTopDown;"""

new_enter = """                            let active_stage = &state.stages[state.selected_stage_idx];
                            state.stage_center = [active_stage.global_lx, active_stage.global_ly];
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
                            
                            state.global_view_state = GlobalViewState::MorphingToTopDown;"""
content = content.replace(old_enter, new_enter)

# 6. Space Key Fix (Reset camera_angle and camera_offset correctly)
old_space = """                            state.view_mode = 0;
                            state.target_morph = 0.0;
                            state.camera_angle = [0.5, 0.0];
                            let rpw = (state.size.width as f64) - 350.0;"""

new_space = """                            state.view_mode = 0;
                            state.target_morph = 0.0;
                            state.camera_angle = [0.0, 0.0];
                            let rpw = (state.size.width as f64) - 350.0;
                            state.camera_offset = [350.0 + (rpw as f32) * 0.5, state.size.height as f32 * 0.5];"""
content = content.replace(old_space, new_space)

# 7. Mouse Wheel Limits
old_wheel = """                let zoom_in = amount > 0.0;
                let factor = if zoom_in { 1.5_f64 } else { 1.0 / 1.5_f64 };
                let target_scale = (state.pos_scale * factor).clamp(state.initial_scale, state.initial_scale * 500.0);"""

new_wheel = """                let zoom_in = amount > 0.0;
                let factor = if zoom_in { 1.5_f64 } else { 1.0 / 1.5_f64 };
                let min_scale = if state.global_view_state != GlobalViewState::Inactive { state.initial_scale / 5000.0 } else { state.initial_scale };
                let target_scale = (state.pos_scale * factor).clamp(min_scale, state.initial_scale * 500.0);"""
content = content.replace(old_wheel, new_wheel)

with open('src/main.rs', 'w', encoding='utf-8') as f:
    f.write(content)
