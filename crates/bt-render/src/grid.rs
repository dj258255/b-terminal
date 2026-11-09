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
    /// 이모지 모양으로 그린다(뒤에 U+FE0F가 붙은 글자, #67). 칸 폭 안에 색 글리프로
    pub emoji: bool,
    /// 이모지 묶음의 첫 칸이면 묶음 글과 차지한 칸 수(#87). 한 글리프로 다듬어지면 그 칸들 가운데에 그린다
    pub cluster: Option<std::sync::Arc<str>>,
    pub cluster_cells: u16,
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
    /// 불투명도(0~1, Ghostty `cursor-opacity`, #135). 1보다 작으면 칸 위에 섞는다
    pub opacity: f32,
}

/// 그릴 격자 하나
pub struct GridView<'a> {
    pub cols: usize,
    pub rows: usize,
    pub cells: &'a [DrawCell],
    pub cursor: Option<DrawCursor>,
    pub selection_bg: Rgb,
    /// 고른 칸의 글자색. 없으면 칸의 글자색
    pub selection_fg: Option<Rgb>,
    /// IME 조합 중인 글(커서 자리에 덮어 그린다, PTY에는 보내지 않는다)
    pub preedit: Option<&'a str>,
    /// 글자색과 바탕의 최소 명암비(Ghostty `minimum-contrast`, #123). 1이면 끔
    pub min_contrast: f32,
}

/// 격자를 (x, y)에 통째로 그린다
pub fn draw_grid(canvas: &mut Canvas, fonts: &mut Fonts, x: i32, y: i32, view: &GridView) {
    let all = vec![true; view.rows];
    draw_grid_rows(canvas, fonts, x, y, view, &all);
}

/// `dirty`가 참인 줄만 다시 그린다(#40). 다시 칠한 세로 범위들 `[위, 아래)`를 돌려준다.
///
/// 글자는 칸 위아래로 조금 넘친다(대체 글꼴의 한글 등). 그래서 바뀐 줄과 그 위아래 한 줄의 바탕을 다시 칠하고,
/// 그 범위 안에서만 한 줄 더 바깥의 글자까지 다시 그린다. 이웃 줄의 넘친 글자가 지워지거나 두 번 겹치지 않는다
pub fn draw_grid_rows(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    x: i32,
    y: i32,
    view: &GridView,
    dirty: &[bool],
) -> Vec<(i32, i32)> {
    let m = fonts.metrics();
    let ch = m.height as i32;
    let rows = view
        .rows
        .min(((canvas.height as i32 - y).max(0) / ch.max(1)) as usize + 1);
    // 바탕을 다시 칠할 줄: 바뀐 줄 ± 1
    let mut paint = vec![false; rows];
    for (r, _) in dirty.iter().enumerate().take(rows).filter(|(_, d)| **d) {
        paint[r.saturating_sub(1)..(r + 2).min(rows)].fill(true);
    }
    let mut spans = Vec::new();
    let mut r = 0;
    while r < rows {
        if !paint[r] {
            r += 1;
            continue;
        }
        let a = r;
        while r < rows && paint[r] {
            r += 1;
        }
        let (top, bottom) = (y + a as i32 * ch, y + r as i32 * ch);
        canvas.set_clip_y(top, bottom);
        draw_rows(canvas, fonts, m, x, y, view, a, r);
        canvas.reset_clip();
        spans.push((top, bottom));
    }
    spans
}

