use std::ops::Range;
use term::data::cursor::Cursor;
use term::data::grids::Grid;
use term::data::{Attribute, Cell, Color, Column, Line, RGBA};
pub mod display;
pub mod renderer;
pub mod text;

#[derive(Debug)]
pub struct Terminal<'config> {
    scheme: &'config [RGBA; 16],

    fg: Color,
    bg: Color,
    attr: Attribute,

    dark_mode: bool,
    pub data: Grid<Cell>,
    pub write_stack: Vec<Cell>,
}

impl<'config> Terminal<'config> {
    pub fn new(max_row: usize, max_col: usize, colorscheme: &'config [RGBA; 16]) -> Self {
        Self {
            scheme: colorscheme,
            fg: Color::IndexBase(7),
            bg: Color::IndexBase(0),
            attr: Attribute::default(),
            dark_mode: false,
            data: Grid::new(max_col, max_row),
            write_stack: Vec::with_capacity(25),
        }
    }

    pub fn resize(&mut self, max_row: usize, max_col: usize) {
        self.data.resize(max_col, max_row, |_| true);
    }

    pub fn input(&mut self, cursor: &mut Cursor, data: Vec<Cell>) {
        self.data.input_insert(data, cursor, |_| true);
    }

    pub fn update(&mut self, cursor: &mut Cursor) {
        self.data
            .input_insert(std::mem::take(&mut self.write_stack), cursor, |_| true);
    }

    pub fn reset_graphic(&mut self) {
        self.fg = Color::IndexBase(7);
        self.bg = Color::IndexBase(0);
        self.attr = Attribute::default();
    }

    fn set_attr(&mut self, val: i64) {}

    pub fn rendition(&mut self, rendition: Vec<i64>) {
        if rendition.len() <= 2 {
            for val in rendition {
                match &val {
                    0 => self.reset_graphic(),
                    1..=27 => self.set_attr(val),
                    30..=37 => {
                        if self.dark_mode {
                            self.fg = Color::IndexBase((val - 30) as usize)
                        } else {
                            self.fg = Color::IndexBase((val - 30 + 8) as usize)
                        }
                    }
                    38 => self.fg = Color::IndexBase(7),
                    40..=47 => {
                        if self.dark_mode {
                            self.bg = Color::IndexBase((val - 30) as usize)
                        } else {
                            self.bg = Color::IndexBase((val - 30 + 8) as usize)
                        }
                    }
                    49 => self.bg = Color::IndexBase(0),
                    _ => {}
                }
            }
            return;
        }

        if rendition.len() > 2 {
            match rendition.as_slice() {
                [pre @ .., 38, 5, index] => {
                    self.rendition(pre.to_vec());
                    self.fg = Color::Index256(*index as usize);
                }
                [pre @ .., 48, 5, index] => {
                    self.rendition(pre.to_vec());
                    self.fg = Color::Index256(*index as usize);
                }
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

    pub fn add_new_cell(&mut self, c: char) {
        self.write_stack.push(Cell {
            c,
            fg: self.fg,
            bg: self.bg,
            attr: self.attr.clone(),
            sixel_data: None,
            erasable: true,
            dirty: false,
        });
    }

    pub fn erase_line_range_unchecked(
        &mut self,
        line: Line,
        range: Range<usize>,
        with_filter: impl Fn(&Cell) -> bool,
    ) {
        if self.data.len() < line.0 {
            return;
        }
        let data = &mut self.data[line];

        for i in range {
            if i > data.len() - 1 {
                return;
            }
            if !with_filter(&data[Column(i)]) {
                continue;
            }
            data[Column(i)].c = ' ';
            data[Column(i)].dirty = true;
            data[Column(i)].bg = Color::IndexBase(0);
            data[Column(i)].fg = Color::IndexBase(7);
            data[Column(i)].attr = Attribute::default();
        }
    }

    /// Erase lines in range
    pub fn erase_range_unchecked(
        &mut self,
        range: Range<usize>,
        mut with_filter: impl FnMut(&&mut Cell) -> bool,
    ) {
        for i in range {
            (&mut self.data[Line(i)])
                .into_iter()
                .take_while(&mut with_filter)
                .for_each(|cell| {
                    cell.c = ' ';
                    cell.dirty = true;
                    cell.bg = Color::IndexBase(0);
                    cell.fg = Color::IndexBase(7);
                    cell.attr = Attribute::default();
                });
        }
    }
}
