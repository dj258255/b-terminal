//! 마우스 보고(xterm 9/1000/1002/1003, 형식 1005/1006/1015/1016). vim·htop·less 같은 프로그램이 마우스를 쓸 때 켠다

use bt_core::config::{MiddleClickAction, MouseShiftCapture};
use std::time::Duration;

/// 이 클릭·움직임·휠을 프로그램(마우스 보고)에 보내는가. `mouse-reporting`을 끄면 늘 아니오.
/// Shift를 누르면 `mouse-shift-capture`가 `true`·`always`일 때만 프로그램이 받고, 아니면 고르기가 받는다
pub fn reports_to_program(reporting: bool, capture: MouseShiftCapture, shift: bool) -> bool {
    reporting && (!shift || capture.sends_shift())
}

/// 두 번·세 번 누르기 간격. 설정이 0이면 플랫폼 값(macOS는 OS 설정, 그 밖에는 500ms)
pub fn repeat_interval(config_ms: u32, platform: Option<Duration>) -> Duration {
    if config_ms > 0 {
        Duration::from_millis(u64::from(config_ms))
    } else {
        platform.unwrap_or(Duration::from_millis(500))
    }
}

/// OS의 두 번 누르기 간격. macOS만 안다
#[cfg(target_os = "macos")]
pub fn platform_repeat_interval() -> Option<Duration> {
    let secs = objc2_app_kit::NSEvent::doubleClickInterval();
    (secs.is_finite() && secs > 0.0).then(|| Duration::from_secs_f64(secs.min(5.0)))
}

#[cfg(not(target_os = "macos"))]
pub fn platform_repeat_interval() -> Option<Duration> {
    None
}

/// 가운데 클릭이 클립보드 붙여넣기인가. `primary-paste`는 macOS에 선택 클립보드가 없어 아니오
pub fn middle_click_pastes(action: MiddleClickAction) -> bool {
    action == MiddleClickAction::ClipboardPaste
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    /// 휠 왼쪽(단추 6, 코드 66). 오른쪽 위 방향이 양수다(Ghostty `up_right` → `six`)
    WheelLeft,
    /// 휠 오른쪽(단추 7, 코드 67)
    WheelRight,
    /// 뒤로 단추(단추 8, 코드 128)
    Back,
    /// 앞으로 단추(단추 9, 코드 129)
    Forward,
    /// 버튼을 누르지 않은 움직임(1003)
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Press,
    Release,
    Motion,
}

/// 어떤 사건을 보고하는가(DECSET 9·1000·1002·1003)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventMode {
    /// 9: 왼쪽·가운데·오른쪽 누름만, 수식 키 없음
    X10,
    /// 1000: 누름과 놓음
    Normal,
    /// 1002: 단추를 누른 채 움직임까지
    Button,
    /// 1003: 모든 움직임
    Any,
}

