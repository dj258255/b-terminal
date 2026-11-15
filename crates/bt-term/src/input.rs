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

/// 키 입력 모양을 바꾸는 DEC 모드(#301). 기본값은 Ghostty `modes.zig`와 같다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyModes {
    /// DECBKM(67). 켜면 Backspace가 BS(0x08), Ctrl+Backspace가 DEL(0x7f). 기본 꺼짐
    pub backarrow_key: bool,
    /// 1035. 켜면 키패드는 늘 숫자 키(응용 모드를 무시). 기본 켬. 지금은 상태만 기억한다
    pub ignore_keypad_with_numlock: bool,
    /// 1036. 켜면 Alt를 누른 글자 앞에 ESC를 붙인다. 기본 켬
    pub alt_esc_prefix: bool,
    /// 1039. Ghostty도 상태만 기억하고 인코딩에는 쓰지 않는다. 기본 꺼짐
    pub alt_sends_escape: bool,
}

impl Default for KeyModes {
    fn default() -> Self {
        KeyModes {
            backarrow_key: false,
            ignore_keypad_with_numlock: true,
            alt_esc_prefix: true,
            alt_sends_escape: false,
        }
    }
}

/// 이름 있는 키 하나를 바이트로 바꾼다. `app_cursor`는 DECCKM(커서 키 응용 모드)
pub fn encode_named(key: NamedKey, mods: Mods, app_cursor: bool) -> Vec<u8> {
    encode_named_with(key, mods, app_cursor, KeyModes::default())
}

/// `encode_named`에 키 입력 모드(DECBKM 등)를 더한 것
pub fn encode_named_with(key: NamedKey, mods: Mods, app_cursor: bool, km: KeyModes) -> Vec<u8> {
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
            // DECBKM(67)를 켜면 둘이 서로 바뀐다(Ghostty `pcStyleFunctionKey`의 `sequence_decbkm`)
            if mods.ctrl != km.backarrow_key {
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

/// 숫자 키패드 키(#312). winit에서는 `KeyLocation::Numpad`로 가린다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeypadKey {
    /// 0~9
    Digit(u8),
    Decimal,
    Divide,
    Multiply,
    Subtract,
    Add,
    Enter,
}

/// 키패드가 응용 모드인가. DECKPAM(`ESC =`)을 켰고 1035가 꺼져 있을 때만 그렇다
/// (Ghostty `pcStyleFunctionKey`: 1035가 켜져 있으면 늘 숫자 키패드)
pub fn keypad_application(app_keypad: bool, km: KeyModes) -> bool {
    app_keypad && !km.ignore_keypad_with_numlock
}

/// 키패드 키 하나를 바이트로 바꾼다(Ghostty `function_keys.zig`의 `kpKeys`).
/// 응용 모드면 `ESC O 끝글자`, 수식 키가 있으면 `ESC O 번호 끝글자`. 숫자 모드면 수식 키와 상관없이 그 글자 그대로
pub fn encode_keypad(key: KeypadKey, mods: Mods, application: bool) -> Vec<u8> {
    let (suffix, normal) = match key {
        KeypadKey::Digit(n) => (b'p' + n.min(9), b'0' + n.min(9)),
        KeypadKey::Decimal => (b'n', b'.'),
        KeypadKey::Divide => (b'o', b'/'),
        KeypadKey::Multiply => (b'j', b'*'),
        KeypadKey::Subtract => (b'm', b'-'),
        KeypadKey::Add => (b'k', b'+'),
        KeypadKey::Enter => (b'M', b'\r'),
    };
    if !application {
        return vec![normal];
    }
    let code = kitty_mods(mods);
    if code == 1 {
        vec![0x1b, b'O', suffix]
    } else {
        format!("\x1bO{code}{}", suffix as char).into_bytes()
    }
}

/// xterm modifyOtherKeys 상태 2(`CSI > 4 ; 2 m`)의 글자 인코딩 `ESC [ 27 ; 수식 ; 코드 ~`(Ghostty `legacy`의 `modify_other_keys_state_2`).
/// 보낼 것이 없으면(수식 키가 없거나 xterm이 옛 꼴로 두는 글자) 없다
pub fn encode_modify_other_char(ch: char, mods: Mods) -> Option<Vec<u8>> {
    let code = kitty_mods(mods);
    if code == 1 {
        return None;
    }
    let cp = ch as u32;
    // xterm IsControlInput(0x40~0x7F), Shift 말고 다른 수식 키가 있을 때, Shift만이면 공백만
    let should_modify = (0x40..=0x7f).contains(&cp) || mods.ctrl || mods.alt || mods.sup || ch == ' ';
    should_modify.then(|| format!("\x1b[27;{code};{cp}~").into_bytes())
}

/// modifyOtherKeys 상태 2에서 Enter·Tab·Backspace·Escape(Ghostty `function_keys.zig`의 `set_other` 항목과 수식 키 표).
/// 수식 키가 있으면 `ESC [ 27 ; 번호 ; 코드 ~`. Backspace의 Ctrl 혼자는 옛 꼴(0x08)이라 없다
pub fn encode_named_modify_other(key: NamedKey, mods: Mods) -> Option<Vec<u8>> {
    let code = kitty_mods(mods);
    if code == 1 {
        return None;
    }
    let cp = match key {
        NamedKey::Enter => 13,
        NamedKey::Tab => 9,
        NamedKey::Escape => 27,
        NamedKey::Backspace if mods.ctrl && !mods.shift && !mods.alt && !mods.sup => return None,
        NamedKey::Backspace => 127,
        _ => return None,
    };
    Some(format!("\x1b[27;{code};{cp}~").into_bytes())
}

/// kitty 키보드 프로토콜 단계(프로그램이 `CSI > 플래그 u`로 켠다). <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyFlags {
    /// 1: 모호한 키 구분(Shift+Enter, Ctrl+글자, Esc …를 CSI u로)
    pub disambiguate: bool,
    /// 2: 키 뗌·반복 보고
    pub report_events: bool,
    /// 4: 대체 키 보고(Shift 누른 글자, 기본 배열 글자)
    pub report_alternates: bool,
    /// 8: 모든 키를 이스케이프로(글자 키도 CSI u)
    pub all_as_escapes: bool,
    /// 16: 연결 글자 보고(키가 만든 글자를 코드로 덧붙임)
    pub report_associated: bool,
}

