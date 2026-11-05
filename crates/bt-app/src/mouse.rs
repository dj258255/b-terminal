//! 마우스 보고(xterm 1000/1002/1003, SGR 1006). vim·htop·less 같은 프로그램이 마우스를 쓸 때 켠다

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

#[cfg(test)]
mod tests {
    use super::*;

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
