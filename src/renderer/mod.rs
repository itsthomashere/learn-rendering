use crate::Color;
use harfbuzz_rs::Feature;
use harfbuzz_rs::Font as HbFont;
use harfbuzz_rs::Tag;
use harfbuzz_rs::UnicodeBuffer;
use rusttype::point;
use rusttype::Font as RtFont;
use rusttype::GlyphId;
use rusttype::Scale;

pub struct Renderer {}

#[derive(Debug, Clone)]
pub struct LineBuffer {
    lines: Vec<TextLine>,
    max_col: u32,
    max_row: u32,
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    text_width: u32,
    line_height: u32,
    scale: Scale,
}

impl LineBuffer {
    pub fn new(scale: Scale, min_x: u32, min_y: u32, max_x: u32, max_y: u32) -> Self {
        let line_height = scale.y.round() as u32;
        let text_width = (scale.y / 2.0).round() as u32;
        let max_col = (max_x - min_x) / text_width;
        let max_row = (max_y - min_y) / line_height;

        Self {
            scale,
            lines: Vec::with_capacity(max_row as usize),
            max_col,
            max_row,
            min_x,
            min_y,
            max_x,
            max_y,
            line_height,
            text_width,
        }
    }

    // TODO: Implement line wrap
    pub fn append_text(&mut self, text: impl AsRef<str>, color: Color) {
        let lines = text
            .as_ref()
            .split_inclusive(|ch: char| ch == '\n' || ch.is_whitespace())
            .collect::<Vec<_>>();
        for line in lines {
            if line.contains('\n') {
                line.strip_suffix('\n').unwrap();
                self.lines.push(TextLine::new());
            } else {
                self.append_last_line(line, color.clone());
            }
        }
    }

    fn append_last_line(&mut self, text: impl AsRef<str>, color: Color) {
        let str_len = text.as_ref().len() as u32;
        if self.can_fit(str_len) {
            match self.lines.last_mut() {
                Some(last_line) => {
                    last_line.append_text(text, color);
                }
                None => {
                    let mut new_line = TextLine::new();
                    new_line.append_text(text, color);
                    self.lines.push(new_line);
                }
            }
            return;
        }
        let mut remainder = self.max_col - self.col_len();
        match self.lines.last_mut() {
            Some(last_line) => {
                // the remainder into the line
                last_line.append_text(&text.as_ref()[..remainder as usize], color.clone());
            }
            None => {
                let mut new_line = TextLine::new();
                new_line.append_text(&text.as_ref()[..remainder as usize], color.clone());
                self.lines.push(new_line);
            }
        }
        // get the required lines to be push into this
        let required_lines = (str_len - remainder) / self.max_col + 1;

        for _ in 0..required_lines {
            if remainder + self.max_col < str_len {
                let mut new_lines = TextLine::new();
                new_lines.append_text(
                    &text.as_ref()[(remainder as usize)..((remainder + self.max_col) as usize)],
                    color.clone(),
                );
                self.lines.push(new_lines);
            } else {
                let mut new_lines = TextLine::new();
                new_lines.append_text(&text.as_ref()[(remainder as usize)..], color.clone());
                self.lines.push(new_lines);
            }
            remainder += self.max_col;
        }
    }

    pub fn can_fit(&self, len: u32) -> bool {
        self.max_col - self.col_len() >= len
    }

    /// Get the current len of the most recent line
    pub fn col_len(&self) -> u32 {
        match self.lines.last() {
            Some(line) => line.len(),
            None => 0,
        }
    }

    pub fn render_all<F>(
        &self,
        hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
        rt_font: &RtFont<'static>,
        mut f: F,
    ) where
        F: FnMut(i32, i32, f32, Color),
    {
        if self.lines.len() <= self.max_row as usize {
            for i in 0..self.lines.len() {
                self.render_line(i, hb_font, rt_font, &mut f);
            }
        } else {
            for i in (self.max_row as usize - self.lines.len())..self.lines.len() {
                self.render_line(i, hb_font, rt_font, &mut f);
            }
        }
    }

