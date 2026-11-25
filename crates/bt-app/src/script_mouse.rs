//! 스크립트 `send mouse button`·`send mouse position`·`send mouse scroll`(#662)의 순수 부분: 좌표, 휠 줄 수, 누름 횟수, 상태.
//!
//! 사람의 마우스(`App::window_event`)와 스크립트는 같은 처리(`app/mouse_core.rs`)를 지나지만 상태는 따로 둔다.
//! 처리는 위치·수식 키(`PtrIn`)와 상태(`PtrState`)를 인자로만 받는다. 사람은 창·칸에 흩어진 값을 한 `PtrState`로 모아 넘기고,
//! 스크립트는 칸마다 둔 `ScriptMouse`의 값을 넘긴다. 그래서 스크립트가 사람의 포인터·누름 횟수·고르는 중·휠 누적을 건드리지 못한다.
//!
//! 위치는 Ghostty처럼 칸 왼쪽 위에서 잰 배율 전 점 좌표다(`embedded.zig` `cursorPosToPixels`가 배율을 곱한다). 칸 밖 값은 칸 안으로 자른다.
//! 휠은 Ghostty `Surface.zig` `scrollCallback`(v1.4.0)의 계산을 따른다. `momentum`은 Ghostty 코어가 읽지 않으므로 이 앱도 읽지 않는다.

// macOS에서만 AppleScript 명령이 부른다(Linux 빌드에서는 시험만 쓴다)
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::time::{Duration, Instant};

use winit::keyboard::ModifiersState;

/// 한 번의 휠 명령이 굴릴 수 있는 줄 수의 상한(가로·세로 각각). 아주 큰 값이 와도 스크롤백을 한 번에 날리지 않게 한다
pub const MAX_SCROLL_LINES: f64 = 1000.0;

/// 마우스 사건에 실린 수식 키. 사람은 `App::mods`(`key-remap`을 거친 것)에서, 스크립트는 명령의 글에서 만든다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PtrMods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub sup: bool,
}

impl PtrMods {
    pub fn from_state(m: ModifiersState) -> Self {
        Self {
            shift: m.shift_key(),
            alt: m.alt_key(),
            ctrl: m.control_key(),
            sup: m.super_key(),
        }
    }
}

/// 스크립트의 수식 키 글(`send key`와 같은 규칙, `script_key::parse_modifiers`)을 읽는다. 글이 없으면 없음.
/// `remap`은 `key-remap`을 적용한다(`send key`와 사람이 누른 키처럼)
pub fn parse_mods(text: Option<&str>, remap: impl FnOnce(ModifiersState) -> ModifiersState) -> Result<PtrMods, String> {
    let state = match text {
        Some(t) => crate::script_key::parse_modifiers(t)?,
        None => ModifiersState::empty(),
    };
    Ok(PtrMods::from_state(remap(state)))
}

/// 마우스 처리가 상태로 읽고 쓰는 값. 사람은 창(`selecting`·`last_click`·휠 누적)과 칸(`down`·`last_cell`)에 흩어진 값을 모아 넘기고,
/// 스크립트는 칸의 `ScriptMouse`가 쥔 것을 넘긴다
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PtrState {
    /// 글을 고르는 중(왼쪽 단추를 누르고 아직 떼지 않았다)
    pub selecting: bool,
    /// 마지막 누름(이어지는 누름이면 처음 누른 자리를 쥔다, `Click`)
    pub last_click: Option<Click>,
    /// 끌어 고르는 중 포인터가 위·아래 가장자리 밖에 있어 자동 스크롤할 방향(#806, `mouse::autoscroll_dir`)
    pub autoscroll: crate::mouse::Autoscroll,
    /// 다음 자동 스크롤 틱의 때. 방향이 없으면 없다(이 값이 없으면 이벤트 루프는 자동 스크롤 때문에 깨지 않는다)
    pub autoscroll_next: Option<Instant>,
    /// 누를 때 대체 화면이었는가. 끄는 중 화면이 바뀌면 누른 자리가 다른 화면의 것이라 끌기를 멈춘다(Ghostty의 `validatedLeftClickPin`)
    pub press_alt: bool,
    /// 눌린 단추 비트(`mouse::button_bit`)
    pub down: u8,
    /// 마지막에 보고한 칸(움직임 중복 제거)
    pub last_cell: Option<(usize, usize)>,
    /// 두 번 누르기로 링크 전체를 고른 칸(#701). 끌어서 이 칸을 벗어나면 그 칸 낱말부터 낱말 단위로 끈다
    pub link_word: Option<(usize, usize)>,
    /// 세로·가로 휠 누적(줄)
    pub accum: f64,
    pub accum_x: f64,
}

