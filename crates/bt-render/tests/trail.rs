//! 커서 흔적(kitty `cursor_trail`, #891)의 상태 갱신과 사각형 쪼개기. 시간은 값으로 넣어 준다(시계를 읽지 않는다)

use std::time::Duration;

use bt_core::Rgb;
use bt_render::trail::{
    CursorTrail, FRAME, PxRect, RectF, TrailFrame, TrailInput, TrailLayout, TrailPaint, TrailParams, TrailQuad,
    trail_rects,
};

const CW: u32 = 10;
const CH: u32 = 20;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// kitty 기본값: `cursor_trail 3`, decay `0.1 0.4`, 문턱 2칸
fn params() -> TrailParams {
    TrailParams {
        delay: ms(3),
        decay_fast: 0.1,
        decay_slow: 0.4,
        threshold: (2, 2),
    }
}

fn input(col: usize, row: usize, visible: bool) -> TrailInput {
    TrailInput {
        cell: (col, row),
        rect: RectF::new(
            (col as u32 * CW) as f32,
            (row as u32 * CH) as f32,
            ((col as u32 + 1) * CW) as f32,
            ((row as u32 + 1) * CH) as f32,
        ),
        visible,
        layout: TrailLayout {
            cell: (CW, CH),
            grid: (80, 24),
        },
    }
}

/// 커서가 (0,0)에서 오래 가만히 있어 불투명도가 1이 된 흔적
fn settled(p: &TrailParams) -> (CursorTrail, Duration) {
    let mut t = CursorTrail::new();
    let mut now = ms(0);
    while now <= ms(1000) {
        let f = t.update(now, p, &input(0, 0, true));
        assert!(f.quad.is_none(), "가만히 있는 커서에는 흔적이 없다");
        now += ms(8);
    }
    (t, now)
}

/// `from`부터 `until`까지 8ms 간격으로 돌려 프레임을 모은다
fn run(
    t: &mut CursorTrail,
    p: &TrailParams,
    inp: &TrailInput,
    from: Duration,
    until: Duration,
) -> Vec<(Duration, TrailFrame)> {
    let mut out = Vec::new();
    let mut now = from;
    while now <= until {
        out.push((now, t.update(now, p, inp)));
        now += ms(8);
    }
    out
}

#[test]
fn idle_cursor_asks_for_no_wakeup() {
    let p = params();
    let (mut t, now) = settled(&p);
    let f = t.update(now, &p, &input(0, 0, true));
    assert_eq!(f.quad, None);
    assert!(!f.repaint, "그릴 것이 없으면 다시 그리지 않는다");
    assert_eq!(f.wake, None, "가만히 있을 때는 깨우지 않는다(CPU를 쓰지 않는다)");
}

#[test]
fn far_jump_animates_then_stops_and_asks_for_no_more_frames() {
    let p = params();
    let (mut t, start) = settled(&p);
    let frames = run(&mut t, &p, &input(50, 20, true), start, start + ms(1200));
    // 옮긴 것을 본 첫 프레임은 지연(3ms) 때문에 아직 그리지 않고, 지연이 끝날 때 깨워 달라고 한다
    let first = &frames[0].1;
    assert!(first.quad.is_none());
    assert_eq!(first.wake, Some(ms(3)));
    // 그 뒤 움직이는 동안은 프레임마다 깨운다
    let drawn: Vec<_> = frames.iter().filter(|(_, f)| f.quad.is_some()).collect();
    assert!(drawn.len() > 10, "흔적이 여러 프레임 그려진다: {}", drawn.len());
    assert!(drawn.iter().all(|(_, f)| f.wake == Some(FRAME) && f.repaint));
    // 끝나면 지우려고 한 번 다시 그리고(그리는 것은 없음), 그 뒤로는 깨우지 않는다
    let last_drawn = frames.iter().rposition(|(_, f)| f.quad.is_some()).unwrap();
    let erase = &frames[last_drawn + 1].1;
    assert!(erase.quad.is_none() && erase.repaint && erase.wake.is_none());
    for (_, f) in &frames[last_drawn + 2..] {
        assert!(!f.repaint && f.wake.is_none() && f.quad.is_none());
    }
    // 가장 느린 모서리의 감쇠 시간(0.4초) 안팎에서 끝난다
    let ended = frames[last_drawn].0 - start;
    assert!(ended >= ms(300) && ended <= ms(700), "{ended:?}");
}

