use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    NormalText = 0,
    Bold = 1,
    Reverse = 2,
    Underline = 3,
    Blink = 4,
    BoldReverse = 5,
    BoldUnderline = 6,
    BoldBlink = 7,
    ReverseUnderline = 8,
    ReverseBlink = 9,
    UnderlineBlink = 10,
    BoldReverseUnderline = 11,
    BoldReverseBlink = 12,
    BoldUnderlineBlink = 13,
    ReverseUnderlineBlink = 14,
    BoldReverseUnderlineBlink = 15,
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexedColor {
    Black = 0,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground = 256,
    Background,
    Cursor,
    DimBlack,
    DimRed,
    DimGreen,
    DimYellow,
    DimBlue,
    DimMagenta,
    DimCyan,
    DimWhite,
    BrightForeground,
    DimForeground,
}

pub enum CursorStyle {
    Block,
    BlinkBlock,
    BlinkUnderline,
    UnderLine,
}

impl Default for Attribute {
    fn default() -> Self {
        Self::NormalText
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cursor<T> {
    inner: T,
    position: (usize, usize),
    need_wraping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    c: char,
    fg: Color,
    bg: Color,
    attr: Attribute,
}

#[derive(Debug, Clone)]
pub struct Grid<T> {
    pub cursor: Cursor<T>,
    pub saved_cursor: Cursor<T>,

    buffer: GridBuffer<T>,
    columns: usize,
    lines: usize,
    display_offset: usize,
    max_scroll: usize,
}

#[derive(Debug, Clone)]
pub struct GridBuffer<T> {
    rows: VecDeque<Row<T>>,
    start: usize,
    visible: usize,
    len: usize,
}

#[derive(Debug, Clone)]
pub struct Row<T> {
    data: Vec<T>,
    max: usize,
}
