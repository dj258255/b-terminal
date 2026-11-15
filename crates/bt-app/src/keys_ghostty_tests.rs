//! Ghostty `src/input/key_encode.zig`(main 7fa26e8b675f)의 `legacy:` 시험 47개 가운데 이 인코더로 옮길 수 있는 것(#567).
//! 앱이 실제로 부르는 입구(`encode_legacy`, `keys::encode`가 winit 이벤트에서 읽은 값을 넘기는 자리)에서 시험한다.
//! winit `KeyEvent`는 밖에서 만들 수 없어 그 아래 단계부터 부른다. 기대 바이트는 Ghostty 시험의 값 그대로다.
//!
//! 옮기지 않은 시험(IME 길이라 이 인코더를 지나지 않는다): Ghostty는 macOS에서 IME 조합 중인 글을 키 이벤트에 `utf8`로 실어 보내
//! Backspace·Enter·Escape를 그 글과 함께 다루지만, b-terminal은 조합 중인 키 이벤트를 winit이 올려 주지 않고 확정 글자만 `Ime::Commit`으로
//! PTY에 직접 보낸다(AGENTS.md).
//! - 1990줄 `legacy: backspace with utf8 (dead key state)`
//! - 2038줄 `legacy: enter with utf8 (dead key state)`
//! - 2049줄 `legacy: esc with utf8 (dead key state)`
//!
//! Ghostty `Options`의 기본값(1035 꺼짐, 1036 꺼짐, `macos_option_as_alt = false`)을 그대로 쓰므로 `Opts::default()`가 터미널의 기본 모드와 다르다.

use super::*;
use winit::keyboard::{NativeKey, SmolStr};

const SHIFT: ModifiersState = ModifiersState::SHIFT;
const CTRL: ModifiersState = ModifiersState::CONTROL;
const ALT: ModifiersState = ModifiersState::ALT;
const SUPER: ModifiersState = ModifiersState::SUPER;
const NONE: ModifiersState = ModifiersState::empty();

/// Ghostty `KeyEvent`에 해당하는 시험 입력
struct Ev {
    code: Option<KeyCode>,
    utf8: &'static str,
    unshifted: u32,
    mods: ModifiersState,
}

fn ev(code: KeyCode) -> Ev {
    Ev {
        code: Some(code),
        utf8: "",
        unshifted: 0,
        mods: NONE,
    }
}

/// Ghostty의 `.key = .unidentified`
fn unidentified() -> Ev {
    Ev {
        code: None,
        utf8: "",
        unshifted: 0,
        mods: NONE,
    }
}

impl Ev {
    fn utf8(mut self, s: &'static str) -> Self {
        self.utf8 = s;
        self
    }
    fn unshifted(mut self, c: char) -> Self {
        self.unshifted = c as u32;
        self
    }
    fn mods(mut self, m: ModifiersState) -> Self {
        self.mods = m;
        self
    }
}

/// Ghostty `Options`에 해당하는 시험 입력(기본값도 같다)
#[derive(Clone, Copy, Default)]
struct Opts {
    keypad_key_application: bool,
    backarrow_key_mode: bool,
    ignore_keypad_with_numlock: bool,
    alt_esc_prefix: bool,
    modify_other_keys_state_2: bool,
    /// `macos_option_as_alt`(`.true`면 true, `.false`면 false)
    option_as_alt: bool,
}

/// 물리 키가 winit에서 이름 키로 오는 것. winit이 이 키들에 글자를 주지 않는 것과 같다
fn named_of(code: KeyCode) -> Option<WKey> {
    use KeyCode as C;
    Some(match code {
        C::Enter | C::NumpadEnter => WKey::Enter,
        C::Tab => WKey::Tab,
        C::Backspace => WKey::Backspace,
        C::Escape => WKey::Escape,
        C::Space => WKey::Space,
        C::ArrowUp => WKey::ArrowUp,
        C::ArrowDown => WKey::ArrowDown,
        C::ArrowLeft => WKey::ArrowLeft,
        C::ArrowRight => WKey::ArrowRight,
        C::Home => WKey::Home,
        C::End => WKey::End,
        C::PageUp => WKey::PageUp,
        C::PageDown => WKey::PageDown,
        C::Insert => WKey::Insert,
        C::Delete => WKey::Delete,
        C::F1 => WKey::F1,
        C::F2 => WKey::F2,
        C::F3 => WKey::F3,
        C::F4 => WKey::F4,
        C::F5 => WKey::F5,
        C::F6 => WKey::F6,
        C::F7 => WKey::F7,
        C::F8 => WKey::F8,
        C::F9 => WKey::F9,
        C::F10 => WKey::F10,
        C::F11 => WKey::F11,
        C::F12 => WKey::F12,
        C::F13 => WKey::F13,
        C::F14 => WKey::F14,
        C::F15 => WKey::F15,
        C::F16 => WKey::F16,
        C::F17 => WKey::F17,
        C::F18 => WKey::F18,
        C::F19 => WKey::F19,
        C::F20 => WKey::F20,
        C::F21 => WKey::F21,
        C::F22 => WKey::F22,
        C::F23 => WKey::F23,
        C::F24 => WKey::F24,
        C::F25 => WKey::F25,
        C::Help => WKey::Help,
        C::ContextMenu => WKey::ContextMenu,
        _ => return None,
    })
}

