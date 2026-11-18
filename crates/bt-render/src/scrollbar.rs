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

/// 막대를 눌렀을 때 맞은 곳
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// 손잡이를 눌렀다. `grab`은 손잡이 맨 위에서 누른 곳까지의 거리(끄는 동안 이 차이를 유지한다)
    Thumb { grab: u32 },
    /// 손잡이 밖 빈 곳을 눌렀다. `up`이면 손잡이 위쪽(위로 한 쪽)
    Page { up: bool },
}

/// 트랙 맨 위에서 `y`픽셀 아래를 눌렀을 때 맞은 곳. 막대가 없으면(스크롤백 없음) 없다.
/// 트랙 밖(위·아래 패딩)은 가장자리로 본다
pub fn hit(track: u32, rows: usize, history: usize, offset: usize, y: f64) -> Option<Hit> {
    let (ty, len) = thumb(track, rows, history, offset)?;
    let y = y.clamp(0.0, (track - 1) as f64);
    let (start, end) = (ty as f64, (ty + len) as f64);
    Some(if y < start {
        Hit::Page { up: true }
    } else if y >= end {
        Hit::Page { up: false }
    } else {
        Hit::Thumb {
            grab: (y - start).floor() as u32,
        }
    })
}

/// 손잡이를 `grab`만큼 쥔 채 포인터가 트랙 맨 위에서 `y`픽셀에 있을 때, 화면 맨 위에 올 줄
/// (스크롤백 맨 위를 0으로 센 절대 줄, `0..=history`). 막대를 그리는 `thumb`와 같은 기하의 거꾸로 계산이다
pub fn drag_top(track: u32, rows: usize, history: usize, y: f64, grab: u32) -> Option<usize> {
    // 손잡이 길이는 `thumb`와 같은 식(최소 길이 포함)으로 얻는다
    let (_, len) = thumb(track, rows, history, 0)?;
    let room = (track - len) as u64;
    if room == 0 {
        return None;
    }
    let top_px = (y - grab as f64).clamp(0.0, room as f64).round() as u64;
    // `thumb`의 `y = room * top / history`를 거꾸로 푼다(반올림)
    Some(((top_px * history as u64 + room / 2) / room) as usize)
}

/// 시스템 설정 "스크롤 막대 클릭 시: 누른 자리로 이동"(#630): 트랙 맨 위에서 `y`픽셀 아래를 눌렀을 때 손잡이 가운데가
/// 그 자리에 오게 하는 줄(`drag_top`과 같은 단위)과, 이어서 끌 때 쥔 곳(손잡이 가운데)
pub fn jump_top(track: u32, rows: usize, history: usize, y: f64) -> Option<(usize, u32)> {
    let (_, len) = thumb(track, rows, history, 0)?;
    let grab = len / 2;
    drag_top(track, rows, history, y, grab).map(|row| (row, grab))
}

