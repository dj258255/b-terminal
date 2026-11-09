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
