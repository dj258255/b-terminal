//! 터미널 화면(`bt_term::Screen`과 같은 꼴의 값)을 픽셀로 그린다

use bt_core::Rgb;
use bt_core::config::CellColor;

use crate::canvas::Canvas;
use crate::font::{CellMetrics, FontStyle, Fonts};
use crate::underline::Underline;

/// 렌더러가 그리는 셀 한 칸. bt-term에 기대지 않으려고 따로 둔다(앱이 옮겨 담는다)
#[derive(Debug, Clone, PartialEq)]
pub struct DrawCell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    pub strike: bool,
    pub wide: bool,
    pub spacer: bool,
    pub selected: bool,
    /// 이모지 모양으로 그린다(뒤에 U+FE0F가 붙은 글자, #67). 칸 폭 안에 색 글리프로
    pub emoji: bool,
    /// 이모지 묶음의 첫 칸이면 묶음 글과 차지한 칸 수(#87). 한 글리프로 다듬어지면 그 칸들 가운데에 그린다
    pub cluster: Option<std::sync::Arc<str>>,
    pub cluster_cells: u16,
    /// 윗줄(SGR 53, #277). 글자색 줄을 칸 맨 위에 긋는다
    pub overline: bool,
    /// 밑줄 색(SGR 58). 없으면 글자색(#276)
    pub underline_color: Option<Rgb>,
    /// 숨김(SGR 8). 바탕만 칠하고 글자·밑줄·윗줄·취소선은 그리지 않는다(고름·찾기·최소 명암비와 상관없이, #834)
    pub hidden: bool,
    /// 흐림(SGR 2)이면 `faint-opacity`. 글자색은 반전·고름·찾기·최소 명암비를 정한 마지막에 이 알파로 바탕에 섞는다(#835)
    pub faint: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Beam,
    Underline,
    Hollow,
    /// 암호 입력 중의 자물쇠(Ghostty `.lock`, #258)
    Lock,
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
    /// 고른 칸의 바탕·글자색을 그 칸의 색에서 가져온다(Ghostty `selection-* = cell-foreground`·`cell-background`, #849). 없으면 `selection_bg`·`selection_fg`
    pub selection_bg_cell: Option<CellColor>,
    pub selection_fg_cell: Option<CellColor>,
    /// IME 조합 중인 글(커서 자리에 덮어 그린다, PTY에는 보내지 않는다)
    pub preedit: Option<&'a str>,
    /// 글자색과 바탕의 최소 명암비(Ghostty `minimum-contrast`, #123). 1이면 끔
    pub min_contrast: f32,
}

impl GridView<'_> {
    /// 고른 칸의 바탕색. `selection-background = cell-foreground`면 칸의 글자색, `cell-background`면 칸의 바탕색(#849).
    /// `cell.fg`·`cell.bg`는 반전(SGR 7)을 이미 반영한 보이는 색이라 Ghostty `generic.zig`의 `inverse` 갈림과 같은 값이다
    pub fn selected_bg(&self, cell: &DrawCell) -> Rgb {
        match self.selection_bg_cell {
            Some(CellColor::Foreground) => cell.fg,
            Some(CellColor::Background) => cell.bg,
            None => self.selection_bg,
        }
    }

    /// 고른 칸의 글자색(설정이 없으면 `selection_fg`, 그것도 없으면 칸의 글자색)
    pub fn selected_fg(&self, cell: &DrawCell) -> Rgb {
        match self.selection_fg_cell {
            Some(CellColor::Foreground) => cell.fg,
            Some(CellColor::Background) => cell.bg,
            None => self.selection_fg.unwrap_or(cell.fg),
        }
    }
}

/// 격자 아래에 끼워 그릴 것: kitty 그림의 "칸 바탕 아래"와 "글자 아래" 층(#547, Ghostty `ImageLayer`).
/// 그리는 차례는 칸 바탕 아래 그림 → 칸 바탕 → 글자 아래 그림 → 글자(→ 글자 위 그림은 부른 쪽이 격자 뒤에 그린다)
#[derive(Default)]
pub struct Under<'a> {
    /// 칸 바탕을 칠하기 전에 그린다. 있으면 `default_bg` 바탕의 칸은 칠하지 않는다(Ghostty는 기본 바탕 칸을 그리지 않는다): 그림이 비친다
    pub below_bg: Option<&'a dyn Fn(&mut Canvas)>,
    /// 칸 바탕을 칠한 뒤 글자보다 먼저 그린다
    pub below_text: Option<&'a dyn Fn(&mut Canvas)>,
    /// 화면의 기본 바탕색(`below_bg`가 있을 때 이 바탕 칸은 칠하지 않는다)
    pub default_bg: Option<Rgb>,
}

