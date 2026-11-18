//! 스크립트 `send key`(#650)의 키 이름·수식 키 해석과 키 입력 만들기.
//!
//! 키 이름은 Ghostty `Ghostty.Input.Key`의 rawValue(`enter`, `arrowUp`, `a`, `digit1`, `bracketLeft`, ...) 그대로다(대소문자를 가린다).
//! 수식 키는 Ghostty `Ghostty.Input.Mods+AppleScript.swift`처럼 쉼표로 가른 `shift`·`control`·`option`·`command`다.
//! 이 모듈은 이름을 winit 물리 키로 바꾸고, 그 키를 사람이 눌렀다면 winit이 올렸을 `KeyInput`(미국 자판 배열 기준)으로 만든다.
//! 바이트는 여기서 만들지 않는다: 사람이 누른 키와 같은 인코더(`keys::encode_input`)가 만든다.
//!
//! 자판 배열은 미국(ANSI)으로 본다. Ghostty는 지금 입력 소스(자판 배열)로 한 번 바꾸지만(`KeyboardLayout.character`) 이 구현은 배열을
//! 읽지 않는다. 그래서 다른 배열에서도 `a`는 늘 A 자리의 키고, 그 글자는 `a`다. 죽은 키는 쓰지 않는다.

// macOS에서만 AppleScript 명령이 부른다(Linux 빌드에서는 시험만 쓴다)
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use winit::keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, NativeKey, PhysicalKey, SmolStr};

use crate::keys::KeyInput;