impl KittyFlags {
    /// 플래그가 하나라도 켜졌는가(Ghostty `kitty_flags.int() != 0`)
    pub fn active(self) -> bool {
        self.disambiguate
            || self.report_events
            || self.report_alternates
            || self.all_as_escapes
            || self.report_associated
    }
}

fn kitty_mods(m: Mods) -> u8 {
    1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8 + 8 * m.sup as u8
}

/// 글자 키를 바이트로 바꾼다. Ctrl은 제어 문자로, Alt는 ESC를 앞에 붙인다(macOS Option을 Alt로 쓸 때)
pub fn encode_text(text: &str, mods: Mods) -> Vec<u8> {
    encode_text_with(text, mods, KeyModes::default())
}

/// `encode_text`에 키 입력 모드(1036)를 더한 것
pub fn encode_text_with(text: &str, mods: Mods, km: KeyModes) -> Vec<u8> {
    let mut out = Vec::new();
    if mods.ctrl {
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.clone().next()) {
            if let Some(b) = ctrl_byte(c) {
                // 제어 문자 길은 1036과 상관없이 Alt가 ESC를 앞에 붙인다(Ghostty `legacy`의 ctrlSeq 길)
                if mods.alt {
                    out.push(0x1b);
                }
                out.push(b);
                return out;
            }
        }
    }
    // 글자 길은 1036이 켜져 있을 때만 ESC를 붙인다(Ghostty `legacyAltPrefix`)
    if mods.alt && km.alt_esc_prefix {
        out.push(0x1b);
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

/// 복사하는 글의 줄 끝 공백·탭을 지운다(Ghostty `clipboard-trim-trailing-spaces`).
/// 글자가 있는 줄의 끝 공백도, 공백뿐인 줄도 비운다. 줄 가운데 공백과 줄바꿈 자체는 그대로 둔다
pub fn trim_trailing_spaces(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            // CRLF의 CR은 줄바꿈의 일부라 남기고, 그 앞의 공백·탭만 지운다
            let (body, cr) = match line.strip_suffix('\r') {
                Some(b) => (b, "\r"),
                None => (line, ""),
            };
            format!("{}{cr}", body.trim_end_matches([' ', '\t']))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_trailing_spaces_per_line() {
        assert_eq!(trim_trailing_spaces("ls -la   "), "ls -la");
        assert_eq!(trim_trailing_spaces("a  \nb\t\t\nc"), "a\nb\nc");
        assert_eq!(trim_trailing_spaces("a b   c  "), "a b   c", "가운데 공백은 그대로");
        assert_eq!(
            trim_trailing_spaces("a\n   \n\t\nb"),
            "a\n\n\nb",
            "공백뿐인 줄은 빈 줄이 된다"
        );
        assert_eq!(trim_trailing_spaces("a  \n"), "a\n", "끝 줄바꿈은 남긴다");
        assert_eq!(
            trim_trailing_spaces("a  \r\nb \r\n"),
            "a\r\nb\r\n",
            "CRLF의 CR은 남긴다"
        );
        assert_eq!(trim_trailing_spaces("  앞 공백 유지  한글  "), "  앞 공백 유지  한글");
        assert_eq!(trim_trailing_spaces(""), "");
    }

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

    fn km(f: impl FnOnce(&mut KeyModes)) -> KeyModes {
        let mut k = KeyModes::default();
        f(&mut k);
        k
    }

    // ---- DECBKM (Ghostty key_encode.zig 2285~2325줄) ----

    #[test]
    fn backspace_follows_decbkm_like_ghostty() {
        let ctrl = Mods { ctrl: true, ..NONE };
        let reset = km(|k| k.backarrow_key = false);
        let set = km(|k| k.backarrow_key = true);
        // 꺼짐: 0x7f, Ctrl이면 0x08
        assert_eq!(encode_named_with(NamedKey::Backspace, NONE, false, reset), b"\x7f");
        assert_eq!(encode_named_with(NamedKey::Backspace, ctrl, false, reset), b"\x08");
        // 켬: 0x08, Ctrl이면 0x7f
        assert_eq!(encode_named_with(NamedKey::Backspace, NONE, false, set), b"\x08");
        assert_eq!(encode_named_with(NamedKey::Backspace, ctrl, false, set), b"\x7f");
        // Ctrl+Shift+Backspace(2259줄)는 꺼짐에서 0x08
        let cs = Mods {
            ctrl: true,
            shift: true,
            ..NONE
        };
        assert_eq!(encode_named_with(NamedKey::Backspace, cs, false, reset), b"\x08");
        // Alt는 ESC를 앞에 붙인다(2269줄 ctrl+alt+shift는 ESC 0x08)
        let all = Mods {
            ctrl: true,
            alt: true,
            shift: true,
            ..NONE
        };
        assert_eq!(encode_named_with(NamedKey::Backspace, all, false, reset), b"\x1b\x08");
        assert_eq!(encode_named_with(NamedKey::Backspace, all, false, set), b"\x1b\x7f");
        // 기본값은 지금까지와 같다
        assert_eq!(encode_named(NamedKey::Backspace, NONE, false), b"\x7f");
        assert_eq!(KeyModes::default(), km(|_| {}));
    }

    #[test]
    fn decbkm_does_not_touch_other_keys() {
        let set = km(|k| k.backarrow_key = true);
        for key in [
            NamedKey::Enter,
            NamedKey::Tab,
            NamedKey::Escape,
            NamedKey::Delete,
            NamedKey::Up,
        ] {
            assert_eq!(
                encode_named_with(key, NONE, false, set),
                encode_named(key, NONE, false),
                "{key:?}"
            );
        }
    }

    // ---- 1036 (Ghostty key_encode.zig 2112~2231줄) ----

    #[test]
    fn alt_prefix_follows_mode_1036() {
        let alt = Mods { alt: true, ..NONE };
        let on = km(|k| k.alt_esc_prefix = true);
        let off = km(|k| k.alt_esc_prefix = false);
        // legacy: alt+c, alt+unicode(2116~2200줄)
        assert_eq!(encode_text_with("c", alt, on), b"\x1bc");
        assert_eq!(encode_text_with("é", alt, on), "\x1bé".as_bytes());
        assert_eq!(encode_text_with("界", alt, on), "\x1b界".as_bytes());
        assert_eq!(encode_text_with("😀", alt, on), "\x1b😀".as_bytes());
        assert_eq!(encode_text_with("ф", alt, on), "\x1bф".as_bytes());
        // 꺼짐: 접두 없이 글자 그대로(legacyAltPrefix가 false를 돌려 writeAll(utf8))
        assert_eq!(encode_text_with("c", alt, off), b"c");
        assert_eq!(encode_text_with("界", alt, off), "界".as_bytes());
        // Ctrl+Alt+c는 C0 길이라 모드와 상관없이 ESC가 앞에 붙는다(2095줄 근처, 456~461줄)
        let ca = Mods {
            ctrl: true,
            alt: true,
            ..NONE
        };
        assert_eq!(encode_text_with("c", ca, on), b"\x1b\x03");
        assert_eq!(encode_text_with("c", ca, off), b"\x1b\x03");
        // 기본값은 지금처럼 ESC를 붙인다
        assert_eq!(encode_text("b", alt), b"\x1bb");
    }

    #[test]
    fn named_keys_with_alt_ignore_mode_1036() {
        // pcStyleFunctionKey 표가 정하는 키(Enter·Escape·Tab·Backspace)는 1036과 상관없다
        let alt = Mods { alt: true, ..NONE };
        let off = km(|k| k.alt_esc_prefix = false);
        assert_eq!(encode_named_with(NamedKey::Enter, alt, false, off), b"\x1b\r");
        assert_eq!(encode_named_with(NamedKey::Backspace, alt, false, off), b"\x1b\x7f");
    }

    #[test]
    fn modes_1035_and_1039_are_remembered_but_do_not_change_bytes() {
        let d = KeyModes::default();
        assert!(d.ignore_keypad_with_numlock, "1035는 기본 켬(modes.zig 404줄)");
        assert!(d.alt_esc_prefix, "1036은 기본 켬(405줄)");
        assert!(!d.alt_sends_escape && !d.backarrow_key, "1039·67은 기본 꺼짐");
        let alt = Mods { alt: true, ..NONE };
        let k = km(|k| {
            k.ignore_keypad_with_numlock = false;
            k.alt_sends_escape = true;
        });
        assert_eq!(encode_text_with("c", alt, k), b"\x1bc");
        assert_eq!(encode_named_with(NamedKey::Backspace, NONE, false, k), b"\x7f");
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
