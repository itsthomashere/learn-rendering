struct VertexInput {
    @location(0) position: vec2<f32>,  // Vertex position in pixels
    @location(1) tex_coords: vec2<f32>, // Texture coordinates [0, 1]
    @location(2) bg_color: vec4<f32>,   // Background color (RGBA)
    @location(3) fg_color: vec4<f32>,   // Foreground color (RGBA)
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>, // Output position for clipping
    @location(0) tex_coords: vec2<f32>,         // Passed to fragment shader
    @location(1) bg_color: vec4<f32>,           // Background color
    @location(2) fg_color: vec4<f32>,           // Foreground color
};


@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 0.0, 1.0);
    out.tex_coords = model.tex_coords;
    out.bg_color = model.bg_color;
    out.fg_color = model.fg_color;
    return out;
}


struct FragmentInput {
    @location(0) tex_coords: vec2<f32>, // Texture coordinates
    @location(1) bg_color: vec4<f32>,   // Background color (RGBA)
    @location(2) fg_color: vec4<f32>,   // Foreground color (RGBA)
};


@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {

    return input.fg_color;
}