#[test]
fn corners_follow_at_different_speeds_so_the_trail_stretches() {
    let p = params();
    let (mut t, start) = settled(&p);
    let target = input(50, 0, true);
    let frames = run(&mut t, &p, &target, start, start + ms(60));
    let q = frames.iter().rev().find_map(|(_, f)| f.quad).expect("그려진 흔적");
    // 모서리 순서: (오른쪽 위, 오른쪽 아래, 왼쪽 아래, 왼쪽 위). 오른쪽으로 뛰었으니 오른쪽 모서리가 더 가까이 따라왔다
    let (right, left) = (q.x[0], q.x[3]);
    assert!(right > left + 1.0, "앞 모서리가 뒤 모서리보다 앞선다: {right} {left}");
    assert!(right < 510.0 && left < 500.0, "아직 도착하지 않았다");
    // 사각형 모양은 늘어난다(오른쪽 모서리는 0.1초에, 왼쪽 모서리는 0.4초에 가까워진다)
    assert!(q.x[0] - q.x[3] > CW as f32 * 2.0);
}

#[test]
fn small_moves_below_the_threshold_have_no_trail() {
    let p = params();
    let (mut t, start) = settled(&p);
    // 2칸 오른쪽·아래: 문턱(2, 2) 이하라 흔적 없이 바로 간다
    let frames = run(&mut t, &p, &input(2, 2, true), start, start + ms(200));
    assert!(frames.iter().all(|(_, f)| f.quad.is_none() && !f.repaint));
    // 3칸이면 흔적이 생긴다
    let (mut t, start) = settled(&p);
    let frames = run(&mut t, &p, &input(3, 0, true), start, start + ms(200));
    assert!(frames.iter().any(|(_, f)| f.quad.is_some()));
    // 문턱 0이면 한 칸 옮겨도 생긴다
    let p0 = TrailParams {
        threshold: (0, 0),
        ..params()
    };
    let (mut t, start) = settled(&p0);
    let frames = run(&mut t, &p0, &input(1, 0, true), start, start + ms(200));
    assert!(frames.iter().any(|(_, f)| f.quad.is_some()));
}

#[test]
fn a_cursor_that_keeps_moving_is_not_followed_until_it_rests() {
    let p = TrailParams {
        delay: ms(50),
        ..params()
    };
    let (mut t, start) = settled(&p);
    // 20ms 간격으로 계속 다른 칸을 가리키는 커서(복잡한 화면 갱신 중): 쉬기 전에는 흔적이 없다
    let mut now = start;
    for i in 0..10 {
        let f = t.update(now, &p, &input(10 + i * 4, 5, true));
        assert!(f.quad.is_none(), "{i}번째");
        assert!(f.wake.is_some_and(|w| w <= ms(50)));
        now += ms(20);
    }
    // 쉬기 시작한 지 50ms가 지나면 그때 따라가기 시작한다
    let rest = input(46, 5, true);
    let frames = run(&mut t, &p, &rest, now, now + ms(100));
    let first = frames.iter().position(|(_, f)| f.quad.is_some()).expect("쉰 뒤 흔적");
    assert!(
        frames[first].0 - now >= ms(30),
        "지연 전에는 그리지 않는다: {:?}",
        frames[first].0 - now
    );
}

#[test]
fn hidden_cursor_fades_the_trail_out_and_then_follows_nothing() {
    let p = params();
    let (mut t, start) = settled(&p);
    // 커서를 숨긴 채 뛴다(kitty: 불투명도가 decay_slow 동안 0으로 줄어든다)
    let frames = run(&mut t, &p, &input(40, 10, false), start, start + ms(1000));
    let opac: Vec<f32> = frames
        .iter()
        .filter(|(_, f)| f.quad.is_some())
        .map(|(_, f)| f.opacity)
        .collect();
    assert!(!opac.is_empty());
    assert!(
        opac.windows(2).all(|w| w[1] <= w[0] + 1e-6),
        "불투명도는 줄기만 한다: {opac:?}"
    );
    assert!(opac[0] < 1.0);
    // 다 사라진 뒤에는 숨은 커서의 이동이 흔적을 만들지 않는다
    let later = start + ms(1000);
    let frames = run(&mut t, &p, &input(5, 2, false), later, later + ms(300));
    assert!(frames.iter().all(|(_, f)| f.quad.is_none()));
}

