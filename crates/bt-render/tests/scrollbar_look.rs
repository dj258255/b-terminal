//! 스크롤 막대 모양(kitty `scrollbar_*`, #917)의 픽셀 시험.
//! CPU 길과 GPU 길은 같은 `draw_look`을 같은 `Canvas`에 그리므로(앱의 두 그리기 자리 모두), 여기서 픽셀로 고정한다

use bt_core::Rgb;
use bt_core::config::{ScrollbarColor, ScrollbarStyle};
use bt_render::Canvas;
use bt_render::scrollbar::{self, Look};

const BG: Rgb = Rgb { r: 10, g: 20, b: 30 };
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };
const SEL: Rgb = Rgb { r: 40, g: 80, b: 160 };
const W: u32 = 120;
const H: u32 = 480;

fn frame(f: impl FnOnce(&mut Canvas)) -> Vec<u32> {
    let mut buf = vec![0u32; (W * H) as usize];
    let mut c = Canvas::new(&mut buf, W, H);
    c.clear(BG);
    f(&mut c);
    buf
}

fn look(style: &ScrollbarStyle, scale: f64, cell: (u32, u32)) -> Look {
    Look::resolve(style, scale, cell, FG, SEL)
}

/// 처음부터 있던 그리기 한 줄(`draw`)과 같은 모양으로 그린 화면
fn legacy(scale: f64, rows: usize, history: usize, offset: usize, hovering: bool, cell_h: u32) -> Vec<u32> {
    frame(|c| {
        let width = (scrollbar::BAR_WIDTH as f64 * scale).round() as u32;
        scrollbar::draw(
            c,
            W as i32 - width as i32 - scale.round().max(1.0) as i32,
            5,
            rows as u32 * cell_h,
            width,
            rows,
            history,
            offset,
            FG,
            if hovering { 170 } else { 110 },
        );
    })
}

#[test]
fn no_settings_draws_exactly_the_pixels_of_before() {
    let style = ScrollbarStyle::default();
    for scale in [1.0, 2.0, 1.5] {
        for cell in [(8u32, 16u32), (9, 19), (18, 38)] {
            for hovering in [false, true] {
                for (rows, history, offset) in [
                    (10usize, 50usize, 0usize),
                    (10, 50, 25),
                    (10, 500, 499),
                    (10, 100_000, 3),
                ] {
                    let l = look(&style, scale, cell);
                    let got = frame(|c| {
                        scrollbar::draw_look(
                            c,
                            W as i32,
                            5,
                            rows as u32 * cell.1,
                            &l,
                            hovering,
                            rows,
                            history,
                            offset,
                        )
                    });
                    let want = legacy(scale, rows, history, offset, hovering, cell.1);
                    assert!(
                        got == want,
                        "scale {scale} cell {cell:?} hover {hovering} {rows}/{history}/{offset}"
                    );
                    assert_ne!(got, frame(|_| {}), "막대가 그려져야 시험이 뜻이 있다");
                }
            }
        }
    }
}

#[test]
fn no_settings_resolves_to_the_old_numbers() {
    let l = look(&ScrollbarStyle::default(), 2.0, (9, 19));
    assert_eq!(
        (l.width, l.hover_width, l.gap, l.radius, l.min_thumb),
        (12, 12, 2, 0, 20)
    );
    assert_eq!(
        (l.handle_alpha, l.handle_alpha_hover, l.track_alpha, l.track_alpha_hover),
        (110, 170, 0, 0)
    );
    assert_eq!((l.handle, l.track), (FG, FG));
    assert_eq!(l.gutter(2.0), 24, "마우스 띠도 그대로(6×2×배율)");
}

