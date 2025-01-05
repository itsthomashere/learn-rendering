use harfbuzz_rs::{Feature, Font as HbFont, Tag, UnicodeBuffer};
use rusttype::{point, Font as RtFont, GlyphId, Scale};
use term::data::grids::Grid;
use term::data::Attribute;
use term::data::Cell;
use term::data::Color;
use term::data::RGBA;
use vte::ansi::Audible;
use vte::ansi::ControlFunction;
use vte::ansi::Editing;
use vte::ansi::GraphicCharset;
use vte::ansi::Management;
use vte::ansi::Synchronization;
use vte::ansi::TextProc;
use vte::ansi::Visual;
use vte::Handler;
use winit::dpi::PhysicalSize;

impl Handler for Renderer {
    fn print(&mut self, consume: vte::VtConsume) {
        let control: ControlFunction = consume.into();
        match control {
            ControlFunction::Print(c) => {
                self.buf.push(Cell {
                    c,
                    fg: self.fg,
                    bg: self.bg,
                    attr: Attribute::default(),
                    sixel_data: None,
                    dirty: true,
                    erasable: true,
                });
            }
            _ => unreachable!(),
        }
    }

    fn execute(&mut self, consume: vte::VtConsume) {
        let control: ControlFunction = consume.into();
        self.execute_control(control);
    }

    fn esc_dispatch(&mut self, consume: vte::VtConsume) {
        let control: ControlFunction = consume.into();
        // println!("esc dispatch {:?}", control);
        self.execute_control(control);
    }