#[test]
fn trail_fades_in_when_the_cursor_is_shown_again() {
    let p = params();
    let (mut t, start) = settled(&p);
    let hidden = run(&mut t, &p, &input(0, 0, false), start, start + ms(600));
    assert!(hidden.iter().all(|(_, f)| f.quad.is_none()));
    let now = start + ms(608);
    // 다시 보이자마자 뛰면 불투명도가 0에서 올라오는 중이라 흔적이 흐리다
    let frames = run(&mut t, &p, &input(30, 0, true), now, now + ms(400));
    let first = frames
        .iter()
        .find_map(|(_, f)| f.quad.map(|_| f.opacity))
        .expect("흔적");
    assert!(first < 0.5, "{first}");
}

#[test]
fn layout_change_snaps_without_a_trail() {
    let p = params();
    let (mut t, start) = settled(&p);
    let mut big = input(30, 10, true);
    big.layout.cell = (CW * 2, CH * 2);
    big.rect = RectF::new(600.0, 400.0, 620.0, 440.0);
    let frames = run(&mut t, &p, &big, start, start + ms(200));
    assert!(
        frames.iter().all(|(_, f)| f.quad.is_none()),
        "글자 크기가 바뀌면 흔적 없이 따라간다"
    );
}

#[test]
fn first_frame_snaps_to_the_cursor() {
    let p = params();
    let mut t = CursorTrail::new();
    let frames = run(&mut t, &p, &input(40, 12, true), ms(0), ms(300));
    assert!(
        frames.iter().all(|(_, f)| f.quad.is_none()),
        "처음 본 커서는 원점에서 날아오지 않는다"
    );
}

#[test]
fn zero_decay_does_not_divide_by_zero() {
    let p = TrailParams {
        decay_fast: 0.0,
        decay_slow: 0.0,
        ..params()
    };
    let (mut t, start) = settled(&p);
    let frames = run(&mut t, &p, &input(40, 10, true), start, start + ms(100));
    for (_, f) in &frames {
        if let Some(q) = f.quad {
            assert!(q.x.iter().chain(&q.y).all(|v| v.is_finite()));
        }
        assert!(f.opacity.is_finite());
    }
}

fn paint(quad: TrailQuad, mask: RectF) -> TrailPaint {
    TrailPaint {
        quad,
        mask,
        color: Rgb::new(200, 100, 50),
        alpha: 200,
    }
}

fn covered(rects: &[PxRect], x: i32, y: i32) -> bool {
    rects
        .iter()
        .any(|r| x >= r.x && x < r.x + r.w as i32 && y >= r.y && y < r.y + r.h as i32)
}

#[test]
fn axis_aligned_quad_becomes_one_rect_without_the_cursor() {
    // 위쪽 줄 4~8칸의 오른쪽으로 뻗은 직사각형 흔적: (100,0)~(300,40), 커서는 (260,0)~(300,40)
    let q = TrailQuad {
        x: [300.0, 300.0, 100.0, 100.0],
        y: [0.0, 40.0, 40.0, 0.0],
    };
    let mask = RectF::new(260.0, 0.0, 300.0, 40.0);
    let rects = trail_rects(&paint(q, mask), 800, 480);
    assert!(covered(&rects, 100, 0) && covered(&rects, 259, 39));
    assert!(
        !covered(&rects, 99, 0) && !covered(&rects, 260, 10) && !covered(&rects, 299, 39),
        "커서 안은 비운다"
    );
    let area: u32 = rects.iter().map(|r| r.w * r.h).sum();
    assert_eq!(area, 160 * 40);
    assert!(
        rects.len() <= 2,
        "같은 가로 범위의 줄은 한 사각형으로 합친다: {}",
        rects.len()
    );
}

