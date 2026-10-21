// Hyperbolic {7,3} tiling wallpaper shader
// Based on the hyperbolic geometry fragment shader

// Uniforms from the host
struct Uniforms {
    resolution: vec2<f32>,
    camera: vec2<f32>,
    zoom: f32,
    time: f32,
    pan_angle: f32,
    pan_rate: f32,
    rmax: f32,
    palette_colors: array<vec4<f32>, 16>,
    palette_size: u32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0)
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

// Hyperbolic distance in Poincaré disk model
fn hyperbolic_distance(z1: vec2<f32>, z2: vec2<f32>) -> f32 {
    let d = length(z1 - z2);
    let n1 = 1.0 - dot(z1, z1);
    let n2 = 1.0 - dot(z2, z2);
    if n1 <= 0.0 || n2 <= 0.0 {
        return 100.0; // Outside disk
    }
    let arg = 1.0 + 2.0 * d * d / (n1 * n2);
    return acosh(arg);
}

// Möbius transformation for hyperbolic translation
fn mobius_translate(z: vec2<f32>, t: vec2<f32>) -> vec2<f32> {
    let t_conj = vec2<f32>(t.x, -t.y);
    let num = z - t;
    let den = 1.0 - dot(z, t_conj);
    return num / den;
}

// Generate {7,3} tiling centers using recursion
fn generate_centers(center: vec2<f32>, depth: u32, max_depth: u32, centers: ptr<function, array<vec2<f32>, 256>>, count: ptr<function, u32>) {
    if depth >= max_depth {
        return;
    }
    
    // Store this center
    let idx = *count;
    if idx < 256u {
        centers[idx] = center;
        *count = idx + 1u;
    }
    
    // For {7,3} tiling: 7 heptagons around each vertex
    // Each heptagon has 7 neighbors at distance determined by the tiling
    let angle_step = 2.0 * 3.14159265359 / 7.0;
    let neighbor_distance = 0.65; // Approximate for {7,3}
    
    for (var i: u32 = 0u; i < 7u; i++) {
        let angle = f32(i) * angle_step + uniforms.pan_angle;
        let dir = vec2<f32>(cos(angle), sin(angle));
        
        // Move in hyperbolic space
        let t = dir * neighbor_distance;
        let new_center = mobius_translate(center, t);
        
        generate_centers(new_center, depth + 1u, max_depth, centers, count);
    }
}

// Check if point is in a heptagon centered at c
fn in_heptagon(p: vec2<f32>, c: vec2<f32>, radius: f32) -> f32 {
    let d = hyperbolic_distance(p, c);
    return smoothstep(radius - 0.02, radius + 0.02, d);
}

// Color from palette based on depth/index
fn palette_color(index: u32) -> vec4<f32> {
    let size = min(uniforms.palette_size, 16u);
    return uniforms.palette_colors[index % size];
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / uniforms.resolution;
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    
    // Map to Poincaré disk coordinates
    // Center the disk and scale by zoom
    let p = (uv - 0.5) * 2.0 * vec2<f32>(aspect, 1.0) / uniforms.zoom;
    p += uniforms.camera / uniforms.zoom * 0.1; // Camera offset
    
    // Check if inside unit disk
    let r2 = dot(p, p);
    if r2 >= 1.0 {
        // Outside disk - background color
        return vec4<f32>(0.02, 0.02, 0.05, 1.0);
    }
    
    // Generate tiling centers on the fly (simplified for shader)
    // In practice, this would be precomputed or use a more efficient approach
    
    // Simplified {7,3} pattern using polar coordinates
    let r = length(p);
    let theta = atan2(p.y, p.x);
    
    // Add pan rotation
    let rotated_theta = theta - uniforms.pan_angle;
    
    // Heptagonal grid in polar coordinates
    let rings = 5;
    var min_dist = 100.0;
    var best_center = vec2<f32>(0.0, 0.0);
    var best_ring = 0;
    
    for (var ring: i32 = 0; ring <= rings; ring++) {
        let ring_r = f32(ring) * 0.15;
        let count = max(1, ring * 7);
        
        for (var i: i32 = 0; i < count; i++) {
            let angle = f32(i) * 2.0 * 3.14159265359 / f32(count) + uniforms.pan_angle;
            let cx = ring_r * cos(angle);
            let cy = ring_r * sin(angle);
            let center = vec2<f32>(cx, cy);
            
            let d = hyperbolic_distance(p, center);
            if d < min_dist {
                min_dist = d;
                best_center = center;
                best_ring = ring;
            }
        }
    }
    
    // Color based on distance to nearest center (creates cell boundaries)
    let cell_edge = smoothstep(0.12, 0.14, min_dist);
    let cell_center = 1.0 - smoothstep(0.0, 0.05, min_dist);
    
    // Alternate colors by ring and position
    let ring_color_idx = (best_ring + int(rotated_theta * 7.0 / (2.0 * 3.14159265359))) % int(uniforms.palette_size);
    let color = palette_color(u32(ring_color_idx));
    
    // Mix cell color with edge highlight
    let final_color = mix(color, vec4<f32>(1.0, 1.0, 1.0, 1.0) * color, cell_edge * 0.3);
    final_color = mix(vec4<f32>(0.0), final_color, cell_center);
    
    // Add subtle animation
    let pulse = sin(uniforms.time * 0.5 + min_dist * 20.0) * 0.02;
    final_color.rgb += pulse;
    
    // Vignette at edge of disk
    let vignette = smoothstep(0.95, 1.0, r);
    final_color.rgb *= 1.0 - vignette * 0.5;
    
    return final_color;
}