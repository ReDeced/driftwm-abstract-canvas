// Hyperbolic {7,3} tiling wallpaper shader
// Renders a {7,3} hyperbolic tiling: 7 heptagons around each vertex
// Uses Poincaré disk model for hyperbolic geometry

struct Uniforms {
    resolution: vec2<f32>,
    camera: vec2<f32>,
    zoom: f32,
    time: f32,
    pan_angle: f32,
    pan_rate: f32,
    rmax: f32,
    pad: u32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// Hyperbolic color palette
fn palette_color(index: u32) -> vec4<f32> {
    if index == 0u {
        return vec4<f32>(0.05, 0.05, 0.15, 1.0);  // Deep background
    } else if index == 1u {
        return vec4<f32>(0.15, 0.10, 0.30, 1.0);  // Dark accent
    } else if index == 2u {
        return vec4<f32>(0.30, 0.20, 0.50, 1.0);  // Mid tone
    } else if index == 3u {
        return vec4<f32>(0.60, 0.40, 0.80, 1.0);  // Light accent
    } else if index == 4u {
        return vec4<f32>(0.90, 0.70, 1.00, 1.0);  // Highlight
    } else {
        return vec4<f32>(0.05, 0.05, 0.15, 1.0);
    }
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    if vertex_index == 0u {
        return vec4<f32>(-1.0, -1.0, 0.0, 1.0);
    } else if vertex_index == 1u {
        return vec4<f32>(1.0, -1.0, 0.0, 1.0);
    } else if vertex_index == 2u {
        return vec4<f32>(-1.0, 1.0, 0.0, 1.0);
    } else {
        return vec4<f32>(1.0, 1.0, 0.0, 1.0);
    }
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
    return acosh(clamp(arg, 1.0, 1000000.0));
}

// Möbius translation for hyperbolic movement
fn mobius_translate(z: vec2<f32>, t: vec2<f32>) -> vec2<f32> {
    let t_conj = vec2<f32>(t.x, -t.y);
    let num = z - t;
    let den = 1.0 - dot(z, t_conj);
    return num / den;
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / uniforms.resolution;
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    
    // Map to Poincaré disk coordinates
    var p = (uv - 0.5) * 2.0 * vec2<f32>(aspect, 1.0) / uniforms.zoom;
    p = p + uniforms.camera / uniforms.zoom * 0.1;
    
    // Check if inside unit disk
    let r2 = dot(p, p);
    if r2 >= 1.0 {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0); // Outside disk - background
    }
    
    let r = length(p);
    let theta = atan2(p.y, p.x);
    let rotated_theta = theta - uniforms.pan_angle;
    
    // Generate hyperbolic tiling centers using {7,3} pattern
    // For {7,3}: 7 heptagons meet at each vertex
    var min_dist = 100.0;
    var best_center = vec2<f32>(0.0, 0.0);
    var best_ring: i32 = 0;
    
    // Generate centers in hyperbolic rings
    for (var ring: i32 = 0; ring <= 6; ring++) {
        let ring_r = f32(ring) * 0.13;
        let count = max(1, ring * 7);
        
        for (var i: i32 = 0; i < count; i++) {
            let angle = f32(i) * 2.0 * 3.14159265359 / f32(count) + uniforms.pan_angle * 0.5;
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
    let cell_edge = smoothstep(0.14, 0.17, min_dist);
    let cell_center = 1.0 - smoothstep(0.0, 0.06, min_dist);
    
    // Alternate colors by ring and position
    let ring_color_idx = (best_ring + i32(rotated_theta * 7.0 / (2.0 * 3.14159265359))) % 5;
    let color = palette_color(u32(ring_color_idx));
    
    // Mix cell color with edge highlight, fading from center
    let base = mix(color, vec4<f32>(1.0, 1.0, 1.0, 1.0) * color, cell_edge * 0.3);
    let shaded = mix(vec4<f32>(0.0), base, cell_center);
    
    // Add subtle animation pulse and vignette at edge of disk
    // (no swizzle assignment: naga rejects `color.rgb = ...`)
    let pulse = sin(uniforms.time * 0.5 + min_dist * 20.0) * 0.02;
    let vignette = smoothstep(0.90, 1.0, r);
    let rgb = shaded.rgb * (1.0 - vignette * 0.5) + vec3<f32>(pulse, pulse, pulse);
    
    return vec4<f32>(rgb, shaded.a);
}