#[test]
fn slanted_quad_covers_its_area_and_stays_inside_bounds() {
    // 평행사변형: 면적 = 밑변 100 × 높이 40 = 4000
    let q = TrailQuad {
        x: [200.0, 220.0, 120.0, 100.0],
        y: [0.0, 40.0, 40.0, 0.0],
    };
    let rects = trail_rects(&paint(q, RectF::new(-1.0, -1.0, -1.0, -1.0)), 800, 480);
    let area: u32 = rects.iter().map(|r| r.w * r.h).sum();
    assert!((3900..=4100).contains(&area), "{area}");
    // 가장자리는 기울어 있다: 위 줄은 100부터, 아래 줄은 120부터
    assert!(covered(&rects, 101, 1) && !covered(&rects, 101, 38));
    assert!(covered(&rects, 121, 38));
    // 화면 밖은 자른다
    let q = TrailQuad {
        x: [900.0, 900.0, 700.0, 700.0],
        y: [-20.0, 600.0, 600.0, -20.0],
    };
    let rects = trail_rects(&paint(q, RectF::new(-1.0, -1.0, -1.0, -1.0)), 800, 480);
    assert!(
        rects
            .iter()
            .all(|r| r.x >= 0 && r.y >= 0 && r.x + r.w as i32 <= 800 && r.y + r.h as i32 <= 480)
    );
    assert!(covered(&rects, 799, 479) && covered(&rects, 700, 0));
}

mod cpu {
    use super::*;
    use bt_render::{
        Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, Underline, draw_grid, draw_grid_rows,
    };

    const BG: Rgb = Rgb { r: 20, g: 22, b: 30 };

    fn blank() -> DrawCell {
        DrawCell {
            ch: ' ',
            fg: Rgb::new(220, 220, 210),
            bg: BG,
            bold: false,
            italic: false,
            underline: Underline::None,
            strike: false,
            wide: false,
            spacer: false,
            selected: false,
            emoji: false,
            cluster: None,
            cluster_cells: 0,
            overline: false,
            underline_color: None,
            hidden: false,
            faint: None,
        }
    }

    fn frame(f: &mut Fonts, trail: Option<TrailPaint>, dirty: Option<&[bool]>) -> (Vec<u32>, u32, u32) {
        let cells = vec![blank(); 20 * 8];
        let t = bt_core::Theme::default();
        let view = GridView {
            cols: 20,
            rows: 8,
            cells: &cells,
            cursor: Some(DrawCursor {
                col: 10,
                row: 4,
                style: CursorStyle::Block,
                wide: false,
                color: Rgb::new(255, 255, 255),
                text_color: Rgb::new(0, 0, 0),
                opacity: 1.0,
            }),
            selection_bg: t.selection_background,
            selection_fg: t.selection_foreground,
            selection_bg_cell: Default::default(),
            selection_fg_cell: Default::default(),
            preedit: None,
            min_contrast: 1.0,
            trail,
        };
        let m = f.metrics();
        let (w, h) = (m.width * 20, m.height * 8);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(BG);
        match dirty {
            None => draw_grid(&mut c, f, 0, 0, &view),
            Some(d) => {
                draw_grid_rows(&mut c, f, 0, 0, &view, d);
            }
        }
        (buf, w, h)
    }

    fn px(buf: &[u32], w: u32, x: u32, y: u32) -> u32 {
        buf[(y * w + x) as usize] & 0x00ff_ffff
    }

    fn setup() -> Option<(Fonts, TrailPaint)> {
        let mut f = Fonts::new(&FontConfig::default()).ok()?;
        let m = f.metrics();
        let (cw, ch) = (m.width as f32, m.height as f32);
        // 왼쪽 위(2칸, 1줄)에서 커서(10칸, 4줄)까지 뻗은 사각형
        let quad = TrailQuad {
            x: [11.0 * cw, 11.0 * cw, 3.0 * cw, 2.0 * cw],
            y: [4.0 * ch, 5.0 * ch, 2.0 * ch, 1.0 * ch],
        };
        let mask = RectF::new(10.0 * cw, 4.0 * ch, 11.0 * cw, 5.0 * ch);
        let _ = &mut f;
        Some((
            f,
            TrailPaint {
                quad,
                mask,
                color: Rgb::new(200, 0, 0),
                alpha: 128,
            },
        ))
    }