/// Ghostty `Ghostty.Input.Key`의 모든 이름(175개)과 이 앱이 아는 물리 키. `None`은 이름은 맞지만 winit에 해당 키가 없어 보낼 것이 없는 키다
/// (키패드의 방향·Home 같은 위치 키와 `numpadSeparator`)
const KEY_NAMES: &[(&str, Option<KeyCode>)] = &[
    ("backquote", Some(KeyCode::Backquote)),
    ("backslash", Some(KeyCode::Backslash)),
    ("bracketLeft", Some(KeyCode::BracketLeft)),
    ("bracketRight", Some(KeyCode::BracketRight)),
    ("comma", Some(KeyCode::Comma)),
    ("digit0", Some(KeyCode::Digit0)),
    ("digit1", Some(KeyCode::Digit1)),
    ("digit2", Some(KeyCode::Digit2)),
    ("digit3", Some(KeyCode::Digit3)),
    ("digit4", Some(KeyCode::Digit4)),
    ("digit5", Some(KeyCode::Digit5)),
    ("digit6", Some(KeyCode::Digit6)),
    ("digit7", Some(KeyCode::Digit7)),
    ("digit8", Some(KeyCode::Digit8)),
    ("digit9", Some(KeyCode::Digit9)),
    ("equal", Some(KeyCode::Equal)),
    ("intlBackslash", Some(KeyCode::IntlBackslash)),
    ("intlRo", Some(KeyCode::IntlRo)),
    ("intlYen", Some(KeyCode::IntlYen)),
    ("a", Some(KeyCode::KeyA)),
    ("b", Some(KeyCode::KeyB)),
    ("c", Some(KeyCode::KeyC)),
    ("d", Some(KeyCode::KeyD)),
    ("e", Some(KeyCode::KeyE)),
    ("f", Some(KeyCode::KeyF)),
    ("g", Some(KeyCode::KeyG)),
    ("h", Some(KeyCode::KeyH)),
    ("i", Some(KeyCode::KeyI)),
    ("j", Some(KeyCode::KeyJ)),
    ("k", Some(KeyCode::KeyK)),
    ("l", Some(KeyCode::KeyL)),
    ("m", Some(KeyCode::KeyM)),
    ("n", Some(KeyCode::KeyN)),
    ("o", Some(KeyCode::KeyO)),
    ("p", Some(KeyCode::KeyP)),
    ("q", Some(KeyCode::KeyQ)),
    ("r", Some(KeyCode::KeyR)),
    ("s", Some(KeyCode::KeyS)),
    ("t", Some(KeyCode::KeyT)),
    ("u", Some(KeyCode::KeyU)),
    ("v", Some(KeyCode::KeyV)),
    ("w", Some(KeyCode::KeyW)),
    ("x", Some(KeyCode::KeyX)),
    ("y", Some(KeyCode::KeyY)),
    ("z", Some(KeyCode::KeyZ)),
    ("minus", Some(KeyCode::Minus)),
    ("period", Some(KeyCode::Period)),
    ("quote", Some(KeyCode::Quote)),
    ("semicolon", Some(KeyCode::Semicolon)),
    ("slash", Some(KeyCode::Slash)),
    ("altLeft", Some(KeyCode::AltLeft)),
    ("altRight", Some(KeyCode::AltRight)),
    ("backspace", Some(KeyCode::Backspace)),
    ("capsLock", Some(KeyCode::CapsLock)),
    ("contextMenu", Some(KeyCode::ContextMenu)),
    ("controlLeft", Some(KeyCode::ControlLeft)),
    ("controlRight", Some(KeyCode::ControlRight)),
    ("enter", Some(KeyCode::Enter)),
    ("metaLeft", Some(KeyCode::SuperLeft)),
    ("metaRight", Some(KeyCode::SuperRight)),
    ("shiftLeft", Some(KeyCode::ShiftLeft)),
    ("shiftRight", Some(KeyCode::ShiftRight)),
    ("space", Some(KeyCode::Space)),
    ("tab", Some(KeyCode::Tab)),
    ("convert", Some(KeyCode::Convert)),
    ("kanaMode", Some(KeyCode::KanaMode)),
    ("nonConvert", Some(KeyCode::NonConvert)),
    ("delete", Some(KeyCode::Delete)),
    ("end", Some(KeyCode::End)),
    ("help", Some(KeyCode::Help)),
    ("home", Some(KeyCode::Home)),
    ("insert", Some(KeyCode::Insert)),
    ("pageDown", Some(KeyCode::PageDown)),
    ("pageUp", Some(KeyCode::PageUp)),
    ("arrowDown", Some(KeyCode::ArrowDown)),
    ("arrowLeft", Some(KeyCode::ArrowLeft)),
    ("arrowRight", Some(KeyCode::ArrowRight)),
    ("arrowUp", Some(KeyCode::ArrowUp)),
    ("numLock", Some(KeyCode::NumLock)),
    ("numpad0", Some(KeyCode::Numpad0)),
    ("numpad1", Some(KeyCode::Numpad1)),
    ("numpad2", Some(KeyCode::Numpad2)),
    ("numpad3", Some(KeyCode::Numpad3)),
    ("numpad4", Some(KeyCode::Numpad4)),
    ("numpad5", Some(KeyCode::Numpad5)),
    ("numpad6", Some(KeyCode::Numpad6)),
    ("numpad7", Some(KeyCode::Numpad7)),
    ("numpad8", Some(KeyCode::Numpad8)),
    ("numpad9", Some(KeyCode::Numpad9)),
    ("numpadAdd", Some(KeyCode::NumpadAdd)),
    ("numpadBackspace", Some(KeyCode::NumpadBackspace)),
    ("numpadClear", Some(KeyCode::NumpadClear)),
    ("numpadClearEntry", Some(KeyCode::NumpadClearEntry)),
    ("numpadComma", Some(KeyCode::NumpadComma)),
    ("numpadDecimal", Some(KeyCode::NumpadDecimal)),
    ("numpadDivide", Some(KeyCode::NumpadDivide)),
    ("numpadEnter", Some(KeyCode::NumpadEnter)),
    ("numpadEqual", Some(KeyCode::NumpadEqual)),
    ("numpadMemoryAdd", Some(KeyCode::NumpadMemoryAdd)),
    ("numpadMemoryClear", Some(KeyCode::NumpadMemoryClear)),
    ("numpadMemoryRecall", Some(KeyCode::NumpadMemoryRecall)),
    ("numpadMemoryStore", Some(KeyCode::NumpadMemoryStore)),
    ("numpadMemorySubtract", Some(KeyCode::NumpadMemorySubtract)),
    ("numpadMultiply", Some(KeyCode::NumpadMultiply)),
    ("numpadParenLeft", Some(KeyCode::NumpadParenLeft)),
    ("numpadParenRight", Some(KeyCode::NumpadParenRight)),
    ("numpadSubtract", Some(KeyCode::NumpadSubtract)),
    ("numpadSeparator", None),
    ("numpadUp", None),
    ("numpadDown", None),
    ("numpadRight", None),
    ("numpadLeft", None),
    ("numpadBegin", None),
    ("numpadHome", None),
    ("numpadEnd", None),
    ("numpadInsert", None),
    ("numpadDelete", None),
    ("numpadPageUp", None),
    ("numpadPageDown", None),
    ("escape", Some(KeyCode::Escape)),
    ("f1", Some(KeyCode::F1)),
    ("f2", Some(KeyCode::F2)),
    ("f3", Some(KeyCode::F3)),
    ("f4", Some(KeyCode::F4)),
    ("f5", Some(KeyCode::F5)),
    ("f6", Some(KeyCode::F6)),
    ("f7", Some(KeyCode::F7)),
    ("f8", Some(KeyCode::F8)),
    ("f9", Some(KeyCode::F9)),
    ("f10", Some(KeyCode::F10)),
    ("f11", Some(KeyCode::F11)),
    ("f12", Some(KeyCode::F12)),
    ("f13", Some(KeyCode::F13)),
    ("f14", Some(KeyCode::F14)),
    ("f15", Some(KeyCode::F15)),
    ("f16", Some(KeyCode::F16)),
    ("f17", Some(KeyCode::F17)),
    ("f18", Some(KeyCode::F18)),
    ("f19", Some(KeyCode::F19)),
    ("f20", Some(KeyCode::F20)),
    ("f21", Some(KeyCode::F21)),
    ("f22", Some(KeyCode::F22)),
    ("f23", Some(KeyCode::F23)),
    ("f24", Some(KeyCode::F24)),
    ("f25", Some(KeyCode::F25)),
    ("fn", Some(KeyCode::Fn)),
    ("fnLock", Some(KeyCode::FnLock)),
    ("printScreen", Some(KeyCode::PrintScreen)),
    ("scrollLock", Some(KeyCode::ScrollLock)),
    ("pause", Some(KeyCode::Pause)),
    ("browserBack", Some(KeyCode::BrowserBack)),
    ("browserFavorites", Some(KeyCode::BrowserFavorites)),
    ("browserForward", Some(KeyCode::BrowserForward)),
    ("browserHome", Some(KeyCode::BrowserHome)),
    ("browserRefresh", Some(KeyCode::BrowserRefresh)),
    ("browserSearch", Some(KeyCode::BrowserSearch)),
    ("browserStop", Some(KeyCode::BrowserStop)),
    ("eject", Some(KeyCode::Eject)),
    ("launchApp1", Some(KeyCode::LaunchApp1)),
    ("launchApp2", Some(KeyCode::LaunchApp2)),
    ("launchMail", Some(KeyCode::LaunchMail)),
    ("mediaPlayPause", Some(KeyCode::MediaPlayPause)),
    ("mediaSelect", Some(KeyCode::MediaSelect)),
    ("mediaStop", Some(KeyCode::MediaStop)),
    ("mediaTrackNext", Some(KeyCode::MediaTrackNext)),
    ("mediaTrackPrevious", Some(KeyCode::MediaTrackPrevious)),
    ("power", Some(KeyCode::Power)),
    ("sleep", Some(KeyCode::Sleep)),
    ("audioVolumeDown", Some(KeyCode::AudioVolumeDown)),
    ("audioVolumeMute", Some(KeyCode::AudioVolumeMute)),
    ("audioVolumeUp", Some(KeyCode::AudioVolumeUp)),
    ("wakeUp", Some(KeyCode::WakeUp)),
    ("copy", Some(KeyCode::Copy)),
    ("cut", Some(KeyCode::Cut)),
    ("paste", Some(KeyCode::Paste)),
];

