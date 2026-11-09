//! 스크롤 막대(Ghostty `scrollbar`). 칸 오른쪽 끝에 얇은 막대를 글 위에 반투명으로 그린다

use crate::Canvas;
use bt_core::Rgb;

/// 막대 폭(픽셀, 배율 1일 때)
pub const BAR_WIDTH: u32 = 6;

/// 막대의 가장 짧은 길이(픽셀). 줄이 아주 많아도 잡을 수 있게
const MIN_THUMB: u32 = 20;

/// 막대(손잡이)의 위치와 길이: `(트랙 맨 위에서 떨어진 거리, 길이)`.
/// `rows`는 보이는 줄 수, `history`는 스크롤백 줄 수, `offset`은 맨 아래에서 위로 올라간 줄 수.
/// 스크롤백이 없거나 트랙이 없으면 없다
pub fn thumb(track: u32, rows: usize, history: usize, offset: usize) -> Option<(u32, u32)> {
    if track == 0 || rows == 0 || history == 0 {
        return None;
    }
    let total = (rows + history) as u64;
    let len = ((track as u64 * rows as u64) / total)
        .max(MIN_THUMB as u64)
        .min(track as u64) as u32;
    // 화면 맨 위 줄이 전체의 몇 번째인가(0..=history)
    let top = (history - offset.min(history)) as u64;
    let room = (track - len) as u64;
    let y = (room * top / history as u64) as u32;
    Some((y, len))
}

/// 마우스가 칸의 오른쪽 끝 띠(폭 `gutter`) 위에 있는가. 칸 바깥 사각형은 `(left, top)`–`(right, bottom)`
pub fn in_gutter(x: f64, y: f64, left: i32, top: i32, right: i32, bottom: i32, gutter: u32) -> bool {
    left < right && x >= (right - gutter as i32) as f64 && x < right as f64 && y >= top as f64 && y < bottom as f64
}

/// 막대를 그린다. 트랙은 `(x, y)`에서 아래로 `track` 픽셀, 폭 `width`. 글을 덮지 않게 반투명(`alpha`)
#[allow(clippy::too_many_arguments)]
pub fn draw(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    track: u32,
    width: u32,
    rows: usize,
    history: usize,
    offset: usize,
    color: Rgb,
    alpha: u8,
) {
    if let Some((dy, len)) = thumb(track, rows, history, offset) {
        canvas.blend_rect(x, y + dy as i32, width, len, color, alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_is_none_without_history() {
        assert_eq!(thumb(200, 24, 0, 0), None);
        assert_eq!(thumb(0, 24, 100, 0), None);
    }

    #[test]
    fn thumb_sits_at_bottom_when_not_scrolled_and_top_when_fully_scrolled() {
        // 보이는 24줄 + 스크롤백 76줄 = 100줄, 트랙 200: 길이 48
        let (y, len) = thumb(200, 24, 76, 0).unwrap();
        assert_eq!(len, 48);
        assert_eq!(y + len, 200, "맨 아래면 트랙 끝에 닿는다");
        let (y, len) = thumb(200, 24, 76, 76).unwrap();
        assert_eq!((y, len), (0, 48), "맨 위까지 올리면 트랙 맨 위");
        let (y, _) = thumb(200, 24, 76, 38).unwrap();
        assert_eq!(y, 76, "가운데: 남는 152의 절반");
    }

    #[test]
    fn thumb_has_minimum_length_and_clamps_offset() {
        let (_, len) = thumb(200, 24, 100_000, 0).unwrap();
        assert_eq!(len, 20);
        let (y, len) = thumb(200, 24, 76, 9999).unwrap();
        assert_eq!((y, len), (0, 48), "넘치는 offset은 맨 위로 본다");
        // 트랙이 최소 길이보다 짧으면 트랙을 넘지 않는다
        assert_eq!(thumb(10, 24, 76, 0), Some((0, 10)));
    }

    #[test]
    fn gutter_is_the_right_edge_strip() {
        assert!(in_gutter(99.0, 10.0, 0, 0, 100, 50, 8));
        assert!(in_gutter(92.0, 10.0, 0, 0, 100, 50, 8));
        assert!(!in_gutter(91.0, 10.0, 0, 0, 100, 50, 8));
        assert!(!in_gutter(100.0, 10.0, 0, 0, 100, 50, 8));
        assert!(!in_gutter(95.0, 60.0, 0, 0, 100, 50, 8));
    }

    #[test]
    fn draw_blends_thumb_pixels_and_leaves_the_rest() {
        let (w, h) = (20u32, 100u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let bg = Rgb { r: 0, g: 0, b: 0 };
        let white = Rgb { r: 255, g: 255, b: 255 };
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(bg);
        // 트랙 100, 24+76줄 → 길이 24, 맨 아래: y 76..100
        draw(&mut c, 14, 0, 100, 6, 24, 76, 0, white, 128);
        let inside = c.pixel(16, 90);
        assert!(inside.r > 100 && inside.r < 160, "반투명으로 섞인다: {inside:?}");
        assert_eq!(c.pixel(16, 10), bg, "손잡이 밖은 그대로");
        assert_eq!(c.pixel(5, 90), bg, "막대 폭 밖은 그대로");
        assert_eq!(c.pixel(16, 75), bg);
        assert_ne!(c.pixel(16, 76), bg);
    }
}