    #[test]
    fn trail_blends_over_cells_and_leaves_the_cursor_cell_alone() {
        let Some((mut f, t)) = setup() else { return };
        let m = f.metrics();
        let (plain, w, _) = frame(&mut f, None, None);
        let (with, _, _) = frame(&mut f, Some(t), None);
        // 사각형 안은 바탕과 붉은색(알파 128)을 섞은 색
        let (cx, cy) = (6 * m.width + m.width / 2, 3 * m.height + m.height / 4);
        assert_eq!(px(&plain, w, cx, cy), 0x0014_161e);
        assert_eq!(px(&with, w, cx, cy), 0x006e_0b0f, "{:06x}", px(&with, w, cx, cy));
        // 커서 칸은 같다
        for (x, y) in [
            (10 * m.width + 1, 4 * m.height + 1),
            (11 * m.width - 1, 5 * m.height - 1),
        ] {
            assert_eq!(px(&plain, w, x, y), px(&with, w, x, y));
        }
        // 흔적 밖은 같다
        assert_eq!(px(&plain, w, 0, 0), px(&with, w, 0, 0));
        assert_eq!(
            px(&plain, w, 19 * m.width, 7 * m.height),
            px(&with, w, 19 * m.width, 7 * m.height)
        );
    }

    #[test]
    fn trail_respects_the_repainted_band_only() {
        let Some((mut f, t)) = setup() else { return };
        let m = f.metrics();
        // 이미 흔적이 그려진 프레임 위에 두 번째 줄만 다시 그리면 그 띠 밖은 건드리지 않는다
        let (full, w, _) = frame(&mut f, Some(t), None);
        let mut dirty = vec![false; 8];
        dirty[2] = true;
        let (part, _, _) = frame(&mut f, Some(t), Some(&dirty));
        // 다시 그린 띠(줄 1~3) 안은 통째 그린 것과 같고, 그 밖의 줄(6, 7)은 건드리지 않아 바탕 그대로다
        let (x, y) = (6 * m.width + m.width / 2, 3 * m.height + m.height / 4);
        assert_eq!(px(&full, w, x, y), px(&part, w, x, y));
        assert_eq!(px(&part, w, 3 * m.width, 6 * m.height + 2), 0x0014_161e);
        assert_ne!(px(&part, w, x, y), 0x0014_161e);
    }

    /// `cursor_target`은 `draw_cursor`가 칠하는 자리와 같다(흔적이 칠하지 않을 자리, 흔적의 목표)
    #[test]
    fn cursor_target_matches_the_painted_cursor() {
        let Ok(mut f) = Fonts::new(&FontConfig::default()) else {
            return;
        };
        let m = f.metrics();
        for (style, wide) in [
            (CursorStyle::Block, false),
            (CursorStyle::Block, true),
            (CursorStyle::Beam, false),
            (CursorStyle::Underline, false),
            (CursorStyle::Underline, true),
        ] {
            let cells = vec![blank(); 20 * 8];
            let t = bt_core::Theme::default();
            let view = GridView {
                cols: 20,
                rows: 8,
                cells: &cells,
                cursor: Some(DrawCursor {
                    col: 7,
                    row: 3,
                    style,
                    wide,
                    color: Rgb::new(255, 255, 255),
                    text_color: Rgb::new(0, 0, 0),
                    opacity: 1.0,
                }),
                selection_bg: t.selection_background,
                selection_fg: t.selection_foreground,
                selection_bg_cell: Default::default(),
                selection_fg_cell: Default::default(),
                preedit: None,
                min_contrast: 1.0,
                trail: None,
            };
            let (w, h) = (m.width * 20, m.height * 8);
            let mut buf = vec![0u32; (w * h) as usize];
            let mut c = Canvas::new(&mut buf, w, h);
            c.clear(BG);
            draw_grid(&mut c, &mut f, 0, 0, &view);
            let lit: Vec<(u32, u32)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| px(&buf, w, x, y) == 0x00ff_ffff)
                .collect();
            let (x0, x1) = (
                lit.iter().map(|p| p.0).min().unwrap(),
                lit.iter().map(|p| p.0).max().unwrap(),
            );
            let (y0, y1) = (
                lit.iter().map(|p| p.1).min().unwrap(),
                lit.iter().map(|p| p.1).max().unwrap(),
            );
            let r = bt_render::trail::cursor_target(style, &m, 7, 3, wide);
            assert_eq!(
                (r.left as u32, r.top as u32, r.right as u32, r.bottom as u32),
                (x0, y0, x1 + 1, y1 + 1),
                "{style:?} wide={wide}"
            );
        }
    }
}
