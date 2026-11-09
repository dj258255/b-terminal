//! 글꼴과 셀 격자 그리기(ADR-002). 창을 모르고 `&mut [u32]`에 그린다

pub mod background;
mod boxdraw;
pub mod canvas;
pub mod cluster;
pub mod font;
pub mod grid;
pub mod proportional;
pub mod scrollbar;
mod sfnt;
pub mod text;

pub use canvas::{Canvas, cells_alpha};
pub use font::{
    CellMetrics, CodepointFont, FaceEntry, FaceSource, FontConfig, FontError, FontSet, FontStyle, FontStyleName, Fonts,
    MetricAdjust, SyntheticStyle, Variations, list_system_faces,
};
pub use grid::{CursorStyle, DrawCell, DrawCursor, GridView, draw_grid, draw_grid_rows};
pub use text::{char_cells, draw_text, text_cells, truncate};
