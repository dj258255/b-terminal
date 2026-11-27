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
    thumb_min(track, rows, history, offset, MIN_THUMB)
}

/// `thumb`에서 손잡이 최소 길이(픽셀)를 `min_thumb`로 정한 것(`scrollbar-min-handle-height`, #917)
pub fn thumb_min(track: u32, rows: usize, history: usize, offset: usize, min_thumb: u32) -> Option<(u32, u32)> {
    if track == 0 || rows == 0 || history == 0 {
        return None;
    }
    let total = (rows + history) as u64;
    let len = ((track as u64 * rows as u64) / total)
        .max(min_thumb as u64)
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
    hit_min(track, rows, history, offset, y, MIN_THUMB)
}

/// `hit`에서 손잡이 최소 길이를 정한 것
pub fn hit_min(track: u32, rows: usize, history: usize, offset: usize, y: f64, min_thumb: u32) -> Option<Hit> {
    let (ty, len) = thumb_min(track, rows, history, offset, min_thumb)?;
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
    drag_top_min(track, rows, history, y, grab, MIN_THUMB)
}

/// `drag_top`에서 손잡이 최소 길이를 정한 것
pub fn drag_top_min(track: u32, rows: usize, history: usize, y: f64, grab: u32, min_thumb: u32) -> Option<usize> {
    // 손잡이 길이는 `thumb`와 같은 식(최소 길이 포함)으로 얻는다
    let (_, len) = thumb_min(track, rows, history, 0, min_thumb)?;
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
    jump_top_min(track, rows, history, y, MIN_THUMB)
}

/// `jump_top`에서 손잡이 최소 길이를 정한 것
pub fn jump_top_min(track: u32, rows: usize, history: usize, y: f64, min_thumb: u32) -> Option<(usize, u32)> {
    let (_, len) = thumb_min(track, rows, history, 0, min_thumb)?;
    let grab = len / 2;
    drag_top_min(track, rows, history, y, grab, min_thumb).map(|row| (row, grab))
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

/// 막대를 그리는 모양을 픽셀로 푼 것(`scrollbar-*` 설정을 칸 크기·배율로 바꾼 결과, #917)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// 손잡이 폭(픽셀)과 마우스를 올렸을 때의 폭
    pub width: u32,
    pub hover_width: u32,
    /// 칸 오른쪽 가장자리와 막대 사이(픽셀)
    pub gap: u32,
    /// 손잡이 모서리 둥글기 반지름(픽셀). 0이면 직각
    pub radius: u32,
    /// 손잡이 최소 길이(픽셀)
    pub min_thumb: u32,
    pub handle: Rgb,
    /// 손잡이 덮음(0~255): 평소·마우스를 올렸을 때
    pub handle_alpha: u8,
    pub handle_alpha_hover: u8,
    pub track: Rgb,
    /// 트랙 덮음(0~255): 평소·마우스를 올렸을 때. 0이면 트랙을 그리지 않는다
    pub track_alpha: u8,
    pub track_alpha_hover: u8,
}

