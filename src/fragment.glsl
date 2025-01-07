#version 140

uniform sampler2D tex;           // texture sampler
in vec2 v_tex_coords;            // input texture coordinates from vertex shader
in vec4 v_fg;                    // input foreground color from vertex shader
out vec4 f_colour;               // output color

void main() {
    // Sample the texture at the given coordinates
    vec4 tex_color = texture(tex, v_tex_coords);

    // Multiply the foreground color with the texture color (modulating alpha channel)
    f_colour = v_fg * vec4(1.0, 1.0, 1.0, tex_color.r); // Use red channel of the texture for alpha blending
}

