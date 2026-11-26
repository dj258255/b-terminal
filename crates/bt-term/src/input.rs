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
    /// F1~F25
    F(u8),
    /// Help(Ghostty `help`). kitty 키보드 프로토콜 표에는 없다
    Help,
    /// 메뉴 키(Ghostty `context_menu`). kitty 키보드 프로토콜 표에는 없다
    ContextMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Cmd(macOS)·Super. kitty 키보드 프로토콜과, PC식 함수 키 표(수식 번호 9~16, Ghostty `mods.binding()`)에서 수식 키로 센다.
    /// 글자 키(Alt·Ctrl 길)는 Super를 수식 키로 세지 않는다
    pub sup: bool,
}

impl Mods {
    /// PC식 함수 키 표의 수식 키 번호(1 + shift + 2·alt + 4·ctrl + 8·super, Ghostty `function_keys.zig`의 `modifiers`). 수식 키가 없으면 1
    fn xterm_param(self) -> u8 {
        kitty_mods(self)
    }

    fn any(self) -> bool {
        self.shift || self.ctrl || self.alt || self.sup
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
        // Enter·Tab·Escape: 수식 키 없음과 Alt 혼자(와 Shift+Tab) 말고는 모두 `CSI 27 ; 수식 ; 코드 ~`(Ghostty `function_keys.zig`의 "Everything else").
        // Shift+Enter·Shift+Esc도 modifyOtherKeys와 상관없이 이 꼴이다
        NamedKey::Enter => match mods {
            m if !m.any() => vec![b'\r'],
            Mods {
                alt: true,
                shift: false,
                ctrl: false,
                sup: false,
            } => b"\x1b\r".to_vec(),
            _ => csi27(mods, 13),
        },
        NamedKey::Tab => match mods {
            m if !m.any() => vec![b'\t'],
            Mods {
                alt: true,
                shift: false,
                ctrl: false,
                sup: false,
            } => b"\x1b\t".to_vec(),
            Mods {
                shift: true,
                alt: false,
                ctrl: false,
                sup: false,
            } => b"\x1b[Z".to_vec(),
            _ => csi27(mods, 9),
        },
        NamedKey::Escape => match mods {
            m if !m.any() => vec![0x1b],
            Mods {
                alt: true,
                shift: false,
                ctrl: false,
                sup: false,
            } => b"\x1b\x1b".to_vec(),
            _ => csi27(mods, 27),
        },
        NamedKey::Backspace => {
            // Ctrl이 있으면 BS(0x08), 아니면 DEL(0x7f). Alt는 ESC를 앞에 붙인다. DECBKM(67)을 켜면 수식 키가 없거나 Ctrl 혼자일 때만
            // 서로 바뀐다(Ghostty `sequence_decbkm`은 표의 마지막 두 칸에만 있다. Shift·Alt·Super가 붙은 칸은 그대로다)
            let swap = km.backarrow_key && !mods.shift && !mods.alt && !mods.sup;
            if mods.ctrl != swap {
                alt_prefix(vec![0x08])
            } else {
                alt_prefix(vec![0x7f])
            }
        }
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
        NamedKey::Help => tilde(28),
        NamedKey::ContextMenu => tilde(29),
        NamedKey::F(n) => match n {
            // F3는 `ESC [ 1 ; 수식 R`이 커서 위치 보고(CPR)와 같아서 xterm은 수식 키가 있으면 `ESC [ 13 ; 수식 ~`로 보낸다(Ghostty `function_keys.zig` f3)
            3 if mods.any() => tilde(13),
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
            // F13~F25(Ghostty `function_keys.zig`). 번호가 건너뛰는 것은 xterm 표 그대로다(27·30·35~41은 쓰지 않는다)
            13 => tilde(25),
            14 => tilde(26),
            15 => tilde(28),
            16 => tilde(29),
            17 => tilde(31),
            18 => tilde(32),
            19 => tilde(33),
            20 => tilde(34),
            21 => tilde(42),
            22 => tilde(43),
            23 => tilde(44),
            24 => tilde(45),
            25 => tilde(46),
            _ => Vec::new(),
        },
    }
}

