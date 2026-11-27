//! 비례폭 글꼴로 한 줄 글 그리기(#241 `window-title-font-family`). 셀 격자가 아니라 글자마다 advance로 자리를 잡는다.
//! 폭 계산과 자르기는 advance 함수만 받는 순수 함수라 글꼴 없이 시험한다

use bt_core::Rgb;

use crate::canvas::Canvas;
use crate::font::{FontStyle, Fonts};

/// 글의 폭(px). `advance`는 글자 하나의 나아가는 폭
pub fn text_width(s: &str, advance: impl FnMut(char) -> f32) -> f32 {
    s.chars().map(advance).sum()
}

/// 폭 `max_px` 안에 들어가게 자른다. 잘렸으면 끝에 `…`(그 폭까지 셈에 넣는다). 돌려주는 글의 폭은 `max_px`를 넘지 않는다
/// (`…` 하나조차 안 들어가면 `…`만 남는다)
pub fn truncate_width(s: &str, max_px: f32, mut advance: impl FnMut(char) -> f32) -> String {
    if text_width(s, &mut advance) <= max_px {
        return s.to_string();
    }
    let ellipsis = advance('…');
    let mut out = String::new();
    let mut used = 0.0;
    for c in s.chars() {
        let a = advance(c);
        if used + a + ellipsis > max_px {
            break;
        }
        out.push(c);
        used += a;
    }
    out.push('…');
    out
}

/// 글을 (x, y: 줄 위쪽)에 그리고 그린 폭(px)을 돌려준다. 글자마다 `Fonts::advance`만큼 나아간다.
/// 넓은 글자(한글·이모지)는 그 폭 안 가운데에 둔다. 상자 그림 글자는 따로 그리지 않고 글꼴의 글리프를 쓴다
pub fn draw_text_proportional(canvas: &mut Canvas, fonts: &mut Fonts, x: i32, y: i32, s: &str, color: Rgb) -> i32 {
    draw_text_proportional_styled(canvas, fonts, x, y, s, color, FontStyle::Regular)
}

/// `draw_text_proportional`에 굵게·기울임을 준다. 자리(advance)는 보통 모양 글꼴로 재므로 굵은 글꼴의 폭이 다르면 글자 사이가 조금 달라진다
pub fn draw_text_proportional_styled(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    x: i32,
    y: i32,
    s: &str,
    color: Rgb,
    style: FontStyle,
) -> i32 {
    let baseline = fonts.metrics().baseline as i32;
    let mut pen = x as f32;
    for c in s.chars() {
        let adv = fonts.advance(c);
        if adv <= 0.0 {
            continue;
        }
        if c != ' ' {
            if let Some(g) = fonts.glyph(c, style) {
                let px = pen.round() as i32;
                let offset = if crate::text::char_cells(c) > 1 {
                    (adv.round() as i32 - g.width as i32) / 2 - g.left
                } else {
                    0
                };
                if g.color.is_some() {
                    canvas.draw_color_glyph(px + offset, y + baseline, &g);
                } else {
                    canvas.draw_glyph(px + offset, y + baseline, &g, color);
                }
            }
        }
        pen += adv;
    }
    (pen - x as f32).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 가짜 비례폭: i·l은 3, W·M은 12, 한글은 16, `…`은 10, 나머지는 8
    fn adv(c: char) -> f32 {
        match c {
            'i' | 'l' => 3.0,
            'W' | 'M' => 12.0,
            '…' => 10.0,
            c if (c as u32) >= 0xAC00 => 16.0,
            _ => 8.0,
        }
    }

    #[test]
    fn width_sums_advances() {
        assert_eq!(text_width("", adv), 0.0);
        assert_eq!(text_width("iiWa", adv), 3.0 + 3.0 + 12.0 + 8.0);
        assert_eq!(text_width("안a", adv), 24.0);
    }

    #[test]
    fn fits_means_unchanged() {
        assert_eq!(truncate_width("iiii", 12.0, adv), "iiii");
        assert_eq!(truncate_width("WW", 24.0, adv), "WW");
    }

    #[test]
    fn truncation_follows_real_width_not_char_count() {
        // 좁은 글자는 많이, 넓은 글자는 적게 남는다. 방 25: `…`(10) 뒤에 15
        assert_eq!(truncate_width("iiiiiiiiii", 25.0, adv), "iiiii…");
        assert_eq!(truncate_width("WWWWWWWWWW", 25.0, adv), "W…");
        // 방 20: `…` 뒤에 10이 들어가지만 W(12)는 안 들어간다
        assert_eq!(truncate_width("WWWWWWWWWW", 20.0, adv), "…");
    }

    #[test]
    fn truncated_never_exceeds_room() {
        for text in [
            "iiiiiiiiii",
            "WWWWWWWWWW",
            "안녕하세요abc",
            "aWiMl한글",
            "mixed WiW 한글 text",
        ] {
            for room in [0.0, 5.0, 10.0, 11.0, 20.0, 33.0, 47.0, 90.0] {
                let out = truncate_width(text, room, adv);
                let w = text_width(&out, adv);
                if out == "…" {
                    // `…` 하나는 방이 모자라도 남긴다
                    continue;
                }
                assert!(w <= room, "{text:?} {room}: {out:?} {w}");
                assert!(out == text || out.ends_with('…'), "{text:?} {room}: {out:?}");
            }
        }
    }

    #[test]
    fn truncation_keeps_as_much_as_fits() {
        // 방 34: `…`(10) 뒤에 24 = 8*3
        assert_eq!(truncate_width("abcdefgh", 34.0, adv), "abc…");
        assert_eq!(truncate_width("abcdefgh", 33.0, adv), "ab…");
        // 한글(16) 두 글자 + `…` = 42
        assert_eq!(truncate_width("안녕하세요", 42.0, adv), "안녕…");
        assert_eq!(truncate_width("안녕하세요", 41.0, adv), "안…");
    }

    #[test]
    fn zero_width_chars_are_free() {
        let a = |c: char| if c == '\u{200d}' { 0.0 } else { 8.0 };
        assert_eq!(text_width("a\u{200d}b", a), 16.0);
    }
}
