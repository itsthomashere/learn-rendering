use crate::Terminal;
use rusttype::Scale;
use term::data::cursor::Cursor;
use term::data::grids::GridIterator;
use term::data::{Cell, Column, Line, RGBA};
use vte::ansi::{
    Audible, ControlFunction, Editing, GraphicCharset, Management, Synchronization, TextProc,
    Visual,
};
use vte::{Handler, VtConsume};

#[derive(Debug)]
pub struct Display<'config> {
    cursor: Cursor,
    saved_cursor: Option<Cursor>,

    pub term: Terminal<'config>,
}

impl<'config> Display<'config> {
    pub fn resize(&mut self, x: u32, y: u32, scale: Scale) {
        let line_height: u32 = scale.y.round() as u32;
        let text_width: u32 = (scale.x / 2.0).round() as u32;
        let max_col = x / text_width;
        let max_row = y / line_height;

        self.term.resize(max_row as usize, max_col as usize);
    }
    pub fn new(x: u32, y: u32, scale: Scale, colorscheme: &'config [RGBA; 16]) -> Self {
        let line_height: u32 = scale.y.round() as u32;
        let text_width: u32 = (scale.x / 2.0).round() as u32;
        let max_col = x / text_width;
        let max_row = y / line_height;
        Self {
            cursor: Cursor::new(Line(0), Column(0)),
            saved_cursor: None,
            term: Terminal::new(max_row as usize, max_col as usize, colorscheme),
        }
    }

    pub fn grid_iter(&self, start: Line) -> GridIterator<Cell> {
        self.term
            .data
            .grid_iter((start, Column(0)), (Line(80), Column(132)))
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
                Visual::GraphicRendition(vec) => {
                    self.term.update(&mut self.cursor);
                    self.term.rendition(vec)
                }
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
                            .erase_line_range_unchecked(line, col.0 + 1..row_len, |_| true);

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
                            .erase_line_range_unchecked(line, col.0 + 1..row_len, |c| c.erasable);

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
                            self.cursor.column.0 + 1..row_len,
                            |_| true,
                        );
                    }
                    1 => {
                        self.term.erase_line_range_unchecked(
                            self.cursor.line,
                            0..self.cursor.column.0 + 1,
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