fn is_numpad(code: KeyCode) -> bool {
    use KeyCode as C;
    matches!(
        code,
        C::Numpad0
            | C::Numpad1
            | C::Numpad2
            | C::Numpad3
            | C::Numpad4
            | C::Numpad5
            | C::Numpad6
            | C::Numpad7
            | C::Numpad8
            | C::Numpad9
            | C::NumpadEnter
    )
}

fn run(e: &Ev, o: Opts) -> Vec<u8> {
    let logical = match e.code.and_then(named_of) {
        Some(n) => Key::Named(n),
        None if !e.utf8.is_empty() => Key::Character(SmolStr::new(e.utf8)),
        None => Key::Unidentified(NativeKey::Unidentified),
    };
    let location = if e.code.is_some_and(is_numpad) {
        KeyLocation::Numpad
    } else {
        KeyLocation::Standard
    };
    let k = LegacyKey {
        logical: &logical,
        location,
        physical: e.code,
        text: (!e.utf8.is_empty()).then_some(e.utf8),
        unshifted: e.unshifted,
        mods: e.mods,
        raw: e.mods,
    };
    let mut modes = bt_term::Modes::default();
    modes.app_keypad = o.keypad_key_application;
    modes.modify_other_keys = o.modify_other_keys_state_2;
    modes.key = KeyModes {
        backarrow_key: o.backarrow_key_mode,
        ignore_keypad_with_numlock: o.ignore_keypad_with_numlock,
        alt_esc_prefix: o.alt_esc_prefix,
        alt_sends_escape: false,
    };
    encode_legacy(&k, modes, o.option_as_alt)
}

fn default() -> Opts {
    Opts::default()
}

fn alt_esc(option_as_alt: bool) -> Opts {
    Opts {
        alt_esc_prefix: true,
        option_as_alt,
        ..default()
    }
}

fn mok2() -> Opts {
    Opts {
        modify_other_keys_state_2: true,
        ..default()
    }
}

// 2060줄
#[test]
#[ignore = "차이 B"]
fn legacy_alt_plus_escape_with_modify_other_state_2() {
    let e = ev(KeyCode::Escape).mods(ALT);
    assert_eq!(run(&e, default()), b"\x1b\x1b");
    assert_eq!(run(&e, mok2()), b"\x1b[27;3;27~");
}

// 2082줄
#[test]
fn legacy_ctrl_plus_shift_plus_minus_underscore_on_us() {
    let e = ev(KeyCode::Minus).mods(CTRL | SHIFT).utf8("_");
    assert_eq!(run(&e, default()), b"\x1f");
}

// 2093줄
#[test]
#[ignore = "차이 D"]
fn legacy_ctrl_plus_alt_plus_c() {
    let e = ev(KeyCode::KeyC).mods(CTRL | ALT).utf8("c");
    assert_eq!(run(&e, default()), b"\x1b\x03");
}

// 2104줄
#[test]
fn legacy_alt_plus_c() {
    let e = ev(KeyCode::KeyC).mods(ALT).utf8("c");
    assert_eq!(run(&e, alt_esc(true)), b"\x1bc");
}

// 2118줄
#[test]
#[ignore = "차이 A"]
fn legacy_alt_plus_e_only_unshifted() {
    let e = ev(KeyCode::KeyE).mods(ALT).unshifted('e');
    assert_eq!(run(&e, alt_esc(true)), b"\x1be");
}

