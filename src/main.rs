use harfbuzz_rs::Face;
use image::{ImageBuffer, Rgba, RgbaImage};
use learn_rendering::display::Display;
use learn_rendering::renderer::Renderer;
use learn_rendering::App;
use rusttype::Scale;
use std::io::Read;
use std::time::Instant;
use term::data::{Color, Column, Line, PositionedCell, ANSI_256, RGBA};
use term::pty::PTY;
use term::ViewPort;
use tracing::Level;
use vte::VTEParser;
use winit::event_loop::EventLoop;

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
    // simple_logger::init().unwrap();
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
    let line_height = scale.y.round() as u32;
    let text_width = (scale.x / 2.0).round() as u32;
    let max_col = max_x / text_width;
    let max_row = max_y / line_height;

    let pty = PTY::new(
        0,
        ViewPort {
            x: max_row as u16,
            y: max_col as u16,
            cx: text_width as u16,
            cy: line_height as u16,
        },
    )
    .unwrap();

    let mut app = App::new(&colorscheme, scale, pty);

    let runner = EventLoop::new().unwrap();

    runner.run_app(&mut app).unwrap();
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
