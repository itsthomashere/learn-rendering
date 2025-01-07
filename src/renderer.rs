use crate::text::{GlyphVertex, TextGenerator};
use rusttype::Scale;
use term::data::{Attribute, Color, Column, GridCell, Line, PositionedCell, ANSI_256, RGBA};

pub struct Renderer<'config> {
    font_loader: TextGenerator,
    max_x: u32,
    max_y: u32,
    cell_width: u32,
    cell_height: u32,
    max_cell: usize,
    line_offset: Line,
    colorscheme: &'config [RGBA; 16],
}

impl<'config> Renderer<'config> {
    pub fn resize(&mut self, max_x: u32, max_y: u32) {
        self.max_x = max_x;
        self.max_y = max_y;
    }
    pub fn new(max_x: u32, max_y: u32, scale: Scale, colorscheme: &'config [RGBA; 16]) -> Self {
        let cell_height: u32 = scale.y.round() as u32;
        let cell_width: u32 = (scale.x / 2.0).round() as u32;
        let max_col = max_x / cell_width;
        let max_row = max_y / cell_height;
        Self {
            font_loader: TextGenerator::new(cell_width, cell_height, scale),
            max_x,
            max_y,
            cell_width,
            cell_height,
            max_cell: (max_col * max_row) as usize,
            line_offset: Line(0),
            colorscheme,
        }
    }
    // pub fn render<I, O>(&mut self, data: I)
    // where
    //     I: Iterator,
    //     I::Item: for<'a> PositionedCell<&'a O>,
    //     O: GridCell,
    // {
    //     self.prepare_render(data);
    // }

    /// Load the cells into the buffer and prepare to render
    ///
    /// * `data`:
    pub fn prepare_render<'a, I, O>(&self, data: I) -> Vec<GlyphVertex>
    where
        I: Iterator,
        I::Item: PositionedCell<&'a O>,
        O: GridCell + 'a,
    {
        let mut result = Vec::with_capacity(self.max_cell);
        let mut current_line: Option<Line> = None;
        let mut current_group: String = String::with_capacity(20);
        let mut start_col: Option<Column> = None;
        let mut last_fg: Option<Color> = None;
        let mut last_bg: Option<Color> = None;
        let mut last_attribute: Option<Attribute> = None;

        for cell in data {
            let (line, col) = cell.position();
            let cell = cell.cell();
            let c = cell.char();
            let fg = cell.fg();
            let bg = cell.bg();
            let attr = cell.attribute();

            // current_line is only none when we're at the beginning
            // that means every things else is none too
            if current_line.is_none() {
                current_line = Some(line);
                start_col = Some(col);
                last_fg = Some(*fg);
                last_bg = Some(*bg);
                last_attribute = Some(attr.clone());
                current_group.push(c);
                continue;
            }

            // If encoutered a new line or different attributed cell
            // drain this chunk and create new chunk
            if current_line.is_some_and(|l| l != line)
                || last_fg.as_ref().is_some_and(|f| f != fg)
                || last_bg.as_ref().is_some_and(|f| f != bg)
                || last_attribute.as_ref().is_some_and(|a| a != attr)
            {
                result.extend(self.font_loader.load(
                    self.max_x,
                    self.max_y,
                    std::mem::take(&mut current_group),
                    last_attribute.take().unwrap(),
                    self.to_rgba(last_fg.take().unwrap()),
                    self.to_rgba(last_bg.take().unwrap()),
                    self.cell_width,
                    self.cell_height,
                    Line(current_line.take().unwrap().0 - self.line_offset.0),
                    start_col.take().unwrap(),
                ));
                start_col = Some(col);
                current_line = Some(line);
                last_fg = Some(*fg);
                last_bg = Some(*bg);
                last_attribute = Some(attr.clone())
            }

            current_group.push(c);
        }

        if !current_group.is_empty() {
            result.extend(self.font_loader.load(
                self.max_x,
                self.max_y,
                std::mem::take(&mut current_group),
                last_attribute.take().unwrap(),
                self.to_rgba(last_fg.take().unwrap()),
                self.to_rgba(last_bg.take().unwrap()),
                self.cell_width,
                self.cell_height,
                Line(current_line.take().unwrap().0 - self.line_offset.0),
                start_col.take().unwrap(),
            ));
        }

        result
    }

    fn to_rgba(&self, color: Color) -> RGBA {
        match color {
            Color::Rgba(rgba) => rgba,
            Color::IndexBase(index) => self.colorscheme[index],
            Color::Index256(index) => ANSI_256[index],
        }
    }
}