/// `ESC [ 27 ; 수식 ; 코드 ~`(xterm modifyOtherKeys 꼴). Enter·Tab·Escape에 수식 키가 붙었을 때의 PC식 함수 키 표 칸
fn csi27(mods: Mods, code: u8) -> Vec<u8> {
    format!("\x1b[27;{};{code}~", mods.xterm_param()).into_bytes()
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

/// 글자 키가 만든 글과, 그 글을 어떻게 읽을지 정하는 키 정보(Ghostty `key.KeyEvent`의 `utf8`·`unshifted_codepoint`와 물리 키)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextKey<'a> {
    /// 키가 만든 글. 없으면 빈 글
    pub utf8: &'a str,
    /// 수식 키 없이 눌렀을 때의 글자 코드(배열 기준). 모르면 0
    pub unshifted: u32,
    /// 물리 키의 US 배열 글자(자판이 러시아어여도 C 키는 `c`). 모르면 없다
    pub physical: Option<u32>,
    /// Alt를 글자 앞 ESC로 보내도 되는가. macOS에서 Option을 Alt로 쓰지 않으면(`macos-option-as-alt`) 거짓이라
    /// OS가 만든 글자가 그대로 나간다. fixterms CSI u는 이것과 상관없이 Alt를 수식 키로 센다(Ghostty는 원래 수식 키 `event.mods`를 쓴다)
    pub alt_text_prefix: bool,
}

/// 글자 키를 바이트로 바꾼다. Ctrl은 제어 문자로, Alt는 ESC를 앞에 붙인다(macOS Option을 Alt로 쓸 때)
pub fn encode_text(text: &str, mods: Mods) -> Vec<u8> {
    encode_text_with(text, mods, KeyModes::default())
}

/// `encode_text`에 키 입력 모드(1036)를 더한 것
pub fn encode_text_with(text: &str, mods: Mods, km: KeyModes) -> Vec<u8> {
    encode_text_key(
        &TextKey {
            utf8: text,
            unshifted: 0,
            physical: None,
            alt_text_prefix: true,
        },
        mods,
        km,
    )
}

/// 글자 키 하나를 바이트로(Ghostty `key_encode.zig`의 `legacy()`에서 이름 키·modifyOtherKeys를 지난 뒤의 길).
/// `mods.alt`는 Option이어도 눌렸으면 참이다(Ghostty `legacy()`의 `all_mods`). 차례: Ctrl 제어 바이트(`ctrl_seq`) → Ctrl이 남았으면 fixterms `CSI 코드 ; 수식 u` → Alt ESC 앞붙이기 → 글 그대로
pub fn encode_text_key(k: &TextKey, mods: Mods, km: KeyModes) -> Vec<u8> {
    // C0 길은 1036과 상관없이 Alt가 ESC를 앞에 붙인다. 다만 Option을 Alt로 쓰지 않으면(`alt_text_prefix` 거짓) Option은
    // 글자를 만드는 데 쓰인 수식 키(consumed)라 빠진다(Ghostty `binding_mods.alt`, macOS 앱의 consumed_mods = 번역 수식 키 − Ctrl·Cmd)
    if let Some(b) = ctrl_seq(k.physical, k.utf8, k.unshifted, mods) {
        return if mods.alt && k.alt_text_prefix {
            vec![0x1b, b]
        } else {
            vec![b]
        };
    }
    // 글이 없으면 Alt 앞붙이기만 남는다(수식 키 없는 글자로)
    if k.utf8.is_empty() {
        return legacy_alt_prefix(k, mods, km).unwrap_or_default();
    }
    if mods.ctrl {
        if let Some(bytes) = ctrl_csi_u(k.utf8, k.unshifted, mods) {
            return bytes;
        }
    }
    if let Some(bytes) = legacy_alt_prefix(k, mods, km) {
        return bytes;
    }
    // macOS에서 Cmd+글자는 글을 보내지 않는다(Terminal.app·iTerm2·텍스트 입력과 같다). 다른 OS는 Super+b도 `b`를 보낸다
    if cfg!(target_os = "macos") && mods.sup {
        return Vec::new();
    }
    k.utf8.as_bytes().to_vec()
}

/// Alt를 누른 글자 앞에 ESC를 붙인다(Ghostty `legacyAltPrefix` 552~620줄). 1036이 꺼져 있거나 Option을 Alt로 쓰지 않으면 없다.
/// 글이 한 바이트면 그 글 그대로(Shift로 만든 `>` 같은 문장 부호를 지키려고)이고, 아니면 수식 키 없는 글자(`unshifted`)를 쓴다.
/// macOS에서 Option이 `≈`처럼 바꾼 글 대신 눌린 키의 글자를 보내기 위해서다. `unshifted`가 없으면 글 전체다
fn legacy_alt_prefix(k: &TextKey, mods: Mods, km: KeyModes) -> Option<Vec<u8>> {
    if !mods.alt || !k.alt_text_prefix || !km.alt_esc_prefix {
        return None;
    }
    let mut out = vec![0x1b];
    if let [b] = k.utf8.as_bytes() {
        out.push(*b);
    } else if k.unshifted > 0 {
        let mut buf = [0u8; 4];
        out.extend_from_slice(char::from_u32(k.unshifted)?.encode_utf8(&mut buf).as_bytes());
    } else if !k.utf8.is_empty() {
        out.extend_from_slice(k.utf8.as_bytes());
    } else {
        return None;
    }
    Some(out)
}

