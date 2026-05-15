struct Uniforms {
    view_proj: mat4x4<f32>,    // 0-64
    translate: vec2<f32>,      // 64-72
    scale: f32,                // 72-76
    thickness: f32,            // 76-80
    resolution: vec2<f32>,     // 80-88
    y_stretch: f32,            // 88-92
    morph: f32,                // 92-96
    color: vec4<f32>,          // 96-112
    mouse_pos: vec2<f32>,      // 112-120
    raw_mouse_x: f32,          // 120-124
    max_dist: f32,             // 124-128
    y_min: f32,                // 128-132
    y_max: f32,                // 132-136
    rel_scale: f32,            // 136-140
    camera_tilt: f32,          // 140-144
    camera_heading: f32,       // 144-148
    _pad0: f32,                // 148-152
    _pad1: f32,                // 152-156
    _pad2: f32,                // 156-160
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) pos: vec4<f32>,   // dist, ele, lx, ly
    @location(1) prev: vec4<f32>,
    @location(2) next: vec4<f32>,
    @location(3) side: f32,
};

struct PolyVertexInput {
    @location(0) pos: vec4<f32>,   // x, y, lx, ly
    @location(1) side: f32,        // 1.0 for top, 0.0 for bottom
    @location(2) flag: f32,
    @location(3) normal: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ele: f32,
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) dist: f32,
    @location(4) morph: f32,
    @location(5) normal: vec3<f32>,
};

fn project_2d(dist: f32, ele: f32) -> vec2<f32> {
    return vec2<f32>(
        dist * uniforms.scale + uniforms.translate.x,
        ele * uniforms.y_stretch * uniforms.scale + uniforms.translate.y
    );
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Position 2D (Profil)
    let p2d = project_2d(model.pos.x, model.pos.y);
    let prev2d = project_2d(model.prev.x, model.prev.y);
    let next2d = project_2d(model.next.x, model.next.y);
    
    let current_dir = normalize(next2d - prev2d);
    let normal = vec2<f32>(-current_dir.y, current_dir.x);
    let screen_pos_2d = p2d + normal * model.side * uniforms.thickness;
    let clip_2d = vec4<f32>((screen_pos_2d / uniforms.resolution) * 2.0 - 1.0, 0.5, 1.0);

    // Position 3D (Trace) - Reduced exaggeration (0.5x) + Tiny offset to avoid Z-fighting
    let world_pos = vec4<f32>(model.pos.z, model.pos.w, (model.pos.y - uniforms.y_min) * uniforms.y_stretch * 0.5 + 0.1, 1.0);
    let prev_world = vec4<f32>(model.prev.z, model.prev.w, (model.prev.y - uniforms.y_min) * uniforms.y_stretch * 0.5 + 0.1, 1.0);
    let next_world = vec4<f32>(model.next.z, model.next.w, (model.next.y - uniforms.y_min) * uniforms.y_stretch * 0.5 + 0.1, 1.0);
    
    let p3d_clip = uniforms.view_proj * world_pos;
    let prev3d_clip = uniforms.view_proj * prev_world;
    let next3d_clip = uniforms.view_proj * next_world;
    
    // Screen space normal for 3D stroke
    let p3d_scr = (p3d_clip.xy / p3d_clip.w + 1.0) * 0.5 * uniforms.resolution;
    let prev3d_scr = (prev3d_clip.xy / prev3d_clip.w + 1.0) * 0.5 * uniforms.resolution;
    let next3d_scr = (next3d_clip.xy / next3d_clip.w + 1.0) * 0.5 * uniforms.resolution;
    
    let dir3d = normalize(next3d_scr - prev3d_scr);
    let normal3d = vec2<f32>(-dir3d.y, dir3d.x);
    let screen_pos_3d = p3d_scr + normal3d * model.side * uniforms.thickness;
    let clip_3d = vec4<f32>((screen_pos_3d / uniforms.resolution) * 2.0 - 1.0, p3d_clip.z / p3d_clip.w, 1.0);

    // Morphing progressif par distance (effet ruban)
    let stagger = 0.5;
    let local_morph = clamp((uniforms.morph * (1.0 + stagger)) - (model.pos.x / uniforms.max_dist) * stagger, 0.0, 1.0);
    out.clip_position = mix(clip_2d, clip_3d, local_morph);
    out.ele = model.pos.y;
    out.uv = vec2<f32>(model.side, 0.0);
    out.world_pos = world_pos.xyz;
    out.dist = model.pos.x;
    out.morph = local_morph;
    out.normal = vec3<f32>(0.0, 0.0, 1.0);
    return out;
}

