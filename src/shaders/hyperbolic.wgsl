// Minimal test shader without uniforms

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

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(0.1, 0.2, 0.3, 1.0);
}
