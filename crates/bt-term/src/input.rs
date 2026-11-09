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
    /// Cmd(macOS)·Super. kitty 키보드 프로토콜에서만 보낸다
    pub sup: bool,
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

/// kitty 키보드 프로토콜 단계(프로그램이 `CSI > 플래그 u`로 켠다). <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyFlags {
    /// 1: 모호한 키 구분(Shift+Enter, Ctrl+글자, Esc …를 CSI u로)
    pub disambiguate: bool,
    /// 8: 모든 키를 이스케이프로(글자 키도 CSI u)
    pub all_as_escapes: bool,
}

impl KittyFlags {
    pub fn active(self) -> bool {
        self.disambiguate || self.all_as_escapes
    }
}

fn kitty_mods(m: Mods) -> u8 {
    1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8 + 8 * m.sup as u8
}

fn csi_u(code: u32, m: Mods) -> Vec<u8> {
    let mods = kitty_mods(m);
    if mods == 1 {
        format!("\x1b[{code}u").into_bytes()
    } else {
        format!("\x1b[{code};{mods}u").into_bytes()
    }
}

/// kitty 프로토콜로 이름 키를 보낸다. 이 프로토콜에서 바뀌지 않는 키(화살표 등은 xterm 꼴 그대로)면 없다
pub fn encode_named_kitty(key: NamedKey, mods: Mods, flags: KittyFlags) -> Option<Vec<u8>> {
    if !flags.active() {
        return None;
    }
    let any_mod = mods.shift || mods.ctrl || mods.alt || mods.sup;
    let code = match key {
        NamedKey::Enter => 13,
        NamedKey::Tab => 9,
        NamedKey::Backspace => 127,
        // Esc는 1단계부터 늘 CSI u(뒤따르는 글자와 헷갈리지 않게)
        NamedKey::Escape => return Some(csi_u(27, mods)),
        _ => return None,
    };
    // 1단계: 수식 키가 없으면 옛 꼴(\r, \t, DEL). 8단계: 늘 CSI u
    if any_mod || flags.all_as_escapes {
        Some(csi_u(code, mods))
    } else {
        None
    }
}

/// kitty 프로토콜로 글자 키를 보낸다. `base`는 수식 키를 뺀 글자(Shift 전)다. 옛 꼴 그대로 보내면 되면 없다
pub fn encode_char_kitty(base: char, mods: Mods, flags: KittyFlags) -> Option<Vec<u8>> {
    if !flags.active() {
        return None;
    }
    let code = base.to_lowercase().next().unwrap_or(base) as u32;
    if flags.all_as_escapes {
        return Some(csi_u(code, mods));
    }
    // 1단계: Ctrl·Alt·Super가 붙은 글자만 CSI u. Shift만 붙은 글자는 그 글자(대문자) 그대로
    if mods.ctrl || mods.alt || mods.sup {
        Some(csi_u(code, mods))
    } else {
        None
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

/// 묻지 않고 붙여넣어도 되는 글인가(Ghostty `clipboard-paste-protection`, #109).
/// 괄호 붙여넣기가 꺼져 있으면 줄바꿈(LF·CR)이 셸에서 곧바로 Enter가 되어 보지 못한 명령이 돈다.
/// 괄호 붙여넣기가 켜져 있으면 프로그램이 붙여넣기로 받으므로 `bracketed_safe`일 때 안전으로 본다
/// (끝 표시를 끼워 빠져나오는 길은 `encode_paste`가 지운다)
pub fn paste_is_safe(text: &str, bracketed: bool, bracketed_safe: bool) -> bool {
    (bracketed && bracketed_safe) || !text.contains(['\n', '\r'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_with_newlines_is_unsafe_unless_bracketed_like_ghostty() {
        assert!(paste_is_safe("ls -la", false, true), "한 줄은 안전");
        assert!(!paste_is_safe("echo hi\nrm -rf ~", false, true), "LF는 Enter가 된다");
        assert!(!paste_is_safe("echo hi\r", false, true), "CR 혼자도 Enter다");
        assert!(!paste_is_safe("curl x | sh\r\n", false, true));
        // 괄호 붙여넣기: 기본(bracketed-safe)이면 안전, 끄면 줄바꿈을 다시 본다
        assert!(paste_is_safe("a\nb", true, true));
        assert!(!paste_is_safe("a\nb", true, false));
        assert!(paste_is_safe("ab", true, false));
        // 끝 표시를 끼운 글은 괄호 붙여넣기여도 지워지지만, 꺼져 있으면 줄바꿈으로 막힌다
        assert!(!paste_is_safe("x\x1b[201~\nrm -rf ~", false, true));
    }

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
        sup: false,
    };
    const K1: KittyFlags = KittyFlags {
        disambiguate: true,
        all_as_escapes: false,
    };
    const K8: KittyFlags = KittyFlags {
        disambiguate: true,
        all_as_escapes: true,
    };

    #[test]
    fn kitty_shift_enter_is_distinct_from_enter() {
        let shift = Mods { shift: true, ..NONE };
        assert_eq!(
            encode_named_kitty(NamedKey::Enter, shift, K1),
            Some(b"\x1b[13;2u".to_vec())
        );
        assert_eq!(
            encode_named_kitty(NamedKey::Enter, NONE, K1),
            None,
            "1단계의 맨 Enter는 CR 그대로"
        );
        assert_eq!(
            encode_named_kitty(NamedKey::Enter, NONE, K8),
            Some(b"\x1b[13u".to_vec())
        );
        assert_eq!(
            encode_named_kitty(NamedKey::Escape, NONE, K1),
            Some(b"\x1b[27u".to_vec())
        );
        assert_eq!(encode_named_kitty(NamedKey::Up, shift, K1), None, "화살표는 xterm 꼴");
        assert_eq!(
            encode_named_kitty(NamedKey::Enter, shift, KittyFlags::default()),
            None,
            "꺼져 있으면 옛 꼴"
        );
    }

    #[test]
    fn kitty_ctrl_and_alt_letters() {
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(encode_char_kitty('c', ctrl, K1), Some(b"\x1b[99;5u".to_vec()));
        let ctrl_shift = Mods {
            ctrl: true,
            shift: true,
            ..NONE
        };
        assert_eq!(
            encode_char_kitty('C', ctrl_shift, K1),
            Some(b"\x1b[99;6u".to_vec()),
            "글자는 소문자 코드"
        );
        assert_eq!(
            encode_char_kitty('a', Mods { shift: true, ..NONE }, K1),
            None,
            "Shift만이면 글자 그대로"
        );
        assert_eq!(encode_char_kitty('a', NONE, K8), Some(b"\x1b[97u".to_vec()));
        assert_eq!(
            encode_char_kitty('a', Mods { alt: true, ..NONE }, K1),
            Some(b"\x1b[97;3u".to_vec())
        );
    }

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
