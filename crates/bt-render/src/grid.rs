//! 터미널 화면(`bt_term::Screen`과 같은 꼴의 값)을 픽셀로 그린다

use bt_core::Rgb;

use crate::canvas::Canvas;
use crate::font::{CellMetrics, FontStyle, Fonts};

/// 렌더러가 그리는 셀 한 칸. bt-term에 기대지 않으려고 따로 둔다(앱이 옮겨 담는다)
#[derive(Debug, Clone, PartialEq)]
pub struct DrawCell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub wide: bool,
    pub spacer: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Beam,
    Underline,
    Hollow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawCursor {
    pub col: usize,
    pub row: usize,
    pub style: CursorStyle,
    pub wide: bool,
    pub color: Rgb,
    pub text_color: Rgb,
}

/// 그릴 격자 하나
pub struct GridView<'a> {
    pub cols: usize,
    pub rows: usize,
    pub cells: &'a [DrawCell],
    pub cursor: Option<DrawCursor>,
    pub selection_bg: Rgb,
    /// IME 조합 중인 글(커서 자리에 덮어 그린다, PTY에는 보내지 않는다)
    pub preedit: Option<&'a str>,
}

/// 격자를 (x, y)에 그린다
pub fn draw_grid(canvas: &mut Canvas, fonts: &mut Fonts, x: i32, y: i32, view: &GridView) {
    let m = fonts.metrics();
    let cw = m.width as i32;
    let ch = m.height as i32;
    // 1) 배경을 모든 줄에 먼저 칠한다. 글자가 셀 위아래로 조금 넘쳐도(대체 글꼴의 한글) 다음 줄 배경에 잘리지 않게
    let visible_rows = view
        .rows
        .min(((canvas.height as i32 - y).max(0) / ch.max(1)) as usize + 1);
    for row in 0..visible_rows {
        let py = y + row as i32 * ch;
        for col in 0..view.cols {
            let cell = &view.cells[row * view.cols + col];
            let bg = if cell.selected { view.selection_bg } else { cell.bg };
            canvas.fill_rect(x + col as i32 * cw, py, m.width, m.height, bg);
        }
    }
    // 2) 글자와 밑줄
    for row in 0..visible_rows {
        let py = y + row as i32 * ch;
        for col in 0..view.cols {
            let cell = &view.cells[row * view.cols + col];
            if cell.spacer {
                continue;
            }
            let px = x + col as i32 * cw;
            let span = if cell.wide { 2 } else { 1 };
            draw_cell_text(canvas, fonts, m, px, py, span, cell.ch, cell.fg, cell.bold, cell.italic);
            if cell.underline {
                canvas.fill_rect(px, py + m.underline_offset as i32, m.width * span, m.stroke, cell.fg);
            }
            if cell.strike {
                canvas.fill_rect(px, py + (m.height / 2) as i32, m.width * span, m.stroke, cell.fg);
            }
        }
    }

    // 3) 커서와 조합 중 글
    if let Some(cur) = view.cursor {
        if cur.row < view.rows && cur.col < view.cols {
            let px = x + cur.col as i32 * cw;
            let py = y + cur.row as i32 * ch;
            if let Some(pre) = view.preedit.filter(|p| !p.is_empty()) {
                draw_preedit(canvas, fonts, m, px, py, pre, cur.color, view);
            } else {
                draw_cursor(
                    canvas,
                    fonts,
                    m,
                    px,
                    py,
                    &cur,
                    &view.cells[cur.row * view.cols + cur.col],
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cell_text(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    m: CellMetrics,
    px: i32,
    py: i32,
    span: u32,
    c: char,
    fg: Rgb,
    bold: bool,
    italic: bool,
) {
    if c == ' ' || c == '\0' {
        return;
    }
    if crate::boxdraw::draw(canvas, c, px, py, m.width * span, m.height, m.stroke, fg) {
        return;
    }
    let Some(g) = fonts.glyph(c, FontStyle::from_flags(bold, italic)) else {
        // 어느 글꼴에도 없는 글자: 빈 상자
        canvas.stroke_rect(px + 1, py + 2, m.width * span - 2, m.height - 4, 1, fg);
        return;
    };
    // 넓은 글자나 셀보다 넓은 글리프는 칸 가운데에 둔다
    let slot = (m.width * span) as i32;
    let offset = if g.width as i32 > slot - g.left || span > 1 {
        (slot - g.width as i32) / 2 - g.left
    } else {
        0
    };
    canvas.draw_glyph(px + offset, py + m.baseline as i32, &g, fg);
}

fn draw_cursor(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    m: CellMetrics,
    px: i32,
    py: i32,
    cur: &DrawCursor,
    cell: &DrawCell,
) {
    let span = if cur.wide { 2 } else { 1 };
    let w = m.width * span;
    match cur.style {
        CursorStyle::Block => {
            canvas.fill_rect(px, py, w, m.height, cur.color);
            draw_cell_text(
                canvas,
                fonts,
                m,
                px,
                py,
                span,
                cell.ch,
                cur.text_color,
                cell.bold,
                cell.italic,
            );
        }
        CursorStyle::Hollow => canvas.stroke_rect(px, py, w, m.height, m.stroke, cur.color),
        CursorStyle::Beam => canvas.fill_rect(px, py, (m.stroke * 2).max(2), m.height, cur.color),
        CursorStyle::Underline => canvas.fill_rect(
            px,
            py + m.height as i32 - (m.stroke * 2) as i32,
            w,
            m.stroke * 2,
            cur.color,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_preedit(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    m: CellMetrics,
    px: i32,
    py: i32,
    text: &str,
    color: Rgb,
    view: &GridView,
) {
    let mut x = px;
    let bg = view.selection_bg;
    for c in text.chars() {
        let span = crate::text::char_cells(c) as u32;
        canvas.fill_rect(x, py, m.width * span, m.height, bg);
        draw_cell_text(canvas, fonts, m, x, py, span, c, color, false, false);
        canvas.fill_rect(
            x,
            py + m.height as i32 - m.stroke as i32,
            m.width * span,
            m.stroke,
            color,
        );
        x += (m.width * span) as i32;
    }
}
