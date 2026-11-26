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

/// 시각 벨(#893): 칸 바깥 사각형 전체에 `color`를 알파 `alpha`로 얹는다(kitty `draw_visual_bell_flash`는 칸 전체에 색을 얹는다)
pub fn draw_flash(canvas: &mut Canvas, outer: Rect, color: Rgb, alpha: u8) {
    canvas.blend_rect(outer.x, outer.y, outer.w, outer.h, color, alpha);
}

/// 시각 벨을 다시 그리는 간격(번쩍임은 0.1초 안팎이라 한 번에 6~7장이다. kitty는 50ms마다 샘플한다)
pub const FLASH_FRAME: std::time::Duration = std::time::Duration::from_millis(16);

/// 시각 벨이 번쩍이는 칸들의 한 걸음(#893): `starts`는 칸마다 번쩍이기 시작한 때. 끝났거나 끈 칸은 지우고,
/// 아직 번쩍이는 칸이 있으면 다음에 깨울 때를, 없으면 `None`(깨우지 않는다)을 돌려 준다
pub fn flash_step(
    starts: &mut [Option<std::time::Instant>],
    vb: &bt_core::visual_bell::VisualBell,
    now: std::time::Instant,
) -> Option<std::time::Instant> {
    let mut active = false;
    for slot in starts.iter_mut() {
        if let Some(at) = *slot {
            if vb.finished(now.saturating_duration_since(at).as_secs_f64()) {
                *slot = None;
            } else {
                active = true;
            }
        }
    }
    active.then_some(now + FLASH_FRAME)
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
    fn flash_blends_the_whole_pane_and_nothing_outside() {
        let (w, h) = (40u32, 30u32);
        let bg = Rgb { r: 10, g: 20, b: 30 };
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.fill_rect(0, 0, w, h, bg);
        let outer = Rect::new(5, 4, 30, 20);
        // 알파 0은 아무것도 바꾸지 않는다
        draw_flash(&mut c, outer, Rgb { r: 255, g: 255, b: 255 }, 0);
        assert_eq!(c.pixel(20, 12), bg);
        draw_flash(&mut c, outer, Rgb { r: 255, g: 255, b: 255 }, 100);
        let mixed = c.pixel(20, 12);
        assert!(mixed.r > 100 && mixed.g > 100 && mixed.b > 100, "{mixed:?}");
        // 칸 안은 모서리까지 같은 값, 밖은 그대로
        for (x, y) in [(5, 4), (34, 4), (5, 23), (34, 23), (6, 5)] {
            assert_eq!(c.pixel(x, y), mixed, "({x},{y})");
        }
        for (x, y) in [(4, 12), (35, 12), (20, 3), (20, 24), (0, 0)] {
            assert_eq!(c.pixel(x, y), bg, "({x},{y})");
        }
    }

    #[test]
    fn flash_step_wakes_only_while_flashing_and_clears_finished_panes() {
        use bt_core::visual_bell::VisualBell;
        use std::time::{Duration, Instant};
        // 0.4초를 적으면 실제 번쩍임은 0.2초(kitty 코드처럼 절반)
        let vb = VisualBell::parse("0.4").unwrap();
        let t0 = Instant::now();
        let mut starts = [Some(t0), None, Some(t0 + Duration::from_millis(150))];
        // 번쩍이는 동안: 다음 깨울 때가 있고 표시는 그대로
        let next = flash_step(&mut starts, &vb, t0 + Duration::from_millis(100));
        assert_eq!(next, Some(t0 + Duration::from_millis(100) + FLASH_FRAME));
        assert!(starts[0].is_some() && starts[1].is_none() && starts[2].is_some());
        // 앞 칸만 끝났다: 표시를 지우지만 뒤 칸이 아직 번쩍이니 깨운다
        let next = flash_step(&mut starts, &vb, t0 + Duration::from_millis(250));
        assert!(next.is_some());
        assert!(starts[0].is_none() && starts[2].is_some());
        // 모두 끝나면 깨울 때를 남기지 않는다
        assert_eq!(flash_step(&mut starts, &vb, t0 + Duration::from_millis(400)), None);
        assert!(starts.iter().all(Option::is_none));
        // 번쩍이는 칸이 없으면 처음부터 깨우지 않는다
        assert_eq!(flash_step(&mut [None, None], &vb, t0), None);
        // 설정을 끄면 번쩍이던 칸도 바로 지운다
        let mut on = [Some(t0)];
        assert_eq!(flash_step(&mut on, &VisualBell::default(), t0), None);
        assert!(on[0].is_none());
    }

    #[test]
    fn a_burst_of_bells_restarts_one_flash_instead_of_stacking() {
        use bt_core::visual_bell::{VisualBell, flash_alpha};
        // 벨이 몇 번 울리든 표시는 칸마다 하나(마지막 벨의 때)라서, 세기는 한 번 울린 것과 같은 곡선 위에 머문다
        // 0.6초를 적으면 실제 번쩍임은 0.3초(kitty 코드처럼 절반)
        let vb = VisualBell::parse("0.6 linear").unwrap();
        let color = Rgb { r: 255, g: 255, b: 255 };
        let mut peak = 0u8;
        for i in 0..1000 {
            // 1ms마다 벨이 울리고, 벨마다 그 칸의 시작 때가 지금으로 바뀐다: 그릴 때의 경과는 늘 0에 가깝다
            let (now, start) = (i as f64 * 0.001, i as f64 * 0.001);
            peak = peak.max(flash_alpha(vb.intensity(now - start), color));
        }
        assert_eq!(peak, 0, "쏟아지는 벨은 처음 세기(0)에서 다시 시작해 밝아지지 않는다");
        // 한 번만 울리면 가운데에서 최대(kitty 밝은 색 0.6 × 255 = 153)
        let single = (0..300)
            .map(|ms| flash_alpha(vb.intensity(ms as f64 / 1000.0), color))
            .max()
            .unwrap();
        assert_eq!(single, 153);
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