/// 키 이름 찾기 결과
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lookup {
    /// Ghostty의 키 이름이 아니다
    Unknown,
    /// Ghostty의 키 이름이지만 이 앱에는 대응하는 키가 없어 보낼 것이 없다
    NoKey,
    Code(KeyCode),
}

/// 키 이름을 물리 키로(Ghostty `Ghostty.Input.Key(rawValue:)`). 대소문자를 가린다
pub(crate) fn lookup(name: &str) -> Lookup {
    match KEY_NAMES.iter().find(|(n, _)| *n == name) {
        Some((_, Some(code))) => Lookup::Code(*code),
        Some((_, None)) => Lookup::NoKey,
        None => Lookup::Unknown,
    }
}

/// 쉼표로 가른 수식 키 글(`shift,control`)을 수식 키 상태로(Ghostty `Mods(scriptModifiers:)`).
/// 이름은 앞뒤 공백을 지우고 소문자로 본다. 빈 조각(`""`, `"shift,,control"`의 가운데)은 건너뛰고, 모르는 이름이 있으면 오류다
pub(crate) fn parse_modifiers(text: &str) -> Result<ModifiersState, String> {
    let mut mods = ModifiersState::empty();
    // Swift `split(separator:)`는 빈 조각을 버린다. 공백만 있는 조각은 버리지 않고 모르는 이름으로 본다
    for part in text.split(',').filter(|p| !p.is_empty()) {
        mods |= match part.trim().to_lowercase().as_str() {
            "shift" => ModifiersState::SHIFT,
            "control" => ModifiersState::CONTROL,
            "option" => ModifiersState::ALT,
            "command" => ModifiersState::SUPER,
            _ => return Err(format!("알 수 없는 수식 키: {text}")),
        };
    }
    Ok(mods)
}

