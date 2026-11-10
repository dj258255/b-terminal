//! `GridView`를 GPU가 그릴 인스턴스 목록으로 바꾼다(GPU를 부르지 않는다).
//!
//! 그리는 순서(CPU `draw_grid`와 같다): 셀 배경 → 윗줄·취소선 → (밑줄 → 글자) → 커서나 조합 중 글 → 상자 커서 안의 글자.
//! 사각형과 글리프는 파이프라인이 달라 목록이 둘이고, `*_pre`는 글자 앞, 나머지는 글자 뒤에 그린다.
//!
//! 밑줄·상자 글자·빈 상자는 CPU가 칸에 그리는 모양을 칸 크기 마스크(`bt_render::sprite`)로 받아 아틀라스에 넣고
//! 글리프(kind 0)로 그린다(Ghostty가 sprite를 글자와 같은 인스턴스로 그리는 것과 같다).
//!
//! 투명 창: `set_see_through`·`set_cells_alpha`를 켜면 칸 바탕이 곱한 알파로 나가고, 렌더러는 바탕을 투명으로 지운 뒤
//! 칸이 화면을 모두 덮는다(그 모드에서 `build`의 `clear`는 쓰지 않는다). 결과 픽셀은 CPU `Canvas`와 같은 곱한 알파 ARGB다

use bt_core::Rgb;
use bt_render::font::CellMetrics;
use bt_render::{
    CursorStyle, DrawCell, DrawCursor, FontStyle, Fonts, GridView, Underline, char_cells, lock_rects, sprite,
    text_color,
};

use crate::glyphs::{Entry, GlyphCache, Key};

/// 단색 사각형 하나(픽셀 좌표)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectInst {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [u8; 4],
}

/// 글리프 하나. `atlas`는 아틀라스 안의 왼쪽 위 텍셀, `kind` 0은 회색조·1은 색
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphInst {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub atlas: [f32; 2],
    pub color: [u8; 4],
    pub kind: u32,
}

pub const RECT_STRIDE: usize = 20;
pub const GLYPH_STRIDE: usize = 32;

impl RectInst {
    pub fn write(&self, out: &mut Vec<u8>) {
        for v in self.pos.iter().chain(&self.size) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.color);
    }
}

impl GlyphInst {
    pub fn write(&self, out: &mut Vec<u8>) {
        for v in self.pos.iter().chain(&self.size).chain(&self.atlas) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.color);
        out.extend_from_slice(&self.kind.to_le_bytes());
    }
}

#[derive(Default)]
pub struct Scene {
    pub rects: Vec<RectInst>,
    /// `rects[..rects_pre]`는 글자 앞(배경), 나머지는 글자 뒤(커서)
    pub rects_pre: usize,
    pub glyphs: Vec<GlyphInst>,
    /// `glyphs[..glyphs_pre]`는 칸의 글자, 나머지는 상자 커서 안의 글자
    pub glyphs_pre: usize,
    /// 투명 창(CPU `Canvas::set_see_through`): (기본 바탕색, 알파). 그 색 바탕은 이 알파로 그린다
    see_through: Option<(Rgb, u8)>,
    /// `background-opacity-cells`(CPU `Canvas::set_cells_alpha`): 색 바탕 칸의 알파
    cells_alpha: Option<u8>,
}

fn rgba(c: Rgb, a: u8) -> [u8; 4] {
    [c.r, c.g, c.b, a]
}

impl Scene {
    /// 투명 창: 앞으로 `bg` 바탕은 `alpha`만큼만 덮는다(CPU `Canvas::set_see_through`와 같다)
    pub fn set_see_through(&mut self, bg: Rgb, alpha: u8) {
        self.see_through = Some((bg, alpha));
    }

    /// 투명 창을 끈다
    pub fn clear_see_through(&mut self) {
        self.see_through = None;
    }

    /// 색 바탕 칸의 알파(CPU `Canvas::set_cells_alpha`와 같다). 없으면 불투명
    pub fn set_cells_alpha(&mut self, alpha: Option<u8>) {
        self.cells_alpha = alpha;
    }

