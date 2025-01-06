use crate::Terminal;
use harfbuzz_rs::{Feature, Font, Tag, UnicodeBuffer};
use rusttype::{point, GlyphId, Scale};
use term::data::cursor::Cursor;
use term::data::{Color, Column, Line, RGBA};
use vte::ansi::{
    Audible, ControlFunction, Editing, GraphicCharset, Management, Synchronization, TextProc,
    Visual,
};
use vte::{Handler, VtConsume};

pub struct Display<'config> {
    cursor: Cursor,
    saved_cursor: Option<Cursor>,
    text_width: u32,
    line_height: u32,
    scale: Scale,

    hb_font: &'config harfbuzz_rs::Owned<Font<'static>>,
    rt_font: &'config rusttype::Font<'static>,

    term: Terminal<'config>,
}

impl<'config> Display<'config> {
    pub fn new(
        x: u32,
        y: u32,
        hb_font: &'config harfbuzz_rs::Owned<Font<'static>>,
        rt_font: &'config rusttype::Font<'static>,
        scale: Scale,
        colorscheme: &'config [RGBA; 16],
    ) -> Self {
        let line_height: u32 = scale.y.round() as u32;
        let text_width: u32 = (scale.x / 2.0).round() as u32;
        let max_row = x / text_width;
        let max_col = y / line_height;
        Self {
            cursor: Cursor::new(Line(0), Column(0)),
            saved_cursor: None,
            text_width,
            line_height,
            hb_font,
            rt_font,
            term: Terminal::new(max_row as usize, max_col as usize, colorscheme),
            scale,
        }
    }

    pub fn render<F>(&mut self, mut f: F)
    where
        F: FnMut(i32, i32, f32, Color),
    {
        if !self.term.write_stack.is_empty() {
            self.term.update(&mut self.cursor);
        }
        for (i, _) in self.term.data.iter_from(0).enumerate() {
            self.render_line(Line(i), &mut f);
        }
    }

