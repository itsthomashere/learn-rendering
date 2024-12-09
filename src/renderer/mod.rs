use crate::Color;
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
    line_height: u32,
    text_width: u32,
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

    pub fn append_text(&mut self, text: impl AsRef<str>, color: Color) {
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

        for i in 0..required_lines {
            let require_len = remainder + (i + 1) * self.max_col;
            if require_len < str_len {
                let mut new_lines = TextLine::new();
                new_lines.append_text(
                    &text.as_ref()[(remainder as usize)..(require_len as usize)],
                    color.clone(),
                );
                self.lines.push(new_lines);
            } else {
                let mut new_lines = TextLine::new();
                new_lines.append_text(&text.as_ref()[(remainder as usize)..], color.clone());
                self.lines.push(new_lines);
            }
            remainder += required_lines;
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
    pub fn len(&self) -> usize {
        self.text.len()
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
}