/// 격자를 (x, y)에 통째로 그린다
pub fn draw_grid(canvas: &mut Canvas, fonts: &mut Fonts, x: i32, y: i32, view: &GridView) {
    draw_grid_under(canvas, fonts, x, y, view, &Under::default());
}

/// `draw_grid`에 칸 바탕 아래·글자 아래 그림을 끼운다
pub fn draw_grid_under(canvas: &mut Canvas, fonts: &mut Fonts, x: i32, y: i32, view: &GridView, under: &Under) {
    let all = vec![true; view.rows];
    draw_rows_dirty(canvas, fonts, x, y, view, &all, under);
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
    draw_rows_dirty(canvas, fonts, x, y, view, dirty, &Under::default())
}

fn draw_rows_dirty(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    x: i32,
    y: i32,
    view: &GridView,
    dirty: &[bool],
    under: &Under,
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
        draw_rows(canvas, fonts, m, x, y, view, (a, r), under);
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
    (a, b): (usize, usize),
    under: &Under,
) {
    let cw = m.width as i32;
    let ch = m.height as i32;
    // 0) 칸 바탕 아래 그림(kitty z < i32::MIN / 2, #547)
    if let Some(f) = under.below_bg {
        f(canvas);
    }
    // 1) 바탕을 먼저 칠한다. 글자가 셀 위아래로 조금 넘쳐도 다음 줄 바탕에 잘리지 않게
    for row in a..b {
        let py = y + row as i32 * ch;
        for col in 0..view.cols {
            let cell = &view.cells[row * view.cols + col];
            let (cx, w, h) = (x + col as i32 * cw, m.width, m.height);
            if cell.selected {
                canvas.fill_rect(cx, py, w, h, view.selected_bg(cell));
            } else if under.below_bg.is_some() && under.default_bg == Some(cell.bg) {
                // 기본 바탕 칸은 그리지 않아 칸 바탕 아래 그림이 비친다
            } else {
                // background-opacity-cells(#177)를 켰으면 색 바탕 칸도 비친다
                canvas.fill_rect_cell(cx, py, w, h, cell.bg);
            }
        }
    }
    // 1.5) 글자 아래 그림(kitty -2^30 <= z < 0): 칸 바탕 위, 글자 아래
    if let Some(f) = under.below_text {
        f(canvas);
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
            // 숨김(SGR 8)은 바탕만 칠하고 글자·밑줄·윗줄·취소선을 그리지 않는다(xterm·Ghostty와 같다, #834)
            if cell.hidden {
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
            let fg = text_color(view, cell, &mut contrast_memo);
            let deco = decoration_color(view, cell);
            // 밑줄은 글자보다 먼저 그려 글자 밑에 깐다(Ghostty와 같다. 색 밑줄이 내려가는 획과 겹쳐도 글자가 읽힌다, #276)
            if cell.underline != Underline::None {
                let color = cell.underline_color.unwrap_or(deco);
                // 넓은 글자는 칸마다 따로 긋는다(Ghostty도 칸마다 밑줄 조각을 얹는다)
                for k in 0..span as i32 {
                    crate::underline::draw(
                        canvas,
                        cell.underline,
                        px + k * cw,
                        py,
                        m.width,
                        m.height,
                        m.underline_offset,
                        m.underline_thickness,
                        color,
                    );
                }
            }
            // 윗줄(#277): 글자 앞에 글자색으로 긋는다(Ghostty generic.zig 3277줄, 밑줄 → 윗줄 → 글자 순서)
            if cell.overline {
                canvas.fill_rect(
                    px,
                    py + m.overline_offset as i32,
                    m.width * span,
                    m.overline_thickness,
                    deco,
                );
            }
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
            if cell.strike {
                canvas.fill_rect(
                    px,
                    py + m.strike_offset as i32,
                    m.width * span,
                    m.strike_thickness,
                    deco,
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

/// 칸의 글자색. 차례는 Ghostty와 같다: 고른 칸이면 `selection_fg`, 최소 명암비(#123)를 켰으면 바탕과 비슷한 색을
/// 흰색이나 검은색으로(그래픽 글자는 뺀다, #839), 마지막으로 흐린 칸(SGR 2)이면 `faint-opacity`를 건다(#835).
/// 명암비는 알파를 걸기 전의 색으로 재고, 명암비가 색을 바꿨으면 흰색·검은색은 알파 없이 그린다
/// (Ghostty 셰이더 `contrasted_color`가 알파 1짜리 색을 돌려준다).
/// 이어지는 칸은 같은 (글자색, 바탕) 쌍이 많아 앞 결과(`memo`)를 쓴다. GPU 렌더러도 이 함수를 쓴다(규칙은 한 벌)
pub fn text_color(view: &GridView, cell: &DrawCell, memo: &mut Option<((Rgb, Rgb), Rgb)>) -> Rgb {
    let fg = if cell.selected { view.selected_fg(cell) } else { cell.fg };
    let bg = if cell.selected { view.selected_bg(cell) } else { cell.bg };
    let contrast = view.min_contrast > 1.0 && !no_min_contrast(cell.ch);
    let adjusted = if contrast {
        match *memo {
            Some((pair, out)) if pair == (fg, bg) => out,
            _ => {
                let out = fg.contrasted(bg, view.min_contrast);
                *memo = Some(((fg, bg), out));
                out
            }
        }
    } else {
        fg
    };
    match cell.faint {
        Some(opacity) if !(contrast && fg.contrast(bg) < view.min_contrast) => adjusted.mix(bg, 1.0 - opacity),
        _ => adjusted,
    }
}

/// 밑줄(밑줄 색이 없을 때)·윗줄·취소선의 색: 칸의 글자색에 흐림만 건다. 고름 글자색과 최소 명암비는 걸지 않는다(지금까지와 같다)
pub fn decoration_color(view: &GridView, cell: &DrawCell) -> Rgb {
    match cell.faint {
        Some(opacity) => cell.fg.mix(
            if cell.selected { view.selected_bg(cell) } else { cell.bg },
            1.0 - opacity,
        ),
        None => cell.fg,
    }
}

/// 최소 명암비를 걸지 않을 글자(Ghostty `renderer/cell.zig` `noMinContrast`·`isGraphicsElement`):
/// 상자 그림(U+2500~257F), 블록(U+2580~259F), 레거시 컴퓨팅 기호(U+1FB00~1FBFF, U+1CC00~1CEBF), 파워라인(U+E0B0~E0D7)
pub fn no_min_contrast(ch: char) -> bool {
    matches!(ch as u32, 0x2500..=0x259F | 0x1FB00..=0x1FBFF | 0x1CC00..=0x1CEBF | 0xE0B0..=0xE0D7)
}

/// 어느 글꼴에도 없는 글자를 칸 `(px, py, w, h)`에 빈 상자로 그린다
pub(crate) fn stroke_missing_glyph(canvas: &mut Canvas, px: i32, py: i32, w: u32, h: u32, color: Rgb) {
    canvas.stroke_rect(px + 1, py + 2, w - 2, h - 4, 1, color);
}

/// 한 줄에서 합자로 바꿀 글리프 번호(칸마다, 0은 그대로, #96).
/// 같은 꼴이고 기본 글꼴에 있는 한 칸 글자가 이어진 묶음만 다듬는다. 공백·넓은 글자·이모지·커서 칸에서 끊는다
/// (커서 밑의 합자는 풀어 보인다. Ghostty와 같다). 공백에서 끊으므로 같은 낱말은 캐시에서 꺼낸다.
/// GPU 렌더러도 이 함수를 쓴다(묶음 규칙은 한 벌, #359)
pub fn ligatures(fonts: &mut Fonts, row: &[DrawCell], cursor_col: Option<usize>) -> Vec<u16> {
    let mut subs = vec![0u16; row.len()];
    // `font-shaping-break`에서 `cursor`를 끄면 커서 칸도 묶음에 넣는다(#174)
    let cursor_col = cursor_col.filter(|_| fonts.breaks_shaping_at_cursor());
    let mut text = String::new();
    let mut col = 0;
    while col < row.len() {
        let style = FontStyle::from_flags(row[col].bold, row[col].italic);
        let joins = |c: usize| {
            let cell = &row[c];
            !cell.spacer
                && !cell.wide
                && !cell.emoji
                && !cell.hidden
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
        stroke_missing_glyph(canvas, px, py, m.width * span, m.height, fg);
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
            // 반투명이면 커서 위 글자도 칸 글자색과 커서 글자색 사이로. 숨김 칸은 글자가 없다(#834)
            let text = if alpha == 255 {
                cur.text_color
            } else {
                cell.fg.mix(cur.text_color, cur.opacity)
            };
            if !cell.hidden {
                draw_cell_text(canvas, fonts, m, px, py, span, cell.ch, text, cell.bold, cell.italic);
            }
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
        CursorStyle::Lock => {
            for (x, y, rw, rh) in lock_rects(px, cy, w, ch) {
                paint(canvas, x, y, rw, rh);
            }
        }
        CursorStyle::Beam => paint(canvas, px, cy, m.cursor_beam.min(w), ch),
        CursorStyle::Underline => {
            let t = (m.stroke * 2).min(ch);
            paint(canvas, px, cy + ch as i32 - t as i32, w, t)
        }
    }
}

/// 자물쇠를 이루는 네모들(x, y, 폭, 높이). 칸 `(x, y, w, h)` 안에 든다: 아래쪽 몸통과 그 위의 ㄇ자 고리(#258)
pub fn lock_rects(x: i32, y: i32, w: u32, h: u32) -> Vec<(i32, i32, u32, u32)> {
    // 몸통은 아래 55%, 고리는 그 위. 좁은 칸에서도 보이게 가장자리를 칸 폭의 1/8만 비운다
    let margin = (w / 8).max(1);
    let bw = w.saturating_sub(2 * margin).max(1);
    let bx = x + ((w - bw) / 2) as i32;
    let body_h = (h * 11 / 20).max(2).min(h);
    let by = y + (h - body_h) as i32;
    let top = y + (h / 10) as i32;
    let t = (bw / 5).max(1);
    let sw = (bw * 3 / 5).max(t * 2 + 1).min(bw);
    let sx = x + ((w - sw) / 2) as i32;
    let rise = (by - top).max(1) as u32;
    let mut v = vec![(bx, by, bw, body_h)];
    if rise > t {
        v.push((sx, top, sw, t));
        v.push((sx, top + t as i32, t, rise - t));
        v.push((sx + sw as i32 - t as i32, top + t as i32, t, rise - t));
    } else {
        v.push((sx, top, sw, rise));
        v.push((sx, top, t, rise));
        v.push((sx + sw as i32 - t as i32, top, t, rise));
    }
    v
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

#[cfg(test)]
mod lock_tests {
    use super::lock_rects;

    #[test]
    fn lock_has_a_body_and_a_shackle_inside_the_box() {
        for (w, h) in [(8u32, 17u32), (16, 34), (10, 20), (4, 8), (30, 18)] {
            let (x, y) = (100, 50);
            let r = lock_rects(x, y, w, h);
            assert!(r.len() >= 4, "{w}x{h}: 몸통 하나와 고리 셋");
            for &(rx, ry, rw, rh) in &r {
                assert!(rw > 0 && rh > 0, "{w}x{h}: 빈 네모 {rx},{ry},{rw},{rh}");
                assert!(rx >= x && ry >= y, "{w}x{h}: 칸 밖(왼쪽·위)");
                assert!(rx + rw as i32 <= x + w as i32, "{w}x{h}: 칸 밖(오른쪽)");
                assert!(ry + rh as i32 <= y + h as i32, "{w}x{h}: 칸 밖(아래)");
            }
            // 몸통은 가장 아래에 있고 고리보다 넓다
            let body = r.iter().max_by_key(|b| b.1 + b.3 as i32).unwrap();
            let top = r.iter().map(|b| b.1).min().unwrap();
            assert!(body.1 > top, "{w}x{h}: 고리가 몸통 위에 있다");
            assert!(body.2 >= r.iter().filter(|b| *b != body).map(|b| b.2).max().unwrap());
        }
    }
}