impl EventMode {
    /// 터미널 모드 스냅숏에서 지금 이벤트 모드. 꺼져 있으면 `None`
    pub fn from_modes(m: &bt_term::Modes) -> Option<Self> {
        if m.mouse_x10 {
            Some(Self::X10)
        } else if m.mouse_motion {
            Some(Self::Any)
        } else if m.mouse_drag {
            Some(Self::Button)
        } else if m.mouse_report {
            Some(Self::Normal)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// 좌표를 바꾸는 데 쓰는 크기(실제 픽셀). Ghostty `renderer.Size`에 해당한다.
/// 좌표는 칸(창) 왼쪽 위가 원점인 "표면 좌표", `pad_*`는 거기서 격자 원점까지의 여백이다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// 칸(창) 전체 크기
    pub screen_w: f64,
    pub screen_h: f64,
    pub cell_w: f64,
    pub cell_h: f64,
    pub pad_left: f64,
    pub pad_top: f64,
    pub cols: usize,
    pub rows: usize,
}

/// 인코딩 설정(Ghostty `mouse_encode.Options`)
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub event: EventMode,
    pub format: bt_term::MouseFormat,
    pub geometry: Geometry,
    /// 단추가 하나라도 눌려 있는가(이번 사건 포함). 칸 밖 움직임은 단추를 누른 채일 때만 보낸다
    pub any_button_pressed: bool,
}

/// 눌린 단추를 기억하는 비트. 휠은 누른 채 있지 않아 없다
pub fn button_bit(b: Button) -> Option<u8> {
    match b {
        Button::Left => Some(1),
        Button::Middle => Some(2),
        Button::Right => Some(4),
        Button::Back => Some(8),
        Button::Forward => Some(16),
        _ => None,
    }
}

/// 눌려 있는 단추 중 움직임에 실을 하나(Ghostty는 가장 먼저 찾은 것). 없으면 `None`
pub fn held_button(down: u8) -> Button {
    [
        Button::Left,
        Button::Middle,
        Button::Right,
        Button::Back,
        Button::Forward,
    ]
    .into_iter()
    .find(|&b| button_bit(b).is_some_and(|bit| down & bit != 0))
    .unwrap_or(Button::None)
}

/// 이 사건을 이벤트 모드에서 보고하는가
fn should_report(button: Button, action: Action, event: EventMode) -> bool {
    match event {
        EventMode::X10 => action == Action::Press && matches!(button, Button::Left | Button::Middle | Button::Right),
        EventMode::Normal => action != Action::Motion,
        EventMode::Button => button != Button::None,
        EventMode::Any => true,
    }
}

fn button_code(button: Button, action: Action, mods: Mods, opts: &Options) -> u32 {
    use bt_term::MouseFormat::{Sgr, SgrPixels};
    let mut acc: u32 = if button == Button::None {
        3
    } else if action == Action::Release && !matches!(opts.format, Sgr | SgrPixels) {
        // 옛 형식의 놓음은 늘 3번 단추
        3
    } else {
        match button {
            Button::Left => 0,
            Button::Middle => 1,
            Button::Right => 2,
            Button::WheelUp => 64,
            Button::WheelDown => 65,
            Button::WheelLeft => 66,
            Button::WheelRight => 67,
            Button::Back => 128,
            Button::Forward => 129,
            Button::None => 3,
        }
    };
    // X10는 수식 키를 넣지 않는다
    if opts.event != EventMode::X10 {
        if mods.shift {
            acc += 4;
        }
        if mods.alt {
            acc += 8;
        }
        if mods.ctrl {
            acc += 16;
        }
    }
    if action == Action::Motion {
        acc += 32;
    }
    acc
}

/// 표면 좌표(실제 픽셀) → 0부터 센 칸. 격자 밖은 가장자리 칸으로 당긴다
fn pos_to_cell(pos: (f64, f64), g: &Geometry) -> (usize, usize) {
    let cell = |v: f64, pad: f64, size: f64, n: usize| {
        let c = ((v - pad) / size.max(1.0)).floor().max(0.0);
        (c as usize).min(n.saturating_sub(1))
    };
    (
        cell(pos.0, g.pad_left, g.cell_w, g.cols),
        cell(pos.1, g.pad_top, g.cell_h, g.rows),
    )
}

/// 마우스 사건 하나를 바이트로(Ghostty `mouse_encode.encode`). `pos`는 표면 좌표(실제 픽셀),
/// `last_cell`은 움직임 중복 제거용으로 마지막에 보고한 칸. 보내지 않을 사건은 빈 바이트
pub fn encode(
    button: Button,
    action: Action,
    mods: Mods,
    pos: (f64, f64),
    opts: &Options,
    last_cell: &mut Option<(usize, usize)>,
) -> Vec<u8> {
    use bt_term::MouseFormat;
    if !should_report(button, action, opts.event) {
        return Vec::new();
    }
    let g = &opts.geometry;
    // 칸 밖: 놓음은 늘 보고한다. 그 밖에는 움직임 모드에서 단추를 누른 채일 때만(칸 안에서 눌러 밖으로 끄는 경우)
    let outside = pos.0 < 0.0 || pos.1 < 0.0 || pos.0 > g.screen_w || pos.1 > g.screen_h;
    if action != Action::Release
        && outside
        && (!matches!(opts.event, EventMode::Button | EventMode::Any) || !opts.any_button_pressed)
    {
        return Vec::new();
    }
    let cell = pos_to_cell(pos, g);
    // 움직임은 칸이 바뀔 때만(픽셀 좌표 형식은 늘)
    if action == Action::Motion && opts.format != MouseFormat::SgrPixels && *last_cell == Some(cell) {
        return Vec::new();
    }
    *last_cell = Some(cell);

    let code = button_code(button, action, mods, opts);
    let (x, y) = (cell.0 + 1, cell.1 + 1);
    let end = if action == Action::Release { 'm' } else { 'M' };
    match opts.format {
        MouseFormat::X10 => {
            // 좌표 하나가 한 바이트라 223까지만
            if cell.0 > 222 || cell.1 > 222 {
                return Vec::new();
            }
            vec![0x1b, b'[', b'M', (32 + code) as u8, (32 + x) as u8, (32 + y) as u8]
        }
        MouseFormat::Utf8 => {
            let mut out = b"\x1b[M".to_vec();
            for v in [32 + code as usize, 32 + x, 32 + y] {
                // 단추 코드는 32를 더해 최대 161, 좌표는 칸 수 + 32라 모두 유효한 글자 값이다
                if let Some(c) = char::from_u32(v as u32) {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            out
        }
        MouseFormat::Sgr => format!("\x1b[<{code};{x};{y}{end}").into_bytes(),
        MouseFormat::Urxvt => format!("\x1b[{};{x};{y}M", 32 + code).into_bytes(),
        MouseFormat::SgrPixels => {
            // 격자 원점(여백을 뺀 자리)에서 잰 실제 픽셀. 격자 밖이면 음수나 칸 크기를 넘는 값도 그대로 보낸다
            let px = (pos.0 - g.pad_left).round() as i32;
            let py = (pos.1 - g.pad_top).round() as i32;
            format!("\x1b[<{code};{px};{py}{end}").into_bytes()
        }
    }
}

/// 프롬프트 줄을 눌러 커서 옮기기(Ghostty `cursor-click-to-move`, #137): 커서 칸에서 누른 칸까지 보낼 ←·→ 바이트.
/// `row`는 커서 줄의 칸들(글자, 넓은 글자의 뒤 칸인가). 넓은 글자는 한 글자로 센다.
/// 글 끝보다 뒤를 누르면 글 끝(커서가 더 뒤면 커서)까지만
pub fn click_to_move(row: &[(char, bool)], cursor: usize, target: usize, app_cursor: bool) -> Vec<u8> {
    // 글 끝: 마지막으로 글자가 있는(빈칸·뒤 칸이 아닌) 칸의 다음
    let end = row
        .iter()
        .rposition(|&(c, spacer)| !spacer && c != ' ' && c != '\0')
        .map_or(0, |i| {
            // 넓은 글자면 뒤 칸까지 포함
            if row.get(i + 1).is_some_and(|x| x.1) {
                i + 2
            } else {
                i + 1
            }
        });
    let mut target = target.min(end.max(cursor)).min(row.len());
    // 넓은 글자의 뒤 칸이면 그 글자로
    while target > 0 && row.get(target).is_some_and(|x| x.1) {
        target -= 1;
    }
    let chars = |a: usize, b: usize| row[a.min(row.len())..b.min(row.len())].iter().filter(|x| !x.1).count();
    let none = bt_term::Mods::default();
    let (key, n) = if target > cursor {
        (bt_term::NamedKey::Right, chars(cursor, target))
    } else {
        (bt_term::NamedKey::Left, chars(target, cursor))
    };
    bt_term::encode_named(key, none, app_cursor).repeat(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_decision() {
        use MouseShiftCapture::*;
        // 보고를 끄면 Shift와 상관없이 프로그램에 보내지 않는다
        for c in [False, True, Always, Never] {
            assert!(!reports_to_program(false, c, false));
            assert!(!reports_to_program(false, c, true));
        }
        // Shift 없이는 켜 있으면 보낸다
        for c in [False, True, Always, Never] {
            assert!(reports_to_program(true, c, false));
        }
        // Shift를 누르면 true·always만 보낸다(기본 false는 고르기)
        assert!(!reports_to_program(true, False, true));
        assert!(!reports_to_program(true, Never, true));
        assert!(reports_to_program(true, True, true));
        assert!(reports_to_program(true, Always, true));
    }

    #[test]
    fn repeat_interval_picks_config_then_platform() {
        let os = Some(Duration::from_millis(350));
        assert_eq!(repeat_interval(300, os), Duration::from_millis(300));
        assert_eq!(repeat_interval(0, os), Duration::from_millis(350));
        assert_eq!(repeat_interval(0, None), Duration::from_millis(500));
    }

    #[test]
    fn middle_click_actions() {
        assert!(middle_click_pastes(MiddleClickAction::ClipboardPaste));
        assert!(!middle_click_pastes(MiddleClickAction::PrimaryPaste));
        assert!(!middle_click_pastes(MiddleClickAction::Ignore));
    }

    #[test]
    fn click_to_move_counts_characters_not_cells() {
        // "$ 한글ab  ": 한·글은 두 칸(뒤 칸은 spacer)
        let row: Vec<(char, bool)> = vec![
            ('$', false),
            (' ', false),
            ('한', false),
            (' ', true),
            ('글', false),
            (' ', true),
            ('a', false),
            ('b', false),
            (' ', false),
            (' ', false),
        ];
        let left = b"\x1b[D".to_vec();
        let right = b"\x1b[C".to_vec();
        assert_eq!(
            click_to_move(&row, 8, 2, false),
            left.repeat(4),
            "한·글·a·b 네 글자 왼쪽"
        );
        assert_eq!(
            click_to_move(&row, 8, 3, false),
            left.repeat(4),
            "넓은 글자 뒤 칸은 그 글자"
        );
        assert_eq!(click_to_move(&row, 2, 7, false), right.repeat(3));
        assert!(
            click_to_move(&row, 8, 9, false).is_empty(),
            "글 끝 뒤는 글 끝까지(이미 거기)"
        );
        assert_eq!(click_to_move(&row, 2, 9, false), right.repeat(4), "글 끝까지만");
        assert!(click_to_move(&row, 6, 6, false).is_empty());
        // 프로그램이 화살표 키를 응용 모드로 받으면 SS3
        assert_eq!(click_to_move(&row, 7, 6, true), b"\x1bOD".to_vec());
    }

    use bt_term::MouseFormat as F;

    // 아래 값은 Ghostty src/input/mouse_encode.zig의 시험 값을 옮긴 것이다(testSize: 화면 1000x1000, 칸 1x1, 여백 없음)
    fn geo() -> Geometry {
        Geometry {
            screen_w: 1000.0,
            screen_h: 1000.0,
            cell_w: 1.0,
            cell_h: 1.0,
            pad_left: 0.0,
            pad_top: 0.0,
            cols: 1000,
            rows: 1000,
        }
    }

    fn opts(event: EventMode, format: F) -> Options {
        Options {
            event,
            format,
            geometry: geo(),
            any_button_pressed: false,
        }
    }

    fn enc(b: Button, a: Action, mods: Mods, pos: (f64, f64), o: &Options) -> Vec<u8> {
        encode(b, a, mods, pos, o, &mut None)
    }

    const NO: Mods = Mods {
        shift: false,
        alt: false,
        ctrl: false,
    };
    const ALL: Mods = Mods {
        shift: true,
        alt: true,
        ctrl: true,
    };

    #[test]
    fn should_report_per_event_mode() {
        use Action::*;
        use Button::*;
        // x10: 왼쪽·가운데·오른쪽 누름만
        for b in [Left, Middle, Right] {
            assert!(should_report(b, Press, EventMode::X10));
        }
        assert!(!should_report(Left, Release, EventMode::X10));
        assert!(!should_report(Left, Motion, EventMode::X10));
        assert!(!should_report(WheelUp, Press, EventMode::X10));
        assert!(!should_report(None, Press, EventMode::X10));
        // normal: 움직임 빼고
        assert!(should_report(Left, Press, EventMode::Normal));
        assert!(should_report(Left, Release, EventMode::Normal));
        assert!(!should_report(Left, Motion, EventMode::Normal));
        // button: 단추가 있어야
        for a in [Press, Release, Motion] {
            assert!(should_report(Left, a, EventMode::Button));
            assert!(!should_report(None, a, EventMode::Button));
            assert!(should_report(None, a, EventMode::Any));
        }
    }

    #[test]
    fn x10_press_left_ignores_modifiers() {
        let o = opts(EventMode::X10, F::X10);
        assert_eq!(
            enc(Button::Left, Action::Press, ALL, (0.0, 0.0), &o),
            [0x1b, b'[', b'M', 32, 33, 33]
        );
    }

    #[test]
    fn x10_event_mode_ignores_release_and_other_buttons() {
        let o = opts(EventMode::X10, F::X10);
        assert!(enc(Button::Left, Action::Release, NO, (0.0, 0.0), &o).is_empty());
        assert!(enc(Button::Back, Action::Press, NO, (0.0, 0.0), &o).is_empty());
    }

    #[test]
    fn normal_ignores_motion_and_button_mode_requires_a_button() {
        let o = opts(EventMode::Normal, F::Sgr);
        assert!(enc(Button::Left, Action::Motion, NO, (0.0, 0.0), &o).is_empty());
        let o = opts(EventMode::Button, F::Sgr);
        assert!(enc(Button::None, Action::Motion, NO, (0.0, 0.0), &o).is_empty());
    }

    #[test]
    fn sgr_release_keeps_button_identity() {
        let o = opts(EventMode::Any, F::Sgr);
        assert_eq!(enc(Button::Right, Action::Release, NO, (4.0, 5.0), &o), b"\x1b[<2;5;6m");
    }

    #[test]
    fn sgr_motion_with_no_button() {
        let o = opts(EventMode::Any, F::Sgr);
        assert_eq!(enc(Button::None, Action::Motion, NO, (1.0, 2.0), &o), b"\x1b[<35;2;3M");
    }

    #[test]
    fn sgr_ctrl_motion_matches_the_old_shape() {
        // 이전 구현의 시험 값: 왼쪽 + Ctrl + 움직임 = 48
        let o = opts(EventMode::Any, F::Sgr);
        let ctrl = Mods { ctrl: true, ..NO };
        assert_eq!(
            enc(Button::Left, Action::Motion, ctrl, (1.0, 1.0), &o),
            b"\x1b[<48;2;2M"
        );
    }

    #[test]
    fn urxvt_with_modifiers_and_release() {
        let o = opts(EventMode::Any, F::Urxvt);
        assert_eq!(enc(Button::Left, Action::Press, ALL, (2.0, 3.0), &o), b"\x1b[60;3;4M");
        // 놓음은 옛 형식처럼 3번 단추
        assert_eq!(enc(Button::Right, Action::Release, NO, (2.0, 3.0), &o), b"\x1b[35;3;4M");
    }

    #[test]
    fn utf8_encodes_large_coordinates() {
        let o = opts(EventMode::Any, F::Utf8);
        let out = enc(Button::Left, Action::Press, NO, (300.0, 400.0), &o);
        assert_eq!(&out[..4], [0x1b, b'[', b'M', 32]);
        let cps: Vec<u32> = std::str::from_utf8(&out[4..]).unwrap().chars().map(u32::from).collect();
        assert_eq!(cps, [333, 433]);
    }

    #[test]
    fn utf8_encodes_extended_mouse_buttons() {
        let o = opts(EventMode::Any, F::Utf8);
        assert_eq!(
            enc(Button::Back, Action::Press, NO, (2.0, 3.0), &o),
            "\x1b[M\u{A0}#$".as_bytes()
        );
    }

    #[test]
    fn x10_coordinate_limit() {
        let o = opts(EventMode::X10, F::X10);
        assert!(enc(Button::Left, Action::Press, NO, (223.0, 0.0), &o).is_empty());
        assert_eq!(enc(Button::Left, Action::Press, NO, (222.0, 0.0), &o).len(), 6);
    }

    #[test]
    fn sgr_wheel_button_mappings() {
        let o = opts(EventMode::Any, F::Sgr);
        for (b, code) in [
            (Button::WheelUp, 64),
            (Button::WheelDown, 65),
            (Button::WheelLeft, 66),
            (Button::WheelRight, 67),
            (Button::Back, 128),
            (Button::Forward, 129),
        ] {
            assert_eq!(
                enc(b, Action::Press, NO, (0.0, 0.0), &o),
                format!("\x1b[<{code};1;1M").into_bytes(),
                "{b:?}"
            );
        }
    }

    #[test]
    fn sgr_pixels_uses_terminal_space_cursor_coordinates() {
        let o = opts(EventMode::Any, F::SgrPixels);
        assert_eq!(
            enc(Button::Left, Action::Press, NO, (10.0, 20.0), &o),
            b"\x1b[<0;10;20M"
        );
        assert_eq!(
            enc(Button::Right, Action::Release, NO, (10.0, 20.0), &o),
            b"\x1b[<2;10;20m"
        );
    }

    #[test]
    fn sgr_pixels_are_measured_from_the_grid_origin_in_real_pixels() {
        // 여백 12x8 실제 픽셀, 칸 10x20: 표면 (52, 48)은 격자 원점에서 (40, 40) 픽셀이다
        let g = Geometry {
            screen_w: 800.0,
            screen_h: 600.0,
            cell_w: 10.0,
            cell_h: 20.0,
            pad_left: 12.0,
            pad_top: 8.0,
            cols: 76,
            rows: 29,
        };
        let o = Options {
            geometry: g,
            ..opts(EventMode::Any, F::SgrPixels)
        };
        assert_eq!(
            enc(Button::Left, Action::Press, NO, (52.0, 48.0), &o),
            b"\x1b[<0;40;40M"
        );
        // 여백 안은 음수 그대로(Ghostty는 터미널 좌표를 자르지 않는다)
        assert_eq!(enc(Button::Left, Action::Press, NO, (2.0, 4.0), &o), b"\x1b[<0;-10;-4M");
        // 같은 자리를 칸 형식으로 보내면 5열·3행
        let o = Options { format: F::Sgr, ..o };
        assert_eq!(enc(Button::Left, Action::Press, NO, (52.0, 48.0), &o), b"\x1b[<0;5;3M");
    }

    #[test]
    fn position_exactly_at_viewport_boundary_is_encoded_in_final_cell() {
        let g = Geometry {
            screen_w: 10.0,
            screen_h: 10.0,
            cell_w: 2.0,
            cell_h: 2.0,
            pad_left: 0.0,
            pad_top: 0.0,
            cols: 5,
            rows: 5,
        };
        let o = Options {
            geometry: g,
            ..opts(EventMode::Any, F::Sgr)
        };
        assert_eq!(enc(Button::Left, Action::Press, NO, (10.0, 10.0), &o), b"\x1b[<0;5;5M");
    }

    #[test]
    fn outside_viewport_motion_needs_a_pressed_button() {
        let o = opts(EventMode::Any, F::Sgr);
        assert!(enc(Button::Left, Action::Motion, NO, (-1.0, -1.0), &o).is_empty());
        let o = Options {
            any_button_pressed: true,
            ..o
        };
        assert_eq!(
            enc(Button::Left, Action::Motion, NO, (-1.0, -1.0), &o),
            b"\x1b[<32;1;1M"
        );
        // 움직임 모드가 아니면(normal) 칸 밖 누름을 버린다. 놓음은 어디서든 보고한다
        let o = opts(EventMode::Normal, F::Sgr);
        assert!(enc(Button::Left, Action::Press, NO, (-1.0, 5.0), &o).is_empty());
        assert_eq!(enc(Button::Left, Action::Release, NO, (-1.0, 5.0), &o), b"\x1b[<0;1;6m");
    }

    #[test]
    fn motion_is_deduped_by_last_cell_except_sgr_pixels() {
        let mut last = None;
        let go = |o: &Options, last: &mut Option<(usize, usize)>| {
            encode(Button::Left, Action::Motion, NO, (5.0, 6.0), o, last)
        };
        assert!(!go(&opts(EventMode::Any, F::Sgr), &mut last).is_empty());
        assert!(go(&opts(EventMode::Any, F::Sgr), &mut last).is_empty());
        assert!(!go(&opts(EventMode::Any, F::SgrPixels), &mut last).is_empty());
    }

    #[test]
    fn legacy_encoding_and_bounds() {
        let o = opts(EventMode::Normal, F::X10);
        assert_eq!(
            enc(Button::Left, Action::Press, NO, (0.0, 0.0), &o),
            [0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(enc(Button::Left, Action::Release, NO, (0.0, 0.0), &o)[3], 35);
        assert!(enc(Button::Left, Action::Press, NO, (300.0, 0.0), &o).is_empty());
    }

    #[test]
    fn event_mode_comes_from_the_snapshot() {
        let m = |f: fn(&mut bt_term::Modes)| {
            let mut m = bt_term::Modes::default();
            f(&mut m);
            EventMode::from_modes(&m)
        };
        assert_eq!(m(|_| {}), None);
        assert_eq!(m(|m| m.mouse_x10 = true), Some(EventMode::X10));
        assert_eq!(m(|m| m.mouse_report = true), Some(EventMode::Normal));
        assert_eq!(m(|m| m.mouse_drag = true), Some(EventMode::Button));
        assert_eq!(m(|m| m.mouse_motion = true), Some(EventMode::Any));
    }
}
