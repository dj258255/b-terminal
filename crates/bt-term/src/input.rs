//! 키 입력과 붙여넣기를 PTY에 쓸 바이트로 바꾼다. 창 라이브러리를 모르도록 자체 키 값을 쓴다

/// 글자가 아닌 키
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1~F12
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Mods {
    /// xterm의 수식 키 번호(1 + shift + 2·alt + 4·ctrl). 수식 키가 없으면 1
    fn xterm_param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }

    fn any(self) -> bool {
        self.shift || self.ctrl || self.alt
    }
}

/// 이름 있는 키 하나를 바이트로 바꾼다. `app_cursor`는 DECCKM(커서 키 응용 모드)
pub fn encode_named(key: NamedKey, mods: Mods, app_cursor: bool) -> Vec<u8> {
    let csi_mod = |final_byte: char| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[1;{}{}", mods.xterm_param(), final_byte).into_bytes()
        } else if app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[{n};{}~", mods.xterm_param()).into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let alt_prefix = |mut bytes: Vec<u8>| -> Vec<u8> {
        if mods.alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    match key {
        NamedKey::Enter => alt_prefix(vec![b'\r']),
        NamedKey::Backspace => {
            if mods.ctrl {
                alt_prefix(vec![0x08])
            } else {
                alt_prefix(vec![0x7f])
            }
        }
        NamedKey::Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                alt_prefix(vec![b'\t'])
            }
        }
        NamedKey::Escape => alt_prefix(vec![0x1b]),
        NamedKey::Up => csi_mod('A'),
        NamedKey::Down => csi_mod('B'),
        NamedKey::Right => csi_mod('C'),
        NamedKey::Left => csi_mod('D'),
        NamedKey::Home => csi_mod('H'),
        NamedKey::End => csi_mod('F'),
        NamedKey::Insert => tilde(2),
        NamedKey::Delete => tilde(3),
        NamedKey::PageUp => tilde(5),
        NamedKey::PageDown => tilde(6),
        NamedKey::F(n) => match n {
            1..=4 => {
                let c = (b'P' + n - 1) as char;
                if mods.any() {
                    format!("\x1b[1;{}{c}", mods.xterm_param()).into_bytes()
                } else {
                    format!("\x1bO{c}").into_bytes()
                }
            }
            5 => tilde(15),
            6 => tilde(17),
            7 => tilde(18),
            8 => tilde(19),
            9 => tilde(20),
            10 => tilde(21),
            11 => tilde(23),
            12 => tilde(24),
            _ => Vec::new(),
        },
    }
}

/// 글자 키를 바이트로 바꾼다. Ctrl은 제어 문자로, Alt는 ESC를 앞에 붙인다(macOS Option을 Alt로 쓸 때)
pub fn encode_text(text: &str, mods: Mods) -> Vec<u8> {
    let mut out = Vec::new();
    if mods.alt {
        out.push(0x1b);
    }
    if mods.ctrl {
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.clone().next()) {
            if let Some(b) = ctrl_byte(c) {
                out.push(b);
                return out;
            }
        }
    }
    out.extend_from_slice(text.as_bytes());
    out
}

fn ctrl_byte(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '7' => Some(0x1f),
        '8' | '?' => Some(0x7f),
        _ => None,
    }
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// 붙여넣기 글을 바이트로 바꾼다.
/// 괄호 붙여넣기 모드면 시작·끝 표시로 감싸고, 글 안의 끝 표시와 ESC는 지운다(붙여넣기 구간을 빠져나와 명령을 실행시키는 길이다).
/// 모드가 꺼져 있으면 줄바꿈을 CR로 바꾼다(셸이 한 줄씩 받는 꼴)
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let cleaned: String = text.replace("\x1b[201~", "").chars().filter(|&c| c != '\x1b').collect();
        let mut out = Vec::with_capacity(cleaned.len() + 12);
        out.extend_from_slice(PASTE_START);
        out.extend_from_slice(cleaned.as_bytes());
        out.extend_from_slice(PASTE_END);
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
    };

    #[test]
    fn arrows_follow_cursor_key_mode() {
        assert_eq!(encode_named(NamedKey::Up, NONE, false), b"\x1b[A");
        assert_eq!(encode_named(NamedKey::Up, NONE, true), b"\x1bOA");
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(encode_named(NamedKey::Left, ctrl, true), b"\x1b[1;5D");
    }

    #[test]
    fn ctrl_letters_become_control_bytes() {
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(encode_text("c", ctrl), vec![3]);
        assert_eq!(encode_text("[", ctrl), vec![0x1b]);
        let alt = Mods { alt: true, ..NONE };
        assert_eq!(encode_text("b", alt), b"\x1bb");
    }

    #[test]
    fn korean_text_passes_through_as_utf8() {
        assert_eq!(encode_text("안녕", NONE), "안녕".as_bytes());
    }

    #[test]
    fn shift_tab_and_function_keys() {
        let shift = Mods { shift: true, ..NONE };
        assert_eq!(encode_named(NamedKey::Tab, shift, false), b"\x1b[Z");
        assert_eq!(encode_named(NamedKey::F(1), NONE, false), b"\x1bOP");
        assert_eq!(encode_named(NamedKey::F(5), NONE, false), b"\x1b[15~");
        assert_eq!(encode_named(NamedKey::Delete, NONE, false), b"\x1b[3~");
    }

    #[test]
    fn bracketed_paste_strips_end_marker_injection() {
        let evil = "echo hi\x1b[201~rm -rf ~\n";
        let out = encode_paste(evil, true);
        assert!(out.starts_with(PASTE_START));
        assert!(out.ends_with(PASTE_END));
        let inner = &out[PASTE_START.len()..out.len() - PASTE_END.len()];
        assert!(!inner.windows(PASTE_END.len()).any(|w| w == PASTE_END));
        assert!(!inner.contains(&0x1b));
    }

    #[test]
    fn plain_paste_turns_newlines_into_cr() {
        assert_eq!(encode_paste("a\nb\r\nc", false), b"a\rb\rc");
    }
}
