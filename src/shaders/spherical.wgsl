// Spherical wallpaper shader — positive curvature (elliptic geometry)
// Renders a closed spherical tiling that wraps around, unlike the open
// hyperbolic disk of the negative-curvature module.

struct Uniforms {
    resolution: vec2<f32>,
    camera: vec2<f32>,
    zoom: f32,
    time: f32,
    pan_angle: f32,
    pan_rate: f32,
    radius: f32,
    palette_colors: array<vec4<f32>, 32>,
    palette_size: u32,
    mode: u32,
    _padding: u32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

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

// Great-circle distance between two points on a unit sphere given as (lat, lon)
fn spherical_distance(a: vec2<f32>, b: vec2<f32>) -> f32 {
    let lat1 = a.x;
    let lon1 = a.y;
    let lat2 = b.x;
    let lon2 = b.y;
    let sin_lat1 = sin(lat1);
    let cos_lat1 = cos(lat1);
    let sin_lat2 = sin(lat2);
    let cos_lat2 = cos(lat2);
    let cos_dlon = cos(lon1 - lon2);
    let arg = sin_lat1 * sin_lat2 + cos_lat1 * cos_lat2 * cos_dlon;
    let clamped = clamp(arg, -1.0, 1.0);
    return acos(clamped);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / uniforms.resolution;
    let aspect = uniforms.resolution.x / uniforms.resolution.y;

    // Map normalized device coordinates to the orthographic view of a sphere.
    let p = (uv - 0.5) * 2.0 * vec2<f32>(aspect, 1.0);
    p += uniforms.camera * 0.15;

    let r2 = dot(p, p);
    if r2 >= 1.0 {
        // Outside the visible disk: background colour
        return vec4<f32>(0.015, 0.015, 0.035, 1.0);
    }

    // Orthographic projection -> spherical coordinates (latitude, longitude)
    let r = sqrt(r2);
    let lat = asin(clamp(p.y, -1.0, 1.0));
    var lon = atan2(p.x, p.y) + uniforms.pan_angle * 0.5;
    // Wrap longitude to [-pi, pi]
    let PI: f32 = 3.14159265359;
    lon = lon - 2.0 * PI * floor((lon + PI) / (2.0 * PI));

    let pos = vec2<f32>(lat, lon);

    // Spherical tiling: centres placed on the sphere, coloured by nearest centre.
    // Positive curvature makes cells converge toward the poles.
    const LAT_BANDS: i32 = 6;
    const LON_BANDS: i32 = 10;

    var min_dist = 100.0;
    var best_index: u32 = 0u;
    var best_ring: i32 = 0;

    for (var i: i32 = 0; i < LAT_BANDS; i++) {
        let lat_c = f32(i) * 3.14159265359 / f32(LAT_BANDS - 1) - 3.14159265359 / 2.0;
        for (var j: i32 = 0; j < LON_BANDS; j++) {
            let lon_c = f32(j) * 2.0 * 3.14159265359 / f32(LON_BANDS) + uniforms.pan_angle * 0.25;
            let centre = vec2<f32>(lat_c, lon_c);
            let d = spherical_distance(pos, centre);
            if d < min_dist {
                min_dist = d;
                best_index = u32((i * LON_BANDS + j) % 32);
                best_ring = i;
            }
        }
    }

    let size = min(uniforms.palette_size, 32u);
    let base_color = uniforms.palette_colors[best_index % size];

    // Cell edges
    let cell_edge = smoothstep(0.12, 0.15, min_dist);
    let cell_center = 1.0 - smoothstep(0.0, 0.04, min_dist);

    let lit = mix(base_color, vec4<f32>(1.0, 1.0, 1.0, 1.0) * base_color, cell_edge * 0.35);
    let mut color = mix(vec4<f32>(0.0), lit, cell_center);

    // Subtle animation pulse
    let pulse = sin(uniforms.time * 0.6 + min_dist * 25.0) * 0.02;
    color.rgb += pulse;

    // Vignette toward the sphere limb
    let vignette = smoothstep(0.85, 1.0, r);
    color.rgb *= 1.0 - vignette * 0.4;

    return color;
}