#[test]
fn handle_color_and_width_follow_settings() {
    // kitty `scrollbar_handle_color`·`scrollbar_width = 1`(칸 너비 한 칸): 칸 9px → 폭 9px
    let style = ScrollbarStyle {
        width: Some(1.0),
        handle_color: ScrollbarColor::Rgb(Rgb::new(255, 0, 0)),
        handle_opacity: Some(1.0),
        gap: Some(0.0),
        ..ScrollbarStyle::default()
    };
    let l = look(&style, 1.0, (9, 18));
    assert_eq!((l.width, l.gap), (9, 0));
    let buf = frame(|c| scrollbar::draw_look(c, W as i32, 0, 180, &l, false, 10, 90, 0));
    let px = |x: u32, y: u32| buf[(y * W + x) as usize];
    let red = 0xff00_0000 | Rgb::new(255, 0, 0).to_u32();
    let bg = 0xff00_0000 | BG.to_u32();
    // 손잡이는 맨 아래(10/100 × 180 = 18px, 최소 20px로 20): y 160..180
    assert_eq!(px(W - 1, 170), red, "오른쪽 끝 열까지 빨강");
    assert_eq!(px(W - 9, 170), red, "폭 9px의 왼쪽 끝");
    assert_eq!(px(W - 10, 170), bg, "폭 밖은 그대로");
    assert_eq!(px(W - 5, 159), bg, "손잡이 위는 그대로");
}

#[test]
fn track_is_drawn_only_when_its_opacity_is_above_zero() {
    let mut style = ScrollbarStyle {
        width: Some(1.0),
        track_color: ScrollbarColor::Rgb(Rgb::new(0, 255, 0)),
        ..ScrollbarStyle::default()
    };
    let l = look(&style, 1.0, (8, 16));
    let none = frame(|c| scrollbar::draw_look(c, W as i32, 0, 160, &l, false, 10, 90, 0));
    // 트랙이 없으면 손잡이 위(y 10)는 바탕 그대로
    let at = |b: &[u32], x: u32, y: u32| b[(y * W + x) as usize];
    let bg = 0xff00_0000 | BG.to_u32();
    assert_eq!(at(&none, W - 5, 10), bg);
    style.track_opacity = 0.5;
    let l = look(&style, 1.0, (8, 16));
    assert_eq!(l.track_alpha, 128);
    let some = frame(|c| scrollbar::draw_look(c, W as i32, 0, 160, &l, false, 10, 90, 0));
    let px = at(&some, W - 5, 10);
    assert_ne!(px, bg, "트랙이 그려진다");
    assert!((px >> 8) & 0xff > 100, "초록 쪽으로 섞였다: {px:#x}");
    // 트랙은 폭 밖에 번지지 않고, 길이만큼만이다
    assert_eq!(at(&some, W - 20, 10), bg);
    assert_eq!(at(&some, W - 5, 165), bg);
}

#[test]
fn hover_widens_the_bar_and_switches_the_track_opacity() {
    let style = ScrollbarStyle {
        width: Some(0.5),
        hover_width: Some(1.0),
        track_opacity: 0.0,
        track_hover_opacity: Some(0.5),
        handle_opacity: Some(1.0),
        gap: Some(0.0),
        ..ScrollbarStyle::default()
    };
    let l = look(&style, 1.0, (8, 16));
    assert_eq!((l.bar_width(false), l.bar_width(true)), (4, 8));
    let calm = frame(|c| scrollbar::draw_look(c, W as i32, 0, 160, &l, false, 10, 90, 0));
    let hot = frame(|c| scrollbar::draw_look(c, W as i32, 0, 160, &l, true, 10, 90, 0));
    let at = |b: &[u32], x: u32, y: u32| b[(y * W + x) as usize];
    let bg = 0xff00_0000 | BG.to_u32();
    assert_eq!(at(&calm, W - 4, 150), at(&calm, W - 1, 150), "평소는 4px 폭의 손잡이");
    assert_eq!(at(&calm, W - 5, 150), bg);
    assert_eq!(at(&calm, W - 8, 10), bg, "평소엔 트랙이 없다");
    assert_ne!(at(&hot, W - 8, 10), bg, "올리면 트랙(폭 8)이 보인다");
    assert_ne!(at(&hot, W - 8, 150), bg, "손잡이도 폭 8");
}

