use gnahc::pty::PTY;
use gnahc::ViewPort;
use gnahc_vte::VTEParser;
use image::{ImageBuffer, Rgba, RgbaImage};
use learn_rendering::renderer::{Render, Renderer};
use rusttype::Scale;
use std::io::Read;
use std::time::Instant;

fn main() {
    let hb_font = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(
        *include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"),
        0,
    )
    .unwrap();

    let rt_font = rusttype::Font::try_from_bytes(include_bytes!(
        "/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"
    ))
    .unwrap();
    let scale = Scale::uniform(32.0);
    let max_x = 1280;
    let max_y = 960;
    let mut renderer = Renderer::new(scale, 0, 0, max_x, max_y);
    let line_height = scale.y.round() as u32;
    let text_width = (scale.x / 2.0).round() as u32;
    let max_col = max_x / text_width;
    let max_row = max_y / line_height;

    let mut pty = PTY::new(
        0,
        ViewPort {
            x: max_row as u16,
            y: max_col as u16,
            cx: text_width as u16,
            cy: line_height as u16,
        },
    )
    .unwrap();

    let mut parser = VTEParser::new();
    let mut buf = vec![0; 2048];
    let mut curr = 0;
    let background_color = Rgba([0, 0, 0, 255]); // Black background
    let mut image: RgbaImage = ImageBuffer::from_fn(max_x, max_y, |_, _| background_color);

    loop {
        match pty.io().read(&mut buf[curr..]) {
            Ok(n) => {
                if n == 0 {
                    break;
                } else {
                    curr += n;
                    if curr > 300 {
                        break;
                    }
                }
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                _ => break,
            },
        }
    }

    parser.parse(&buf[..curr], &mut renderer);
    let current = Instant::now();
    renderer.render_all(&hb_font, &rt_font, |x, y, v, color| {
        let pixel = image.get_pixel_mut(x as u32, y as u32);
        let fg = Rgba([color.r, color.g, color.b, (v * color.a as f32) as u8]);
        *pixel = blend_colors(*pixel, fg, v);
    });
    println!("render time: {}.ns", current.elapsed().as_nanos());

    image.save("output.png").expect("could not write image");
}

fn blend_colors(bg: Rgba<u8>, fg: Rgba<u8>, intensity: f32) -> Rgba<u8> {
    let alpha = intensity; // Use glyph intensity as alpha
    let inv_alpha = 1.0 - alpha;

    Rgba([
        (fg[0] as f32 * alpha + bg[0] as f32 * inv_alpha) as u8,
        (fg[1] as f32 * alpha + bg[1] as f32 * inv_alpha) as u8,
        (fg[2] as f32 * alpha + bg[2] as f32 * inv_alpha) as u8,
        255,
    ])
}