// 2132줄
#[test]
fn legacy_alt_plus_unicode() {
    for (text, cp, expected) in [
        ("é", 'é', "\u{1b}é"),
        ("ő", 'ő', "\u{1b}ő"),
        ("界", '界', "\u{1b}界"),
        ("😀", '😀', "\u{1b}😀"),
    ] {
        let e = unidentified().mods(ALT).utf8(text).unshifted(cp);
        assert_eq!(run(&e, alt_esc(true)), expected.as_bytes(), "{text}");
    }
}

// 2161줄
#[test]
#[ignore = "차이 A"]
fn legacy_alt_plus_unicode_only_unshifted() {
    let e = unidentified().mods(ALT).unshifted('界');
    assert_eq!(run(&e, alt_esc(true)), "\u{1b}界".as_bytes());
}

// 2175줄
#[test]
fn legacy_alt_plus_unicode_without_unshifted() {
    let e = unidentified().mods(ALT).utf8("😀");
    assert_eq!(run(&e, alt_esc(true)), "\u{1b}😀".as_bytes());
}

// 2189줄(Ghostty는 macOS에서만 돈다. 이 인코더는 macOS 앱의 것이라 어디서든 같은 규칙을 쓴다)
#[test]
#[ignore = "차이 A"]
fn legacy_alt_plus_x_macos() {
    let e = ev(KeyCode::KeyC).mods(ALT).utf8("≈").unshifted('c');
    assert_eq!(run(&e, alt_esc(true)), b"\x1bc");
}

// 2206줄
#[test]
fn legacy_shift_plus_alt_plus_period_macos() {
    let e = ev(KeyCode::Period).mods(ALT | SHIFT).utf8(">").unshifted('.');
    assert_eq!(run(&e, alt_esc(true)), b"\x1b>");
}

// 2223줄
#[test]
fn legacy_alt_plus_cyrillic_ef() {
    let e = ev(KeyCode::KeyF).mods(ALT).utf8("ф");
    assert_eq!(run(&e, alt_esc(true)), "\u{1b}ф".as_bytes());
}

// 2237줄
#[test]
fn legacy_ctrl_plus_c() {
    let e = ev(KeyCode::KeyC).mods(CTRL).utf8("c");
    assert_eq!(run(&e, default()), b"\x03");
}

// 2248줄
#[test]
fn legacy_ctrl_plus_space() {
    let e = ev(KeyCode::Space).mods(CTRL).utf8(" ");
    assert_eq!(run(&e, default()), b"\x00");
}

// 2259줄
#[test]
fn legacy_ctrl_plus_shift_plus_backspace() {
    let e = ev(KeyCode::Backspace).mods(CTRL | SHIFT);
    assert_eq!(run(&e, default()), b"\x08");
}

// 2269줄
#[test]
#[ignore = "차이 B"]
fn legacy_ctrl_plus_alt_plus_shift_plus_backspace() {
    let e = ev(KeyCode::Backspace).mods(CTRL | ALT | SHIFT);
    assert_eq!(run(&e, default()), b"\x1b\x08");
    assert_eq!(run(&e, mok2()), b"\x1b[27;8;127~");
}

// 2285줄
#[test]
fn legacy_backspace_decbkm_reset() {
    let e = ev(KeyCode::Backspace);
    assert_eq!(run(&e, default()), b"\x7f");
}

// 2295줄
#[test]
fn legacy_backspace_decbkm_reset_with_ctrl() {
    let e = ev(KeyCode::Backspace).mods(CTRL);
    assert_eq!(run(&e, default()), b"\x08");
}

// 2307줄
#[test]
fn legacy_backspace_decbkm_set() {
    let e = ev(KeyCode::Backspace);
    let o = Opts {
        backarrow_key_mode: true,
        ..default()
    };
    assert_eq!(run(&e, o), b"\x08");
}

// 2317줄
#[test]
fn legacy_backspace_decbkm_set_with_ctrl() {
    let e = ev(KeyCode::Backspace).mods(CTRL);
    let o = Opts {
        backarrow_key_mode: true,
        ..default()
    };
    assert_eq!(run(&e, o), b"\x7f");
}

// 2329줄
#[test]
fn legacy_ctrl_plus_shift_plus_char_with_modify_other_state_2() {
    let e = ev(KeyCode::KeyH).mods(CTRL | SHIFT).utf8("H");
    assert_eq!(run(&e, mok2()), b"\x1b[27;6;72~");
}

// 2342줄
#[test]
fn legacy_ctrl_plus_char_with_modify_other_state_2() {
    let e = ev(KeyCode::KeyP).mods(CTRL).utf8("p");
    assert_eq!(run(&e, mok2()), b"\x1b[27;5;112~");
}

