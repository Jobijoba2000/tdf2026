struct Uniforms {
    translate: vec2<f32>,
    scale: f32,
    thickness: f32,
    resolution: vec2<f32>,
    y_stretch: f32,
    morph: f32,
    color: vec4<f32>,
    mouse_pos: vec2<f32>,
    raw_mouse_x: f32,
    max_dist: f32,
    y_min: f32,
    y_max: f32,
    camera_angle: vec2<f32>,
    camera_offset: vec2<f32>,
    stage_center: vec2<f32>,
    rel_scale: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) pos: vec4<f32>,  // x: dist, y: ele, z: lx, w: ly
    @location(1) prev: vec4<f32>,
    @location(2) next: vec4<f32>,
    @location(3) side: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) ele: f32,
    @location(2) world_pos: vec3<f32>,
    @location(3) dist: f32,
    @location(4) extra: vec3<f32>,
};

fn project_profile(p: vec4<f32>) -> vec2<f32> {
    let stretched_p = vec2<f32>(p.x, p.y * uniforms.y_stretch);
    return stretched_p * uniforms.scale + uniforms.translate;
}

fn project(p: vec2<f32>) -> vec2<f32> {
    return project_profile(vec4<f32>(p.x, p.y, 0.0, 0.0));
}

fn project_trace(p: vec4<f32>) -> vec2<f32> {
    let world_pos = vec3<f32>(p.z - uniforms.stage_center.x, p.w - uniforms.stage_center.y, p.y * uniforms.y_stretch);
    let heading = uniforms.camera_angle.y;
    let cos_h = cos(heading);
    let sin_h = sin(heading);
    let rx = world_pos.x * cos_h - world_pos.y * sin_h;
    let ry = world_pos.x * sin_h + world_pos.y * cos_h;
    let tilt = uniforms.camera_angle.x;
    let cos_t = cos(tilt);
    let sin_t = sin(tilt);
    let final_y = world_pos.z * cos_t - ry * sin_t;
    return vec2<f32>(rx, final_y) * uniforms.scale + uniforms.camera_offset;
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let p_p = project_profile(model.pos);
    let p_t = project_trace(model.pos);
    let p_screen = mix(p_p, p_t, uniforms.morph);
    let prev_p = project_profile(model.prev);
    let prev_t = project_trace(model.prev);
    let prev_screen = mix(prev_p, prev_t, uniforms.morph);
    let next_p = project_profile(model.next);
    let next_t = project_trace(model.next);
    let next_screen = mix(next_p, next_t, uniforms.morph);
    var dir: vec2<f32>;
    if (distance(p_screen, prev_screen) < 0.001) {
        dir = normalize(next_screen - p_screen);
    } else if (distance(p_screen, next_screen) < 0.001) {
        dir = normalize(p_screen - prev_screen);
    } else {
        let dir1 = normalize(p_screen - prev_screen);
        let dir2 = normalize(next_screen - p_screen);
        dir = normalize(dir1 + dir2);
    }
    let normal = vec2<f32>(-dir.y, dir.x);
    let thickness = uniforms.thickness * mix(1.0, 3.0, uniforms.morph);
    let offset = normal * thickness * model.side;
    let final_screen = p_screen + offset;
    out.clip_position = vec4<f32>(
        (final_screen.x / uniforms.resolution.x) * 2.0 - 1.0,
        (final_screen.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.ele = model.pos.y;
    out.uv = vec2<f32>(model.side, 0.0);
    out.world_pos = vec3<f32>(model.pos.z, model.pos.w, model.pos.y);
    out.dist = model.pos.x;
    out.extra = vec3<f32>(0.0);
    return out;
}

@vertex
fn vs_poly(@location(0) pos: vec4<f32>, @location(1) side: f32, @location(2) extra: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    let p_p = project_profile(pos);
    let p_t = project_trace(pos);
    let proj = mix(p_p, p_t, uniforms.morph);
    out.clip_position = vec4<f32>(
        (proj.x / uniforms.resolution.x) * 2.0 - 1.0,
        (proj.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.ele = pos.y;
    out.uv = vec2<f32>(side, 0.0);
    out.world_pos = vec3<f32>(pos.z, pos.w, pos.y);
    out.dist = pos.x;
    out.extra = extra;
    return out;
}

@fragment
fn fs_poly(in: VertexOutput) -> @location(0) vec4<f32> {
    let yellow = vec3<f32>(1.0, 0.9, 0.0);
    let deep_gold = vec3<f32>(0.4, 0.3, 0.0);
    let h_norm = clamp((in.ele - uniforms.y_min) / (uniforms.y_max - uniforms.y_min + 0.1), 0.0, 1.0);
    let height_color = mix(deep_gold, yellow, h_norm);
    var base_color = mix(yellow, height_color, uniforms.morph);
    
    let line_freq = 250.0;
    let dist_mod = fract(in.dist / line_freq + 0.5) - 0.5;
    let dist_pixels = abs(dist_mod * line_freq) / fwidth(in.dist);
    let line_pattern = smoothstep(1.2, 0.0, dist_pixels);
    
    let line_intensity = (line_pattern * 0.8 + in.extra.x * 0.6); 
    base_color = mix(base_color, base_color + vec3<f32>(0.4), line_intensity);
    
    let shading = mix(1.0, 0.8 + in.extra.x * 0.4, uniforms.morph);
    let wall_darkening = mix(1.0, 0.4 + 0.6 * abs(in.uv.x), uniforms.morph);
    
    return vec4<f32>(base_color * wall_darkening * shading, 1.0);
}

@fragment
fn fs_white(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = mix(1.0, 0.4, uniforms.morph);
    return vec4<f32>(1.0, 1.0, 1.0, alpha); 
}

@fragment
fn fs_yellow(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.9, 0.0, 1.0); 
}

@vertex
fn vs_ui(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(
        (pos.x / uniforms.resolution.x) * 2.0 - 1.0,
        (pos.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.uv = vec2<f32>(0.0, 0.0);
    out.ele = 0.0;
    out.world_pos = vec3<f32>(0.0);
    out.dist = 0.0;
    out.extra = vec3<f32>(0.0);
    return out;
}

@fragment
fn fs_ground(in: VertexOutput) -> @location(0) vec4<f32> {
    let black = vec3<f32>(0.0, 0.0, 0.0);
    let white = vec3<f32>(1.0, 1.0, 1.0);
    
    // UVs: u from in.dist, v from in.uv.x (0.0 to 1.0)
    let u = in.dist;
    let v = in.uv.x;
    
    // Sine-based grid: 10 cycles of sin(pi * x) gives 10 stripes
    // sin(x) is positive for [0, pi], so sin(u * 10 * PI)
    let grid_u = step(0.95, sin(u * 31.4159));
    let grid_v = step(0.95, sin(v * 31.4159));
    
    // Borders
    let border_u = step(0.99, u) + step(u, 0.01);
    let border_v = step(0.99, v) + step(v, 0.01);
    
    let grid = clamp(grid_u + grid_v + border_u + border_v, 0.0, 1.0);
    
    var final_color = black;
    if (in.extra.y > 0.5) { // Top surface
        final_color = mix(black, white, grid * 0.6);
    } else {
        final_color = vec3<f32>(0.02); // Dark grey for sides
    }
    
    let alpha = mix(0.0, 1.0, uniforms.morph);
    return vec4<f32>(final_color, alpha);
}

@fragment
fn fs_sidebar_bg(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.1, 0.1, 0.1, 1.0);
}

@fragment
fn fs_header_bg(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_selected_bg(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.2, 0.2, 0.05, 0.8);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return uniforms.color;
}

// --- Text ---
struct TextVertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) anchor: vec2<f32>,
    @location(3) size: f32,
};

struct TextVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_text(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let initial_scale = (uniforms.resolution.x - 500.0) / uniforms.max_dist;
    let rel_scale = uniforms.scale / initial_scale;
    let capped_scale = initial_scale * min(rel_scale, 10.0);
    let anchor_proj = vec2<f32>(
        in.anchor.x * capped_scale + uniforms.translate.x,
        in.anchor.y * uniforms.y_stretch * uniforms.scale + uniforms.translate.y
    );
    let final_pos = anchor_proj + vec2<f32>(in.pos.x, -in.pos.y) * (in.size * uniforms.rel_scale);
    out.position = vec4<f32>(
        (final_pos.x / uniforms.resolution.x) * 2.0 - 1.0,
        (final_pos.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.uv = in.uv;
    return out;
}

@vertex
fn vs_text_screen(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let final_pos = in.anchor + vec2<f32>(in.pos.x, -in.pos.y) * in.size * uniforms.rel_scale;
    out.position = vec4<f32>(
        (final_pos.x / uniforms.resolution.x) * 2.0 - 1.0,
        (final_pos.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.uv = in.uv;
    return out;
}

@vertex
fn vs_text_ui(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let final_pos = in.anchor + vec2<f32>(in.pos.x, -in.pos.y) * in.size;
    out.position = vec4<f32>(
        (final_pos.x / uniforms.resolution.x) * 2.0 - 1.0,
        (final_pos.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.uv = in.uv;
    return out;
}

@group(1) @binding(0) var t_sampler: sampler;
@group(1) @binding(1) var t_color: texture_2d<f32>;

@fragment
fn fs_text_bold(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let tex_size = vec2<f32>(textureDimensions(t_color));
    let bold_off = 1.2 / tex_size;
    let a_center = textureSample(t_color, t_sampler, in.uv).a;
    let a_bold = max(a_center, max(
        max(textureSample(t_color, t_sampler, in.uv + vec2<f32>(bold_off.x, 0.0)).a, 
            textureSample(t_color, t_sampler, in.uv - vec2<f32>(bold_off.x, 0.0)).a),
        max(textureSample(t_color, t_sampler, in.uv + vec2<f32>(0.0, bold_off.y)).a,
            textureSample(t_color, t_sampler, in.uv - vec2<f32>(0.0, bold_off.y)).a)
    ));
    let outline_off = 3.2 / tex_size;
    let o_diag = outline_off * 0.707;
    let a_outline = max(
        max(max(textureSample(t_color, t_sampler, in.uv + vec2<f32>(outline_off.x, 0.0)).a, 
                textureSample(t_color, t_sampler, in.uv - vec2<f32>(outline_off.x, 0.0)).a),
            max(textureSample(t_color, t_sampler, in.uv + vec2<f32>(0.0, outline_off.y)).a,
                textureSample(t_color, t_sampler, in.uv - vec2<f32>(0.0, outline_off.y)).a)),
        max(max(textureSample(t_color, t_sampler, in.uv + vec2<f32>(o_diag.x, o_diag.y)).a, 
                textureSample(t_color, t_sampler, in.uv - vec2<f32>(o_diag.x, o_diag.y)).a),
            max(textureSample(t_color, t_sampler, in.uv + vec2<f32>(o_diag.x, -o_diag.y)).a,
                textureSample(t_color, t_sampler, in.uv - vec2<f32>(o_diag.x, -o_diag.y)).a))
    );
    let final_alpha = max(a_bold, a_outline);
    if (final_alpha < 0.01) { discard; }
    let color = mix(vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0), a_bold);
    return vec4<f32>(color, final_alpha);
}

@fragment
fn fs_text_std(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let a = textureSample(t_color, t_sampler, in.uv).a;
    if (a < 0.01) { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, a);
}

struct ReticuleOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_pos: vec2<f32>,
};

@vertex
fn vs_reticule(@builtin(vertex_index) vertex_index: u32) -> ReticuleOutput {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0, -1.0), vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0), vec2<f32>( 1.0, -1.0), vec2<f32>( 1.0,  1.0)
    );
    var out: ReticuleOutput;
    let p = pos[vertex_index];
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.screen_pos = (p * 0.5 + 0.5) * uniforms.resolution;
    return out;
}

@fragment
fn fs_reticule(in: ReticuleOutput) -> @location(0) vec4<f32> {
    let mouse = uniforms.mouse_pos;
    let pos = in.screen_pos;
    let line_thickness = uniforms.thickness * 0.8;
    let world_y = (pos.y - uniforms.translate.y) / (uniforms.y_stretch * uniforms.scale);
    let range = uniforms.y_max - uniforms.y_min;
    let ext_y = range * 0.05;
    if (world_y < uniforms.y_min - ext_y || world_y > uniforms.y_max + ext_y) { discard; }
    let dist_x_line = abs(pos.x - uniforms.raw_mouse_x);
    if (dist_x_line < line_thickness) {
        let mouse_world_x = (uniforms.raw_mouse_x - uniforms.translate.x) / uniforms.scale;
        if (mouse_world_x >= 0.0 && mouse_world_x <= uniforms.max_dist) {
            return vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
    }
    discard;
}

@vertex
fn vs_dot(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let proj = project(in.anchor);
    let final_pos = proj + in.pos * 6.0;
    out.position = vec4<f32>(
        (final_pos.x / uniforms.resolution.x) * 2.0 - 1.0,
        (final_pos.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    out.uv = in.pos;
    return out;
}

@fragment
fn fs_dot(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.uv);
    if (dist > 1.0) { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, smoothstep(1.0, 0.8, dist));
}