/// Ctrl이 눌렸는데 제어 바이트가 없는 글자를 fixterms `CSI 코드 ; 수식 u`로(Ghostty `legacy()`의 `csiu` 블록).
/// 글이 한 글자가 아니면 없다. 수식 키 번호는 1 + Shift + 2·Alt + 4·Ctrl이고 Super는 넣지 않는다.
/// Shift는 글자가 Shift 없이 낼 수 있는 글자와 같을 때만 넣는다(`Shift+;`의 `:`는 Shift로 만든 글자라 넣지 않는다).
/// A~Z에 Shift가 있으면 소문자 코드로 보낸다(kitty와 같다. fixterms 명세와는 다르다)
fn ctrl_csi_u(utf8: &str, unshifted: u32, mods: Mods) -> Option<Vec<u8>> {
    let mut chars = utf8.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let mut code = c as u32;
    if c.is_ascii_uppercase() && mods.shift {
        code = c.to_ascii_lowercase() as u32;
    }
    let shift = mods.shift && unshifted == code;
    let param = 1 + shift as u8 + 2 * mods.alt as u8 + 4 * mods.ctrl as u8;
    Some(format!("\x1b[{code};{param}u").into_bytes())
}

/// Ctrl+글자의 제어 바이트(Ghostty `ctrlSeq`). `physical`은 물리 키의 US 배열 글자, `utf8`은 키가 만든 글,
/// `unshifted`는 수식 키 없이 눌렀을 때의 글자 코드다. Alt는 보지 않는다(ESC 앞붙이기는 부르는 쪽이 한다).
/// Ctrl 혼자일 때만 있고(`Ctrl+Shift+-`처럼 Shift가 글자를 만든 것은 Shift를 뺀다), `i`·`m`·`[`는 fixterms 명세대로 두지 않는다
pub fn ctrl_seq(physical: Option<u32>, utf8: &str, unshifted: u32, mods: Mods) -> Option<u8> {
    if !mods.ctrl {
        return None;
    }
    let mut shift = mods.shift;
    let mut ch: u32 = if let [b] = utf8.as_bytes() {
        // 글이 정확히 한 바이트면 그것이 바꿀 글자다
        u32::from(*b)
    } else {
        // 러시아어·몽골어 배열의 `c` 키는 U+0441이지만 어느 터미널이나 Ctrl+C다. 물리 키의 글자를 쓴다.
        // Shift는 어떻게 바뀌는지 배열 없이는 모르므로(CSI u로 보낸다) Ctrl 혼자일 때만 쓴다
        let cp = physical.filter(|&cp| cp <= 0xff)?;
        if shift || mods.sup {
            return None;
        }
        cp
    };
    // `Ctrl+Shift+-`(밑줄)처럼 A~Z 밖의 글자는 Shift가 글자를 만든 것이다. fixterms가 정한 `@`만 Shift를 남긴다
    if shift && !(u32::from(b'A')..=u32::from(b'Z')).contains(&ch) && ch != u32::from(b'@') {
        shift = false;
    }
    // Caps Lock으로 대문자가 온 것은 unshifted로 되돌린다
    if (u32::from(b'A')..=u32::from(b'Z')).contains(&ch) && unshifted > 0 && unshifted <= 0xff {
        ch = unshifted;
    }
    // 남은 수식 키가 Ctrl 하나뿐이어야 한다. Ctrl+Shift+m은 CSI u로 보내 Ctrl+m과 구분한다
    if shift || mods.sup {
        return None;
    }
    Some(match u8::try_from(ch).ok()? {
        b' ' | b'2' | b'@' => 0,
        b'/' | b'7' | b'_' => 31,
        b'0' => 48,
        b'1' => 49,
        b'3' => 27,
        b'4' | b'\\' => 28,
        b'5' | b']' => 29,
        b'6' | b'^' | b'~' => 30,
        b'8' | b'?' => 127,
        b'9' => 57,
        c @ (b'a'..=b'z') if !matches!(c, b'i' | b'm') => c - b'a' + 1,
        _ => return None,
    })
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// 두 모드 모두에서 공백으로 바꾸는 바이트(Ghostty `paste.zig` `encode`의 `strip`, xterm의 목록).
/// 줄 편집기·tty 드라이버가 신호나 편집 동작으로 받는 바이트다. 셸의 줄 편집기에 그대로 가면 줄을 지우거나(Ctrl-U·W),
/// 신호를 보내거나(Ctrl-C·Z·\), 다음 바이트를 글자로 받게(Ctrl-V) 한다.
/// VINTR부터 VDISCARD까지 열 개는 프로그램이 tcsetattr로 바꿀 수 있으나 Ghostty처럼 기본 값으로 못 박는다
const PASTE_STRIPPED: [u8; 16] = [
    0x00, // NUL
    0x08, // BS
    0x05, // ENQ
    0x04, // EOT
    0x1B, // ESC
    0x7F, // DEL
    0x03, // VINTR (Ctrl+C)
    0x1C, // VQUIT (Ctrl+\)
    0x15, // VKILL (Ctrl+U)
    0x1A, // VSUSP (Ctrl+Z)
    0x11, // VSTART (Ctrl+Q)
    0x13, // VSTOP (Ctrl+S)
    0x17, // VWERASE (Ctrl+W)
    0x16, // VLNEXT (Ctrl+V)
    0x12, // VREPRINT (Ctrl+R)
    0x0F, // VDISCARD (Ctrl+O)
];

/// 붙여넣기 글을 바이트로 바꾼다(Ghostty `paste.zig` `encode`, #565).
/// 괄호 붙여넣기 모드와 상관없이 위험한 제어 바이트 16종(`PASTE_STRIPPED`)을 공백으로 바꾼다. 글 안의 끝 표시 `ESC[201~`도
/// ESC가 공백이 되어 깨지므로 붙여넣기 구간을 빠져나와 명령을 실행시키는 길이 막힌다.
/// 괄호 모드면 시작·끝 표시로 감싼다. 꺼져 있으면 LF를 CR로 바꾼다(셸이 한 줄씩 받는 꼴). CRLF는 xterm처럼 CR 둘이 된다.
/// 바이트 단위로 바꾸므로 UTF-8 글자(0x80 이상 바이트)는 건드리지 않는다
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + PASTE_START.len() + PASTE_END.len());
    if bracketed {
        out.extend_from_slice(PASTE_START);
    }
    out.extend(text.bytes().map(|b| match b {
        b if PASTE_STRIPPED.contains(&b) => b' ',
        b'\n' if !bracketed => b'\r',
        b => b,
    }));
    if bracketed {
        out.extend_from_slice(PASTE_END);
    }
    out
}

