//! 벨 테두리(`bell-features = border`, #687). 벨이 울린 칸의 바깥 사각형 안쪽 둘레에 노란 띠를 두른다.
//!
//! 모양은 Ghostty macOS 소스(`SurfaceView.swift`의 `BellBorderOverlay`)를 따른다: 두께 3pt, 색 (1.0, 0.8, 0.0)에 알파 0.5,
//! 칸 전체 위에 겹치고 마우스를 막지 않는다. Ghostty는 켜고 끌 때 0.3초 부드럽게 바꾸지만 b-terminal은 바로 켜고 끈다.
//! 본 것은 소스 글뿐이고 Ghostty 창은 띄워 보지 않았다

use bt_core::Rgb;
use bt_render::canvas::Canvas;

use crate::split::Rect;

/// 테두리 색 (1.0, 0.8, 0.0)
const COLOR: Rgb = Rgb { r: 255, g: 204, b: 0 };
/// 알파 0.5
const ALPHA: u8 = 128;
/// 두께(점)
const WIDTH_PT: f64 = 3.0;

/// 테두리 두께(픽셀). 칸이 얇아도 가운데가 남게 절반을 넘기지 않는다
fn thickness(outer: Rect, scale: f64) -> u32 {
    thickness_of(outer, scale, WIDTH_PT)
}

/// 두께 `width_pt`점의 테두리 두께(픽셀)
fn thickness_of(outer: Rect, scale: f64, width_pt: f64) -> u32 {
    ((width_pt * scale).round() as u32)
        .max(1)
        .min(outer.w / 2)
        .min(outer.h / 2)
}

/// 칸 바깥 사각형 안쪽 네 변: 위, 아래, 왼쪽, 오른쪽(서로 겹치지 않아 알파가 두 번 얹히지 않는다)
fn edges(outer: Rect, scale: f64) -> [Rect; 4] {
    edges_of(outer, thickness(outer, scale))
}

/// 두께 `t`픽셀의 네 변
fn edges_of(outer: Rect, t: u32) -> [Rect; 4] {
    let side_h = outer.h - 2 * t;
    [
        Rect::new(outer.x, outer.y, outer.w, t),
        Rect::new(outer.x, outer.y + (outer.h - t) as i32, outer.w, t),
        Rect::new(outer.x, outer.y + t as i32, t, side_h),
        Rect::new(outer.x + (outer.w - t) as i32, outer.y + t as i32, t, side_h),
    ]
}

/// 입력 함께 보내기 묶음의 테두리(#860). Ghostty PR #13745의 `BroadcastGroupBorderOverlay`처럼 두께 2pt, 묶음 색 그대로(알파 1)
const BROADCAST_WIDTH_PT: f64 = 2.0;

/// 묶음 색 테두리를 칸 바깥 사각형 안쪽에 얹는다
pub fn draw_broadcast(canvas: &mut Canvas, outer: Rect, scale: f64, color: Rgb) {
    for e in edges_of(outer, thickness_of(outer, scale, BROADCAST_WIDTH_PT)) {
        canvas.blend_rect(e.x, e.y, e.w, e.h, color, 255);
    }
}

/// 칸 바깥 사각형 안쪽에 테두리를 얹는다
pub fn draw(canvas: &mut Canvas, outer: Rect, scale: f64) {
    for e in edges(outer, scale) {
        canvas.blend_rect(e.x, e.y, e.w, e.h, COLOR, ALPHA);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_only_the_inner_edge_and_leaves_the_middle() {
        let (w, h) = (40u32, 30u32);
        let bg = Rgb { r: 0, g: 0, b: 0 };
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.fill_rect(0, 0, w, h, bg);
        // 칸이 (5,4)부터 30×20. scale 1이면 두께 3
        let outer = Rect::new(5, 4, 30, 20);
        draw(&mut c, outer, 1.0);
        let mixed = c.pixel(5, 4);
        assert!(mixed.r > 100 && mixed.g > 80 && mixed.b == 0, "{mixed:?}");
        // 네 변의 가장 바깥과 안쪽 끝 줄은 같은 값으로 섞인다(모서리도 알파가 한 번만 얹힌다)
        for (x, y) in [
            (5, 10),
            (34, 10),
            (15, 4),
            (15, 23),
            (7, 10),
            (32, 10),
            (15, 6),
            (15, 21),
            (34, 23),
        ] {
            assert_eq!(c.pixel(x, y), mixed, "({x},{y})는 섞여야 한다");
        }
        // 안쪽 가운데와 칸 밖은 그대로
        for (x, y) in [
            (8, 10),
            (20, 12),
            (31, 10),
            (15, 7),
            (15, 20),
            (4, 10),
            (35, 10),
            (15, 3),
            (15, 24),
        ] {
            assert_eq!(c.pixel(x, y), bg, "({x},{y})는 그대로여야 한다");
        }
    }

    #[test]
    fn thickness_follows_scale_and_never_eats_a_small_pane() {
        let big = Rect::new(0, 0, 100, 80);
        assert_eq!(thickness(big, 1.0), 3);
        assert_eq!(thickness(big, 2.0), 6);
        assert_eq!(thickness(big, 1.5), 5);
        assert_eq!(thickness(Rect::new(0, 0, 4, 100), 2.0), 2);
        // 아주 작아도 넘치지 않는다
        let mut buf = vec![0u32; 100];
        let mut c = Canvas::new(&mut buf, 10, 10);
        draw(&mut c, Rect::new(0, 0, 3, 3), 3.0);
    }
}