    /// Insert text at given position
    ///
    /// * `line`: Line number
    /// * `col`: column number
    /// * `color`: [Color]
    pub fn insert_at(&mut self, line: u32, col: u32, text: impl AsRef<str>, color: Color) {
        let str_len = text.as_ref().len() as u32;
        // If we need to insert at the line that doesnt exits yet, insert empty lines to fill in
        if line >= self.lines.len() as u32 {
            for _ in 0..(line - self.lines.len() as u32) {
                self.lines.push(TextLine::new());
            }
            // insert whitespace placeholder
            self.append_last_line(" ".repeat(col as usize), color.clone());
            self.append_text(text, color);
            return;
        }

        let cur_line = self
            .lines
            .get_mut(line as usize)
            .expect("len checked hence the index is valid");

        // If the line can hold the text, we can push straight into the line
        if self.max_col - cur_line.len() >= text.as_ref().len() as u32 {
            cur_line.insert_at(col, text, color);
            return;
        }

        let exceeded_text = cur_line.split_at(line);
        let mut available = self.max_col - cur_line.len();
        let exeeding = text.as_ref().len() as u32 - available;
        let needed = (exeeding / self.max_col) + 1;
        cur_line.insert_at(col, &text.as_ref()[..(available as usize)], color.clone());

        // Insert new line in the middle
        for i in 0..needed {
            let mut new_line = TextLine::new();
            if available + self.max_col < str_len {
                new_line.append_text(
                    &text.as_ref()[(available as usize)..(available + self.max_col) as usize],
                    color.clone(),
                )
            } else {
                new_line.append_text(&text.as_ref()[(available as usize)..], color.clone())
            }
            self.lines.insert((line + i + 1) as usize, new_line);
            available += self.max_col;
        }
        if let Some((text, color)) = exceeded_text {
            self.insert_at(
                line + needed,
                self.lines[(line + needed + 1) as usize].len(),
                text,
                color,
            )
        }
    }

    /// Get an exclusive reference to the last line
    pub fn last_mut(&mut self) -> Option<&mut TextLine> {
        self.lines.last_mut()
    }

    pub fn render_line<F>(
        &self,
        index: usize,
        hb_font: &harfbuzz_rs::Owned<HbFont<'static>>,
        rt_font: &RtFont<'static>,
        f: &mut F,
    ) where
        F: FnMut(i32, i32, f32, Color),
    {
        let line = match self.lines.get(index) {
            Some(line) => line,
            None => return,
        };
        let start_y = (index + 1) as u32 * self.line_height;
        let mut curr_col = 0;
        line.batches.iter().for_each(|batch| {
            let text = &batch.text;
            let color = &batch.color;
            let buffer = UnicodeBuffer::new()
                .add_str(text)
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
                            f(x, y, v, color.clone())
                        }
                    });
                }

                curr_col += 1;
            }
        });
    }
}

/// Text represented in a line
///
/// * `batches`: [TextBatch]
/// * `len`: The len of the text
/// * `height`: line height
/// * `text_width`: width of each glyhp
/// * `max_col`: max number of column
/// * `cur_col`: current column
/// * `x`: starting point of x
/// * `y`: starting point of y
#[cfg(not(test))]
#[derive(Debug, Clone)]
pub struct TextLine {
    batches: Vec<TextBatch>,
    len: u32,
    last_color: Option<Color>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLine {
    batches: Vec<TextBatch>,
    len: u32,
    last_color: Option<Color>,
}

impl TextLine {
    pub fn new() -> Self {
        Self {
            batches: Vec::with_capacity(256),
            len: 0,
            last_color: None,
        }
    }

    pub fn insert_at(&mut self, index: u32, text: impl AsRef<str>, text_color: Color) {
        let mut start = 0;
        let mut idx = None;
        for (i, val) in self.batches.iter().enumerate() {
            start += val.len();
            if start >= index {
                idx = Some(i);
            }
        }

        if let Some(idx) = idx {
            let batch = self.batches.get_mut(idx).expect("len checked");
            let batch_color = batch.color.clone();
            if batch_color == text_color {
                batch.text.insert_str(index as usize, text.as_ref());
            } else {
                let (text_1, text_2) = batch.text.split_at(index as usize);
                let text_2 = text_2.to_string();
                *batch = TextBatch::new(text_1, batch_color.clone());

                self.batches
                    .insert(idx + 1, TextBatch::new(text_2, batch_color));

                self.batches
                    .insert(idx + 1, TextBatch::new(text.as_ref(), text_color))
            }
            self.len += text.as_ref().len() as u32;
        }
    }