    fn csi_dispatch(&mut self, consume: vte::VtConsume) {
        let control: ControlFunction = consume.into();
        // println!("csi dispatch {:?}", control);
        match control {
            ControlFunction::StringTerminator => {
                self.buffer.input(std::mem::take(&mut self.buf), |_| true);
                self.buffer.cursor_mut().y += 1;
                self.buffer.cursor_mut().x = 0;
            }
            ControlFunction::TextProc(TextProc::LineFeed) => {
                self.buffer.input(std::mem::take(&mut self.buf), |_| true);
                self.buffer.cursor_mut().y += 1;
                self.buffer.cursor_mut().x = 0;
            }
            ControlFunction::TextProc(TextProc::CarriageReturn) => {
                self.buffer.cursor_mut().x = 0;
            }
            ControlFunction::Visual(v) => match v {
                Visual::DarkMode(d) => self.dark_mode = d,
                Visual::GraphicRendition(vec) => self.rendition(vec),
                _ => {}
            },
            ControlFunction::Editing(e) => match e {
                Editing::DeleteCharacter(_) => {}
                Editing::DeleteCol(_) => {}
                Editing::DeleteLine(_) => {}
                Editing::EraseInDisplay(flag) => match flag {
                    0 => {
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        // Clear the current line first
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut().skip(x).for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                        for row in self.buffer.visible_iter_mut().skip(y) {
                            row.iter_mut().for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                    }
                    1 => {
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        // Clear the current line first
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut().skip(x).for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                        for row in self.buffer.visible_iter_mut().take(y - 1) {
                            row.iter_mut().for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                    }
                    2 => {
                        for row in self.buffer.visible_iter_mut() {
                            row.iter_mut().for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                    }
                    _ => {}
                },
                Editing::SelectiveEraseDisplay(flag) => match flag {
                    0 => {
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        // Clear the current line first
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut()
                                .skip(x)
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                        for row in self.buffer.visible_iter_mut().skip(y) {
                            row.iter_mut()
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                    }
                    1 => {
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        // Clear the current line first
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut()
                                .skip(x)
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                        for row in self.buffer.visible_iter_mut().take(y - 1) {
                            row.iter_mut()
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                    }
                    2 => {
                        for row in self.buffer.visible_iter_mut() {
                            row.iter_mut()
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                    }
                    _ => {}
                },
                Editing::EraseInLine(flag) => match flag {
                    0 => {
                        // Erase from the begining up until the cursor
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        self.reset_graphic();
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut().skip(x).for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                    }
                    1 => {
                        // clear from the beginning to the cursor
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut().take(x).for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            });
                        }
                    }
                    2 => {
                        let y = self.buffer.cursor().y;
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut().for_each(|cell| {
                                cell.c = ' ';
                                cell.dirty = true;
                                cell.bg = Color::IndexBase(0);
                                cell.fg = Color::IndexBase(7);
                                cell.attr = Attribute::default();
                            })
                        }
                    }
                    _ => {}
                },
                Editing::SelectiveEraseLine(flag) => match flag {
                    0 => {
                        // Erase from the begining up until the cursor
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        self.reset_graphic();
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut()
                                .skip(x)
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                    }
                    1 => {
                        // clear from the beginning to the cursor
                        let x = self.buffer.cursor().x;
                        let y = self.buffer.cursor().y;
                        self.reset_graphic();
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut()
                                .take(x)
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                });
                        }
                    }
                    2 => {
                        let y = self.buffer.cursor().y;
                        self.reset_graphic();
                        if let Some(row) = self.buffer.line_mut(y) {
                            row.iter_mut()
                                .take_while(|cell| cell.erasable)
                                .for_each(|cell| {
                                    cell.c = ' ';
                                    cell.dirty = true;
                                    cell.bg = Color::IndexBase(0);
                                    cell.fg = Color::IndexBase(7);
                                    cell.attr = Attribute::default();
                                })
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            ControlFunction::TextProc(t) => match t {
                TextProc::SaveCursor | TextProc::SaveCursorPosition => {
                    self.buffer.save_cursor();
                }
                TextProc::RestoreCursor | TextProc::RestoreSavedCursor => {
                    self.buffer.restore_cursor();
                }
                _ => {}
            },
            _ => {}

            c => {}
        }
    }

    fn hook(&mut self, consume: vte::VtConsume) {
        println!("dsc hook {:?}", consume);
    }

    fn put(&mut self, consume: vte::VtConsume) {
        println!("dscput {:?}", consume);
    }

    fn unhook(&mut self) {
        println!("unhook");
    }

    fn osc_dispatch(&mut self, consume: vte::VtConsume) {
        println!("osc dispatch {:?}", consume);
    }
}

#[derive(Debug)]
pub struct Terminal<'config> {
    max_col: usize,
    max_row: usize,
    fg: Color,
    bg: Color,
    text_width: u32,
    line_height: u32,
    attr: Attribute,

    buffer: Grid<Cell>,
    write_stack: Vec<Cell>,
    colorscheme: &'config [RGBA; 16],
}

impl<'config> Terminal<'config> {
    pub fn new(
        size: PhysicalSize<u32>,
        text_width: u32,
        line_height: u32,
        colorscheme: &'config [RGBA; 16],
    ) -> Self {
        let max_col = (size.width / text_width) as usize;
        let max_row = (size.height / line_height) as usize;
        Self {
            max_col,
            max_row,
            fg: Color::IndexBase(7),
            bg: Color::IndexBase(0),
            text_width,
            line_height,
            attr: Attribute::default(),
            buffer: Grid::new(max_row, max_col),
            write_stack: Vec::with_capacity(100),
            colorscheme,
        }
    }
}

pub struct Renderer {
    max_col: u32,
    max_row: u32,
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    fg: Color,
    bg: Color,
    text_width: u32,
    line_height: u32,
    scale: Scale,
    dark_mode: bool,

    pub buffer: Grid<Cell>,
    colorscheme: [RGBA; 16],
    buf: Vec<Cell>,
}

impl Renderer {
    pub fn new(
        scale: Scale,
        min_x: u32,
        min_y: u32,
        max_x: u32,
        max_y: u32,
        colorscheme: [RGBA; 16],
    ) -> Self {
        let line_height = scale.y.round() as u32;
        let text_width = (scale.x / 2.0).round() as u32;
        let max_col = (max_x - min_x) / text_width;
        let max_row = (max_y - min_y) / line_height;
        Self {
            max_col,
            max_row,
            min_x,
            min_y,
            max_x,
            max_y,
            text_width,
            line_height,
            scale,
            buffer: Grid::new(max_row as usize, max_col as usize),
            fg: Color::IndexBase(7),
            bg: Color::IndexBase(0),
            buf: Vec::with_capacity(50),
            colorscheme,
            dark_mode: true,
        }
    }

    pub fn append(&mut self, data: Vec<Cell>) {
        self.buffer.input(data, |_| true);
    }

    pub fn update(&mut self) {
        self.buffer.input(std::mem::take(&mut self.buf), |_| true);
    }

    pub fn resize(&mut self, row: usize, col: usize) {
        self.update();
        self.buffer.resize(row, col, |_| true);
    }

    fn set_attr(&mut self, flag: i64) {}

    fn reset_graphic(&mut self) {}

    fn rendition(&mut self, data: Vec<i64>) {
        if data.len() <= 2 {
            for i in data {
                match &i {
                    0..=27 => self.set_attr(i),
                    30..=37 => {
                        if self.dark_mode {
                            self.fg = Color::IndexBase((i - 30) as usize);
                        } else {
                            self.fg = Color::IndexBase((i - 30 + 8) as usize);
                        }
                    }
                    39 => self.fg = Color::IndexBase(7),
                    40..=47 => {
                        if self.dark_mode {
                            self.bg = Color::IndexBase((i - 30) as usize);
                        } else {
                            self.bg = Color::IndexBase((i - 30 + 8) as usize);
                        }
                    }
                    49 => self.bg = Color::IndexBase(0),
                    _ => {}
                }
            }
            return;
        }

        if data.len() > 2 {
            match data.as_slice() {
                [38, 5, index] => self.fg = Color::Index256(*index as usize),
                [48, 5, index] => self.bg = Color::Index256(*index as usize),
                [38, 2, rgb @ ..] => {
                    self.fg = Color::Rgba(RGBA {
                        r: rgb
                            .first()
                            .map_or_else(|| 0, |r| (*r).try_into().unwrap_or(0)),
                        g: rgb
                            .get(1)
                            .map_or_else(|| 0, |g| (*g).try_into().unwrap_or(0)),
                        b: rgb
                            .get(2)
                            .map_or_else(|| 0, |b| (*b).try_into().unwrap_or(0)),
                        a: rgb
                            .get(3)
                            .map_or_else(|| 255, |a| (*a).try_into().unwrap_or(255)),
                    });
                }
                [48, 2, rgb @ ..] => {
                    self.bg = Color::Rgba(RGBA {
                        r: rgb
                            .first()
                            .map_or_else(|| 0, |r| (*r).try_into().unwrap_or(0)),
                        g: rgb
                            .get(1)
                            .map_or_else(|| 0, |g| (*g).try_into().unwrap_or(0)),
                        b: rgb
                            .get(2)
                            .map_or_else(|| 0, |b| (*b).try_into().unwrap_or(0)),
                        a: rgb
                            .get(3)
                            .map_or_else(|| 255, |a| (*a).try_into().unwrap_or(255)),
                    });
                }
                _ => {}
            }
        }
    }

    fn execute_control(&mut self, control: ControlFunction) {
        match control {
            ControlFunction::Null => {}
            ControlFunction::Enquire => {}
            ControlFunction::Audible(Audible::Bell) => {}
            ControlFunction::TextProc(TextProc::Backspace) => {}
            ControlFunction::TextProc(TextProc::HTab) => {}
            ControlFunction::TextProc(TextProc::LineFeed) => {
                self.update();
                self.buffer.cursor_mut().x = 0;
                self.buffer.cursor_mut().y += 1;
            }
            ControlFunction::TextProc(TextProc::VTab) => {}
            ControlFunction::TextProc(TextProc::FormFeed) => {}
            ControlFunction::TextProc(TextProc::CarriageReturn) => {
                self.update();
                self.buffer.cursor_mut().x = 0;
            }
            ControlFunction::Graphic(GraphicCharset::LockingShift1) => {}
            ControlFunction::Graphic(GraphicCharset::LockingShift0) => {}
            ControlFunction::Synchronization(Synchronization::XON) => {}
            ControlFunction::Synchronization(Synchronization::XOFF) => {}
            ControlFunction::Cancel => {}
            ControlFunction::Substitute => {}
            ControlFunction::TextProc(TextProc::Index) => {}
            ControlFunction::TextProc(TextProc::NextLine) => {}
            ControlFunction::TextProc(TextProc::SetHTab) => {}
            ControlFunction::TextProc(TextProc::ReverseIndex) => {
                self.buffer.cursor_mut().y -= 1;
            }
            ControlFunction::Graphic(GraphicCharset::SingleShift2) => {}
            ControlFunction::Graphic(GraphicCharset::SingleShift3) => {}
            ControlFunction::StringTerminator => {}
            ControlFunction::TextProc(TextProc::BackIndex) => {}
            ControlFunction::TextProc(TextProc::SaveCursor) => {}
            ControlFunction::TextProc(TextProc::RestoreCursor) => {}
            ControlFunction::TextProc(TextProc::ForwardIndex) => {}
            ControlFunction::Management(Management::Reset) => {}
            ControlFunction::Visual(Visual::DoubleTop) => {}
            ControlFunction::Visual(Visual::DoubleBottom) => {}
            ControlFunction::Visual(Visual::SingleWidth) => {}
            ControlFunction::Visual(Visual::DoubleWidth) => {}
            ControlFunction::Illegal => {}
            _ => unreachable!(),
        }
    }
}

impl Render for Renderer {
    fn render_all<F>(
        &mut self,
        hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
        rt_font: &RtFont<'static>,
        mut f: F,
    ) where
        F: FnMut(i32, i32, f32, Color),
    {
        if !self.buf.is_empty() {
            self.buffer.input(std::mem::take(&mut self.buf), |_| true);
        }
        for (i, _) in self.buffer.visible_iter().enumerate() {
            self.render_line(i, hb_font, rt_font, &mut f);
        }
    }

    fn render_line<F>(
        &self,
        index: usize,
        hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
        rt_font: &RtFont<'static>,
        mut f: F,
    ) where
        F: FnMut(i32, i32, f32, Color),
    {
        let line = match self.buffer.visible_iter().nth(index) {
            Some(line) if line.is_empty() => return,
            Some(line) => line,
            None => return,
        };
        let mut data = Vec::with_capacity(line.len());
        let mut prev_color: Option<&Color> = None;
        let mut current = String::new();
        'outer: for cell in line.iter() {
            match prev_color {
                Some(color) => {
                    if color == &cell.fg {
                        current.push(cell.c);
                        continue 'outer;
                    } else {
                        data.push((std::mem::take(&mut current), prev_color.unwrap()));
                        current.push(cell.c);
                        prev_color = Some(&cell.fg)
                    }
                }
                None => {
                    prev_color = Some(&cell.fg);
                    current.push(cell.c);
                    continue;
                }
            }
        }
        data.push((current, prev_color.unwrap()));

        let start_y = (index + 1) as u32 * self.line_height;
        let mut curr_col = 0;
        for val in data {
            let data = val.0;
            let color = val.1;
            let buffer = UnicodeBuffer::new()
                .add_str(&data)
                .guess_segment_properties();

            let glyph_buffer = harfbuzz_rs::shape(
                hb_font,
                buffer,
                &[
                    Feature::new(Tag::new('l', 'i', 'g', 'a'), 1, 0..),
                    Feature::new(Tag::new('c', 'a', 'l', 't'), 1, 0..),
                ],
            );
            let positions = glyph_buffer.get_glyph_positions();
            let infos = glyph_buffer.get_glyph_infos();
            let mut iter = positions.iter().zip(infos).peekable();
            while let Some((position, info)) = iter.next() {
                let scale_factor = match iter.peek() {
                    Some((_, next_info)) => next_info.cluster - info.cluster,
                    None => 1,
                };
                let x_offset = position.x_offset as f32 / 64.0;
                let y_offset = position.y_offset as f32 / 64.0;
                let glyph_id = GlyphId(info.codepoint as u16);

                let x = (self.min_x + curr_col * self.text_width) as f32 + x_offset;
                let y = y_offset + (self.min_y + start_y) as f32;

                let scale_factor = match scale_factor > 1 {
                    true => 1.0 / (1.0 + scale_factor as f32 * 0.1),
                    false => 1.0,
                };
                let scale = Scale {
                    x: self.scale.x * scale_factor,
                    y: self.scale.y * scale_factor,
                };

                let glyph = rt_font
                    .glyph(glyph_id)
                    .scaled(scale)
                    .positioned(point(x, y));

                if let Some(round_box) = glyph.pixel_bounding_box() {
                    glyph.draw(|x, y, v| {
                        let x = x as i32 + round_box.min.x;
                        let y = y as i32 + round_box.min.y;

                        if x >= 0 && x < self.max_x as i32 && y >= 0 && y < self.max_y as i32 {
                            f(x, y, v, *color)
                        }
                    });
                }

                curr_col += 1;
            }
        }
    }
}

pub trait Render {
    fn render_all<F>(
        &mut self,
        hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
        rt_font: &RtFont<'static>,
        f: F,
    ) where
        F: FnMut(i32, i32, f32, Color);