/// winit이 macOS에서 이름 키로 올리는 물리 키(`code_to_key`와 같은 표에 F21~F25·Help·ContextMenu를 더했다. 이 인코더가 아는 이름 키다)
fn named_key(code: KeyCode) -> Option<NamedKey> {
    use KeyCode as C;
    Some(match code {
        C::Enter | C::NumpadEnter => NamedKey::Enter,
        C::Tab => NamedKey::Tab,
        C::Space => NamedKey::Space,
        C::Backspace => NamedKey::Backspace,
        C::Escape => NamedKey::Escape,
        C::Delete => NamedKey::Delete,
        C::Insert => NamedKey::Insert,
        C::Home => NamedKey::Home,
        C::End => NamedKey::End,
        C::PageUp => NamedKey::PageUp,
        C::PageDown => NamedKey::PageDown,
        C::ArrowUp => NamedKey::ArrowUp,
        C::ArrowDown => NamedKey::ArrowDown,
        C::ArrowLeft => NamedKey::ArrowLeft,
        C::ArrowRight => NamedKey::ArrowRight,
        C::Help => NamedKey::Help,
        C::ContextMenu => NamedKey::ContextMenu,
        C::NumLock => NamedKey::NumLock,
        C::ShiftLeft | C::ShiftRight => NamedKey::Shift,
        C::ControlLeft | C::ControlRight => NamedKey::Control,
        C::AltLeft | C::AltRight => NamedKey::Alt,
        C::SuperLeft | C::SuperRight => NamedKey::Super,
        C::F1 => NamedKey::F1,
        C::F2 => NamedKey::F2,
        C::F3 => NamedKey::F3,
        C::F4 => NamedKey::F4,
        C::F5 => NamedKey::F5,
        C::F6 => NamedKey::F6,
        C::F7 => NamedKey::F7,
        C::F8 => NamedKey::F8,
        C::F9 => NamedKey::F9,
        C::F10 => NamedKey::F10,
        C::F11 => NamedKey::F11,
        C::F12 => NamedKey::F12,
        C::F13 => NamedKey::F13,
        C::F14 => NamedKey::F14,
        C::F15 => NamedKey::F15,
        C::F16 => NamedKey::F16,
        C::F17 => NamedKey::F17,
        C::F18 => NamedKey::F18,
        C::F19 => NamedKey::F19,
        C::F20 => NamedKey::F20,
        C::F21 => NamedKey::F21,
        C::F22 => NamedKey::F22,
        C::F23 => NamedKey::F23,
        C::F24 => NamedKey::F24,
        C::F25 => NamedKey::F25,
        _ => return None,
    })
}

/// 미국 배열에서 Shift를 눌렀을 때의 글자
fn shifted_char(c: char) -> char {
    match c {
        'a'..='z' => c.to_ascii_uppercase(),
        '1' => '!',
        '2' => '@',
        '3' => '#',
        '4' => '$',
        '5' => '%',
        '6' => '^',
        '7' => '&',
        '8' => '*',
        '9' => '(',
        '0' => ')',
        '`' => '~',
        '-' => '_',
        '=' => '+',
        '[' => '{',
        ']' => '}',
        '\\' => '|',
        ';' => ':',
        '\'' => '"',
        ',' => '<',
        '.' => '>',
        '/' => '?',
        other => other,
    }
}