// 2355줄
#[test]
fn legacy_ctrl_plus_char_without_modify_other_state_2() {
    let e = ev(KeyCode::KeyP).mods(CTRL).utf8("p");
    assert_eq!(run(&e, default()), b"\x10");
}

// 2366줄(우리 인코더에는 consumed_mods가 없다. Shift가 글자를 만드는 데 쓰였어도 수식 키로 센다)
#[test]
fn legacy_ctrl_plus_shift_plus_char_with_modify_other_state_2_and_consumed_mods() {
    let e = ev(KeyCode::KeyH).mods(CTRL | SHIFT).utf8("H");
    assert_eq!(run(&e, mok2()), b"\x1b[27;6;72~");
}

// 2380줄
#[test]
fn legacy_alt_plus_digit_with_modify_other_state_2() {
    let e = ev(KeyCode::Digit8).mods(ALT).utf8("8");
    let o = Opts {
        option_as_alt: true,
        ..mok2()
    };
    assert_eq!(run(&e, o), b"\x1b[27;3;56~");
}

// 2395줄(Ghostty는 macOS에서만 돈다)
#[test]
fn legacy_alt_plus_digit_with_modify_other_state_2_and_macos_option_as_alt_false() {
    // 유럽 배열에서 Option+8이 만드는 글자
    let e = ev(KeyCode::Digit8).mods(ALT).utf8("[");
    let o = Opts {
        option_as_alt: false,
        ..mok2()
    };
    assert_eq!(run(&e, o), b"[");
}

// 2411줄
#[test]
fn legacy_fixterm_awkward_letters() {
    let e = ev(KeyCode::KeyI).mods(CTRL).utf8("i");
    assert_eq!(run(&e, default()), b"\x1b[105;5u");
    let e = ev(KeyCode::KeyM).mods(CTRL).utf8("m");
    assert_eq!(run(&e, default()), b"\x1b[109;5u");
    let e = ev(KeyCode::BracketLeft).mods(CTRL).utf8("[");
    assert_eq!(run(&e, default()), b"\x1b[91;5u");
    let e = ev(KeyCode::Digit2).mods(CTRL | SHIFT).utf8("@").unshifted('2');
    assert_eq!(run(&e, default()), b"\x1b[64;5u");
}

// 2454줄
#[test]
fn legacy_ctrl_plus_shift_plus_letter_ascii() {
    let e = ev(KeyCode::KeyM).mods(CTRL | SHIFT).utf8("M").unshifted('m');
    assert_eq!(run(&e, default()), b"\x1b[109;6u");
}

// 2468줄
#[test]
fn legacy_shift_plus_function_key_should_use_all_mods() {
    let e = ev(KeyCode::ArrowUp).mods(SHIFT);
    assert_eq!(run(&e, default()), b"\x1b[1;2A");
}

// 2479줄
#[test]
fn legacy_keypad_enter() {
    let e = ev(KeyCode::NumpadEnter);
    assert_eq!(run(&e, default()), b"\r");
}

// 2490줄
#[test]
fn legacy_keypad_1() {
    let e = ev(KeyCode::Numpad1).utf8("1");
    assert_eq!(run(&e, default()), b"1");
}

// 2502줄
#[test]
fn legacy_keypad_1_with_modify_other_state_2() {
    let e = ev(KeyCode::Numpad1).mods(CTRL).utf8("1");
    assert_eq!(run(&e, mok2()), b"1");
    let o = Opts {
        keypad_key_application: true,
        ..mok2()
    };
    assert_eq!(run(&e, o), b"\x1bO5q");
}

// 2531줄
#[test]
fn legacy_keypad_1_with_application_keypad() {
    let e = ev(KeyCode::Numpad1).utf8("1");
    let o = Opts {
        keypad_key_application: true,
        ..default()
    };
    assert_eq!(run(&e, o), b"\x1bOq");
}

// 2545줄(winit은 NumLock 상태를 알려 주지 않아 Ghostty의 `num_lock` 수식 키는 없다. 결과는 같다)
#[test]
fn legacy_keypad_1_with_application_keypad_and_numlock() {
    let e = ev(KeyCode::Numpad1).utf8("1");
    let o = Opts {
        keypad_key_application: true,
        ..default()
    };
    assert_eq!(run(&e, o), b"\x1bOq");
}

