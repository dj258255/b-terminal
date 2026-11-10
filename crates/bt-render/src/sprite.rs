//! 칸 크기 회색조 마스크로 돌려주는 그리기(밑줄·상자 글자·빈 상자). GPU 렌더러가 이 모양을 아틀라스에 넣어
//! 글리프처럼 그린다. 모양은 CPU 렌더러가 칸에 직접 그리는 것과 같은 함수로 그려 한 벌만 둔다.
//!
//! 마스크는 `w × h` 바이트(줄 우선)이고 값이 덮음 정도(0~255)다. 글자색을 곱해 얹으면 CPU가 칸에 그린 것과 같다.

use bt_core::Rgb;

use crate::canvas::Canvas;
use crate::underline::{self, Underline};

/// 새 캔버스(투명)에 `draw`로 흰색을 그리고 파란 채널을 덮음 정도로 읽는다.
/// 투명 바탕에 흰색을 알파 `a`로 섞으면 파란 채널이 `a`가 된다
fn render(w: u32, h: u32, draw: impl FnOnce(&mut Canvas) -> bool) -> Option<Vec<u8>> {
    if w == 0 || h == 0 {
        return None;
    }
    let mut buf = vec![0u32; (w * h) as usize];
    let mut canvas = Canvas::new(&mut buf, w, h);
    if !draw(&mut canvas) {
        return None;
    }
    Some(buf.iter().map(|p| (*p & 0xff) as u8).collect())
}

const WHITE: Rgb = Rgb { r: 255, g: 255, b: 255 };

/// 밑줄 한 칸(`w × h`)의 마스크. 인자의 뜻은 `underline::draw`와 같다. 밑줄이 없으면 없다
pub fn underline_mask(kind: Underline, w: u32, h: u32, pos: u32, thick: u32) -> Option<Vec<u8>> {
    if kind == Underline::None {
        return None;
    }
    render(w, h, |c| {
        underline::draw(c, kind, 0, 0, w, h, pos, thick, WHITE);
        true
    })
}

/// 직접 그릴 수 있는 글자의 범위(U+2500~259F). 이 밖의 글자는 `box_mask`가 늘 없다(아래 시험이 지킨다).
/// 렌더러가 글자마다 마스크를 만들어 보지 않게 먼저 가르는 데 쓴다
pub fn may_be_box_char(c: char) -> bool {
    matches!(c as u32, 0x2500..=0x259F)
}

/// 상자 그림·블록 글자 `c`를 `w × h` 칸에 직접 그린 마스크. 직접 그리지 않는 글자(글꼴로 그린다)면 없다
pub fn box_mask(c: char, w: u32, h: u32, stroke: u32) -> Option<Vec<u8>> {
    render(w, h, |canvas| {
        crate::boxdraw::draw(canvas, c, 0, 0, w, h, stroke, WHITE)
    })
}

/// 어느 글꼴에도 없는 글자의 빈 상자 마스크(`draw_cell_text`와 같은 자리·두께)
pub fn missing_glyph_mask(w: u32, h: u32) -> Option<Vec<u8>> {
    render(w, h, |canvas| {
        crate::grid::stroke_missing_glyph(canvas, 0, 0, w, h, WHITE);
        true
    })
}

/// 칸 전체를 채운 마스크(조합 중 글의 밑줄 막대처럼 단색 사각형을 글리프로 그릴 때)
pub fn solid_mask(w: u32, h: u32) -> Option<Vec<u8>> {
    render(w, h, |canvas| {
        canvas.fill_rect(0, 0, w, h, WHITE);
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `may_be_box_char`가 거짓인 글자는 직접 그리지 않는다(boxdraw에 글자를 더하면 범위도 넓혀야 한다)
    #[test]
    fn boxdraw_draws_nothing_outside_the_box_range() {
        for u in 0u32..0x30000 {
            let Some(c) = char::from_u32(u) else { continue };
            if may_be_box_char(c) {
                continue;
            }
            assert!(box_mask(c, 8, 16, 1).is_none(), "U+{u:04X}");
        }
    }

    #[test]
    fn masks_have_cell_size_and_some_ink() {
        let m = underline_mask(Underline::Single, 8, 16, 13, 1).unwrap();
        assert_eq!(m.len(), 8 * 16);
        assert_eq!(m.iter().filter(|a| **a == 255).count(), 8);
        assert!(underline_mask(Underline::None, 8, 16, 13, 1).is_none());
        let b = box_mask('─', 8, 16, 1).unwrap();
        assert_eq!(b.iter().filter(|a| **a == 255).count(), 8);
        let shade = box_mask('▒', 8, 16, 1).unwrap();
        assert!(shade.iter().all(|a| *a == 128));
        assert!(box_mask('a', 8, 16, 1).is_none());
        assert_eq!(solid_mask(3, 2).unwrap(), vec![255; 6]);
        assert!(missing_glyph_mask(8, 16).unwrap().contains(&255));
    }
}
