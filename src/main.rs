use harfbuzz_rs::Face;
use image::{ImageBuffer, Rgba, RgbaImage};
use learn_rendering::display::Display;
use rusttype::Scale;
use std::io::Read;
use std::time::Instant;
use term::data::{Color, ANSI_256, RGBA};
use term::pty::PTY;
use term::ViewPort;
use tracing::Level;
use vte::VTEParser;

fn hex_to_color(hex: &str) -> Result<RGBA, String> {
    if !hex.starts_with('#') || (hex.len() != 7 && hex.len() != 9) {
        return Err("Invalid hex string format".to_string());
    }

    let r = u8::from_str_radix(&hex[1..3], 16).map_err(|_| "Invalid red value")?;
    let g = u8::from_str_radix(&hex[3..5], 16).map_err(|_| "Invalid green value")?;
    let b = u8::from_str_radix(&hex[5..7], 16).map_err(|_| "Invalid blue value")?;
    let a = if hex.len() == 9 {
        u8::from_str_radix(&hex[7..9], 16).map_err(|_| "Invalid alpha value")?
    } else {
        255 // Default alpha to fully opaque if not provided
    };

    Ok(RGBA { r, g, b, a })
}

// fn main() {
//     tracing_subscriber::fmt()
//         .with_level(true)
//         .with_max_level(Level::TRACE)
//         .with_ansi(true)
//         .init();
//     let face = Face::from_bytes(
//         include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"),
//         0,
//     );
//     let rt_font: rusttype::Font =
//         rusttype::Font::try_from_bytes_and_index(&face.face_data(), face.index()).unwrap();
//     let hb_font = harfbuzz_rs::Font::new(face);
//
//     let colorscheme: [Color; 16] = [
//         hex_to_color("#000000").unwrap(),
//         hex_to_color("#dc143c").unwrap(),
//         hex_to_color("#32cd32").unwrap(),
//         hex_to_color("#ffd700").unwrap(),
//         hex_to_color("#0072bb").unwrap(),
//         hex_to_color("#c71585").unwrap(),
//         hex_to_color("#0072bb").unwrap(),
//         hex_to_color("#dadada").unwrap(),
//         hex_to_color("#808080").unwrap(),
//         hex_to_color("#dc143c").unwrap(),
//         hex_to_color("#32cd32").unwrap(),
//         hex_to_color("#ffd700").unwrap(),
//         hex_to_color("#0072bb").unwrap(),
//         hex_to_color("#c71585").unwrap(),
//         hex_to_color("#0072bb").unwrap(),
//         hex_to_color("#f5f5f5").unwrap(),
//     ];
//
//     let scale = Scale::uniform(32.0);
//     let line_height = scale.y.round() as u32;
//     let text_width = (scale.x / 2.0).round() as u32;
//
//     let mut display = Display::new(text_width, line_height, &colorscheme);
//
//     let event_loop = EventLoop::new().unwrap();
//
//     let _ = event_loop.run_app(&mut display);
// }

fn main() {
    tracing_subscriber::fmt()
        .with_level(true)
        .with_max_level(Level::TRACE)
        .with_ansi(true)
        .init();
    let face = Face::from_bytes(
        include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"),
        0,
    );
    let rt_font: rusttype::Font =
        rusttype::Font::try_from_vec_and_index(face.face_data().as_ref().to_owned(), face.index())
            .unwrap();
    let hb_font = harfbuzz_rs::Font::new(face);

    let colorscheme: [RGBA; 16] = [
        hex_to_color("#000000").unwrap(),
        hex_to_color("#dc143c").unwrap(),
        hex_to_color("#32cd32").unwrap(),
        hex_to_color("#ffd700").unwrap(),
        hex_to_color("#0072bb").unwrap(),
        hex_to_color("#c71585").unwrap(),
        hex_to_color("#0072bb").unwrap(),
        hex_to_color("#dadada").unwrap(),
        hex_to_color("#808080").unwrap(),
        hex_to_color("#dc143c").unwrap(),
        hex_to_color("#32cd32").unwrap(),
        hex_to_color("#ffd700").unwrap(),
        hex_to_color("#0072bb").unwrap(),
        hex_to_color("#c71585").unwrap(),
        hex_to_color("#0072bb").unwrap(),
        hex_to_color("#f5f5f5").unwrap(),
    ];

    let scale = Scale::uniform(32.0);
    let max_x = 1280;
    let max_y = 960;
    let mut renderer = Display::new(max_x, max_y, &hb_font, &rt_font, scale, &colorscheme);
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
                    if curr > 128 {
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
    renderer.render(|x, y, v, color| {
        let pixel = image.get_pixel_mut(x as u32, y as u32);
        let color = match color {
            Color::Rgba(rgba) => rgba,
            Color::IndexBase(index) => colorscheme[index],
            Color::Index256(index) => ANSI_256[index],
        };
        let fg = Rgba([color.r, color.g, color.b, (v * color.a as f32) as u8]);
        *pixel = blend_colors(*pixel, fg, v);
    });
    println!("render time: {}.ms", current.elapsed().as_millis());

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
