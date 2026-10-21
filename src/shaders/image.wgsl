// Image wallpaper shader

@group(0) @binding(0) var wallpaper_texture: texture_2d<f32>;
@group(0) @binding(1) var wallpaper_sampler: sampler;

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

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    // UV coordinates (0,0) = top-left
    let uv = frag_coord.xy / vec2<f32>(textureDimensions(wallpaper_texture));
    
    // Sample texture
    let color = textureSample(wallpaper_texture, wallpaper_sampler, uv);
    return color;
}