    pub fn split_at(&mut self, position: u32) -> Option<(String, Color)> {
        let mut start = 0;
        let mut idx = None;

        for (i, val) in self.batches.iter().enumerate() {
            start += val.len();
            if start >= position {
                idx = Some(i);
            }
        }
        println!("index: {idx:?}");
        match idx {
            Some(idx) => {
                let target = self.batches.get_mut(idx).expect("checked len");
                let str = target.text.split_at((start - position) as usize);
                let exceeded = str.1.to_string();
                println!("exceeded : {exceeded}");
                target.text = str.0.to_string();
                self.len -= exceeded.len() as u32;

                Some((exceeded, target.color.clone()))
            }
            None => None,
        }
    }

    pub fn append_text(&mut self, text: impl AsRef<str>, text_color: Color) {
        match self.last_color.as_ref() {
            Some(color) if color == &text_color => {
                self.batches
                    .last_mut()
                    .expect("always available if last color is Some")
                    .add_text(&text);
                self.len += text.as_ref().len() as u32;
            }
            _ => {
                self.batches.push(TextBatch::new(&text, text_color.clone()));
                self.last_color = Some(text_color);
                self.len += text.as_ref().len() as u32;
            }
        }
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0 && self.batches.is_empty()
    }
}

impl Default for TextLine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBatch {
    text: String,
    color: Color,
}

impl TextBatch {
    pub fn new(text: impl AsRef<str>, color: Color) -> Self {
        Self {
            text: text.as_ref().into(),
            color,
        }
    }

    pub fn add_text(&mut self, text: impl AsRef<str>) {
        self.text.push_str(text.as_ref());
    }
    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_line_buffer_append_same_color() {
        let scale = Scale::uniform(32.0);
        let max_x = 160;
        let max_y = 160;
        let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);

        assert_eq!(line_buffer.max_col, 10);
        assert_eq!(line_buffer.max_row, 5);

        let color_1 = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        line_buffer.append_text("12345678901", color_1.clone());
        line_buffer.append_text("2345678901", color_1.clone());

        let inner = line_buffer.lines;

        assert_eq!(
            inner,
            vec![
                TextLine {
                    batches: vec![TextBatch {
                        text: "1234567890".to_string(),
                        color: color_1.clone()
                    }],
                    len: 10,
                    last_color: Some(color_1.clone())
                },
                TextLine {
                    batches: vec![TextBatch {
                        text: "1234567890".to_string(),
                        color: color_1.clone()
                    }],
                    len: 10,
                    last_color: Some(color_1.clone())
                },
                TextLine {
                    batches: vec![TextBatch {
                        text: "1".to_string(),
                        color: color_1.clone()
                    }],
                    len: 1,
                    last_color: Some(color_1.clone())
                }
            ]
        )
    }

    #[test]
    fn text_line_buffer_append_diff_color() {
        let scale = Scale::uniform(32.0);
        let max_x = 160;
        let max_y = 160;
        let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);

        assert_eq!(line_buffer.max_col, 10);
        assert_eq!(line_buffer.max_row, 5);

        let color_1 = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let color_2 = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        line_buffer.append_text("12345", color_1.clone());
        line_buffer.append_text("12345", color_2.clone());
        line_buffer.append_text("23456", color_2.clone());
        line_buffer.append_text("23456", color_1.clone());
        line_buffer.append_text("123", color_1.clone());

        let inner = line_buffer.lines;

        assert_eq!(
            inner,
            vec![
                TextLine {
                    batches: vec![
                        TextBatch {
                            text: "12345".to_string(),
                            color: color_1.clone()
                        },
                        TextBatch {
                            text: "12345".to_string(),
                            color: color_2.clone()
                        }
                    ],
                    len: 10,
                    last_color: Some(color_2.clone())
                },
                TextLine {
                    batches: vec![
                        TextBatch {
                            text: "23456".to_string(),
                            color: color_2.clone()
                        },
                        TextBatch {
                            text: "23456".to_string(),
                            color: color_1.clone()
                        }
                    ],
                    len: 10,
                    last_color: Some(color_1.clone())
                },
                TextLine {
                    batches: vec![TextBatch {
                        text: "123".to_string(),
                        color: color_1.clone()
                    }],
                    len: 3,
                    last_color: Some(color_1.clone())
                }
            ]
        )
    }

    #[test]
    fn test_line_buffer_with_new_line() {
        let scale = Scale::uniform(32.0);
        let max_x = 160;
        let max_y = 160;
        let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);

        assert_eq!(line_buffer.max_col, 10);
        assert_eq!(line_buffer.max_row, 5);
        let color_1 = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };

        let inner = line_buffer.lines;
    }
}