fn alpha_of(opacity: f32) -> u8 {
    (opacity.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Look {
    /// 설정과 칸 크기에서 모양을 푼다. `scale`은 화면 배율, `cell`은 (칸 너비, 칸 높이) 픽셀.
    /// 정하지 않은 값은 kitty 기본값이 아니라 처음부터 그리던 막대의 값을 쓴다: 폭 `6×scale`, 간격 `scale`(최소 1),
    /// 최소 길이 20px, 직각, 덮음 110(마우스 올리면 170)/255, 트랙 없음.
    /// kitty처럼 길이는 칸 크기에 배수를 곱해 버린다(`(int)`)
    pub fn resolve(
        style: &bt_core::config::ScrollbarStyle,
        scale: f64,
        cell: (u32, u32),
        foreground: Rgb,
        selection_background: Rgb,
    ) -> Look {
        let (cw, ch) = (cell.0 as f32, cell.1 as f32);
        let px = |mult: f32, unit: f32| (mult * unit) as u32;
        let width = style
            .width
            .map_or((BAR_WIDTH as f64 * scale).round() as u32, |m| px(m, cw));
        let hover_width = style.hover_width.map_or(width, |m| px(m, cw));
        let gap = style.gap.map_or(scale.round().max(1.0) as u32, |m| px(m, cw));
        let radius = style.radius.map_or(0, |m| px(m, cw));
        let min_thumb = style.min_handle_height.map_or(MIN_THUMB, |m| px(m, ch));
        let (handle_alpha, handle_alpha_hover) = match style.handle_opacity {
            Some(o) => (alpha_of(o), alpha_of(o)),
            None => (HANDLE_ALPHA, HANDLE_ALPHA_HOVER),
        };
        let track_alpha = alpha_of(style.track_opacity);
        Look {
            width,
            hover_width,
            gap,
            radius,
            min_thumb,
            handle: style.handle_color.resolve(foreground, selection_background),
            handle_alpha,
            handle_alpha_hover,
            track: style.track_color.resolve(foreground, selection_background),
            track_alpha,
            track_alpha_hover: style.track_hover_opacity.map_or(track_alpha, alpha_of),
        }
    }

    /// 마우스가 막대 띠 위에 있을 때의 폭
    pub fn bar_width(&self, hovering: bool) -> u32 {
        if hovering { self.hover_width } else { self.width }
    }

    /// 마우스를 받는 오른쪽 끝 띠의 폭(픽셀): 처음부터 있던 `BAR_WIDTH×2×scale`보다 좁아지지 않고,
    /// 간격과 가장 넓은 폭이 더 넓으면 그것을 덮는다
    pub fn gutter(&self, scale: f64) -> u32 {
        let legacy = (BAR_WIDTH as f64 * scale * 2.0).round() as u32;
        legacy.max(self.gap + self.width.max(self.hover_width))
    }
}

/// 처음부터 그리던 손잡이 덮음(평소, 마우스를 올렸을 때)
const HANDLE_ALPHA: u8 = 110;
const HANDLE_ALPHA_HOVER: u8 = 170;

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 막대 하나를 그린다(트랙과 손잡이). 칸 오른쪽 바깥 끝 `right`에서 `look.gap`과 폭만큼 왼쪽에 놓는다.
/// 트랙은 위에서 `top`, 길이 `track`. 설정이 없으면 `draw`가 그리던 것과 같은 픽셀이다
#[allow(clippy::too_many_arguments)]
pub fn draw_look(
    canvas: &mut Canvas,
    right: i32,
    top: i32,
    track: u32,
    look: &Look,
    hovering: bool,
    rows: usize,
    history: usize,
    offset: usize,
) {
    let width = look.bar_width(hovering);
    let x = right - width as i32 - look.gap as i32;
    let track_alpha = if hovering {
        look.track_alpha_hover
    } else {
        look.track_alpha
    };
    if track_alpha > 0 && width > 0 {
        canvas.blend_rect(x, top, width, track, look.track, track_alpha);
    }
    let alpha = if hovering {
        look.handle_alpha_hover
    } else {
        look.handle_alpha
    };
    let Some((dy, len)) = thumb_min(track, rows, history, offset, look.min_thumb) else {
        return;
    };
    let y = top + dy as i32;
    if look.radius == 0 {
        canvas.blend_rect(x, y, width, len, look.handle, alpha);
        return;
    }
    // 둥근 손잡이: kitty `rounded_rect.slang`처럼 부호 있는 거리로 가장자리를 1픽셀 부드럽게
    let r = (look.radius as f32).min(width as f32 / 2.0).min(len as f32 / 2.0);
    let (hw, hh) = (width as f32 / 2.0, len as f32 / 2.0);
    let (bx, by) = (hw - r, hh - r);
    for row in 0..len {
        for col in 0..width {
            let (px, py) = ((col as f32 + 0.5 - hw).abs() - bx, (row as f32 + 0.5 - hh).abs() - by);
            let dist = px.max(0.0).hypot(py.max(0.0)) + px.max(py).min(0.0) - r;
            let cover = smoothstep(-1.0, 1.0, -dist);
            let a = (alpha as f32 * cover).round() as u8;
            if a > 0 {
                canvas.blend_px(x + col as i32, y + row as i32, look.handle, a);
            }
        }
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
