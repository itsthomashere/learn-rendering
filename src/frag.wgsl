struct VertexOutput {
    @builtin(position) position: vec4<f32>, // Clip space position
    @location(0) tex_coords: vec2<f32>,     // Texture coordinates
    @location(1) bg: vec4<f32>,             // Background color
    @location(2) fg: vec4<f32>,             // Foreground color
};

@fragment
fn main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Blend foreground and background colors as a placeholder
    return mix(input.bg, input.fg, 0.5);
}