@vertex
fn vs_axes(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    let p2d = project_2d(model.pos.x, model.pos.y);
    let prev2d = project_2d(model.prev.x, model.prev.y);
    let next2d = project_2d(model.next.x, model.next.y);
    
    let dir = normalize(next2d - prev2d);
    let normal = vec2<f32>(-dir.y, dir.x);
    let final_p2d = p2d + normal * model.side * uniforms.thickness;

    // Convert to NDC space
    out.clip_position = vec4<f32>((final_p2d / uniforms.resolution) * 2.0 - 1.0, 0.0, 1.0);
    out.morph = uniforms.morph;
    out.ele = 0.0;
    out.uv = vec2<f32>(0.0, 0.0);
    out.world_pos = vec3<f32>(0.0, 0.0, 0.0);
    out.dist = 0.0;
    out.normal = vec3<f32>(0.0, 0.0, 1.0);
    return out;
}

@vertex
fn vs_poly(model: PolyVertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Position 2D
    let p2d_scr = project_2d(model.pos.x, model.pos.y); // Use distance, elevation for 2D profile
    let clip_2d = vec4<f32>((p2d_scr / uniforms.resolution) * 2.0 - 1.0, 0.51, 1.0);

    // Position 3D - Reduced exaggeration (0.5x) - Grounded at Z=0
    let world_pos = vec4<f32>(model.pos.z, model.pos.w, (model.pos.y - uniforms.y_min) * uniforms.y_stretch * 0.5, 1.0);
    let clip_3d = uniforms.view_proj * world_pos;

    // Morphing progressif par distance
    let stagger = 0.5;
    let local_morph = clamp((uniforms.morph * (1.0 + stagger)) - (model.pos.x / uniforms.max_dist) * stagger, 0.0, 1.0);
    out.clip_position = mix(clip_2d, clip_3d, local_morph);
    out.ele = model.pos.y;
    out.uv = vec2<f32>(model.side, model.flag);
    out.world_pos = world_pos.xyz;
    out.dist = model.pos.x;
    out.morph = local_morph;
    out.normal = vec3<f32>(model.normal, 0.0);
    return out;
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
    out.morph = uniforms.morph;
    out.normal = vec3<f32>(0.0, 0.0, 1.0);
    return out;
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
fn fs_poly(in: VertexOutput) -> @location(0) vec4<f32> {
    // Jaune équilibré
    let yellow = uniforms.color.rgb * 0.75;
    
    // 1. NORMALE PIVOTÉE
    let raw_normal = normalize(in.normal);
    let h = uniforms.camera_heading;
    let cos_h = cos(h);
    let sin_h = sin(h);
    let rotated_normal = vec3<f32>(
        raw_normal.x * cos_h - raw_normal.y * sin_h,
        raw_normal.x * sin_h + raw_normal.y * cos_h,
        0.0
    );
    
    // 2. ÉCLAIRAGE MULTI-SOURCES (Équilibré pour le contraste)
    let light_dir1 = normalize(vec3<f32>(-0.8, 0.4, 0.5)); 
    let diff1 = max(dot(rotated_normal, light_dir1), 0.0);
    let light_dir2 = normalize(vec3<f32>(0.8, -0.4, 0.3));
    let diff2 = max(dot(rotated_normal, light_dir2), 0.0);
    
    // Ambient réduit pour donner plus de relief
    let ambient = 0.35;
    
    // 3. EFFETS PREMIUM (Subtils)
    let view_dir = normalize(vec3<f32>(0.0, 0.0, 1.0));
    
    // Spéculaire très fin
    let half_dir = normalize(light_dir1 + view_dir);
    let spec = pow(max(dot(rotated_normal, half_dir), 0.0), 64.0);
    
    // Fresnel très discret pour le contour
    let fresnel = pow(1.0 - max(dot(rotated_normal, view_dir), 0.0), 5.0);
    
    // Gradient vertical doux
    let height_factor = smoothstep(0.0, 600.0, in.world_pos.z);
    let depth_grad = mix(0.9, 1.1, height_factor);
    
    // 4. COMBINAISON
    let border_glow = mix(0.8, 1.0, smoothstep(0.0, 0.03, in.uv.x) * smoothstep(1.0, 0.97, in.uv.x));
    
    // On combine l'éclairage (Key 40% + Fill 20%)
    let lighting = (ambient + diff1 * 0.4 + diff2 * 0.2) * border_glow * depth_grad;
    
    let final_lighting = mix(0.85, lighting, in.morph);
    
    // Éclat additif très léger (blancs)
    let shine = (spec * 0.2 + fresnel * 0.15) * in.morph;
    let final_color = yellow * final_lighting + vec3<f32>(shine);
    
    return vec4<f32>(final_color, 1.0);
}

@fragment
fn fs_yellow(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.85, 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0); // Blanc pur
}

