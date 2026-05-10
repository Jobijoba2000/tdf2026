struct Uniforms {
    translate: vec2<f32>,
    scale: f32,
    thickness: f32,
    resolution: vec2<f32>,
    y_stretch: f32,
    _pad1: f32,
    color: vec4<f32>,
    mouse_pos: vec2<f32>,
    _pad2: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) prev: vec2<f32>,
    @location(2) next: vec2<f32>,
    @location(3) side: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

fn project(p: vec2<f32>) -> vec2<f32> {
    let stretched_p = vec2<f32>(p.x, p.y * uniforms.y_stretch);
    return stretched_p * uniforms.scale + uniforms.translate;
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let p_screen = project(model.pos);
    let prev_screen = project(model.prev);
    let next_screen = project(model.next);

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
    let offset = normal * uniforms.thickness * model.side;
    let final_screen = p_screen + offset;
    
    out.clip_position = vec4<f32>(
        (final_screen.x / uniforms.resolution.x) * 2.0 - 1.0,
        (final_screen.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    return out;
}

@vertex
fn vs_poly(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    let proj = project(pos);
    out.clip_position = vec4<f32>(
        (proj.x / uniforms.resolution.x) * 2.0 - 1.0,
        (proj.y / uniforms.resolution.y) * 2.0 - 1.0,
        0.0, 1.0
    );
    return out;
}

@fragment
fn fs_poly() -> @location(0) vec4<f32> {
    // Couleur #444 plus marquée (environ 0.25 en sRGB)
    return vec4<f32>(0.2, 0.2, 0.2, 1.0); 
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
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
    let anchor_proj = project(in.anchor);
    // On multiplie par rel_scale pour que la police suive le zoom
    let final_pos = anchor_proj + vec2<f32>(in.pos.x, -in.pos.y) * (in.size * uniforms._pad1);
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
    // Anchor est déjà en pixels écran. On multiplie la taille par l'échelle relative
    let final_pos = in.anchor + vec2<f32>(in.pos.x, -in.pos.y) * in.size * uniforms._pad1;
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
fn fs_text(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_color, t_sampler, in.uv).a;
    if (alpha < 0.01) { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, alpha);
}

// --- Reticule ---
struct ReticuleOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_pos: vec2<f32>,
};

@vertex
fn vs_reticule(@builtin(vertex_index) vertex_index: u32) -> ReticuleOutput {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0)
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
    let dash_period = 15.0;
    let center_size = 15.0;
    let cross_thickness = uniforms.thickness * 1.5;

    let dist_x = abs(pos.x - mouse.x);
    let dist_y = abs(pos.y - mouse.y);

    let world_y = (pos.y - uniforms.translate.y) / (uniforms.y_stretch * uniforms.scale);
    let ext_y = 270.0;
    if (world_y < -ext_y || world_y > 2700.0 + ext_y) {
        discard;
    }

    if (dist_x < center_size && dist_y < center_size) {
        if (dist_x < cross_thickness || dist_y < cross_thickness) {
            return vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
    }

    if (dist_x < line_thickness) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }
    discard;
}

@vertex
fn vs_dot(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let proj = project(in.anchor);
    // Point de 12px de diamètre
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
