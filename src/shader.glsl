#version 140

in vec2 position;
in vec2 tex_coords;
in vec4 colour;

out vec2 v_tex_coords;
out vec4 v_colour;

void vs_main() {
    gl_Position = vec4(position, 0.0, 1.0);
    v_tex_coords = tex_coords;
    v_colour = colour;
}

uniform sampler2D tex;
in vec2 v_tex_coords;
in vec4 v_colour;
out vec4 f_colour;

void fs_main() {
    f_colour = v_colour * vec4(1.0, 1.0, 1.0, texture(tex, v_tex_coords).r);
}