/// winit `code_to_location`와 같다: 좌우 수식 키와 키패드의 위치
fn location_of(code: KeyCode) -> KeyLocation {
    use KeyCode as C;
    match code {
        C::ShiftLeft | C::ControlLeft | C::AltLeft | C::SuperLeft => KeyLocation::Left,
        C::ShiftRight | C::ControlRight | C::AltRight | C::SuperRight => KeyLocation::Right,
        C::NumLock
        | C::NumpadDecimal
        | C::NumpadMultiply
        | C::NumpadAdd
        | C::NumpadDivide
        | C::NumpadEnter
        | C::NumpadSubtract
        | C::NumpadEqual
        | C::Numpad0
        | C::Numpad1
        | C::Numpad2
        | C::Numpad3
        | C::Numpad4
        | C::Numpad5
        | C::Numpad6
        | C::Numpad7
        | C::Numpad8
        | C::Numpad9 => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

/// 수식 키 없는 키와 Shift를 누른 키(논리 키). 글자가 없고 이름도 없는 키는 둘 다 `Unidentified`다
fn layout_keys(code: KeyCode) -> (Key, Key) {
    if let Some(n) = named_key(code) {
        return (Key::Named(n), Key::Named(n));
    }
    let ch = if code == KeyCode::NumpadComma {
        Some(',')
    } else {
        crate::keys::base_layout_codepoint(code).and_then(char::from_u32)
    };
    match ch {
        Some(c) => {
            // 키패드는 Shift와 상관없이 그 글자다(NumLock을 켠 때)
            let shifted = if location_of(code) == KeyLocation::Numpad {
                c
            } else {
                shifted_char(c)
            };
            (
                Key::Character(c.to_string().into()),
                Key::Character(shifted.to_string().into()),
            )
        }
        None => (
            Key::Unidentified(NativeKey::Unidentified),
            Key::Unidentified(NativeKey::Unidentified),
        ),
    }
}

/// 키를 눌렀을 때(`pressed`) 또는 뗐을 때 winit이 올렸을 입력을 미국 자판 배열로 만든다
pub(crate) fn synthesize(code: KeyCode, mods: ModifiersState, pressed: bool) -> KeyInput {
    let (base, shifted) = layout_keys(code);
    let logical = if mods.shift_key() { shifted } else { base.clone() };
    // winit(macOS)은 누를 때만 논리 키의 글을 준다
    let text = if pressed {
        logical.to_text().map(SmolStr::new)
    } else {
        None
    };
    KeyInput {
        pressed,
        repeat: false,
        logical,
        without_modifiers: base,
        location: location_of(code),
        physical: PhysicalKey::Code(code),
        text,
    }
}

/// `send key` 한 번이 인코더에 넘길 것
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    /// 보낼 키. 이 앱에 대응하는 키가 없는 이름이면 `None`(오류는 아니다)
    pub input: Option<KeyInput>,
    /// 스크립트가 준 수식 키(`key-remap`을 적용하기 전)
    pub mods: ModifiersState,
}

/// 명령의 키 이름·수식 키·누름/뗌을 해석한다. Ghostty와 같은 차례로 본다: 키 이름, 수식 키
pub(crate) fn prepare(key: &str, modifiers: Option<&str>, release: bool) -> Result<Prepared, String> {
    let code = match lookup(key) {
        Lookup::Unknown => return Err(format!("알 수 없는 키 이름: {key}")),
        Lookup::NoKey => None,
        Lookup::Code(c) => Some(c),
    };
    let mods = match modifiers {
        Some(text) => parse_modifiers(text)?,
        None => ModifiersState::empty(),
    };
    let input = code.map(|c| synthesize(c, mods, !release));
    Ok(Prepared { input, mods })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::config::OptionAsAlt;
    use bt_term::Modes;

    const NONE: ModifiersState = ModifiersState::empty();
    const SHIFT: ModifiersState = ModifiersState::SHIFT;
    const CTRL: ModifiersState = ModifiersState::CONTROL;
    const ALT: ModifiersState = ModifiersState::ALT;
    const SUPER: ModifiersState = ModifiersState::SUPER;

    /// 사람이 누른 키와 같은 길(`keys::encode_input`)로 바이트를 만든다. `key-remap`은 쓰지 않는다
    fn bytes(key: &str, mods: &str, release: bool, modes: Modes, option_as_alt: OptionAsAlt) -> Vec<u8> {
        let p = prepare(key, Some(mods), release).expect("해석");
        let Some(input) = p.input else { return Vec::new() };
        let sided = crate::keys::sided_from_state(p.mods);
        let meta = crate::keys::option_counts_as_alt(option_as_alt, sided);
        crate::keys::encode_input(&input, p.mods, p.mods, modes, meta)
    }

    fn press(key: &str, mods: &str) -> Vec<u8> {
        bytes(key, mods, false, Modes::default(), OptionAsAlt::None)
    }

    fn kitty(flags: u8) -> Modes {
        let mut m = Modes::default();
        // 플래그 1 disambiguate, 2 이벤트 종류, 8 모든 키를 이스케이프로
        m.kitty.disambiguate = flags & 1 != 0;
        m.kitty.report_events = flags & 2 != 0;
        m.kitty.all_as_escapes = flags & 8 != 0;
        m
    }

    #[test]
    fn every_ghostty_key_name_is_known() {
        // Ghostty `Ghostty.Input.Key`의 rawValue 175개(받아 둔 원본에서 뽑은 목록)
        assert_eq!(KEY_NAMES.len(), 175);
        for (name, code) in KEY_NAMES {
            let want = code.map_or(Lookup::NoKey, Lookup::Code);
            assert_eq!(lookup(name), want, "{name}");
        }
        let mut names: Vec<&str> = KEY_NAMES.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 175, "이름이 겹친다");
    }

    #[test]
    fn key_names_are_case_sensitive_ghostty_raw_values() {
        assert_eq!(lookup("enter"), Lookup::Code(KeyCode::Enter));
        assert_eq!(lookup("a"), Lookup::Code(KeyCode::KeyA));
        assert_eq!(lookup("arrowUp"), Lookup::Code(KeyCode::ArrowUp));
        assert_eq!(lookup("digit1"), Lookup::Code(KeyCode::Digit1));
        assert_eq!(lookup("bracketLeft"), Lookup::Code(KeyCode::BracketLeft));
        assert_eq!(lookup("metaLeft"), Lookup::Code(KeyCode::SuperLeft));
        assert_eq!(lookup("numpadUp"), Lookup::NoKey);
        // Ghostty는 rawValue로만 찾는다: 대소문자가 다르거나 다른 표기(arrow_up, return, ctrl)는 모르는 이름이다
        for bad in [
            "Enter", "ENTER", "A", "arrow_up", "return", "ctrl", "", " enter", "enter ", "f26", "digit10",
        ] {
            assert_eq!(lookup(bad), Lookup::Unknown, "{bad:?}");
        }
    }

    #[test]
    fn modifiers_follow_ghostty_script_modifiers() {
        assert_eq!(parse_modifiers("shift"), Ok(SHIFT));
        assert_eq!(parse_modifiers("control"), Ok(CTRL));
        assert_eq!(parse_modifiers("option"), Ok(ALT));
        assert_eq!(parse_modifiers("command"), Ok(SUPER));
        assert_eq!(parse_modifiers("shift,control"), Ok(SHIFT | CTRL));
        // 앞뒤 공백을 지우고 소문자로 본다
        assert_eq!(parse_modifiers(" Shift , CONTROL ,option"), Ok(SHIFT | CTRL | ALT));
        // Swift `split`처럼 빈 조각은 건너뛴다
        assert_eq!(parse_modifiers(""), Ok(NONE));
        assert_eq!(parse_modifiers("shift,,control"), Ok(SHIFT | CTRL));
        assert_eq!(parse_modifiers(",shift,"), Ok(SHIFT));
        // 공백만 있는 조각은 건너뛰지 않고 모르는 이름이다
        assert!(parse_modifiers("shift, ,control").is_err());
        for bad in [
            "ctrl",
            "alt",
            "super",
            "cmd",
            "shift+control",
            "shift control",
            "meta",
            "fn",
        ] {
            let e = parse_modifiers(bad).expect_err(bad);
            assert!(e.contains(bad), "{e}");
        }
    }

    #[test]
    fn prepare_checks_the_key_before_the_modifiers() {
        // Ghostty `parse`는 키 이름을 먼저 본다
        let e = prepare("nope", Some("nope"), false).unwrap_err();
        assert!(e.contains("알 수 없는 키 이름: nope"), "{e}");
        let e = prepare("enter", Some("hyper"), false).unwrap_err();
        assert!(e.contains("알 수 없는 수식 키"), "{e}");
        // 이름은 맞지만 이 앱에 키가 없다: 오류가 아니라 보낼 것이 없다
        assert!(prepare("numpadUp", None, false).unwrap().input.is_none());
    }

    #[test]
    fn plain_keys_send_the_bytes_a_person_typing_them_sends() {
        assert_eq!(press("enter", ""), b"\r");
        assert_eq!(press("tab", ""), b"\t");
        assert_eq!(press("escape", ""), b"\x1b");
        assert_eq!(press("backspace", ""), vec![0x7f]);
        assert_eq!(press("space", ""), b" ");
        assert_eq!(press("a", ""), b"a");
        assert_eq!(press("a", "shift"), b"A");
        assert_eq!(press("digit1", "shift"), b"!");
        assert_eq!(press("bracketLeft", ""), b"[");
        assert_eq!(press("bracketLeft", "shift"), b"{");
        assert_eq!(press("arrowUp", ""), b"\x1b[A");
        assert_eq!(press("arrowLeft", "control"), b"\x1b[1;5D");
        assert_eq!(press("delete", ""), b"\x1b[3~");
        assert_eq!(press("pageDown", ""), b"\x1b[6~");
        assert_eq!(press("f1", ""), b"\x1bOP");
        assert_eq!(press("f5", ""), b"\x1b[15~");
        assert_eq!(press("tab", "shift"), b"\x1b[Z");
    }

    #[test]
    fn control_letters_become_control_bytes() {
        assert_eq!(press("c", "control"), vec![0x03]);
        assert_eq!(press("d", "control"), vec![0x04]);
        assert_eq!(press("l", "control"), vec![0x0c]);
        assert_eq!(press("a", "control"), vec![0x01]);
    }

    #[test]
    fn cursor_keys_follow_decckm() {
        let modes = Modes {
            app_cursor: true,
            ..Modes::default()
        };
        let up = |m| bytes("arrowUp", "", false, m, OptionAsAlt::None);
        assert_eq!(up(modes), b"\x1bOA");
        assert_eq!(up(Modes::default()), b"\x1b[A");
    }

    #[test]
    fn option_is_alt_only_when_option_as_alt_says_so() {
        let alt_b = |setting| bytes("b", "option", false, Modes::default(), setting);
        assert_eq!(alt_b(OptionAsAlt::Both), b"\x1bb");
        assert_eq!(
            alt_b(OptionAsAlt::Left),
            b"\x1bb",
            "스크립트의 option은 왼쪽 Option이다"
        );
        // 오른쪽 Option만 Alt로 쓰는 설정이면 스크립트의 option(왼쪽)은 Alt가 아니다
        assert_eq!(alt_b(OptionAsAlt::Right), b"b");
        assert_eq!(alt_b(OptionAsAlt::None), b"b");
        // 이름 키는 option-as-alt와 상관없이 Alt다(방향 키)
        assert_eq!(
            bytes("arrowLeft", "option", false, Modes::default(), OptionAsAlt::None),
            b"\x1b[1;3D"
        );
    }

    #[test]
    fn kitty_keyboard_protocol_is_used_when_the_program_enabled_it() {
        // 플래그 1(disambiguate): Ctrl+C는 CSI 99;5u, Enter는 그대로 \r, Escape는 CSI 27u
        let m = kitty(1);
        let k = |key, mods| bytes(key, mods, false, m, OptionAsAlt::None);
        assert_eq!(k("c", "control"), b"\x1b[99;5u");
        assert_eq!(k("enter", ""), b"\r");
        assert_eq!(k("escape", ""), b"\x1b[27u");
        assert_eq!(k("a", ""), b"a");
        // 이벤트 종류 보고(플래그 2)와 모든 키 이스케이프(8): 누름은 CSI u, 뗌은 `:3`이 붙는다
        let m = kitty(1 | 2 | 8);
        assert_eq!(bytes("enter", "", false, m, OptionAsAlt::None), b"\x1b[13u");
        assert_eq!(bytes("enter", "", true, m, OptionAsAlt::None), b"\x1b[13;1:3u");
        // kitty가 꺼져 있으면 뗌은 아무것도 보내지 않는다
        assert_eq!(bytes("enter", "", true, Modes::default(), OptionAsAlt::None), b"");
    }

    #[test]
    fn keypad_keys_use_the_keypad_path() {
        assert_eq!(press("numpad5", ""), b"5");
        assert_eq!(press("numpadEnter", ""), b"\r");
        assert_eq!(press("numpadAdd", ""), b"+");
        // 키패드 응용 모드(DECKPAM에 1035를 끈 때)
        let mut modes = Modes {
            app_keypad: true,
            ..Modes::default()
        };
        modes.key.ignore_keypad_with_numlock = false;
        assert_eq!(bytes("numpadEnter", "", false, modes, OptionAsAlt::None), b"\x1bOM");
        assert_eq!(bytes("numpad5", "", false, modes, OptionAsAlt::None), b"\x1bOu");
    }

    #[test]
    fn keys_without_bytes_send_nothing() {
        for k in [
            "shiftLeft",
            "controlRight",
            "capsLock",
            "mediaPlayPause",
            "intlRo",
            "numpadUp",
        ] {
            assert_eq!(press(k, ""), b"", "{k}");
        }
    }

    #[test]
    fn synthesized_input_matches_what_winit_reports() {
        // 글자 키: 논리 키는 Shift를 반영하고, 수식 키 없는 키는 배열의 글자, 글은 논리 키의 글
        let i = synthesize(KeyCode::KeyA, SHIFT, true);
        assert_eq!(i.logical, Key::Character("A".into()));
        assert_eq!(i.without_modifiers, Key::Character("a".into()));
        assert_eq!(i.text.as_deref(), Some("A"));
        assert!(i.pressed && !i.repeat);
        // 뗄 때는 글이 없다(winit은 뗌에 글을 주지 않는다)
        assert_eq!(synthesize(KeyCode::KeyA, NONE, false).text, None);
        // Option은 글자를 바꾸지 않는다(Alt 해석은 인코더가 한다)
        assert_eq!(synthesize(KeyCode::KeyB, ALT, true).logical, Key::Character("b".into()));
        // 이름 키와 그 글
        let e = synthesize(KeyCode::Enter, NONE, true);
        assert_eq!(e.logical, Key::Named(NamedKey::Enter));
        assert_eq!(e.text.as_deref(), Some("\r"));
        assert_eq!(synthesize(KeyCode::Space, NONE, true).text.as_deref(), Some(" "));
        // 키패드는 위치가 키패드다
        let n = synthesize(KeyCode::Numpad5, NONE, true);
        assert_eq!(n.location, KeyLocation::Numpad);
        assert_eq!(n.logical, Key::Character("5".into()));
        assert_eq!(
            synthesize(KeyCode::NumpadEnter, NONE, true).location,
            KeyLocation::Numpad
        );
        // 왼쪽·오른쪽 수식 키
        assert_eq!(synthesize(KeyCode::ShiftRight, NONE, true).location, KeyLocation::Right);
        assert_eq!(synthesize(KeyCode::AltLeft, NONE, true).location, KeyLocation::Left);
        // 물리 키는 그대로
        assert_eq!(
            synthesize(KeyCode::KeyA, NONE, true).physical,
            PhysicalKey::Code(KeyCode::KeyA)
        );
    }

    #[test]
    fn the_keybind_trigger_uses_the_key_without_modifier_letters() {
        use crate::keymap::Mods;
        let trigger = |key, mods| {
            let p = prepare(key, mods, false).unwrap();
            crate::keys::trigger_of(&p.input.unwrap().without_modifiers, p.mods).unwrap()
        };
        let t = trigger("bracketRight", Some("shift,command"));
        assert_eq!(t.key, "bracket_right");
        assert_eq!(
            t.mods,
            Mods {
                shift: true,
                ctrl: false,
                alt: false,
                sup: true
            }
        );
        assert_eq!(trigger("arrowUp", None).key, "arrow_up");
        assert_eq!(trigger("c", Some("command")).key, "c");
    }

    #[test]
    fn release_is_a_release_input() {
        let p = prepare("a", None, true).unwrap();
        assert!(!p.input.unwrap().pressed);
        let p = prepare("a", None, false).unwrap();
        assert!(p.input.unwrap().pressed);
    }
}