// 2559줄
#[test]
fn legacy_keypad_1_with_application_keypad_and_numlock_ignore() {
    let e = ev(KeyCode::Numpad1).utf8("1");
    let o = Opts {
        keypad_key_application: true,
        ignore_keypad_with_numlock: true,
        ..default()
    };
    assert_eq!(run(&e, o), b"1");
}

// 2574줄
#[test]
#[ignore = "차이 E"]
fn legacy_f1() {
    for (code, expected) in [
        (KeyCode::F1, &b"\x1b[1;5P"[..]),
        (KeyCode::F2, b"\x1b[1;5Q"),
        (KeyCode::F3, b"\x1b[13;5~"),
        (KeyCode::F4, b"\x1b[1;5S"),
        (KeyCode::F5, b"\x1b[15;5~"),
    ] {
        assert_eq!(run(&ev(code).mods(CTRL), default()), expected, "{code:?}");
    }
}

// 2633줄
#[test]
#[ignore = "차이 F"]
fn legacy_f13_through_f25() {
    for (code, n) in [
        (KeyCode::F13, 25),
        (KeyCode::F14, 26),
        (KeyCode::F15, 28),
        (KeyCode::F16, 29),
        (KeyCode::F17, 31),
        (KeyCode::F18, 32),
        (KeyCode::F19, 33),
        (KeyCode::F20, 34),
        (KeyCode::F21, 42),
        (KeyCode::F22, 43),
        (KeyCode::F23, 44),
        (KeyCode::F24, 45),
        (KeyCode::F25, 46),
    ] {
        assert_eq!(run(&ev(code), default()), format!("\x1b[{n}~").as_bytes(), "{code:?}");
        assert_eq!(
            run(&ev(code).mods(CTRL), mok2()),
            format!("\x1b[{n};5~").as_bytes(),
            "{code:?} ctrl, modifyOtherKeys 2"
        );
    }
}

// 2681줄
#[test]
#[ignore = "차이 G"]
fn legacy_help_and_context_menu() {
    for (code, n) in [(KeyCode::Help, 28), (KeyCode::ContextMenu, 29)] {
        assert_eq!(run(&ev(code), default()), format!("\x1b[{n}~").as_bytes(), "{code:?}");
        assert_eq!(
            run(&ev(code).mods(CTRL), mok2()),
            format!("\x1b[{n};5~").as_bytes(),
            "{code:?} ctrl, modifyOtherKeys 2"
        );
    }
}

// 2718줄·2731줄(winit은 수식 키를 좌우로 알려 주지만 이 인코더는 좌우를 구분하지 않는다. Ghostty도 구분하지 않아 결과가 같다)
#[test]
fn legacy_left_shift_plus_tab_and_right_shift_plus_tab() {
    let e = ev(KeyCode::Tab).mods(SHIFT);
    assert_eq!(run(&e, default()), b"\x1b[Z");
}

// 2744줄
#[test]
fn legacy_hu_layout_ctrl_plus_o_double_acute_sends_proper_codepoint() {
    let e = ev(KeyCode::BracketLeft).mods(CTRL).utf8("ő").unshifted('ő');
    let out = run(&e, default());
    assert_eq!(&out[1..], b"[337;5u", "{out:?}");
    assert_eq!(out[0], 0x1b);
}

// 2757줄
#[test]
#[ignore = "차이 H"]
fn legacy_super_only_on_macos_with_text() {
    let e = ev(KeyCode::KeyB).mods(SUPER).utf8("b");
    assert_eq!(run(&e, default()), b"");
}

// 2770줄
#[test]
#[ignore = "차이 H"]
fn legacy_super_and_other_mods_on_macos_with_text() {
    let e = ev(KeyCode::KeyB).mods(SUPER | SHIFT).utf8("B");
    assert_eq!(run(&e, default()), b"");
}

// 2783줄(winit은 Backspace에 0x7f 글자를 싣기도 한다. 이름 키가 먼저라 글자는 쓰지 않는다)
#[test]
fn legacy_backspace_with_del_utf8_decbkm_reset() {
    let e = ev(KeyCode::Backspace).utf8("\x7f").unshifted('\u{8}');
    assert_eq!(run(&e, default()), b"\x7f");
}

// 2794줄
#[test]
fn legacy_backspace_with_del_utf8_decbkm_set() {
    let e = ev(KeyCode::Backspace).utf8("\x7f").unshifted('\u{8}');
    let o = Opts {
        backarrow_key_mode: true,
        ..default()
    };
    assert_eq!(run(&e, o), b"\x08");
}
