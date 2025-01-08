use harfbuzz_rs::{shape, Feature, Font, Tag, UnicodeBuffer};
use rusttype::gpu_cache::Cache;
use rusttype::{point, vector, Font as RTFont, GlyphId, Rect, Scale};
use term::data::{Attribute, Column, Line, RGBA};

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub bg: [f32; 4],
    pub fg: [f32; 4],
}

impl GlyphVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4];
    pub const fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct TextGenerator {
    bold_hb: harfbuzz_rs::Owned<Font<'static>>,
    italic_hb: harfbuzz_rs::Owned<Font<'static>>,
    regular_hb: harfbuzz_rs::Owned<Font<'static>>,
    cache: Cache<'static>,
    scale: Scale,

    bold_rt: RTFont<'static>,
    italic_rt: RTFont<'static>,
    regular_rt: RTFont<'static>,
}

impl TextGenerator {
    /// Load font
    /// TODO: change this to new implementation to load font
    pub fn new(width: u32, height: u32, scale: Scale) -> Self {
        let regular = include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-Regular.ttf");
        let bold = include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-Bold.ttf");
        let italic = include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-Italic.ttf");

        let regular_rt = RTFont::try_from_bytes(regular).unwrap();
        let regular_hb = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(*regular, 0).unwrap();
        let bold_rt = RTFont::try_from_bytes(bold).unwrap();
        let bold_hb = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(*bold, 0).unwrap();
        let italic_rt = RTFont::try_from_bytes(italic).unwrap();
        let italic_hb = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(*italic, 0).unwrap();

        Self {
            bold_hb,
            italic_hb,
            regular_hb,
            bold_rt,
            italic_rt,
            regular_rt,
            cache: Cache::builder()
                .multithread(true)
                .dimensions(width, height)
                .build(),
            scale,
        }
    }

