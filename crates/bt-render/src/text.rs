//! 사이드바·상태 줄처럼 격자 밖의 글을 그린다

use bt_core::Rgb;
use unicode_width::UnicodeWidthChar;

use crate::canvas::Canvas;
use crate::font::{FontStyle, Fonts};

/// 동아시아 모호 폭 글자를 두 칸으로 세는지(`ambiguous-width = wide`, #857).
/// 격자의 폭은 엔진이 정하고 셀의 `WIDE` 표시를 따른다. 이 값은 셀이 없는 곳(IME 조합 글, 제목·상태 줄,
/// 넓은 글자 글꼴 배율)만 엔진과 같은 규칙으로 세게 한다
static AMBIGUOUS_WIDE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 모호 폭 설정을 바꾼다(앱이 설정을 읽을 때)
pub fn set_ambiguous_wide(on: bool) {
    AMBIGUOUS_WIDE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// 글자가 차지하는 칸 수(0·1·2). 제어 문자는 0
pub fn char_cells(c: char) -> usize {
    if AMBIGUOUS_WIDE.load(std::sync::atomic::Ordering::Relaxed) {
        c.width_cjk().unwrap_or(0)
    } else {
        c.width().unwrap_or(0)
    }
}

/// 글의 칸 수
pub fn text_cells(s: &str) -> usize {
    s.chars().map(char_cells).sum()
}

/// 칸 수 안에 들어가게 자른다. 잘렸으면 끝에 `…`
pub fn truncate(s: &str, max_cells: usize) -> String {
    if text_cells(s) <= max_cells {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_cells(c);
        if used + w + 1 > max_cells {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// 글을 (x, y: 줄 위쪽)에 그리고 그린 폭(픽셀)을 돌려준다. 폭은 셀 단위로 맞춘다
pub fn draw_text(canvas: &mut Canvas, fonts: &mut Fonts, x: i32, y: i32, s: &str, color: Rgb, bold: bool) -> i32 {
    let m = fonts.metrics();
    let mut px = x;
    for c in s.chars() {
        let span = char_cells(c) as u32;
        if span == 0 {
            continue;
        }
        if c != ' ' && !crate::boxdraw::draw(canvas, c, px, y, m.width * span, m.height, m.stroke, color) {
            if let Some(g) = fonts.glyph(c, FontStyle::from_flags(bold, false)) {
                let slot = (m.width * span) as i32;
                let offset = if span > 1 {
                    (slot - g.width as i32) / 2 - g.left
                } else {
                    0
                };
                // 색 글리프(이모지)는 bitmap이 비어 있다. 셀 격자처럼 RGBA로 그린다(#112)
                if g.color.is_some() {
                    canvas.draw_color_glyph(px + offset, y + m.baseline as i32, &g);
                } else {
                    canvas.draw_glyph(px + offset, y + m.baseline as i32, &g, color);
                }
            }
        }
        px += (m.width * span) as i32;
    }
    px - x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_counts_wide_chars_as_two() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("한글입니다", 6), "한글…");
        assert_eq!(text_cells("한a"), 3);
    }
}
