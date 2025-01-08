struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) bg: vec4<f32>,
    @location(3) fg: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) bg: vec4<f32>,
    @location(2) fg: vec4<f32>,
};

@vertex
fn vs_main(
    input: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4(input.position, 0.0, 1.0);
    out.tex_coords = input.tex_coords;
    out.bg = input.bg;
    out.fg = input.fg;
    return out;
}

@group(0) @binding(0)
var tex: texture_2d<f32>;

@group(0) @binding(1)
var samplerr: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texture_color = textureSample(tex, samplerr, in.tex_coords).r;

    return in.fg * vec4(1.0, 1.0, 1.0, texture_color);
}

