//! 글꼴과 셀 격자 그리기(ADR-002). 창을 모르고 `&mut [u32]`에 그린다

pub mod background;
mod boxdraw;
pub mod canvas;
pub mod cluster;
pub mod font;
pub mod grid;
pub mod kitty;
pub mod proportional;
pub mod scrollbar;
mod sfnt;
pub mod sprite;
pub mod text;
pub mod trail;
pub mod underline;

pub use canvas::{Canvas, cells_alpha};
pub use font::{
    CellMetrics, CodepointFont, FaceEntry, FaceSource, FontConfig, FontError, FontSet, FontStyle, FontStyleName, Fonts,
    MetricAdjust, SyntheticStyle, TITLE_FONT_SIZE, Variations, forget_system_fonts, list_system_faces,
};
pub use grid::{
    CursorStyle, DrawCell, DrawCursor, GridView, Under, decoration_color, draw_grid, draw_grid_rows, draw_grid_shifted,
    draw_grid_under, ligatures, lock_rects, mark_bar_width, no_min_contrast, text_color,
};
pub use text::{char_cells, draw_text, draw_text_styled, set_ambiguous_wide, text_cells, truncate};
pub use underline::Underline;