    /// 바탕에 알파가 있어 렌더러가 투명하게 지우고 모든 칸을 그려야 하는가
    pub fn is_transparent(&self) -> bool {
        self.see_through.is_some() || self.cells_alpha.is_some()
    }

    /// CPU `Canvas::solid`: 투명 창의 기본 바탕색이면 그 알파, 아니면 불투명
    fn solid_alpha(&self, c: Rgb) -> u8 {
        match self.see_through {
            Some((bg, a)) if bg == c => a,
            _ => 255,
        }
    }

    /// CPU `Canvas::fill_rect_cell`: 색 바탕 칸은 `cells_alpha`, 기본 바탕색은 `solid_alpha`
    fn cell_alpha(&self, c: Rgb) -> u8 {
        match self.cells_alpha {
            Some(a) if self.see_through.is_none_or(|(bg, _)| bg != c) => a,
            _ => self.solid_alpha(c),
        }
    }

    pub fn clear(&mut self) {
        self.rects.clear();
        self.glyphs.clear();
        self.rects_pre = 0;
        self.glyphs_pre = 0;
    }

    fn rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb, alpha: u8) {
        if w == 0 || h == 0 {
            return;
        }
        self.rects.push(RectInst {
            pos: [x as f32, y as f32],
            size: [w as f32, h as f32],
            color: rgba(color, alpha),
        });
    }

    /// 글리프를 기준선 자리 `(x, baseline_y)`에 놓는다(`Canvas::draw_glyph`와 같은 자리 계산)
    fn glyph(&mut self, x: i32, baseline_y: i32, e: Entry, fg: Rgb) {
        if e.w == 0 || e.h == 0 {
            return;
        }
        self.glyphs.push(GlyphInst {
            pos: [(x + e.left) as f32, (baseline_y - e.top) as f32],
            size: [e.w as f32, e.h as f32],
            atlas: [e.x as f32, e.y as f32],
            color: rgba(fg, 255),
            kind: e.color as u32,
        });
    }

    /// 마스크 글리프를 칸 왼쪽 위 `(x, y)`에 놓는다(글리프와 달리 기준선을 거치지 않는다)
    fn sprite(&mut self, x: i32, y: i32, e: Entry, color: Rgb) {
        if e.w == 0 || e.h == 0 {
            return;
        }
        self.glyphs.push(GlyphInst {
            pos: [x as f32, y as f32],
            size: [e.w as f32, e.h as f32],
            atlas: [e.x as f32, e.y as f32],
            color: rgba(color, 255),
            kind: 0,
        });
    }

    /// 격자 하나를 목록으로 만든다. `clear`와 같은 바탕색 칸은 그리지 않는다(결과가 같다)
    pub fn build(&mut self, cache: &mut GlyphCache, fonts: &mut Fonts, view: &GridView, clear: Rgb) {
        self.clear();
        let m = fonts.metrics();
        cache.check_metrics(m);
        let (cw, ch) = (m.width as i32, m.height as i32);

        // 1) 바탕. 투명 창이면 렌더러가 투명으로 지우므로 기본 바탕 칸도 알파로 그린다(칸이 화면을 덮는다)
        let transparent = self.is_transparent();
        for row in 0..view.rows {
            for col in 0..view.cols {
                let cell = &view.cells[row * view.cols + col];
                let (color, alpha) = if cell.selected {
                    (view.selection_bg, self.solid_alpha(view.selection_bg))
                } else {
                    (cell.bg, self.cell_alpha(cell.bg))
                };
                if transparent || color != clear {
                    self.rect(col as i32 * cw, row as i32 * ch, m.width, m.height, color, alpha);
                }
            }
        }
        // 2) 윗줄·취소선(글자색 줄)
        for row in 0..view.rows {
            let py = row as i32 * ch;
            for col in 0..view.cols {
                let cell = &view.cells[row * view.cols + col];
                if cell.spacer || !(cell.overline || cell.strike) {
                    continue;
                }
                let span = if cell.wide { 2 } else { 1 };
                let px = col as i32 * cw;
                if cell.overline {
                    self.rect(
                        px,
                        py + m.overline_offset as i32,
                        m.width * span,
                        m.overline_thickness,
                        cell.fg,
                        255,
                    );
                }
                if cell.strike {
                    self.rect(
                        px,
                        py + m.strike_offset as i32,
                        m.width * span,
                        m.strike_thickness,
                        cell.fg,
                        255,
                    );
                }
            }
        }
        self.rects_pre = self.rects.len();

        // 3) 글자와 밑줄
        let mut contrast_memo: Option<((Rgb, Rgb), Rgb)> = None;
        for row in 0..view.rows {
            let py = row as i32 * ch;
            let mut skip_until = 0;
            for col in 0..view.cols {
                let cell = &view.cells[row * view.cols + col];
                if cell.spacer || col < skip_until {
                    continue;
                }
                let px = col as i32 * cw;
                if let Some(text) = &cell.cluster {
                    let span = cell.cluster_cells.max(1) as i32;
                    let e = cache.get(Key::Cluster(text.clone()), || fonts.cluster_glyph(text));
                    if let Some(e) = e {
                        let ox = (m.width as i32 * span - e.w as i32) / 2;
                        self.glyph(px + ox, py + m.baseline as i32, e, Rgb::new(255, 255, 255));
                        skip_until = col + span as usize;
                        continue;
                    }
                }
                let span = if cell.wide { 2 } else { 1 };
                let fg = text_color(view, cell, &mut contrast_memo);
                // 밑줄은 글자 밑에 깐다. 넓은 글자는 칸마다 따로 긋는다(CPU와 같다)
                if cell.underline != Underline::None {
                    let color = cell.underline_color.unwrap_or(cell.fg);
                    let e = cache.get_mask(
                        Key::Underline(
                            cell.underline as u8,
                            m.width,
                            m.height,
                            m.underline_offset,
                            m.underline_thickness,
                        ),
                        m.width,
                        m.height,
                        || {
                            sprite::underline_mask(
                                cell.underline,
                                m.width,
                                m.height,
                                m.underline_offset,
                                m.underline_thickness,
                            )
                        },
                    );
                    if let Some(e) = e {
                        for k in 0..span as i32 {
                            self.sprite(px + k * cw, py, e, color);
                        }
                    }
                }
                let emoji = if cell.emoji {
                    cache.get(Key::Emoji(cell.ch, span), || fonts.emoji_glyph(cell.ch, span))
                } else {
                    None
                };
                match emoji {
                    Some(e) => {
                        let ox = (m.width as i32 * span as i32 - e.w as i32) / 2;
                        self.glyph(px + ox, py + m.baseline as i32, e, cell.fg);
                    }
                    None => self.cell_text(cache, fonts, m, px, py, span, cell, fg),
                }
            }
        }
        self.glyphs_pre = self.glyphs.len();

        // 4) 커서와 조합 중 글
        if let Some(cur) = view.cursor {
            if cur.row < view.rows && cur.col < view.cols {
                let px = cur.col as i32 * cw;
                let py = cur.row as i32 * ch;
                if let Some(pre) = view.preedit.filter(|p| !p.is_empty()) {
                    self.preedit(cache, fonts, m, px, py, pre, cur.color, view.selection_bg);
                } else {
                    self.cursor(
                        cache,
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

    /// 조합 중 글(CPU `draw_preedit`): 글자마다 `selection_bg` 칸, 글자, 아래 줄(커서색)
    #[allow(clippy::too_many_arguments)]
    fn preedit(
        &mut self,
        cache: &mut GlyphCache,
        fonts: &mut Fonts,
        m: CellMetrics,
        px: i32,
        py: i32,
        text: &str,
        color: Rgb,
        bg: Rgb,
    ) {
        let mut x = px;
        for c in text.chars() {
            let span = char_cells(c) as u32;
            let w = m.width * span;
            self.rect(x, py, w, m.height, bg, self.solid_alpha(bg));
            self.text_char(cache, fonts, m, x, py, span, c, color, false, false);
            // 아래 줄은 글자 위에 얹는다(글자와 같은 층의 단색 마스크)
            if let Some(e) = cache.get_mask(Key::Solid(w, m.stroke), w, m.stroke, || sprite::solid_mask(w, m.stroke)) {
                self.sprite(x, py + m.height as i32 - m.stroke as i32, e, color);
            }
            x += w as i32;
        }
    }

    /// 칸의 글자 하나(`draw_cell_text`와 같다)
    #[allow(clippy::too_many_arguments)]
    fn cell_text(
        &mut self,
        cache: &mut GlyphCache,
        fonts: &mut Fonts,
        m: CellMetrics,
        px: i32,
        py: i32,
        span: u32,
        cell: &DrawCell,
        fg: Rgb,
    ) {
        self.text_char(cache, fonts, m, px, py, span, cell.ch, fg, cell.bold, cell.italic);
    }

    /// 글자 `c` 하나를 칸 `(px, py)`에: 상자 글자는 직접 그린 마스크로, 글리프가 없으면 빈 상자로.
    /// 넓은 글자나 칸보다 넓은 글리프는 칸 가운데에 둔다
    #[allow(clippy::too_many_arguments)]
    fn text_char(
        &mut self,
        cache: &mut GlyphCache,
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
        let w = m.width * span;
        if sprite::may_be_box_char(c) {
            let e = cache.get_mask(Key::BoxChar(c, w, m.height, m.box_thickness), w, m.height, || {
                sprite::box_mask(c, w, m.height, m.box_thickness)
            });
            if let Some(e) = e {
                self.sprite(px, py, e, fg);
                return;
            }
        }
        let style = FontStyle::from_flags(bold, italic);
        let Some(e) = cache.get(Key::Char(c, style), || fonts.glyph(c, style)) else {
            // 어느 글꼴에도 없는 글자: 빈 상자
            let e = cache.get_mask(Key::Missing(w, m.height), w, m.height, || {
                sprite::missing_glyph_mask(w, m.height)
            });
            if let Some(e) = e {
                self.sprite(px, py, e, fg);
            }
            return;
        };
        let slot = w as i32;
        let offset = if e.w as i32 > slot - e.left || span > 1 {
            (slot - e.w as i32) / 2 - e.left
        } else {
            0
        };
        // 색 글리프는 자기 색을 쓴다(셰이더가 kind로 가른다)
        self.glyph(px + offset, py + m.baseline as i32, e, fg);
    }

    #[allow(clippy::too_many_arguments)]
    fn cursor(
        &mut self,
        cache: &mut GlyphCache,
        fonts: &mut Fonts,
        m: CellMetrics,
        px: i32,
        py: i32,
        cur: &DrawCursor,
        cell: &DrawCell,
    ) {
        let span = if cur.wide { 2 } else { 1 };
        let w = m.width * span;
        let alpha = (cur.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let ch = m.cursor_height.min(m.height);
        let cy = py + ((m.height - ch) / 2) as i32;
        let paint = |s: &mut Scene, x: i32, y: i32, rw: u32, rh: u32| s.rect(x, y, rw, rh, cur.color, alpha);
        match cur.style {
            CursorStyle::Block => {
                paint(self, px, cy, w, ch);
                let text = if alpha == 255 {
                    cur.text_color
                } else {
                    cell.fg.mix(cur.text_color, cur.opacity)
                };
                self.cell_text(cache, fonts, m, px, py, span, cell, text);
            }
            CursorStyle::Hollow => {
                let t = m.cursor_hollow.min(ch / 2).max(1);
                paint(self, px, cy, w, t);
                paint(self, px, cy + ch as i32 - t as i32, w, t);
                paint(self, px, cy + t as i32, t, ch.saturating_sub(2 * t));
                paint(
                    self,
                    px + w as i32 - t as i32,
                    cy + t as i32,
                    t,
                    ch.saturating_sub(2 * t),
                );
            }
            CursorStyle::Lock => {
                for (x, y, rw, rh) in lock_rects(px, cy, w, ch) {
                    paint(self, x, y, rw, rh);
                }
            }
            CursorStyle::Beam => paint(self, px, cy, m.cursor_beam.min(w), ch),
            CursorStyle::Underline => {
                let t = (m.stroke * 2).min(ch);
                paint(self, px, cy + ch as i32 - t as i32, w, t)
            }
        }
    }
}