/// 빈 곳을 누르면 옮기는 줄 수: 한 쪽에서 겹치는 한 줄을 뺀 만큼(최소 1)
pub fn page_lines(rows: usize) -> usize {
    rows.saturating_sub(1).max(1)
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

    /// 시스템 설정 "스크롤 막대 클릭 시: 누른 자리로 이동"(#630): 누른 곳이 손잡이 가운데가 되는 줄
    #[test]
    fn jump_top_centers_thumb_on_click() {
        // 24줄 + 스크롤백 76줄, 트랙 200: 길이 48, 남는 152
        let (row, grab) = jump_top(200, 24, 76, 100.0).unwrap();
        assert_eq!(grab, 24, "손잡이 가운데를 쥔 것으로 본다");
        assert_eq!(row, 38, "가운데를 누르면 가운데 줄");
        assert_eq!(jump_top(200, 24, 76, 0.0).unwrap().0, 0, "맨 위");
        assert_eq!(jump_top(200, 24, 76, 199.0).unwrap().0, 76, "맨 아래");
        assert_eq!(jump_top(200, 24, 0, 100.0), None, "스크롤백이 없으면 없다");
    }

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

    #[test]
    fn hit_finds_thumb_with_grab_offset_or_page_direction() {
        // 트랙 200, 24+76줄: 길이 48. 맨 아래면 손잡이는 152..200
        assert_eq!(hit(200, 24, 76, 0, 170.0), Some(Hit::Thumb { grab: 18 }));
        assert_eq!(hit(200, 24, 76, 0, 152.0), Some(Hit::Thumb { grab: 0 }));
        assert_eq!(hit(200, 24, 76, 0, 199.9), Some(Hit::Thumb { grab: 47 }));
        assert_eq!(hit(200, 24, 76, 0, 100.0), Some(Hit::Page { up: true }));
        assert_eq!(hit(200, 24, 76, 0, 151.9), Some(Hit::Page { up: true }));
        // 맨 위까지 올리면 손잡이는 0..48
        assert_eq!(hit(200, 24, 76, 76, 0.0), Some(Hit::Thumb { grab: 0 }));
        assert_eq!(hit(200, 24, 76, 76, 48.0), Some(Hit::Page { up: false }));
        assert_eq!(hit(200, 24, 76, 76, 150.0), Some(Hit::Page { up: false }));
        // 트랙 밖은 가장자리로
        assert_eq!(hit(200, 24, 76, 76, -5.0), Some(Hit::Thumb { grab: 0 }));
        assert_eq!(hit(200, 24, 76, 0, 260.0), Some(Hit::Thumb { grab: 47 }));
        assert_eq!(hit(200, 24, 76, 38, -5.0), Some(Hit::Page { up: true }));
    }

    #[test]
    fn hit_is_none_without_a_bar() {
        assert_eq!(hit(200, 24, 0, 0, 100.0), None);
        assert_eq!(hit(0, 24, 76, 0, 0.0), None);
        assert_eq!(hit(200, 0, 76, 0, 0.0), None);
    }

    #[test]
    fn hit_uses_the_minimum_thumb_for_huge_scrollback() {
        // 스크롤백이 아주 길면 손잡이는 최소 20픽셀. 맨 아래: 180..200
        assert_eq!(hit(200, 24, 100_000, 0, 190.0), Some(Hit::Thumb { grab: 10 }));
        assert_eq!(hit(200, 24, 100_000, 0, 179.0), Some(Hit::Page { up: true }));
    }

    #[test]
    fn drag_top_keeps_the_grab_offset_and_clamps_at_both_ends() {
        // 손잡이 길이 48, 남는 152, 스크롤백 76
        assert_eq!(drag_top(200, 24, 76, 170.0, 18), Some(76), "맨 아래");
        assert_eq!(drag_top(200, 24, 76, 18.0, 18), Some(0), "맨 위");
        assert_eq!(drag_top(200, 24, 76, 94.0, 18), Some(38), "가운데");
        assert_eq!(drag_top(200, 24, 76, -100.0, 18), Some(0), "위로 넘어가면 맨 위");
        assert_eq!(drag_top(200, 24, 76, 9999.0, 18), Some(76), "아래로 넘어가면 맨 아래");
        // 손잡이 맨 위를 잡았든(0) 맨 아래를 잡았든(47) 손잡이가 닿는 끝은 같다
        assert_eq!(drag_top(200, 24, 76, 152.0, 0), Some(76));
        assert_eq!(drag_top(200, 24, 76, 199.0, 47), Some(76));
    }

    #[test]
    fn drag_top_is_the_inverse_of_thumb() {
        // 스크롤백이 남는 길이 이하면 줄마다 손잡이 자리가 달라서 정확히 되돌아온다
        for top in 0..=76usize {
            let (y, _) = thumb(200, 24, 76, 76 - top).unwrap();
            assert_eq!(drag_top(200, 24, 76, y as f64 + 7.0, 7), Some(top), "top={top}");
        }
    }

    #[test]
    fn drag_top_with_minimum_thumb_and_very_long_history() {
        // 길이 20, 남는 180, 스크롤백 100_000
        assert_eq!(drag_top(200, 24, 100_000, 0.0, 0), Some(0));
        assert_eq!(drag_top(200, 24, 100_000, 90.0, 0), Some(50_000));
        assert_eq!(drag_top(200, 24, 100_000, 180.0, 0), Some(100_000));
        // 아주 큰 값에서도 넘치지 않는다
        let big = 4_000_000_000usize;
        assert_eq!(drag_top(1000, 24, big, 1000.0, 0), Some(big));
        assert_eq!(drag_top(1000, 24, big, 0.0, 0), Some(0));
    }

    #[test]
    fn drag_top_is_none_without_a_bar_or_room() {
        assert_eq!(drag_top(200, 24, 0, 100.0, 0), None);
        assert_eq!(drag_top(0, 24, 76, 100.0, 0), None);
        // 트랙이 너무 짧아 손잡이가 트랙을 다 채우면 움직일 자리가 없다
        assert_eq!(drag_top(10, 24, 76, 5.0, 0), None);
    }

    #[test]
    fn page_is_rows_minus_one_line_overlap() {
        assert_eq!(page_lines(24), 23);
        assert_eq!(page_lines(2), 1);
        assert_eq!(page_lines(1), 1);
        assert_eq!(page_lines(0), 1);
    }
}