/// 묻지 않고 붙여넣어도 되는 글인가(Ghostty `clipboard-paste-protection`, `Surface.zig` `completeClipboardPaste` 6281~6301줄, #109·#565).
/// 괄호 붙여넣기가 켜져 있으면: 글에 끝 표시 `ESC[201~`가 있으면 `bracketed_safe`와 상관없이 안전하지 않다(구간을 빠져나가
/// 명령을 끼우려는 글이다). 없고 `bracketed_safe`면 안전하다.
/// 그 밖에는(꺼져 있거나 `bracketed_safe`가 아니면) 줄바꿈(LF·CR)이나 끝 표시가 없을 때만 안전하다.
/// 줄바꿈은 괄호 붙여넣기가 꺼져 있으면 셸에서 곧바로 Enter가 되어 보지 못한 명령이 돈다
pub fn paste_is_safe(text: &str, bracketed: bool, bracketed_safe: bool) -> bool {
    let has_end = text.contains("\x1b[201~");
    if bracketed {
        if has_end {
            return false;
        }
        if bracketed_safe {
            return true;
        }
    }
    !has_end && !text.contains(['\n', '\r'])
}

/// kitty `paste_actions`의 `confirm-if-large` 상한(kitty `window.py`, 16KiB). 이 값을 넘는 글(UTF-8 바이트 수)이 크다
pub const LARGE_PASTE_BYTES: usize = 16 * 1024;

/// 큰 붙여넣기인가(kitty `confirm-if-large`, `len(btext) > 16 * 1024`). 글자 수가 아니라 UTF-8 바이트 수로 센다
pub fn paste_is_large(text: &str) -> bool {
    text.len() > LARGE_PASTE_BYTES
}