    fn render_line<F>(&self, index: Line, mut f: F)
    where
        F: FnMut(i32, i32, f32, Color),
    {
        if self.term.data.len() < index.0 {
            return;
        }

        let line = &self.term.data[index];
        if line.len() == 0 {
            return;
        }

        let mut data = Vec::with_capacity(line.len());
        let mut prev_color: Option<&Color> = None;
        let mut current = String::new();
        'outer: for cell in line.into_iter() {
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

        let start_y = (index.0 + 1) as u32 * self.line_height;
        let mut curr_col = 0;
        for val in data {
            let data = val.0;
            let color = val.1;
            let buffer = UnicodeBuffer::new()
                .add_str(&data)
                .guess_segment_properties();

            let glyph_buffer = harfbuzz_rs::shape(
                self.hb_font,
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

                let x = (curr_col * self.text_width) as f32 + x_offset;
                let y = y_offset + start_y as f32;

                let scale_factor = match scale_factor > 1 {
                    true => 1.0 / (1.0 + scale_factor as f32 * 0.1),
                    false => 1.0,
                };

                let scale = Scale {
                    x: self.scale.x * scale_factor,
                    y: self.scale.y * scale_factor,
                };

                let glyph = self
                    .rt_font
                    .glyph(glyph_id)
                    .scaled(scale)
                    .positioned(point(x, y));

                if let Some(round_box) = glyph.pixel_bounding_box() {
                    glyph.draw(|x, y, v| {
                        let x = x as i32 + round_box.min.x;
                        let y = y as i32 + round_box.min.y;

                        f(x, y, v, *color)
                    });
                }

                curr_col += 1;
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
                self.term.update(&mut self.cursor);
                self.cursor.column.0 = 0;
                self.cursor.line.0 += 1;
            }
            ControlFunction::TextProc(TextProc::VTab) => {}
            ControlFunction::TextProc(TextProc::FormFeed) => {}
            ControlFunction::TextProc(TextProc::CarriageReturn) => {
                self.term.update(&mut self.cursor);
                self.cursor.column.0 = 0;
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
                self.cursor.line.0 -= 1;
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

    fn add_new_cell(&mut self, c: char) {
        self.term.add_new_cell(c)
    }
}

impl Handler for Display<'_> {
    fn print(&mut self, consume: vte::VtConsume) {
        match consume {
            VtConsume::Print(c) => self.add_new_cell(c),
            _ => unreachable!(),
        }
    }

    fn execute(&mut self, consume: vte::VtConsume) {
        self.execute_control(consume.into());
    }

    fn esc_dispatch(&mut self, consume: vte::VtConsume) {
        self.execute_control(consume.into());
    }

    fn csi_dispatch(&mut self, consume: vte::VtConsume) {
        let control: ControlFunction = consume.into();
        // println!("csi dispatch {:?}", control);
        match control {
            ControlFunction::Visual(v) => match v {
                Visual::DarkMode(d) => self.term.dark_mode = d,
                Visual::GraphicRendition(vec) => self.term.rendition(vec),
                _ => {}
            },
            ControlFunction::Editing(e) => match e {
                Editing::DeleteCharacter(_) => {}
                Editing::DeleteCol(_) => {}
                Editing::DeleteLine(_) => {}
                Editing::EraseInDisplay(flag) => match flag {
                    0 => {
                        let col = self.cursor.column;
                        let line = self.cursor.line;
                        // Clear the current line first
                        let row_len = self.term.data[line].len();
                        let col_len = self.term.data.len();
                        self.term
                            .erase_line_range_unchecked(line, col.0..row_len, |_| true);

                        self.term
                            .erase_range_unchecked((line.0 + 1)..col_len, |_| true);
                    }
                    1 => {
                        let col = self.cursor.column;
                        let line = self.cursor.line;
                        // Clear from the top of the display

                        self.term.erase_range_unchecked(0..line.0, |_| true);
                        self.term
                            .erase_line_range_unchecked(line, 0..col.0, |_| true);
                    }
                    2 => {
                        let col_len = self.term.data.len();
                        self.term.erase_range_unchecked(0..col_len, |_| true);
                    }
                    _ => {}
                },
                Editing::SelectiveEraseDisplay(flag) => match flag {
                    0 => {
                        let col = self.cursor.column;
                        let line = self.cursor.line;
                        // Clear the current line first
                        let row_len = self.term.data[line].len();
                        let col_len = self.term.data.len();
                        self.term
                            .erase_line_range_unchecked(line, col.0..row_len, |c| c.erasable);

                        self.term
                            .erase_range_unchecked((line.0 + 1)..col_len, |c| c.erasable);
                    }
                    1 => {
                        let col = self.cursor.column;
                        let line = self.cursor.line;
                        // Clear from the top of the display

                        self.term.erase_range_unchecked(0..line.0, |c| c.erasable);
                        self.term
                            .erase_line_range_unchecked(line, 0..col.0, |c| c.erasable);
                    }
                    2 => {
                        let col_len = self.term.data.len();
                        self.term.erase_range_unchecked(0..col_len, |c| c.erasable);
                    }
                    _ => {}
                },
                Editing::EraseInLine(flag) => match flag {
                    0 => {
                        let row_len = self.term.data[self.cursor.line].len();
                        self.term.erase_line_range_unchecked(
                            self.cursor.line,
                            self.cursor.column.0..row_len,
                            |_| true,
                        );
                    }
                    1 => {
                        self.term.erase_line_range_unchecked(
                            self.cursor.line,
                            0..self.cursor.column.0,
                            |_| true,
                        );
                    }
                    2 => {
                        let row_len = self.term.data[self.cursor.line].len();
                        self.term
                            .erase_line_range_unchecked(self.cursor.line, 0..row_len, |_| true);
                    }
                    _ => {}
                },
                Editing::SelectiveEraseLine(flag) => match flag {
                    0 => {
                        let row_len = self.term.data[self.cursor.line].len();
                        self.term.erase_line_range_unchecked(
                            self.cursor.line,
                            self.cursor.column.0..row_len,
                            |c| c.erasable,
                        );
                    }
                    1 => {
                        self.term.erase_line_range_unchecked(
                            self.cursor.line,
                            0..self.cursor.column.0,
                            |c| c.erasable,
                        );
                    }
                    2 => {
                        let row_len = self.term.data[self.cursor.line].len();
                        self.term
                            .erase_line_range_unchecked(self.cursor.line, 0..row_len, |c| {
                                c.erasable
                            });
                    }
                    _ => {}
                },
                _ => {}
            },
            ControlFunction::TextProc(t) => match t {
                TextProc::SaveCursor | TextProc::SaveCursorPosition => {
                    self.saved_cursor = Some(self.cursor.clone())
                }
                TextProc::RestoreCursor | TextProc::RestoreSavedCursor => {
                    if let Some(cursor) = self.saved_cursor.take() {
                        self.cursor = cursor;
                    }
                }
                _ => {}
            },
            _ => {}

            c => {}
        }
    }

    fn hook(&mut self, consume: vte::VtConsume) {}

    fn put(&mut self, consume: vte::VtConsume) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, consume: vte::VtConsume) {}
}