    /// Generate bitmap representation for the data
    ///
    /// * `text`: String data
    /// * `attribute`: Attribute
    /// * `cell_witdh`: Cell witdh
    /// * `text_height`: Text_height
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &self,
        max_x: u32,
        max_y: u32,
        text: impl AsRef<str>,
        attribute: Attribute,
        fg: RGBA,
        bg: RGBA,
        cell_witdh: u32,
        cell_height: u32,
        line: Line,
        col: Column,
    ) -> Vec<GlyphVertex> {
        match attribute {
            Attribute::Bold => self.load_internal(
                max_x,
                max_y,
                &self.bold_hb,
                &self.bold_rt,
                text,
                fg,
                bg,
                cell_witdh,
                cell_height,
                line,
                col,
            ),
            _ => self.load_internal(
                max_x,
                max_y,
                &self.regular_hb,
                &self.regular_rt,
                text,
                fg,
                bg,
                cell_witdh,
                cell_height,
                line,
                col,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn load_internal(
        &self,
        max_x: u32,
        max_y: u32,
        hb: &harfbuzz_rs::Owned<Font<'static>>,
        rt: &RTFont<'static>,
        text: impl AsRef<str>,
        fg: RGBA,
        bg: RGBA,
        cell_witdh: u32,
        cell_height: u32,
        line: Line,
        col: Column,
    ) -> Vec<GlyphVertex> {
        let text = text.as_ref();

        let mut res = Vec::with_capacity(text.len());
        let buf = shape(
            hb,
            UnicodeBuffer::new()
                .add_str(text)
                .guess_segment_properties(),
            &[
                Feature::new(Tag::new('l', 'i', 'g', 'a'), 1, 0..),
                Feature::new(Tag::new('c', 'a', 'l', 't'), 1, 0..),
            ],
        );

        let position = buf.get_glyph_positions();
        let info = buf.get_glyph_infos();
        let mut start_x = col.0 as f32 * cell_witdh as f32;
        let start_y = line.0 as f32 * cell_witdh as f32;

        let mut iter = position.iter().zip(info).peekable();

        while let Some((position, info)) = iter.next() {
            let scale_factor = match iter.peek() {
                Some((_, next_info)) => next_info.cluster - info.cluster,
                None => 1,
            };
            let glyph_id = GlyphId(info.codepoint as u16);
            let scale_factor = match scale_factor > 1 {
                true => 1.0 / (1.0 + scale_factor as f32 * 0.1),
                false => 1.0,
            };
            let scale = Scale {
                x: self.scale.x * scale_factor,
                y: self.scale.y * scale_factor,
            };

            let x_offset = position.x_offset as f32 / 64.0;
            let y_offset = position.y_offset as f32 / 64.0;
            let x = start_x + x_offset;
            let y = y_offset + start_y;

            let glyph = rt.glyph(glyph_id).scaled(scale).positioned(point(x, y));

            let origin = point(0.0, 0.0);

            let screen_rect = Rect {
                min: rusttype::Point {
                    x: start_x / max_x as f32,
                    y: start_y / max_y as f32,
                },
                max: rusttype::Point {
                    x: (start_x + cell_witdh as f32) / max_x as f32,
                    y: (start_y + cell_height as f32) / max_y as f32,
                },
            };

            let gl_rect = Rect {
                min: origin
                    + (vector(
                        screen_rect.min.x / max_x as f32 - 0.5,
                        1.0 - screen_rect.min.y / max_y as f32 - 0.5,
                    )) * 2.0,
                max: origin
                    + (vector(
                        screen_rect.max.x / max_x as f32 - 0.5,
                        1.0 - screen_rect.max.y / max_y as f32 - 0.5,
                    )) * 2.0,
            };

            let uv_rect = glyph
                .pixel_bounding_box()
                .map(|val| Rect {
                    min: origin
                        + (vector(
                            val.min.x as f32 / max_x as f32 - 0.5,
                            1.0 - val.min.y as f32 / max_y as f32 - 0.5,
                        )) * 2.0,
                    max: origin
                        + (vector(
                            val.max.x as f32 / max_x as f32 - 0.5,
                            1.0 - val.max.y as f32 / max_y as f32 - 0.5,
                        )) * 2.0,
                })
                .unwrap_or(screen_rect);

            let screen_rect = gl_rect;
            let bg = [
                bg.r as f32 / 255.0,
                bg.g as f32 / 255.0,
                bg.b as f32 / 255.0,
                bg.a as f32 / 255.0,
            ];

            let fg = [
                fg.r as f32 / 255.0,
                fg.g as f32 / 255.0,
                fg.b as f32 / 255.0,
                fg.a as f32 / 255.0,
            ];
            res.extend(vec![
                GlyphVertex {
                    position: [screen_rect.min.x, screen_rect.min.y],
                    tex_coords: [uv_rect.min.x as f32, uv_rect.max.y as f32],
                    bg,
                    fg,
                },
                GlyphVertex {
                    position: [screen_rect.min.x, screen_rect.min.y],
                    tex_coords: [uv_rect.min.x as f32, uv_rect.min.y as f32],
                    bg,
                    fg,
                },
                GlyphVertex {
                    position: [screen_rect.max.x, screen_rect.min.y],
                    tex_coords: [uv_rect.max.x as f32, uv_rect.min.y as f32],
                    bg,
                    fg,
                },
                GlyphVertex {
                    position: [screen_rect.max.x, screen_rect.min.y],
                    tex_coords: [uv_rect.max.x as f32, uv_rect.min.y as f32],
                    bg,
                    fg,
                },
                GlyphVertex {
                    position: [screen_rect.max.x, screen_rect.max.y],
                    tex_coords: [uv_rect.max.x as f32, uv_rect.max.y as f32],
                    bg,
                    fg,
                },
                GlyphVertex {
                    position: [screen_rect.min.x, screen_rect.max.y],
                    tex_coords: [uv_rect.min.x as f32, uv_rect.max.y as f32],
                    bg,
                    fg,
                },
            ]);

            start_x += cell_witdh as f32;
        }

        res
    }
}