/// kitty `url_prefixes` 기본값(kitty `options/definition.py`). `quote-urls-at-prompt`가 URL로 보는 scheme이다
const URL_PREFIXES: [&str; 15] = [
    "file", "ftp", "ftps", "gemini", "git", "gopher", "http", "https", "irc", "ircs", "kitty", "mailto", "news",
    "sftp", "ssh",
];

/// 글이 URL이면 셸 낱말 하나가 되게 감싼다(kitty `quote-urls-at-prompt`, `window.py` `paste_with_actions`).
/// URL은 `scheme:` 뒤에 `//`가 오거나 scheme이 `mailto`·`irc`인 글이다(kitty 코드 그대로
/// `scheme in ('mailto', 'irc')`, `ircs`는 `//`가 있어야 한다). scheme은 대소문자를 가린다. 첫 줄 뒤에 다른 글이 있어도 글 전체를 감싼다.
/// 감싸는 방식은 Python `shlex.quote`와 같다: 글자가 모두 `[A-Za-z0-9_@%+=:,./-]`이면 그대로, 아니면 작은따옴표로 감싸고
/// 안의 작은따옴표는 `'"'"'`로 바꾼다. 작은따옴표 안에서는 셸이 어떤 글자도 해석하지 않으므로 `;`·`$()`·백틱이 명령이 되지 않는다.
/// 위험한 제어 바이트는 여기서 지우지 않는다. 이어서 `encode_paste`가 공백으로 바꾼다
pub fn quote_url_for_shell(text: &str) -> Option<String> {
    let (scheme, rest) = text.split_once(':')?;
    if !URL_PREFIXES.contains(&scheme) {
        return None;
    }
    // Python `.`은 줄바꿈을 먹지 않는다: `.+`는 첫 줄까지만이다
    let first_line = rest.split('\n').next().unwrap_or("");
    if first_line.is_empty() || !(first_line.starts_with("//") || matches!(scheme, "mailto" | "irc")) {
        return None;
    }
    Some(shell_quote(text))
}

/// Python `shlex.quote`
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if s.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'@' | b'%' | b'+' | b'=' | b':' | b',' | b'.' | b'/' | b'-')
    }) {
        return s.into();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// 줄바꿈(CRLF·LF·CR)을 공백 하나로 바꾼다(kitty `replace-newline`). kitty는 LF를 `ESC E`(NEL)로 바꾸지만 이 저장소는
/// 붙여넣는 글의 ESC를 공백으로 바꾸므로(`encode_paste`) 공백이 된다. CR도 Enter라 같이 바꾼다
pub fn replace_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(' ');
            }
            '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// 붙여넣기 동작을 글에 적용한다(kitty `paste_actions`의 글을 바꾸는 동작). `encode_paste` 앞에서 부른다.
/// `quote_urls`는 `at_prompt`일 때만 한다. 여기서 바뀐 글도 이어서 `paste_is_safe`와 `encode_paste`를 지나므로
/// 위험 바이트 공백 바꾸기·끝 표시 깨기·줄바꿈 확인은 그대로다
pub fn apply_paste_actions(text: String, quote_urls: bool, at_prompt: bool, replace_newline: bool) -> String {
    let quoted = if quote_urls && at_prompt {
        quote_url_for_shell(&text)
    } else {
        None
    };
    let text = quoted.unwrap_or(text);
    if replace_newline { replace_newlines(&text) } else { text }
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
        // `[`는 fixterms 명세대로 제어 문자(ESC)로 바꾸지 않고 CSI u로 보낸다(Ghostty `ctrlSeq`, 시험 `legacy: fixterm awkward letters`)
        assert_eq!(encode_text("[", ctrl), b"\x1b[91;5u");
        assert_eq!(encode_text("\\", ctrl), vec![0x1c]);
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
        // DECBKM은 수식 키 없음·Ctrl 혼자 칸에만 있다(function_keys.zig 마지막 두 칸). 수식 키가 더 붙은 칸은 켜도 그대로다(#571)
        assert_eq!(encode_named_with(NamedKey::Backspace, all, false, set), b"\x1b\x08");
        assert_eq!(encode_named_with(NamedKey::Backspace, cs, false, set), b"\x08");
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
        assert_eq!(
            encode_paste("a\nb\r\nc", false),
            b"a\rb\r\rc",
            "CRLF는 Ghostty·xterm처럼 CR 둘(#565)"
        );
    }
}

#[cfg(test)]
#[path = "paste_tests.rs"]
mod paste_tests;
