#version 140

in vec2 position;     // position: 2D vector (x, y)
in vec2 tex_coords;   // tex_coords: 2D texture coordinates (u, v)
in vec4 bg;           // bg: background color (RGBA)
in vec4 fg;           // fg: foreground color (RGBA)

out vec2 v_tex_coords; // output texture coordinates to the fragment shader
out vec4 v_fg;         // output foreground color to the fragment shader

void main() {
    // Set the position of the vertex, in clip space
    gl_Position = vec4(position, 0.0, 1.0);

    // Pass texture coordinates to the fragment shader
    v_tex_coords = tex_coords;

    // Pass foreground color to the fragment shader
    v_fg = fg;
}