@fragment
fn fs_axes(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = clamp(1.0 - in.morph * 2.0, 0.0, 1.0); // Disparaît vite
    return vec4<f32>(uniforms.color.rgb, uniforms.color.a * alpha);
}

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
fn fs_text_graph(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let a_center = textureSample(t_color, t_sampler, in.uv).a;
    let size = vec2<f32>(textureDimensions(t_color, 0));
    let offset = 2.5 / size; 
    
    let a1 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(offset.x, 0.0)).a;
    let a2 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(offset.x, 0.0)).a;
    let a3 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(0.0, offset.y)).a;
    let a4 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(0.0, offset.y)).a;
    let a5 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(offset.x, offset.y)).a;
    let a6 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(offset.x, offset.y)).a;
    let a7 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(offset.x, -offset.y)).a;
    let a8 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(offset.x, -offset.y)).a;
    let outline = max(max(max(a1, a2), max(a3, a4)), max(max(a5, a6), max(a7, a8)));

    if (outline < 0.01 && a_center < 0.01) { discard; }
    
    // Simuler du gras
    let bold_a = smoothstep(0.2, 0.5, a_center);
    let final_color = mix(vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0), bold_a);
    return vec4<f32>(final_color, max(bold_a, outline));
}

@fragment
fn fs_text_bold(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let a_center = textureSample(t_color, t_sampler, in.uv).a;
    
    // Contour noir par échantillonnage des voisins
    let size = vec2<f32>(textureDimensions(t_color, 0));
    let offset = 1.5 / size;
    let a1 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(offset.x, 0.0)).a;
    let a2 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(offset.x, 0.0)).a;
    let a3 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(0.0, offset.y)).a;
    let a4 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(0.0, offset.y)).a;
    let outline = max(max(a1, a2), max(a3, a4));

    if (outline < 0.01 && a_center < 0.01) { discard; }
    
    let final_color = mix(vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0), a_center);
    return vec4<f32>(final_color, max(a_center, outline * 0.8));
}

@fragment
fn fs_text_std(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let a_center = textureSample(t_color, t_sampler, in.uv).a;
    
    let size = vec2<f32>(textureDimensions(t_color, 0));
    let offset = 1.2 / size;
    let a1 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(offset.x, 0.0)).a;
    let a2 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(offset.x, 0.0)).a;
    let a3 = textureSample(t_color, t_sampler, in.uv + vec2<f32>(0.0, offset.y)).a;
    let a4 = textureSample(t_color, t_sampler, in.uv - vec2<f32>(0.0, offset.y)).a;
    let outline = max(max(a1, a2), max(a3, a4));

    if (outline < 0.01 && a_center < 0.01) { discard; }
    
    let final_color = mix(vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0), a_center);
    return vec4<f32>(final_color, max(a_center, outline * 0.7));
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
    if (world_y < uniforms.y_min - range * 0.05 || world_y > uniforms.y_max + range * 0.05) { discard; }
    if (abs(pos.x - uniforms.raw_mouse_x) < line_thickness) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }
    discard;
}

@vertex
fn vs_dot(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let stretched_p = vec2<f32>(in.anchor.x, in.anchor.y * uniforms.y_stretch);
    let proj = stretched_p * uniforms.scale + uniforms.translate;
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
