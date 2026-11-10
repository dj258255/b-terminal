//! `GridView`를 GPU가 그릴 인스턴스 목록으로 바꾼다(GPU를 부르지 않는다).
//!
//! 그리는 순서(CPU `draw_grid`와 같다): 셀 배경 → 윗줄·취소선 → 글자 → 커서 → 상자 커서 안의 글자.
//! 사각형과 글리프는 파이프라인이 달라 목록이 둘이고, `*_pre`는 글자 앞, 나머지는 글자 뒤에 그린다

use bt_core::Rgb;
use bt_render::font::CellMetrics;
use bt_render::{CursorStyle, DrawCell, DrawCursor, FontStyle, Fonts, GridView, lock_rects};

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
}

fn rgba(c: Rgb, a: u8) -> [u8; 4] {
    [c.r, c.g, c.b, a]
}

impl Scene {
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

    /// 격자 하나를 목록으로 만든다. `clear`와 같은 바탕색 칸은 그리지 않는다(결과가 같다)
    pub fn build(&mut self, cache: &mut GlyphCache, fonts: &mut Fonts, view: &GridView, clear: Rgb) {
        self.clear();
        let m = fonts.metrics();
        cache.check_metrics(m);
        let (cw, ch) = (m.width as i32, m.height as i32);

        // 1) 바탕
        for row in 0..view.rows {
            for col in 0..view.cols {
                let cell = &view.cells[row * view.cols + col];
                if cell.bg != clear {
                    self.rect(col as i32 * cw, row as i32 * ch, m.width, m.height, cell.bg, 255);
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

        // 3) 글자
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
                    None => self.cell_text(cache, fonts, m, px, py, span, cell, cell.fg),
                }
            }
        }
        self.glyphs_pre = self.glyphs.len();

        // 4) 커서
        if let Some(cur) = view.cursor {
            if cur.row < view.rows && cur.col < view.cols {
                let px = cur.col as i32 * cw;
                let py = cur.row as i32 * ch;
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

    /// 칸의 글자 하나(`draw_cell_text`에서 상자 글자를 뺀 것). 넓은 글자나 칸보다 넓은 글리프는 칸 가운데에 둔다
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
        let c = cell.ch;
        if c == ' ' || c == '\0' {
            return;
        }
        let style = FontStyle::from_flags(cell.bold, cell.italic);
        let Some(e) = cache.get(Key::Char(c, style), || fonts.glyph(c, style)) else {
            return;
        };
        let slot = (m.width * span) as i32;
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