/// `[a, b)` 줄의 바탕, 그리고 그 위아래 한 줄을 더한 범위의 글자·커서를 그린다(자르기는 부른 쪽이 정한다)
#[allow(clippy::too_many_arguments)]
fn draw_rows(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    m: CellMetrics,
    x: i32,
    y: i32,
    view: &GridView,
    a: usize,
    b: usize,
) {
    let cw = m.width as i32;
    let ch = m.height as i32;
    // 1) 바탕을 먼저 칠한다. 글자가 셀 위아래로 조금 넘쳐도 다음 줄 바탕에 잘리지 않게
    for row in a..b {
        let py = y + row as i32 * ch;
        for col in 0..view.cols {
            let cell = &view.cells[row * view.cols + col];
            let (cx, w, h) = (x + col as i32 * cw, m.width, m.height);
            if cell.selected {
                canvas.fill_rect(cx, py, w, h, view.selection_bg);
            } else {
                // background-opacity-cells(#177)를 켰으면 색 바탕 칸도 비친다
                canvas.fill_rect_cell(cx, py, w, h, cell.bg);
            }
        }
    }
    // 2) 글자와 밑줄. 이웃 줄에서 넘쳐 들어온 글자도 그린다
    let text_rows = a.saturating_sub(1)..(b + 1).min(view.rows);
    let mut contrast_memo: Option<((Rgb, Rgb), Rgb)> = None;
    for row in text_rows.clone() {
        let py = y + row as i32 * ch;
        // 이모지 묶음을 한 글리프로 그렸으면 그 칸들은 건너뛴다
        let mut skip_until = 0;
        let cursor_col = view.cursor.filter(|c| c.row == row).map(|c| c.col);
        let subs = ligatures(fonts, &view.cells[row * view.cols..(row + 1) * view.cols], cursor_col);
        for col in 0..view.cols {
            let cell = &view.cells[row * view.cols + col];
            if cell.spacer || col < skip_until {
                continue;
            }
            if let Some(text) = &cell.cluster {
                if let Some(g) = fonts.cluster_glyph(text) {
                    let span = cell.cluster_cells.max(1) as i32;
                    let px = x + col as i32 * cw;
                    let ox = (m.width as i32 * span - g.width as i32) / 2;
                    canvas.draw_color_glyph(px + ox, py + m.baseline as i32, &g);
                    skip_until = col + span as usize;
                    continue;
                }
            }
            let px = x + col as i32 * cw;
            let span = if cell.wide { 2 } else { 1 };
            let fg = match (cell.selected, view.selection_fg) {
                (true, Some(c)) => c,
                _ => cell.fg,
            };
            // 최소 명암비(#123): 바탕과 너무 비슷한 글자색은 흰색이나 검은색으로. 이어지는 칸은 같은 쌍이 많아 앞 결과를 쓴다
            let fg = if view.min_contrast > 1.0 {
                let bg = if cell.selected { view.selection_bg } else { cell.bg };
                match contrast_memo {
                    Some((pair, out)) if pair == (fg, bg) => out,
                    _ => {
                        let out = fg.contrasted(bg, view.min_contrast);
                        contrast_memo = Some(((fg, bg), out));
                        out
                    }
                }
            } else {
                fg
            };
            // U+FE0F가 붙은 글자는 그 칸 안에 색 이모지로(색 글꼴에 없으면 글자로)
            let emoji = cell.emoji.then(|| fonts.emoji_glyph(cell.ch, span)).flatten();
            let style = FontStyle::from_flags(cell.bold, cell.italic);
            // 합자 글리프는 칸 자리에 그대로 둔다(앞 칸으로 넘쳐 그리는 것이 글꼴의 뜻이다)
            let lig = match subs.get(col) {
                Some(&gid) if gid != 0 => fonts.glyph_by_id(gid, style),
                _ => None,
            };
            match (emoji, lig) {
                (Some(g), _) => {
                    let ox = (m.width as i32 * span as i32 - g.width as i32) / 2;
                    canvas.draw_color_glyph(px + ox, py + m.baseline as i32, &g);
                }
                (None, Some(g)) => canvas.draw_glyph(px, py + m.baseline as i32, &g, fg),
                (None, None) => draw_cell_text(canvas, fonts, m, px, py, span, cell.ch, fg, cell.bold, cell.italic),
            }
            if cell.underline {
                canvas.fill_rect(
                    px,
                    py + m.underline_offset as i32,
                    m.width * span,
                    m.underline_thickness,
                    cell.fg,
                );
            }
            if cell.strike {
                canvas.fill_rect(
                    px,
                    py + m.strike_offset as i32,
                    m.width * span,
                    m.strike_thickness,
                    cell.fg,
                );
            }
        }
    }

    // 3) 커서와 조합 중 글
    if let Some(cur) = view.cursor {
        if text_rows.contains(&cur.row) && cur.row < view.rows && cur.col < view.cols {
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

/// 한 줄에서 합자로 바꿀 글리프 번호(칸마다, 0은 그대로, #96).
/// 같은 꼴이고 기본 글꼴에 있는 한 칸 글자가 이어진 묶음만 다듬는다. 공백·넓은 글자·이모지·커서 칸에서 끊는다
/// (커서 밑의 합자는 풀어 보인다. Ghostty와 같다). 공백에서 끊으므로 같은 낱말은 캐시에서 꺼낸다
fn ligatures(fonts: &mut Fonts, row: &[DrawCell], cursor_col: Option<usize>) -> Vec<u16> {
    let mut subs = vec![0u16; row.len()];
    let mut text = String::new();
    let mut col = 0;
    while col < row.len() {
        let style = FontStyle::from_flags(row[col].bold, row[col].italic);
        let joins = |c: usize| {
            let cell = &row[c];
            !cell.spacer
                && !cell.wide
                && !cell.emoji
                && cell.cluster.is_none()
                && cell.ch > ' '
                // 상자 그림·블록은 직접 그린다
                && !matches!(cell.ch as u32, 0x2500..=0x259F)
                && Some(c) != cursor_col
                && FontStyle::from_flags(cell.bold, cell.italic) == style
                && fonts.primary_has(cell.ch, style)
        };
        let start = col;
        while col < row.len() && joins(col) {
            col += 1;
        }
        if col - start >= 2 {
            text.clear();
            text.extend(row[start..col].iter().map(|c| c.ch));
            let shaped = fonts.shape_run(&text, style);
            subs[start..col].copy_from_slice(&shaped);
        }
        if col == start {
            col += 1;
        }
    }
    subs
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
    if crate::boxdraw::draw(canvas, c, px, py, m.width * span, m.height, m.box_thickness, fg) {
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
    if g.color.is_some() {
        canvas.draw_color_glyph(px + offset, py + m.baseline as i32, &g);
    } else {
        canvas.draw_glyph(px + offset, py + m.baseline as i32, &g, fg);
    }
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
    // 불투명도(#135): 1이면 칠하고, 아니면 칸 위에 섞는다
    let alpha = (cur.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let paint = |canvas: &mut Canvas, x: i32, y: i32, rw: u32, rh: u32| {
        if alpha == 255 {
            canvas.fill_rect(x, y, rw, rh, cur.color);
        } else {
            canvas.blend_rect(x, y, rw, rh, cur.color, alpha);
        }
    };
    // 커서 높이(`adjust-cursor-height`)는 셀 가운데에 둔다. 아래로 가는 선(밑줄 커서)은 커서 아래에 붙는다
    let ch = m.cursor_height.min(m.height);
    let cy = py + ((m.height - ch) / 2) as i32;
    match cur.style {
        CursorStyle::Block => {
            paint(canvas, px, cy, w, ch);
            // 반투명이면 커서 위 글자도 칸 글자색과 커서 글자색 사이로
            let text = if alpha == 255 {
                cur.text_color
            } else {
                cell.fg.mix(cur.text_color, cur.opacity)
            };
            draw_cell_text(canvas, fonts, m, px, py, span, cell.ch, text, cell.bold, cell.italic);
        }
        CursorStyle::Hollow => {
            let t = m.cursor_hollow.min(ch / 2).max(1);
            paint(canvas, px, cy, w, t);
            paint(canvas, px, cy + ch as i32 - t as i32, w, t);
            paint(canvas, px, cy + t as i32, t, ch.saturating_sub(2 * t));
            paint(
                canvas,
                px + w as i32 - t as i32,
                cy + t as i32,
                t,
                ch.saturating_sub(2 * t),
            );
        }
        CursorStyle::Beam => paint(canvas, px, cy, m.cursor_beam.min(w), ch),
        CursorStyle::Underline => {
            let t = (m.stroke * 2).min(ch);
            paint(canvas, px, cy + ch as i32 - t as i32, w, t)
        }
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
