// Hyperbolic {5,4} tiling wallpaper shader
// True hyperbolic pentagon tiling: centers are generated on the CPU by
// reflecting the central pentagon across its geodesic sides (BFS), then
// uploaded as a uniform array. Fragment shader does hyperbolic Voronoi
// over those centers: nearest center wins, borders drawn where the two
// nearest hyperbolic distances are close.

struct Uniforms {
    resolution: vec2<f32>,
    camera: vec2<f32>,
    zoom: f32,
    time: f32,
    pan_angle: f32,
    pan_rate: f32,
    rmax: f32,
    center_count: u32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// Per-cell data: xy = Poincare-disk center, z = palette color index
@group(0) @binding(1) var<uniform> centers: array<vec4<f32>, 256>;

fn palette_color(index: u32) -> vec4<f32> {
    if index == 0u {
        return vec4<f32>(0.06, 0.06, 0.16, 1.0);  // Deep background
    } else if index == 1u {
        return vec4<f32>(0.16, 0.11, 0.32, 1.0);  // Dark accent
    } else if index == 2u {
        return vec4<f32>(0.32, 0.21, 0.52, 1.0);  // Mid tone
    } else if index == 3u {
        return vec4<f32>(0.55, 0.38, 0.75, 1.0);  // Light accent
    } else {
        return vec4<f32>(0.75, 0.60, 0.95, 1.0);  // Highlight
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

// Hyperbolic distance in the Poincaré disk model
fn hyperbolic_distance(z1: vec2<f32>, z2: vec2<f32>) -> f32 {
    let d = length(z1 - z2);
    let n1 = 1.0 - dot(z1, z1);
    let n2 = 1.0 - dot(z2, z2);
    if n1 <= 0.0 || n2 <= 0.0 {
        return 100.0;
    }
    let arg = 1.0 + 2.0 * d * d / (n1 * n2);
    return acosh(clamp(arg, 1.0, 1000000.0));
}

// Möbius translation: hyperbolic isometry moving origin by t
fn mobius_translate(z: vec2<f32>, t: vec2<f32>) -> vec2<f32> {
    let t_conj = vec2<f32>(t.x, -t.y);
    let num = z + t;
    let den = 1.0 + dot(t, z);
    return num / den;
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / uniforms.resolution;
    let aspect = uniforms.resolution.x / uniforms.resolution.y;

    // Map to Poincaré disk, apply zoom
    var p = (uv - 0.5) * 2.0 * vec2<f32>(aspect, 1.0) / uniforms.zoom;

    // Hyperbolic camera pan (camera in canvas pixels -> small disk shift)
    let cam = -uniforms.camera * 0.00012;
    p = mobius_translate(p, cam);

    // Slow rotation of the tiling
    let ca = cos(uniforms.pan_angle);
    let sa = sin(uniforms.pan_angle);
    p = vec2<f32>(p.x * ca - p.y * sa, p.x * sa + p.y * ca);

    let r2 = dot(p, p);
    if r2 >= 1.0 {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0);
    }
    let r = length(p);

    // Hyperbolic Voronoi: find nearest and second-nearest cell centers
    var best = 100.0;
    var second = 100.0;
    var best_col = 0.0;
    let n = min(uniforms.center_count, 256u);
    for (var i: u32 = 0u; i < n; i++) {
        let c = centers[i];
        let d = hyperbolic_distance(p, c.xy);
        if d < best {
            second = best;
            best = d;
            best_col = c.z;
        } else if d < second {
            second = d;
        }
    }

    // Cell borders where the two nearest distances converge
    let edge = 1.0 - smoothstep(0.008, 0.045, second - best);
    let color = palette_color(u32(best_col) % 5u);

    // Light edge toward white, keep interior saturated
    let base = mix(color, vec4<f32>(0.95, 0.92, 1.0, 1.0), edge * 0.6);

    // Subtle breathing pulse tied to distance from cell center
    let pulse = sin(uniforms.time * 0.8 + best * 12.0) * 0.02;

    // Vignette toward the rim of the disk
    let vignette = smoothstep(0.88, 1.0, r);

    let rgb = base.rgb * (1.0 - vignette * 0.5) + vec3<f32>(pulse, pulse, pulse);
    return vec4<f32>(rgb, 1.0);
}