    fn render_line<F>(
        &self,
        index: usize,
        hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
        rt_font: &RtFont<'static>,
        f: F,
    ) where
        F: FnMut(i32, i32, f32, Color);
}

// #[derive(Debug, Clone)]
// pub struct LineBuffer {
//     lines: Vec<TextLine>,
//     max_col: u32,
//     max_row: u32,
//     min_x: u32,
//     min_y: u32,
//     max_x: u32,
//     max_y: u32,
//     text_width: u32,
//     line_height: u32,
//     scale: Scale,
// }
//
// impl LineBuffer {
//     pub fn new(scale: Scale, min_x: u32, min_y: u32, max_x: u32, max_y: u32) -> Self {
//         let line_height = scale.y.round() as u32;
//         let text_width = (scale.y / 2.0).round() as u32;
//         let max_col = (max_x - min_x) / text_width;
//         let max_row = (max_y - min_y) / line_height;
//
//         Self {
//             scale,
//             lines: Vec::with_capacity(max_row as usize),
//             max_col,
//             max_row,
//             min_x,
//             min_y,
//             max_x,
//             max_y,
//             line_height,
//             text_width,
//         }
//     }
//
//     // TODO: Implement line wrap
//     pub fn append_text(&mut self, text: impl AsRef<str>, color: Color) {
//         let lines = text
//             .as_ref()
//             .split_inclusive(|ch: char| ch == '\n' || ch.is_whitespace())
//             .collect::<Vec<_>>();
//         for line in lines {
//             if line.contains('\n') {
//                 line.strip_suffix('\n').unwrap();
//                 self.lines.push(TextLine::new());
//             } else {
//                 self.append_last_line(line, color.clone());
//             }
//         }
//     }
//
//     fn append_last_line(&mut self, text: impl AsRef<str>, color: Color) {
//         let str_len = text.as_ref().len() as u32;
//         if self.can_fit(str_len) {
//             match self.lines.last_mut() {
//                 Some(last_line) => {
//                     last_line.append_text(text, color);
//                 }
//                 None => {
//                     let mut new_line = TextLine::new();
//                     new_line.append_text(text, color);
//                     self.lines.push(new_line);
//                 }
//             }
//             return;
//         }
//         let mut remainder = self.max_col - self.col_len();
//         match self.lines.last_mut() {
//             Some(last_line) => {
//                 // the remainder into the line
//                 last_line.append_text(&text.as_ref()[..remainder as usize], color.clone());
//             }
//             None => {
//                 let mut new_line = TextLine::new();
//                 new_line.append_text(&text.as_ref()[..remainder as usize], color.clone());
//                 self.lines.push(new_line);
//             }
//         }
//         // get the required lines to be push into this
//         let required_lines = (str_len - remainder) / self.max_col + 1;
//
//         for _ in 0..required_lines {
//             if remainder + self.max_col < str_len {
//                 let mut new_lines = TextLine::new();
//                 new_lines.append_text(
//                     &text.as_ref()[(remainder as usize)..((remainder + self.max_col) as usize)],
//                     color.clone(),
//                 );
//                 self.lines.push(new_lines);
//             } else {
//                 let mut new_lines = TextLine::new();
//                 new_lines.append_text(&text.as_ref()[(remainder as usize)..], color.clone());
//                 self.lines.push(new_lines);
//             }
//             remainder += self.max_col;
//         }
//     }
//
//     pub fn can_fit(&self, len: u32) -> bool {
//         self.max_col - self.col_len() >= len
//     }
//
//     /// Get the current len of the most recent line
//     pub fn col_len(&self) -> u32 {
//         match self.lines.last() {
//             Some(line) => line.len(),
//             None => 0,
//         }
//     }
//
//     pub fn render_all<F>(
//         &self,
//         hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
//         rt_font: &RtFont<'static>,
//         mut f: F,
//     ) where
//         F: FnMut(i32, i32, f32, Color),
//     {
//         if self.lines.len() <= self.max_row as usize {
//             for i in 0..self.lines.len() {
//                 self.render_line(i, hb_font, rt_font, &mut f);
//             }
//         } else {
//             for i in (self.max_row as usize - self.lines.len())..self.lines.len() {
//                 self.render_line(i, hb_font, rt_font, &mut f);
//             }
//         }
//     }
//
//     /// Insert text at given position
//     ///
//     /// * `line`: Line number
//     /// * `col`: column number
//     /// * `color`: [Color]
//     pub fn insert_at(&mut self, line: u32, col: u32, text: impl AsRef<str>, color: Color) {
//         let str_len = text.as_ref().len() as u32;
//         // If we need to insert at the line that doesnt exits yet, insert empty lines to fill in
//         if line >= self.lines.len() as u32 {
//             for _ in 0..(line - self.lines.len() as u32) {
//                 self.lines.push(TextLine::new());
//             }
//             // insert whitespace placeholder
//             self.append_last_line(" ".repeat(col as usize), color.clone());
//             self.append_text(text, color);
//             return;
//         }
//
//         let cur_line = self
//             .lines
//             .get_mut(line as usize)
//             .expect("len checked hence the index is valid");
//
//         // If the line can hold the text, we can push straight into the line
//         if self.max_col - cur_line.len() >= text.as_ref().len() as u32 {
//             cur_line.insert_at(col, text, color);
//             return;
//         }
//
//         let exceeded_text = cur_line.split_at(line);
//         let mut available = self.max_col - cur_line.len();
//         let exeeding = text.as_ref().len() as u32 - available;
//         let needed = (exeeding / self.max_col) + 1;
//         cur_line.insert_at(col, &text.as_ref()[..(available as usize)], color.clone());
//
//         // Insert new line in the middle
//         for i in 0..needed {
//             let mut new_line = TextLine::new();
//             if available + self.max_col < str_len {
//                 new_line.append_text(
//                     &text.as_ref()[(available as usize)..(available + self.max_col) as usize],
//                     color.clone(),
//                 )
//             } else {
//                 new_line.append_text(&text.as_ref()[(available as usize)..], color.clone())
//             }
//             self.lines.insert((line + i + 1) as usize, new_line);
//             available += self.max_col;
//         }
//         if let Some((text, color)) = exceeded_text {
//             self.insert_at(
//                 line + needed,
//                 self.lines[(line + needed + 1) as usize].len(),
//                 text,
//                 color,
//             )
//         }
//     }
//
//     /// Get an exclusive reference to the last line
//     pub fn last_mut(&mut self) -> Option<&mut TextLine> {
//         self.lines.last_mut()
//     }
//
//     pub fn render_line<F>(
//         &self,
//         index: usize,
//         hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
//         rt_font: &RtFont<'static>,
//         f: &mut F,
//     ) where
//         F: FnMut(i32, i32, f32, Color),
//     {
//         let line = match self.lines.get(index) {
//             Some(line) => line,
//             None => return,
//         };
//         let start_y = (index + 1) as u32 * self.line_height;
//         let mut curr_col = 0;
//         line.batches.iter().for_each(|batch| {
//             let text = &batch.text;
//             let color = &batch.color;
//             let buffer = UnicodeBuffer::new()
//                 .add_str(text)
//                 .guess_segment_properties();
//             let glyph_buffer = harfbuzz_rs::shape(
//                 hb_font,
//                 buffer,
//                 &[
//                     Feature::new(Tag::new('l', 'i', 'g', 'a'), 1, 0..),
//                     Feature::new(Tag::new('c', 'a', 'l', 't'), 1, 0..),
//                 ],
//             );
//             let positions = glyph_buffer.get_glyph_positions();
//             let infos = glyph_buffer.get_glyph_infos();
//             let mut iter = positions.iter().zip(infos).peekable();
//             while let Some((position, info)) = iter.next() {
//                 let scale_factor = match iter.peek() {
//                     Some((_, next_info)) => next_info.cluster - info.cluster,
//                     None => 1,
//                 };
//                 let x_offset = position.x_offset as f32 / 64.0;
//                 let y_offset = position.y_offset as f32 / 64.0;
//                 let glyph_id = GlyphId(info.codepoint as u16);
//
//                 let x = (self.min_x + curr_col * self.text_width) as f32 + x_offset;
//                 let y = y_offset + (self.min_y + start_y) as f32;
//
//                 let scale_factor = match scale_factor > 1 {
//                     true => 1.0 / (1.0 + scale_factor as f32 * 0.1),
//                     false => 1.0,
//                 };
//                 let scale = Scale {
//                     x: self.scale.x * scale_factor,
//                     y: self.scale.y * scale_factor,
//                 };
//
//                 let glyph = rt_font
//                     .glyph(glyph_id)
//                     .scaled(scale)
//                     .positioned(point(x, y));
//
//                 if let Some(round_box) = glyph.pixel_bounding_box() {
//                     glyph.draw(|x, y, v| {
//                         let x = x as i32 + round_box.min.x;
//                         let y = y as i32 + round_box.min.y;
//
//                         if x >= 0 && x < self.max_x as i32 && y >= 0 && y < self.max_y as i32 {
//                             f(x, y, v, color.clone())
//                         }
//                     });
//                 }
//
//                 curr_col += 1;
//             }
//         });
//     }
// }
//
// /// Text represented in a line
// ///
// /// * `batches`: [TextBatch]
// /// * `len`: The len of the text
// /// * `height`: line height
// /// * `text_width`: width of each glyhp
// /// * `max_col`: max number of column
// /// * `cur_col`: current column
// /// * `x`: starting point of x
// /// * `y`: starting point of y
// #[cfg(not(test))]
// #[derive(Debug, Clone)]
// pub struct TextLine {
//     batches: Vec<TextBatch>,
//     len: u32,
//     last_color: Option<Color>,
// }
//
// #[cfg(test)]
// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct TextLine {
//     batches: Vec<TextBatch>,
//     len: u32,
//     last_color: Option<Color>,
// }
//
// impl TextLine {
//     pub fn new() -> Self {
//         Self {
//             batches: Vec::with_capacity(256),
//             len: 0,
//             last_color: None,
//         }
//     }
//
//     pub fn insert_at(&mut self, index: u32, text: impl AsRef<str>, text_color: Color) {
//         let mut start = 0;
//         let mut idx = None;
//         for (i, val) in self.batches.iter().enumerate() {
//             start += val.len();
//             if start >= index {
//                 idx = Some(i);
//             }
//         }
//
//         if let Some(idx) = idx {
//             let batch = self.batches.get_mut(idx).expect("len checked");
//             let batch_color = batch.color.clone();
//             if batch_color == text_color {
//                 batch.text.insert_str(index as usize, text.as_ref());
//             } else {
//                 let (text_1, text_2) = batch.text.split_at(index as usize);
//                 let text_2 = text_2.to_string();
//                 *batch = TextBatch::new(text_1, batch_color.clone());
//
//                 self.batches
//                     .insert(idx + 1, TextBatch::new(text_2, batch_color));
//
//                 self.batches
//                     .insert(idx + 1, TextBatch::new(text.as_ref(), text_color))
//             }
//             self.len += text.as_ref().len() as u32;
//         }
//     }
//
//     pub fn split_at(&mut self, position: u32) -> Option<(String, Color)> {
//         let mut start = 0;
//         let mut idx = None;
//
//         for (i, val) in self.batches.iter().enumerate() {
//             start += val.len();
//             if start >= position {
//                 idx = Some(i);
//             }
//         }
//         println!("index: {idx:?}");
//         match idx {
//             Some(idx) => {
//                 let target = self.batches.get_mut(idx).expect("checked len");
//                 let str = target.text.split_at((start - position) as usize);
//                 let exceeded = str.1.to_string();
//                 println!("exceeded : {exceeded}");
//                 target.text = str.0.to_string();
//                 self.len -= exceeded.len() as u32;
//
//                 Some((exceeded, target.color.clone()))
//             }
//             None => None,
//         }
//     }
//
//     pub fn append_text(&mut self, text: impl AsRef<str>, text_color: Color) {
//         match self.last_color.as_ref() {
//             Some(color) if color == &text_color => {
//                 self.batches
//                     .last_mut()
//                     .expect("always available if last color is Some")
//                     .add_text(&text);
//                 self.len += text.as_ref().len() as u32;
//             }
//             _ => {
//                 self.batches.push(TextBatch::new(&text, text_color.clone()));
//                 self.last_color = Some(text_color);
//                 self.len += text.as_ref().len() as u32;
//             }
//         }
//     }
//
//     pub fn len(&self) -> u32 {
//         self.len
//     }
//
//     pub fn is_empty(&self) -> bool {
//         self.len == 0 && self.batches.is_empty()
//     }
// }
//
// impl Default for TextLine {
//     fn default() -> Self {
//         Self::new()
//     }
// }
//
// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct TextBatch {
//     text: String,
//     color: Color,
// }
//
// impl TextBatch {
//     pub fn new(text: impl AsRef<str>, color: Color) -> Self {
//         Self {
//             text: text.as_ref().into(),
//             color,
//         }
//     }
//
//     pub fn add_text(&mut self, text: impl AsRef<str>) {
//         self.text.push_str(text.as_ref());
//     }
//     pub fn len(&self) -> u32 {
//         self.text.len() as u32
//     }
//
//     pub fn is_empty(&self) -> bool {
//         self.text.is_empty()
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn text_line_buffer_append_same_color() {
//         let scale = Scale::uniform(32.0);
//         let max_x = 160;
//         let max_y = 160;
//         let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);
//
//         assert_eq!(line_buffer.max_col, 10);
//         assert_eq!(line_buffer.max_row, 5);
//
//         let color_1 = Color {
//             r: 255,
//             g: 255,
//             b: 255,
//             a: 255,
//         };
//         line_buffer.append_text("12345678901", color_1.clone());
//         line_buffer.append_text("2345678901", color_1.clone());
//
//         let inner = line_buffer.lines;
//
//         assert_eq!(
//             inner,
//             vec![
//                 TextLine {
//                     batches: vec![TextBatch {
//                         text: "1234567890".to_string(),
//                         color: color_1.clone()
//                     }],
//                     len: 10,
//                     last_color: Some(color_1.clone())
//                 },
//                 TextLine {
//                     batches: vec![TextBatch {
//                         text: "1234567890".to_string(),
//                         color: color_1.clone()
//                     }],
//                     len: 10,
//                     last_color: Some(color_1.clone())
//                 },
//                 TextLine {
//                     batches: vec![TextBatch {
//                         text: "1".to_string(),
//                         color: color_1.clone()
//                     }],
//                     len: 1,
//                     last_color: Some(color_1.clone())
//                 }
//             ]
//         )
//     }
//
//     #[test]
//     fn text_line_buffer_append_diff_color() {
//         let scale = Scale::uniform(32.0);
//         let max_x = 160;
//         let max_y = 160;
//         let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);
//
//         assert_eq!(line_buffer.max_col, 10);
//         assert_eq!(line_buffer.max_row, 5);
//
//         let color_1 = Color {
//             r: 255,
//             g: 255,
//             b: 255,
//             a: 255,
//         };
//         let color_2 = Color {
//             r: 0,
//             g: 0,
//             b: 0,
//             a: 255,
//         };
//         line_buffer.append_text("12345", color_1.clone());
//         line_buffer.append_text("12345", color_2.clone());
//         line_buffer.append_text("23456", color_2.clone());
//         line_buffer.append_text("23456", color_1.clone());
//         line_buffer.append_text("123", color_1.clone());
//
//         let inner = line_buffer.lines;
//
//         assert_eq!(
//             inner,
//             vec![
//                 TextLine {
//                     batches: vec![
//                         TextBatch {
//                             text: "12345".to_string(),
//                             color: color_1.clone()
//                         },
//                         TextBatch {
//                             text: "12345".to_string(),
//                             color: color_2.clone()
//                         }
//                     ],
//                     len: 10,
//                     last_color: Some(color_2.clone())
//                 },
//                 TextLine {
//                     batches: vec![
//                         TextBatch {
//                             text: "23456".to_string(),
//                             color: color_2.clone()
//                         },
//                         TextBatch {
//                             text: "23456".to_string(),
//                             color: color_1.clone()
//                         }
//                     ],
//                     len: 10,
//                     last_color: Some(color_1.clone())
//                 },
//                 TextLine {
//                     batches: vec![TextBatch {
//                         text: "123".to_string(),
//                         color: color_1.clone()
//                     }],
//                     len: 3,
//                     last_color: Some(color_1.clone())
//                 }
//             ]
//         )
//     }
//
//     #[test]
//     fn test_line_buffer_with_new_line() {
//         let scale = Scale::uniform(32.0);
//         let max_x = 160;
//         let max_y = 160;
//         let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);
//
//         assert_eq!(line_buffer.max_col, 10);
//         assert_eq!(line_buffer.max_row, 5);
//         let color_1 = Color {
//             r: 255,
//             g: 255,
//             b: 255,
//             a: 255,
//         };
//
//         let inner = line_buffer.lines;
//     }
// }