/// 칸마다 둔 스크립트 마우스 상태. 사람의 상태와 아무것도 나누지 않는다
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptMouse {
    pub state: PtrState,
    /// 마지막으로 보낸 수식 키(자동 스크롤 틱이 끌기에 쓴다)
    pub mods: PtrMods,
    /// 마지막으로 보낸 위치(칸 왼쪽 위에서 잰 점 좌표). 보낸 적이 없으면 (0, 0)이다
    pub pos_pt: (f64, f64),
    /// 마지막으로 움직임을 처리한 위치(칸 왼쪽 위에서 잰 실제 픽셀). 1픽셀 미만의 움직임을 거르는 데 쓴다
    pub sent_rel: Option<(f64, f64)>,
    /// 링크 수식 키 누르기의 진행(#705, `mouse::LinkPress`)
    pub link_click: crate::mouse::LinkPress,
}

/// 칸 왼쪽 위에서 잰 실제 픽셀: 점 좌표 × 배율을 칸(크기 `size`, 실제 픽셀) 안으로 자른다(마지막 픽셀은 크기 - 1)
pub fn rel_px(size: (f64, f64), x_pt: f64, y_pt: f64, scale: f64) -> (f64, f64) {
    let clamp = |v: f64, len: f64| (v * scale).clamp(0.0, (len - 1.0).max(0.0));
    (clamp(x_pt, size.0), clamp(y_pt, size.1))
}

/// 앞 위치에서 가로나 세로로 1픽셀 이상 움직였는가(없던 위치면 늘 참). Ghostty `embedded.zig` `cursorPosCallback`이
/// 1픽셀 미만의 움직임은 처리하지 않는다
pub fn moved_enough(prev: Option<(f64, f64)>, next: (f64, f64)) -> bool {
    match prev {
        Some(p) => (p.0 - next.0).abs() >= 1.0 || (p.1 - next.1).abs() >= 1.0,
        None => true,
    }
}

/// 휠 명령 값을 줄 수로 바꾼다. 돌려주는 값은 (가로, 세로)이고 양수가 위·왼쪽이다. `cell`은 칸 크기(너비, 높이 픽셀),
/// `discrete`·`precise`는 `mouse-scroll-multiplier`의 두 값이다. Ghostty `Surface.zig` `scrollCallback`(v1.4.0, 3584줄 이하. 세로 3599~3653줄, 가로 3657~3675줄)과 같다:
/// - 세로, 정밀: 값이 픽셀이다. 줄 = 픽셀 / 줄 높이 × 정밀 배율
/// - 세로, 정밀 아님: 값이 눈금 수다. macOS는 크기를 1 이상으로 올리고(0.1도 한 눈금) 줄 = 눈금 × 이산 배율
/// - 가로, 정밀: 픽셀 / 칸 너비(배율 없음). 정밀 아님: 반올림한 눈금 수(배율 없음)
///
/// `momentum`은 Ghostty 코어가 읽지 않아(`ScrollMods`에 실려 오지만 `scrollCallback`은 `precision`만 본다) 여기에도 없다
pub fn scroll_lines(precision: bool, x: f64, y: f64, cell: (f64, f64), discrete: f64, precise: f64) -> (f64, f64) {
    let (cw, ch) = (cell.0.max(1.0), cell.1.max(1.0));
    let lines_y = if y == 0.0 {
        0.0
    } else if precision {
        y / ch * precise
    } else {
        let ticks = if y > 0.0 { y.max(1.0) } else { y.min(-1.0) };
        ticks * discrete
    };
    let lines_x = if x == 0.0 {
        0.0
    } else if precision {
        x / cw
    } else {
        x.round()
    };
    let cap = |v: f64| v.clamp(-MAX_SCROLL_LINES, MAX_SCROLL_LINES);
    (cap(lines_x), cap(lines_y))
}

/// 왼쪽 누름 한 번의 기록. 이어지는 누름이면 `time`만 새로 하고 나머지는 처음 누른 자리를 그대로 쥔다
/// (Ghostty `SelectionGesture`의 `left_click_xpos`·`left_click_pin`은 처음 누름에서만 정한다)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Click {
    /// 가장 최근 누른 때
    pub time: Instant,
    /// 처음 누른 칸(열, 줄)
    pub col: usize,
    pub row: usize,
    /// 누름 횟수(1~3)
    pub count: u8,
    /// 처음 누른 자리(창 실제 픽셀). 이어지는 누름인지 거리로 가른다
    pub origin: (f64, f64),
}

