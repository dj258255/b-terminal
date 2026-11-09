//! 마우스 보고(xterm 1000/1002/1003, SGR 1006). vim·htop·less 같은 프로그램이 마우스를 쓸 때 켠다

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
    /// 버튼을 누르지 않은 움직임(1003)
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Press,
    Release,
    Motion,
}

/// 마우스 사건 하나를 바이트로. col·row는 0부터
#[allow(clippy::too_many_arguments)]
pub fn encode(
    button: Button,
    action: Action,
    col: usize,
    row: usize,
    shift: bool,
    alt: bool,
    ctrl: bool,
    sgr: bool,
) -> Vec<u8> {
    let mut code: u32 = match button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
        Button::None => 3,
        Button::WheelUp => 64,
        Button::WheelDown => 65,
    };
    if action == Action::Motion {
        code += 32;
    }
    if shift {
        code += 4;
    }
    if alt {
        code += 8;
    }
    if ctrl {
        code += 16;
    }
    let (x, y) = (col + 1, row + 1);
    if sgr {
        let end = if action == Action::Release { 'm' } else { 'M' };
        return format!("\x1b[<{code};{x};{y}{end}").into_bytes();
    }
    // 옛 꼴: 놓기는 3번, 좌표는 223까지만
    if action == Action::Release {
        code = 3 + (code & !3);
    }
    if x > 223 || y > 223 {
        return Vec::new();
    }
    vec![0x1b, b'[', b'M', 32 + code as u8, 32 + x as u8, 32 + y as u8]
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

    #[test]
    fn sgr_press_release_and_wheel() {
        assert_eq!(
            encode(Button::Left, Action::Press, 0, 0, false, false, false, true),
            b"\x1b[<0;1;1M"
        );
        assert_eq!(
            encode(Button::Left, Action::Release, 4, 2, false, false, false, true),
            b"\x1b[<0;5;3m"
        );
        assert_eq!(
            encode(Button::WheelUp, Action::Press, 0, 0, false, false, false, true),
            b"\x1b[<64;1;1M"
        );
        assert_eq!(
            encode(Button::Left, Action::Motion, 1, 1, false, false, true, true),
            b"\x1b[<48;2;2M"
        );
    }

    #[test]
    fn legacy_encoding_and_bounds() {
        assert_eq!(
            encode(Button::Left, Action::Press, 0, 0, false, false, false, false),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(
            encode(Button::Left, Action::Release, 0, 0, false, false, false, false)[3],
            35
        );
        assert!(encode(Button::Left, Action::Press, 300, 0, false, false, false, false).is_empty());
    }
}