#[test]
fn radius_rounds_the_handle_corners_and_leaves_the_middle() {
    let style = ScrollbarStyle {
        width: Some(1.0),
        radius: Some(0.5),
        handle_opacity: Some(1.0),
        gap: Some(0.0),
        ..ScrollbarStyle::default()
    };
    let l = look(&style, 1.0, (10, 20));
    assert_eq!((l.width, l.radius), (10, 5));
    let buf = frame(|c| scrollbar::draw_look(c, W as i32, 0, 200, &l, false, 10, 90, 0));
    let px = |x: u32, y: u32| buf[(y * W + x) as usize];
    let bg = 0xff00_0000 | BG.to_u32();
    let fg = 0xff00_0000 | FG.to_u32();
    // 손잡이는 y 180..200(10/100×200 = 20px)
    assert_eq!(px(W - 5, 190), fg, "가운데는 그대로 채워진다");
    assert_eq!(px(W - 1, 180), bg, "위 오른쪽 모서리는 깎인다");
    assert_eq!(px(W - 10, 199), bg, "아래 왼쪽 모서리도");
    assert_ne!(px(W - 5, 180), bg, "위 변 가운데는 걸친다(가장자리 1픽셀을 부드럽게)");
    assert_eq!(px(W - 5, 182), fg, "두 픽셀 안쪽은 꽉 찬다");
}

#[test]
fn zero_radius_matches_plain_rect_and_a_huge_radius_does_not_panic() {
    let base = ScrollbarStyle {
        width: Some(1.0),
        handle_opacity: Some(0.5),
        ..ScrollbarStyle::default()
    };
    let l = look(&base, 1.0, (8, 16));
    let a = frame(|c| scrollbar::draw_look(c, W as i32, 0, 160, &l, false, 10, 90, 0));
    let b = frame(|c| {
        c.blend_rect(W as i32 - 8 - l.gap as i32, 160 - 20, 8, 20, FG, 128);
    });
    assert_eq!(a, b);
    let huge = ScrollbarStyle {
        radius: Some(50.0),
        ..base
    };
    let l = look(&huge, 1.0, (8, 16));
    let _ = frame(|c| scrollbar::draw_look(c, W as i32, 0, 160, &l, false, 10, 90, 0));
}

#[test]
fn min_handle_height_is_in_cell_heights_and_drives_the_geometry() {
    let style = ScrollbarStyle {
        min_handle_height: Some(3.0),
        ..ScrollbarStyle::default()
    };
    let l = look(&style, 1.0, (8, 16));
    assert_eq!(l.min_thumb, 48);
    // 줄이 아주 많아도 손잡이는 48px 밑으로 줄지 않는다
    assert_eq!(scrollbar::thumb_min(160, 10, 100_000, 0, l.min_thumb).unwrap().1, 48);
    assert_eq!(
        scrollbar::thumb(160, 10, 100_000, 0).unwrap().1,
        20,
        "정하지 않은 길은 그대로 20px"
    );
    // 누르기·끌기도 같은 길이로 본다: 맨 아래에서 손잡이는 112..160
    assert_eq!(
        scrollbar::hit_min(160, 10, 100_000, 0, 130.0, 48),
        Some(scrollbar::Hit::Thumb { grab: 18 })
    );
    assert_eq!(
        scrollbar::hit(160, 10, 100_000, 0, 130.0),
        Some(scrollbar::Hit::Page { up: true })
    );
    assert_eq!(scrollbar::jump_top_min(160, 10, 100_000, 0.0, 48).unwrap().1, 24);
    assert!(scrollbar::drag_top_min(160, 10, 100_000, 56.0, 24, 48).is_some());
}

#[test]
fn units_are_cell_width_multiples_truncated_like_kitty() {
    let style = ScrollbarStyle {
        width: Some(0.5),
        hover_width: Some(1.0),
        gap: Some(0.1),
        radius: Some(0.3),
        ..ScrollbarStyle::default()
    };
    // 칸 너비 9: 4.5→4, 9, 0.9→0, 2.7→2
    let l = look(&style, 1.0, (9, 18));
    assert_eq!((l.width, l.hover_width, l.gap, l.radius), (4, 9, 0, 2));
    // 설정으로 넓힌 폭·간격은 마우스 띠(gutter)가 덮는다
    let wide = ScrollbarStyle {
        hover_width: Some(4.0),
        gap: Some(1.0),
        ..ScrollbarStyle::default()
    };
    let l = look(&wide, 1.0, (10, 20));
    assert_eq!(l.gutter(1.0), 10 + 40);
}