/// 이번 왼쪽 누름의 기록. 앞 누름과의 간격이 `interval` 이하이고 처음 누른 자리에서 픽셀 거리가 `cell_width` 이하이면
/// 이어지는 누름이라 횟수가 `min(앞 횟수 + 1, 3)`이다(Ghostty `SelectionGesture.pressRepeat`: 네 번째도 3, 거리는 칸 단위가 아니라 픽셀).
/// 아니면 새 한 번이다. 앞 누름보다 이른 때(시계가 거꾸로 감)도 새 한 번이다
pub fn next_click(
    last: Option<Click>,
    now: Instant,
    cell: (usize, usize),
    pos: (f64, f64),
    cell_width: f64,
    interval: Duration,
) -> Click {
    if let Some(l) = last {
        let within = now.checked_duration_since(l.time).is_some_and(|d| d <= interval);
        let dist = ((pos.0 - l.origin.0).powi(2) + (pos.1 - l.origin.1).powi(2)).sqrt();
        if within && dist <= cell_width {
            return Click {
                time: now,
                count: (l.count + 1).min(3),
                ..l
            };
        }
    }
    Click {
        time: now,
        col: cell.0,
        row: cell.1,
        count: 1,
        origin: pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 스크립트 위치는 칸 왼쪽 위에서 잰 점 좌표이고, 실제 픽셀은 점 × 배율이다. 칸 밖 값은 칸 안으로 자른다
    #[test]
    fn position_is_points_times_scale_clamped_into_the_pane() {
        let size = (800.0, 400.0);
        assert_eq!(rel_px(size, 10.0, 20.0, 2.0), (20.0, 40.0));
        assert_eq!(rel_px(size, 10.5, 0.25, 2.0), (21.0, 0.5));
        // 칸 밖: 안으로 자른다(마지막 픽셀은 w-1)
        assert_eq!(rel_px(size, -5.0, -1e9, 2.0), (0.0, 0.0));
        assert_eq!(rel_px(size, 1e9, 1e9, 2.0), (799.0, 399.0));
        assert_eq!(rel_px(size, f64::MAX, f64::MAX, 2.0), (799.0, 399.0));
        // 아주 작은 칸에서도 음수가 되지 않는다
        assert_eq!(rel_px((0.0, 0.0), 3.0, 3.0, 1.0), (0.0, 0.0));
    }

    /// 위치를 한 번도 보내지 않았으면 칸 왼쪽 위 (0, 0)이다
    #[test]
    fn an_unset_position_is_the_top_left_corner() {
        let sm = ScriptMouse::default();
        assert_eq!(sm.pos_pt, (0.0, 0.0));
        assert_eq!(rel_px((800.0, 400.0), sm.pos_pt.0, sm.pos_pt.1, 2.0), (0.0, 0.0));
        assert!(sm.sent_rel.is_none());
    }

    /// Ghostty `embedded.zig` `cursorPosCallback`: 앞 위치에서 가로·세로 모두 1픽셀 미만이면 움직임으로 치지 않는다
    #[test]
    fn a_move_under_one_pixel_is_not_a_move_like_ghostty() {
        assert!(moved_enough(None, (0.0, 0.0)));
        assert!(!moved_enough(Some((5.0, 5.0)), (5.5, 5.9)));
        assert!(moved_enough(Some((5.0, 5.0)), (6.0, 5.0)));
        assert!(moved_enough(Some((5.0, 5.0)), (5.0, 3.5)));
    }

    /// 정밀 스크롤: 픽셀 값이다. 줄 = 픽셀 / 줄 높이 × 정밀 배율(Surface.zig scrollCallback)
    #[test]
    fn precision_scroll_is_pixels_over_the_cell_height() {
        let (x, y) = scroll_lines(true, 0.0, 32.0, (8.0, 16.0), 3.0, 1.0);
        assert_eq!((x, y), (0.0, 2.0));
        let (_, y) = scroll_lines(true, 0.0, -8.0, (8.0, 16.0), 3.0, 2.0);
        assert_eq!(y, -1.0);
        // 가로는 픽셀 / 칸 너비, 배율 없음
        let (x, _) = scroll_lines(true, 24.0, 0.0, (8.0, 16.0), 3.0, 2.0);
        assert_eq!(x, 3.0);
    }

    /// 정밀하지 않은 스크롤: 눈금 수다. macOS는 크기를 1 이상으로 올린다(0.1도 한 눈금). 줄 = 눈금 × 이산 배율.
    /// 가로는 반올림한 눈금 수이고 배율이 없다
    #[test]
    fn wheel_ticks_are_at_least_one_and_scaled_like_ghostty_on_macos() {
        let at = |y: f64| scroll_lines(false, 0.0, y, (8.0, 16.0), 3.0, 1.0).1;
        assert_eq!(at(0.1), 3.0);
        assert_eq!(at(-0.1), -3.0);
        assert_eq!(at(2.0), 6.0);
        assert_eq!(at(0.0), 0.0);
        let ax = |x: f64| scroll_lines(false, x, 0.0, (8.0, 16.0), 3.0, 1.0).0;
        assert_eq!(ax(1.4), 1.0);
        assert_eq!(ax(-2.6), -3.0);
        assert_eq!(ax(0.4), 0.0);
    }

    /// 아주 큰 값이 와도 한 번에 굴리는 줄 수에 상한이 있다
    #[test]
    fn scroll_lines_are_capped() {
        let (x, y) = scroll_lines(true, 1e300, -1e300, (8.0, 16.0), 3.0, 1.0);
        assert_eq!((x, y), (MAX_SCROLL_LINES, -MAX_SCROLL_LINES));
    }

    /// 누름 횟수는 상태마다 센다: 사람이 한 번 누른 직후 스크립트가 같은 칸을 한 번 누르면 두 번 누르기가 아니다
    #[test]
    fn a_script_press_after_a_human_press_is_not_a_double_click() {
        let now = Instant::now();
        let every = Duration::from_millis(500);
        let at = (35.0, 80.0);
        let human = PtrState::default();
        let script = ScriptMouse::default();
        // 사람이 (3, 4)를 눌렀다
        let human_click = next_click(human.last_click, now, (3, 4), at, 10.0, every);
        let human = PtrState {
            last_click: Some(human_click),
            ..human
        };
        assert_eq!(human_click.count, 1);
        // 스크립트가 바로 같은 칸을 누른다: 스크립트 상태에는 앞 누름이 없다
        let script_click = next_click(script.state.last_click, now, (3, 4), at, 10.0, every);
        assert_eq!(script_click.count, 1);
        // 사람 쪽 기록은 그대로다
        assert_eq!(human.last_click, Some(human_click));
        // 스크립트끼리는 센다: 1, 2, 3, 3(네 번째도 3, Ghostty `pressRepeat`)
        let mut st = script.state;
        for want in [1u8, 2, 3, 3] {
            let c = next_click(st.last_click, now, (3, 4), at, 10.0, every);
            assert_eq!(c.count, want);
            st.last_click = Some(c);
        }
        // 처음 누른 자리에서 칸 너비보다 멀거나 간격이 길면 1
        assert_eq!(
            next_click(st.last_click, now, (6, 4), (65.0, 80.0), 10.0, every).count,
            1
        );
        assert_eq!(
            next_click(
                st.last_click,
                now + every + Duration::from_millis(1),
                (3, 4),
                at,
                10.0,
                every
            )
            .count,
            1
        );
    }

    /// Ghostty `SelectionGesture` 시험 1949(repeat increments)·1963(clamps at triple click)·2004(distant press resets)·
    /// 2019(expired repeat resets)·2037(backwards repeat time resets)과 `Surface.zig` 4129줄(`max_distance` = 칸 너비)
    #[test]
    fn click_count_follows_ghostty_press_repeat() {
        let t0 = Instant::now();
        let every = Duration::from_millis(500);
        let press = |last: Option<Click>, now: Instant, cell: (usize, usize), pos: (f64, f64)| {
            next_click(last, now, cell, pos, 10.0, every)
        };
        // 1963: 네 번 빠르게 누르면 [1, 2, 3, 3]
        let mut last = None;
        let mut counts = vec![];
        for _ in 0..4 {
            let c = press(last, t0, (1, 1), (15.0, 25.0));
            counts.push(c.count);
            last = Some(c);
        }
        assert_eq!(counts, vec![1, 2, 3, 3]);
        // 이어지는 누름은 처음 누른 자리를 쥔다: 거리는 처음 자리에서 잰다
        let first = press(None, t0, (1, 1), (19.0, 25.0));
        let second = press(Some(first), t0, (2, 1), (21.0, 25.0));
        assert_eq!(
            (second.count, second.col, second.row, second.origin),
            (2, 1, 1, (19.0, 25.0))
        );
        // 칸 경계를 넘었어도 픽셀 거리가 칸 너비 이하면 이어진다(우리 옛 규칙은 칸이 다르면 끊었다)
        let third = press(Some(second), t0, (2, 1), (29.0, 25.0));
        assert_eq!(third.count, 3);
        // 같은 칸이어도 세로로 칸 너비(10px)보다 멀면 새 누름이다(옛 규칙은 같은 칸이면 이어졌다)
        let far = press(Some(first), t0, (1, 1), (19.0, 36.0));
        assert_eq!(far.count, 1);
        // 2004: 칸 너비를 넘는 거리(11px)는 끊고, 정확히 10px은 이어진다(`distance > max_distance`)
        assert_eq!(press(Some(first), t0, (3, 1), (29.0, 25.0)).count, 2);
        assert_eq!(press(Some(first), t0, (3, 1), (29.5, 25.0)).count, 1);
        // 2019: 간격이 지났으면 새 누름. 간격과 같으면 이어진다(`since > repeat_interval`)
        assert_eq!(press(Some(first), t0 + every, (1, 1), (19.0, 25.0)).count, 2);
        assert_eq!(
            press(Some(first), t0 + every + Duration::from_millis(1), (1, 1), (19.0, 25.0)).count,
            1
        );
        // 2037: 앞 누름보다 이른 때는 새 누름
        let late = press(None, t0 + Duration::from_secs(2), (1, 1), (19.0, 25.0));
        assert_eq!(
            press(Some(late), t0 + Duration::from_secs(1), (1, 1), (19.0, 25.0)).count,
            1
        );
    }

    /// 수식 키 글은 `send key`와 같은 규칙이다(쉼표, 소문자, 모르는 이름은 오류)
    #[test]
    fn modifiers_follow_send_key() {
        assert_eq!(parse_mods(None, |m| m), Ok(PtrMods::default()));
        let m = parse_mods(Some("Shift, option"), |m| m).expect("읽힌다");
        assert_eq!(
            m,
            PtrMods {
                shift: true,
                alt: true,
                ctrl: false,
                sup: false
            }
        );
        let m = parse_mods(Some("control,command"), |m| m).expect("읽힌다");
        assert_eq!(
            m,
            PtrMods {
                shift: false,
                alt: false,
                ctrl: true,
                sup: true
            }
        );
        assert!(parse_mods(Some("shift,hyper"), |m| m).is_err());
        // `key-remap`은 사람이 누른 키처럼 적용한다
        let swap = |m: winit::keyboard::ModifiersState| {
            if m.shift_key() {
                winit::keyboard::ModifiersState::CONTROL
            } else {
                m
            }
        };
        assert_eq!(
            parse_mods(Some("shift"), swap),
            Ok(PtrMods {
                ctrl: true,
                ..PtrMods::default()
            })
        );
    }

    /// 스크립트 마우스 상태는 사람의 상태 이름을 모른다: 공유 처리(`app/mouse_core.rs`)와 스크립트 갈래(`app/script.rs`의 마우스 부분)는
    /// 사람의 포인터·호버·고르는 중·누름 횟수·링크 누름·초점 누름·휠 누적·칸 끌기·막대 누름·칸의 눌린 단추와 사람의 수식 키를 읽거나 쓰지 않는다.
    /// 값은 모두 `PtrIn`(위치·수식 키)과 `PtrState`(상태)로 넘어온다
    #[test]
    fn shared_mouse_code_never_touches_the_human_state() {
        const FORBIDDEN: &[&str] = &[
            // 사람의 수식 키는 `App::mods`(winit `ModifiersState`)다. 마우스 사건의 수식 키는 `input.mods`(`PtrMods`)로만 읽는다
            "self.mods.shift_key",
            "self.mods.alt_key",
            "self.mods.control_key",
            "self.mods.super_key",
            "(self.mods)",
            "self.raw_mods",
            "self.w.cursor_pos",
            "self.w.selecting",
            "self.w.last_click",
            "self.w.link_click",
            "self.w.focus_click",
            "self.w.scroll_accum",
            "self.w.pane_drag",
            "self.w.scrollbar_press",
            "self.w.hover",
            ".mouse_down",
            ".mouse_last_cell",
        ];
        let core = include_str!("app/mouse_core.rs");
        let script = include_str!("app/script.rs");
        let script_mouse_part = script
            .split("// 마우스 명령의 시작")
            .nth(1)
            .and_then(|s| s.split("// 마우스 명령의 끝").next())
            .expect("script.rs에 마우스 명령 구간 표시가 있다");
        for (name, src) in [
            ("app/mouse_core.rs", core),
            ("app/script.rs 마우스 구간", script_mouse_part),
        ] {
            // 주석은 건너뛴다(사람의 상태를 설명하는 글은 괜찮다)
            for (n, line) in src.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                for bad in FORBIDDEN {
                    assert!(
                        !code.contains(bad),
                        "{name}:{}에서 사람 상태 {bad}를 건드린다: {line}",
                        n + 1
                    );
                }
            }
        }
    }
}
