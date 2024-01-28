use ratatui::prelude::*;
use style::palette::tailwind;

pub const PALETTES: [tailwind::Palette; 4] = [
    tailwind::RED,
    tailwind::BLUE,
    tailwind::EMERALD,
    tailwind::INDIGO,
];
pub const INFO_TEXT: &str =
    "(q) quit | (Esc) previous | (↑) move up | (↓) move down | (c) next color";

pub const ITEM_HEIGHT: usize = 4;

#[derive(Clone, Debug)]
pub struct TableColors {
    pub(crate) buffer_bg: Color,
    pub(crate) header_bg: Color,
    pub(crate) header_fg: Color,
    pub(crate) row_fg: Color,
    pub(crate) selected_style_fg: Color,
    pub(crate) normal_row_color: Color,
    pub(crate) alt_row_color: Color,
    pub(crate) footer_border_color: Color,
}

impl TableColors {
    pub const fn new(color: &tailwind::Palette) -> Self {
        Self {
            buffer_bg: tailwind::SLATE.c950,
            header_bg: color.c900,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_style_fg: color.c400,
            normal_row_color: tailwind::SLATE.c950,
            alt_row_color: tailwind::SLATE.c900,
            footer_border_color: color.c400,
        }
    }
}
