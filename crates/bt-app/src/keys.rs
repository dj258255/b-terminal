//! winit 키 이벤트를 PTY 바이트로 바꾼다. 앱 단축키(Cmd-…)는 여기서 거른다

use bt_term::{
    KeyAction, KeyModes, KeypadKey, KittyKey, KittyKeyEvent, ModifierKey, Mods as TermMods, NamedKey, TextKey,
    encode_keypad, encode_kitty_key, encode_modify_other_char, encode_named_modify_other, encode_named_with,
    encode_text_key, keypad_application,
};

use crate::keymap::{Mods, Trigger, normalize_key};
use bt_core::config::{KeyRemap, ModKind, ModSide, OptionAsAlt, SidedMods, apply_key_remap};
use winit::event::{ElementState, KeyEvent, Modifiers};
use winit::keyboard::{Key, KeyCode, KeyLocation, ModifiersKeyState, ModifiersState, NamedKey as WKey, PhysicalKey};

/// winit의 수정 키 상태를 좌우 따로 적은 것으로. 좌우를 알려 주지 않으면(`Unknown`) 왼쪽으로 본다.
/// macOS의 winit은 장치별 비트(NX_DEVICE*KEYMASK)로 좌우를 알려 준다(좌우가 둘 다 `Unknown`이면 안 눌린 것이거나 모르는 것)
pub fn sided_mods(m: &Modifiers) -> SidedMods {
    let s = m.state();
    let pressed =
        |l: ModifiersKeyState, r: ModifiersKeyState| (l == ModifiersKeyState::Pressed, r == ModifiersKeyState::Pressed);
    sided_from([
        (s.shift_key(), pressed(m.lshift_state(), m.rshift_state())),
        (s.control_key(), pressed(m.lcontrol_state(), m.rcontrol_state())),
        (s.alt_key(), pressed(m.lalt_state(), m.ralt_state())),
        (s.super_key(), pressed(m.lsuper_state(), m.rsuper_state())),
    ])
}

/// 종류마다 (눌렸는가, (왼쪽이 눌렸다고 알려졌는가, 오른쪽이)). Shift·Ctrl·Alt·Super 차례
fn sided_from(kinds: [(bool, (bool, bool)); 4]) -> SidedMods {
    let mut out = SidedMods::default();
    for (kind, (down, (l, r))) in [ModKind::Shift, ModKind::Ctrl, ModKind::Alt, ModKind::Super]
        .into_iter()
        .zip(kinds)
    {
        if l {
            out = out.with(kind, ModSide::Left);
        }
        if r {
            out = out.with(kind, ModSide::Right);
        }
        // 눌렸는데 어느 쪽인지 모르면 왼쪽으로
        if down && !l && !r {
            out = out.with(kind, ModSide::Left);
        }
    }
    out
}

/// 수정 키 상태를 왼쪽 키를 누른 것으로 적는다(스크립트의 `send key`는 좌우를 말하지 않는다)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn sided_from_state(s: ModifiersState) -> SidedMods {
    let pressed = |on: bool| (on, (false, false));
    sided_from([
        pressed(s.shift_key()),
        pressed(s.control_key()),
        pressed(s.alt_key()),
        pressed(s.super_key()),
    ])
}

/// 좌우를 합쳐 winit의 수정 키 상태로
pub fn to_state(m: SidedMods) -> ModifiersState {
    let mut s = ModifiersState::empty();
    s.set(ModifiersState::SHIFT, m.has(ModKind::Shift));
    s.set(ModifiersState::CONTROL, m.has(ModKind::Ctrl));
    s.set(ModifiersState::ALT, m.has(ModKind::Alt));
    s.set(ModifiersState::SUPER, m.has(ModKind::Super));
    s
}

/// 누른 수정 키에 `key-remap`을 적용한 상태. 키 묶음 판단과 PTY 인코딩이 이 상태를 쓴다
pub fn remapped_state(remaps: &[KeyRemap], pressed: SidedMods) -> ModifiersState {
    to_state(apply_key_remap(remaps, pressed))
}

/// 키 이벤트를 키 묶음 트리거로(Ghostty 키 이름). 수식 키를 뺀 글자를 쓴다(Shift+]도 bracket_right)
pub fn trigger(event: &KeyEvent, mods: ModifiersState) -> Option<Trigger> {
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    let key = layout_trigger_key(&event.key_without_modifiers(), mods, || {
        command_layer(event.physical_key)
    });
    trigger_of(&key, mods)
}

/// 지금 자판의 ⌘ 층 글자(#709). macOS만 읽을 수 있다
#[cfg(target_os = "macos")]
fn command_layer(key: PhysicalKey) -> Option<String> {
    winit::platform::macos::command_layer_char(key)
}

#[cfg(not(target_os = "macos"))]
fn command_layer(_key: PhysicalKey) -> Option<String> {
    None
}

/// 한글처럼 글자 키가 라틴 글자가 아닌 자판에서 ⌃·⌘ 단축키를 맞추려고, 트리거에 쓸 키를 고른다(#709).
/// `command_layer`는 지금 자판의 ⌘ 층 글자를 돌려준다(macOS 메뉴 단축키가 쓰는 글자, 한글 2벌식 T 자리면 `t`)
pub(crate) fn layout_trigger_key(
    without_modifiers: &Key,
    mods: ModifiersState,
    command_layer: impl FnOnce() -> Option<String>,
) -> Key {
    let Key::Character(s) = without_modifiers else {
        return without_modifiers.clone();
    };
    // ⌃·⌘ 없는 키는 입력기가 받을 글자이고, 라틴 글자 자판이면 이미 맞는다
    if !(mods.control_key() || mods.super_key()) || s.is_ascii() {
        return without_modifiers.clone();
    }
    match command_layer().and_then(|c| latin_of(&c)) {
        Some(c) => Key::Character(c.to_string().into()),
        None => without_modifiers.clone(),
    }
}

/// ⌘ 층 글자를 쓸 수 있으면 소문자 한 글자로. ASCII 그림 글자 한 개가 아니면(없음, 제어 문자, 비ASCII, 두 글자) None
fn latin_of(command_layer: &str) -> Option<char> {
    let mut chars = command_layer.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_graphic() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// 확인 막대에서 친 글자 키의 답 글자(#721). 막대가 떠 있는 동안은 입력기를 꺼 글자 키도 `KeyboardInput`으로 온다
pub fn answer_char(event: &KeyEvent) -> Option<char> {
    answer_char_of(&event.logical_key, || command_layer(event.physical_key))
}

/// `answer_char`의 판단. 자판 글자가 ASCII면 그대로 쓰고, 아니면(한글 2벌식 Y 자리는 `ㅛ`) 키 묶음과 같은 ⌘ 층 글자로 읽는다(ADR-070)
pub(crate) fn answer_char_of(logical: &Key, command_layer: impl FnOnce() -> Option<String>) -> Option<char> {
    let Key::Character(s) = logical else { return None };
    let c = s.chars().next()?;
    if c.is_ascii() {
        return Some(c);
    }
    Some(command_layer().and_then(|l| latin_of(&l)).unwrap_or(c))
}

/// 수식 키를 뺀 키로 키 묶음 트리거를 만든다(`trigger`와 스크립트의 `send key`가 같이 쓴다)
pub(crate) fn trigger_of(without_modifiers: &Key, mods: ModifiersState) -> Option<Trigger> {
    let key = match without_modifiers {
        Key::Character(s) => normalize_key(s),
        Key::Named(n) => named_key_name(*n)?.to_string(),
        _ => return None,
    };
    Some(Trigger {
        mods: trigger_mods(mods),
        key,
    })
}

/// 숫자 패드 키의 Ghostty 물리 키 이름(`numpad_1`, `numpad_enter`…)으로 만든 트리거. 숫자 패드가 아니면 없다.
/// 숫자 패드 `1`은 글자 `1`로도 읽혀 `trigger`가 `digit_1`을 주므로, 이 트리거는 그것과 따로 `kp_1` 묶음을 맞추는 데 쓴다.
/// macOS·X11·Windows의 winit이 주는 숫자 패드 코드만 만든다(숫자 패드 방향키 `numpad_up` 등은 winit에 코드가 없어 만들 수 없다)
pub(crate) fn numpad_trigger(physical: PhysicalKey, mods: Mods) -> Option<Trigger> {
    let PhysicalKey::Code(code) = physical else { return None };
    let key = match code {
        KeyCode::Numpad0 => "numpad_0",
        KeyCode::Numpad1 => "numpad_1",
        KeyCode::Numpad2 => "numpad_2",
        KeyCode::Numpad3 => "numpad_3",
        KeyCode::Numpad4 => "numpad_4",
        KeyCode::Numpad5 => "numpad_5",
        KeyCode::Numpad6 => "numpad_6",
        KeyCode::Numpad7 => "numpad_7",
        KeyCode::Numpad8 => "numpad_8",
        KeyCode::Numpad9 => "numpad_9",
        KeyCode::NumpadAdd => "numpad_add",
        KeyCode::NumpadBackspace => "numpad_backspace",
        KeyCode::NumpadClear => "numpad_clear",
        KeyCode::NumpadClearEntry => "numpad_clear_entry",
        KeyCode::NumpadComma => "numpad_comma",
        KeyCode::NumpadDecimal => "numpad_decimal",
        KeyCode::NumpadDivide => "numpad_divide",
        KeyCode::NumpadEnter => "numpad_enter",
        KeyCode::NumpadEqual => "numpad_equal",
        KeyCode::NumpadMemoryAdd => "numpad_memory_add",
        KeyCode::NumpadMemoryClear => "numpad_memory_clear",
        KeyCode::NumpadMemoryRecall => "numpad_memory_recall",
        KeyCode::NumpadMemoryStore => "numpad_memory_store",
        KeyCode::NumpadMemorySubtract => "numpad_memory_subtract",
        KeyCode::NumpadMultiply => "numpad_multiply",
        KeyCode::NumpadParenLeft => "numpad_paren_left",
        KeyCode::NumpadParenRight => "numpad_paren_right",
        KeyCode::NumpadSubtract => "numpad_subtract",
        _ => return None,
    };
    Some(Trigger {
        mods,
        key: key.to_string(),
    })
}

/// 키 묶음 트리거의 수정 키
fn trigger_mods(mods: ModifiersState) -> Mods {
    Mods {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key(),
        sup: mods.super_key(),
    }
}

/// PTY로 보낼 글자의 수정 키. Option은 `option-as-alt`일 때만 Alt다
fn term_mods(mods: ModifiersState, option_as_alt: bool) -> TermMods {
    TermMods {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key() && option_as_alt,
        sup: false,
    }
}

fn named_key_name(n: WKey) -> Option<&'static str> {
    Some(match n {
        WKey::Enter => "enter",
        WKey::Tab => "tab",
        WKey::Escape => "escape",
        WKey::Backspace => "backspace",
        WKey::Delete => "delete",
        WKey::Insert => "insert",
        WKey::Space => "space",
        WKey::ArrowUp => "arrow_up",
        WKey::ArrowDown => "arrow_down",
        WKey::ArrowLeft => "arrow_left",
        WKey::ArrowRight => "arrow_right",
        WKey::Home => "home",
        WKey::End => "end",
        WKey::PageUp => "page_up",
        WKey::PageDown => "page_down",
        WKey::F1 => "f1",
        WKey::F2 => "f2",
        WKey::F3 => "f3",
        WKey::F4 => "f4",
        WKey::F5 => "f5",
        WKey::F6 => "f6",
        WKey::F7 => "f7",
        WKey::F8 => "f8",
        WKey::F9 => "f9",
        WKey::F10 => "f10",
        WKey::F11 => "f11",
        WKey::F12 => "f12",
        WKey::F13 => "f13",
        WKey::F14 => "f14",
        WKey::F15 => "f15",
        WKey::F16 => "f16",
        WKey::F17 => "f17",
        WKey::F18 => "f18",
        WKey::F19 => "f19",
        WKey::F20 => "f20",
        WKey::F21 => "f21",
        WKey::F22 => "f22",
        WKey::F23 => "f23",
        WKey::F24 => "f24",
        WKey::F25 => "f25",
        WKey::CapsLock => "caps_lock",
        WKey::ContextMenu => "context_menu",
        WKey::NumLock => "num_lock",
        WKey::ScrollLock => "scroll_lock",
        WKey::PrintScreen => "print_screen",
        WKey::Pause => "pause",
        WKey::Help => "help",
        WKey::Fn => "fn",
        WKey::FnLock => "fn_lock",
        WKey::Convert => "convert",
        WKey::KanaMode => "kana_mode",
        WKey::NonConvert => "non_convert",
        WKey::Copy => "copy",
        WKey::Cut => "cut",
        WKey::Paste => "paste",
        WKey::BrowserBack => "browser_back",
        WKey::BrowserFavorites => "browser_favorites",
        WKey::BrowserForward => "browser_forward",
        WKey::BrowserHome => "browser_home",
        WKey::BrowserRefresh => "browser_refresh",
        WKey::BrowserSearch => "browser_search",
        WKey::BrowserStop => "browser_stop",
        WKey::Eject => "eject",
        WKey::LaunchApplication1 => "launch_app_1",
        WKey::LaunchApplication2 => "launch_app_2",
        WKey::LaunchMail => "launch_mail",
        WKey::MediaPlayPause => "media_play_pause",
        WKey::MediaStop => "media_stop",
        WKey::MediaTrackNext => "media_track_next",
        WKey::MediaTrackPrevious => "media_track_previous",
        WKey::Power => "power",
        WKey::Standby => "sleep",
        WKey::AudioVolumeDown => "audio_volume_down",
        WKey::AudioVolumeMute => "audio_volume_mute",
        WKey::AudioVolumeUp => "audio_volume_up",
        WKey::WakeUp => "wake_up",
        _ => return None,
    })
}

/// 지금 누른 Option이 Alt로 쓰이는가(`macos-option-as-alt`). `left`·`right`는 그쪽 Option일 때만 그렇다
/// (Ghostty `key_encode.zig` 299~307줄 kitty 연결 글자, 403~407줄 modifyOtherKeys 2, 566~569줄 글자 앞 ESC).
/// 어느 쪽인지는 Ghostty macOS 앱처럼 센다: 오른쪽 Option이 눌렸으면 오른쪽이고, 아니면 왼쪽이다(양쪽이면 오른쪽, `Ghostty.Input.swift` 90줄).
/// 우리 `sided_from`은 눌렸는데 좌우를 모르면 왼쪽으로 본다
pub fn option_counts_as_alt(setting: OptionAsAlt, raw: SidedMods) -> bool {
    let right = raw.has_side(ModKind::Alt, ModSide::Right);
    match setting {
        OptionAsAlt::None => false,
        OptionAsAlt::Both => true,
        OptionAsAlt::Left => !right,
        OptionAsAlt::Right => right,
    }
}

/// 바뀐 상태의 Alt를 PTY에 Alt로 보낼지. `option-as-alt`는 진짜 Option 키에만 걸린다(`raw`: 바꾸기 전 상태).
/// Option을 누르지 않았는데 Alt가 있으면 `key-remap`이 만든 Alt라 늘 Alt다
fn option_gate(option_as_alt: bool, raw: ModifiersState) -> bool {
    option_as_alt || !raw.alt_key()
}

/// Ctrl을 눌렀는데 바뀐 상태에는 Ctrl이 없는가. 이때 OS가 만든 글자(Ctrl-C의 `\x03`)는 보내지 않고 글자 키를 쓴다
fn ctrl_text_replaced(raw: ModifiersState, mods: ModifiersState) -> bool {
    raw.control_key() && !mods.control_key()
}

/// 인코딩에 쓸 키 입력 모드. kitty 키보드 프로토콜이 켜져 있으면 DECBKM(67)과 1036을 무시한다
/// (Ghostty `key_encode.zig`의 `kitty()`는 두 옵션을 읽지 않는다. Backspace는 늘 0x7f)
fn effective_key_modes(modes: &bt_term::Modes) -> KeyModes {
    if modes.kitty.active() {
        KeyModes {
            backarrow_key: false,
            alt_esc_prefix: true,
            ..modes.key
        }
    } else {
        modes.key
    }
}

/// 숫자 키패드 위치에서 눌린 키(#312). winit이 `KeyLocation::Numpad`로 알려 준다.
/// NumLock이 꺼져 화살표·Home 같은 이름 키로 오는 것은 커서 키와 같은 바이트라 여기서 다루지 않는다
fn keypad_key(key: &Key, location: KeyLocation) -> Option<KeypadKey> {
    if location != KeyLocation::Numpad {
        return None;
    }
    match key {
        Key::Named(WKey::Enter) => Some(KeypadKey::Enter),
        Key::Character(s) => {
            let mut chars = s.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            match c {
                '0'..='9' => Some(KeypadKey::Digit(c as u8 - b'0')),
                '.' | ',' => Some(KeypadKey::Decimal),
                '/' => Some(KeypadKey::Divide),
                '*' => Some(KeypadKey::Multiply),
                '-' => Some(KeypadKey::Subtract),
                '+' => Some(KeypadKey::Add),
                _ => None,
            }
        }
        _ => None,
    }
}

/// 물리 키를 kitty 프로토콜의 키로(Ghostty `key.Key`와 `kitty.zig` 표의 키). 표에 없는 키는 `Text`
fn kitty_key_of(code: KeyCode) -> KittyKey {
    use KeyCode as C;
    use NamedKey as N;
    match code {
        C::Escape => KittyKey::Named(N::Escape),
        C::Enter => KittyKey::Named(N::Enter),
        C::Tab => KittyKey::Named(N::Tab),
        C::Backspace => KittyKey::Named(N::Backspace),
        C::Insert => KittyKey::Named(N::Insert),
        C::Delete => KittyKey::Named(N::Delete),
        C::ArrowLeft => KittyKey::Named(N::Left),
        C::ArrowRight => KittyKey::Named(N::Right),
        C::ArrowUp => KittyKey::Named(N::Up),
        C::ArrowDown => KittyKey::Named(N::Down),
        C::PageUp => KittyKey::Named(N::PageUp),
        C::PageDown => KittyKey::Named(N::PageDown),
        C::Home => KittyKey::Named(N::Home),
        C::End => KittyKey::Named(N::End),
        C::F1 => KittyKey::Named(N::F(1)),
        C::F2 => KittyKey::Named(N::F(2)),
        C::F3 => KittyKey::Named(N::F(3)),
        C::F4 => KittyKey::Named(N::F(4)),
        C::F5 => KittyKey::Named(N::F(5)),
        C::F6 => KittyKey::Named(N::F(6)),
        C::F7 => KittyKey::Named(N::F(7)),
        C::F8 => KittyKey::Named(N::F(8)),
        C::F9 => KittyKey::Named(N::F(9)),
        C::F10 => KittyKey::Named(N::F(10)),
        C::F11 => KittyKey::Named(N::F(11)),
        C::F12 => KittyKey::Named(N::F(12)),
        C::F13 => KittyKey::Named(N::F(13)),
        C::F14 => KittyKey::Named(N::F(14)),
        C::F15 => KittyKey::Named(N::F(15)),
        C::F16 => KittyKey::Named(N::F(16)),
        C::F17 => KittyKey::Named(N::F(17)),
        C::F18 => KittyKey::Named(N::F(18)),
        C::F19 => KittyKey::Named(N::F(19)),
        C::F20 => KittyKey::Named(N::F(20)),
        C::F21 => KittyKey::Named(N::F(21)),
        C::F22 => KittyKey::Named(N::F(22)),
        C::F23 => KittyKey::Named(N::F(23)),
        C::F24 => KittyKey::Named(N::F(24)),
        C::F25 => KittyKey::Named(N::F(25)),
        C::CapsLock => KittyKey::CapsLock,
        C::ScrollLock => KittyKey::ScrollLock,
        C::NumLock => KittyKey::NumLock,
        C::PrintScreen => KittyKey::PrintScreen,
        C::Pause => KittyKey::Pause,
        C::Numpad0 => KittyKey::Keypad(KeypadKey::Digit(0)),
        C::Numpad1 => KittyKey::Keypad(KeypadKey::Digit(1)),
        C::Numpad2 => KittyKey::Keypad(KeypadKey::Digit(2)),
        C::Numpad3 => KittyKey::Keypad(KeypadKey::Digit(3)),
        C::Numpad4 => KittyKey::Keypad(KeypadKey::Digit(4)),
        C::Numpad5 => KittyKey::Keypad(KeypadKey::Digit(5)),
        C::Numpad6 => KittyKey::Keypad(KeypadKey::Digit(6)),
        C::Numpad7 => KittyKey::Keypad(KeypadKey::Digit(7)),
        C::Numpad8 => KittyKey::Keypad(KeypadKey::Digit(8)),
        C::Numpad9 => KittyKey::Keypad(KeypadKey::Digit(9)),
        C::NumpadDecimal => KittyKey::Keypad(KeypadKey::Decimal),
        C::NumpadDivide => KittyKey::Keypad(KeypadKey::Divide),
        C::NumpadMultiply => KittyKey::Keypad(KeypadKey::Multiply),
        C::NumpadSubtract => KittyKey::Keypad(KeypadKey::Subtract),
        C::NumpadAdd => KittyKey::Keypad(KeypadKey::Add),
        C::NumpadEnter => KittyKey::Keypad(KeypadKey::Enter),
        C::NumpadEqual => KittyKey::NumpadEqual,
        C::NumpadComma => KittyKey::NumpadSeparator,
        C::ShiftLeft => KittyKey::Modifier(ModifierKey::Shift { right: false }),
        C::ShiftRight => KittyKey::Modifier(ModifierKey::Shift { right: true }),
        C::ControlLeft => KittyKey::Modifier(ModifierKey::Ctrl { right: false }),
        C::ControlRight => KittyKey::Modifier(ModifierKey::Ctrl { right: true }),
        C::AltLeft => KittyKey::Modifier(ModifierKey::Alt { right: false }),
        C::AltRight => KittyKey::Modifier(ModifierKey::Alt { right: true }),
        C::SuperLeft => KittyKey::Modifier(ModifierKey::Super { right: false }),
        C::SuperRight => KittyKey::Modifier(ModifierKey::Super { right: true }),
        _ => KittyKey::Text,
    }
}

/// 물리 키의 US 배열 글자(kitty의 기본 배열 키). Ghostty `key.zig`의 `codepoint_map`과 같은 표다.
/// 글자가 없는 키(Enter, 방향 키 …)는 없다
pub(crate) fn base_layout_codepoint(code: KeyCode) -> Option<u32> {
    use KeyCode as C;
    let c = match code {
        C::KeyA => 'a',
        C::KeyB => 'b',
        C::KeyC => 'c',
        C::KeyD => 'd',
        C::KeyE => 'e',
        C::KeyF => 'f',
        C::KeyG => 'g',
        C::KeyH => 'h',
        C::KeyI => 'i',
        C::KeyJ => 'j',
        C::KeyK => 'k',
        C::KeyL => 'l',
        C::KeyM => 'm',
        C::KeyN => 'n',
        C::KeyO => 'o',
        C::KeyP => 'p',
        C::KeyQ => 'q',
        C::KeyR => 'r',
        C::KeyS => 's',
        C::KeyT => 't',
        C::KeyU => 'u',
        C::KeyV => 'v',
        C::KeyW => 'w',
        C::KeyX => 'x',
        C::KeyY => 'y',
        C::KeyZ => 'z',
        C::Digit0 | C::Numpad0 => '0',
        C::Digit1 | C::Numpad1 => '1',
        C::Digit2 | C::Numpad2 => '2',
        C::Digit3 | C::Numpad3 => '3',
        C::Digit4 | C::Numpad4 => '4',
        C::Digit5 | C::Numpad5 => '5',
        C::Digit6 | C::Numpad6 => '6',
        C::Digit7 | C::Numpad7 => '7',
        C::Digit8 | C::Numpad8 => '8',
        C::Digit9 | C::Numpad9 => '9',
        C::Semicolon => ';',
        C::Space => ' ',
        C::Quote => '\'',
        C::Comma => ',',
        C::Backquote => '`',
        C::Period | C::NumpadDecimal => '.',
        C::Slash | C::NumpadDivide => '/',
        C::Minus | C::NumpadSubtract => '-',
        C::Equal | C::NumpadEqual => '=',
        C::BracketLeft => '[',
        C::BracketRight => ']',
        C::Backslash => '\\',
        C::Tab => '\t',
        C::NumpadMultiply => '*',
        C::NumpadAdd => '+',
        _ => return None,
    };
    Some(c as u32)
}

/// 수식 키 단독 이벤트가 바꾸는 수식 키 하나. 누르면 켜고 떼면 끈다
fn modifier_key_set(key: KittyKey, mods: &mut TermMods, down: bool) {
    if let KittyKey::Modifier(m) = key {
        match m {
            ModifierKey::Shift { .. } => mods.shift = down,
            ModifierKey::Ctrl { .. } => mods.ctrl = down,
            ModifierKey::Alt { .. } => mods.alt = down,
            ModifierKey::Super { .. } => mods.sup = down,
        }
    }
}

/// 키 하나가 인코더에 주는 값. winit 키 이벤트에서 읽거나(`KeyInput::from_event`) 스크립트의 `send key`가 만든다.
/// 사람이 누른 키와 스크립트가 보낸 키가 같은 인코더(`encode_input`)를 지나게 하려고 winit `KeyEvent`(밖에서 만들 수 없다)와 떼어 놓았다
#[derive(Debug, Clone)]
pub(crate) struct KeyInput {
    /// 누름이면 참, 뗌이면 거짓
    pub pressed: bool,
    pub repeat: bool,
    /// 글자를 반영한 논리 키
    pub logical: Key,
    /// 수식 키 없이 눌렀을 때의 논리 키(배열 기준)
    pub without_modifiers: Key,
    pub location: KeyLocation,
    pub physical: PhysicalKey,
    /// 키가 만든 글(누를 때만)
    pub text: Option<winit::keyboard::SmolStr>,
}

impl KeyInput {
    pub(crate) fn from_event(event: &KeyEvent) -> Self {
        use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
        Self {
            pressed: event.state == ElementState::Pressed,
            repeat: event.repeat,
            logical: event.logical_key.clone(),
            without_modifiers: event.key_without_modifiers(),
            location: event.location,
            physical: event.physical_key,
            text: event.text.clone(),
        }
    }

    fn code(&self) -> Option<KeyCode> {
        match self.physical {
            PhysicalKey::Code(c) => Some(c),
            PhysicalKey::Unidentified(_) => None,
        }
    }
}

/// kitty 키보드 프로토콜이 켜져 있을 때 키 이벤트를 바이트로(Ghostty `key_encode.zig`의 `kitty()`).
/// 누름·반복·뗌을 모두 여기로 보낸다. 뗌과 반복은 인코더가 플래그 2가 있을 때만 쓴다.
/// IME가 쓴 키는 winit이 이 이벤트로 올려 주지 않으므로(조합 중 누름·뗌은 버리고 확정만 `Ime::Commit`으로 온다) 조합 중 글자는 여기 오지 않는다
fn encode_kitty(
    input: &KeyInput,
    mods: ModifiersState,
    raw: ModifiersState,
    kitty: bt_term::KittyFlags,
    option_as_alt: bool,
) -> Vec<u8> {
    let action = match (input.pressed, input.repeat) {
        (false, _) => KeyAction::Release,
        (true, true) => KeyAction::Repeat,
        (true, false) => KeyAction::Press,
    };
    let code = input.code();
    let key = code.map(kitty_key_of).unwrap_or(KittyKey::Text);
    // Alt는 Option이어도 수식 키로 알리고, 글을 막는지는 option-as-alt가 정한다(Ghostty `macos_option_as_alt`)
    let alt_prevents_text = option_gate(option_as_alt, raw);
    let mut m = TermMods {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key(),
        sup: mods.super_key(),
    };
    modifier_key_set(key, &mut m, action != KeyAction::Release);
    // 누를 때의 글은 winit이 주고, 뗄 때는 주지 않아 같은 키의 논리 글자로 같은 대체 키를 만든다
    let utf8 = match action {
        KeyAction::Release => input.logical.to_text().unwrap_or(""),
        _ => input.text.as_deref().unwrap_or(""),
    };
    // 제어 문자로 시작하는 글(winit이 Tab·Enter·Backspace·Esc에 넣는 `\t`·`\r`·`\u{8}`·`\u{1b}`)은 글로 넘기지 않는다.
    // 넘기면 인코더가 Shift를 글에 쓰인 것으로 보고 빼서 Shift+Tab이 `\t`가 된다(#735, Ghostty `String.keyEventText`)
    let utf8 = if utf8.starts_with(|c: char| c < ' ' || c == '\u{7f}') {
        ""
    } else {
        utf8
    };
    // 글을 만드는 데 Shift가 쓰인다. Option도 Alt로 쓰지 않으면 글자를 바꾸는 데 쓰인다
    let consumed = TermMods {
        shift: true,
        alt: !alt_prevents_text,
        ..TermMods::default()
    };
    // 수식 키 없는 글자(배열 기준). 이름 키(Space)는 글자로 센다
    let unshifted = match &input.without_modifiers {
        Key::Character(s) => s.chars().next().map_or(0, |c| c as u32),
        Key::Named(WKey::Space) => ' ' as u32,
        _ => 0,
    };
    let ev = KittyKeyEvent {
        action,
        key,
        mods: m,
        // winit은 Caps Lock·Num Lock 상태를 알려 주지 않는다
        caps_lock: false,
        num_lock: false,
        consumed,
        composing: false,
        utf8,
        unshifted,
        base: code.and_then(base_layout_codepoint),
        alt_prevents_text,
    };
    encode_kitty_key(&ev, kitty)
}

/// 키 이벤트를 PTY 바이트로. 보낼 것이 없으면 빈 벡터. `mods`는 `key-remap`을 적용한 상태, `raw`는 바꾸기 전 상태
pub fn encode(
    event: &KeyEvent,
    mods: ModifiersState,
    raw: ModifiersState,
    modes: bt_term::Modes,
    option_as_alt: bool,
) -> Vec<u8> {
    encode_input(&KeyInput::from_event(event), mods, raw, modes, option_as_alt)
}

/// `encode`의 본체. 사람이 누른 키(winit 이벤트)와 스크립트가 보낸 키가 모두 여기를 지난다
pub(crate) fn encode_input(
    input: &KeyInput,
    mods: ModifiersState,
    raw: ModifiersState,
    modes: bt_term::Modes,
    option_as_alt: bool,
) -> Vec<u8> {
    let kitty = modes.kitty;
    // 프로그램이 kitty 키보드 프로토콜을 켰으면 모든 키(누름·반복·뗌)를 그 길로(#41, #546)
    if kitty.active() {
        return encode_kitty(input, mods, raw, kitty, option_as_alt);
    }
    // 켜지 않았으면 뗌은 보내지 않고 누름·반복은 옛 길 그대로다
    if !input.pressed {
        return Vec::new();
    }
    let unshifted = match &input.without_modifiers {
        Key::Character(s) => s.chars().next().map_or(0, |c| c as u32),
        Key::Named(WKey::Space) => ' ' as u32,
        _ => 0,
    };
    let key = LegacyKey {
        logical: &input.logical,
        location: input.location,
        physical: input.code(),
        text: input.text.as_deref(),
        unshifted,
        mods,
        raw,
    };
    encode_legacy(&key, modes, option_as_alt)
}

/// 옛 길(kitty 키보드 프로토콜이 꺼진 프로그램)이 키 이벤트에서 읽는 것. winit 이벤트는 만들 수 없어 시험하지 못하므로 읽는 값만 따로 둔다
/// (Ghostty `key.KeyEvent`의 key·utf8·unshifted_codepoint·mods에 해당한다)
pub(crate) struct LegacyKey<'a> {
    /// 글자를 반영한 논리 키(이름 키 또는 글자)
    pub logical: &'a Key,
    pub location: KeyLocation,
    /// 물리 키(자판 배열과 상관없음). 모르면 없다
    pub physical: Option<KeyCode>,
    /// 키가 만든 글(`KeyEvent::text`)
    pub text: Option<&'a str>,
    /// 수식 키 없이 눌렀을 때의 글자 코드(배열 기준). 모르면 0
    pub unshifted: u32,
    /// `key-remap`을 적용한 수식 키
    pub mods: ModifiersState,
    /// 바꾸기 전 수식 키
    pub raw: ModifiersState,
}

/// 옛 길의 바이트(Ghostty `key_encode.zig`의 `legacy()`). 누름·반복만 온다
pub(crate) fn encode_legacy(k: &LegacyKey, modes: bt_term::Modes, option_as_alt: bool) -> Vec<u8> {
    let (mods, raw) = (k.mods, k.raw);
    let app_cursor = modes.app_cursor;
    let km = effective_key_modes(&modes);
    let m = term_mods(mods, option_gate(option_as_alt, raw));
    // 키패드 응용 모드와 modifyOtherKeys는 Cmd(Super)도 수식 키로 센다(Ghostty `mods.binding()`). kitty가 켜져 있으면 위에서 이미 갈렸다
    let ms = TermMods {
        sup: mods.super_key(),
        ..m
    };
    // 이름 키(방향·Enter·F 키·키패드)는 Option을 눌렀으면 option-as-alt와 상관없이 Alt다. OS가 글자를 만들지 않는 키라
    // 글자를 지킬 까닭이 없다(Ghostty `pcStyleFunctionKey`는 `all_mods`를 본다)
    let mn = TermMods {
        alt: mods.alt_key(),
        ..m
    };
    let mns = TermMods { sup: ms.sup, ..mn };
    let keypad_app = keypad_application(modes.app_keypad, modes.key);
    let modify_other = modes.modify_other_keys;
    // 키패드 키는 응용 모드가 아니어도 이 길이다: 숫자 모드는 수식 키와 상관없이 그 글자 그대로(Ghostty `kpKeys`의 normal 칸, 수식 키 any)라
    // Ctrl+키패드 2가 NUL이 되거나 Shift+키패드 Enter가 CSI 27이 되지 않는다
    if let Some(kp) = keypad_key(k.logical, k.location) {
        return encode_keypad(kp, mns, keypad_app);
    }
    if let Key::Named(named) = k.logical {
        let key = match named {
            WKey::Enter => Some(NamedKey::Enter),
            WKey::Backspace => Some(NamedKey::Backspace),
            WKey::Tab => Some(NamedKey::Tab),
            WKey::Escape => Some(NamedKey::Escape),
            WKey::ArrowUp => Some(NamedKey::Up),
            WKey::ArrowDown => Some(NamedKey::Down),
            WKey::ArrowLeft => Some(NamedKey::Left),
            WKey::ArrowRight => Some(NamedKey::Right),
            WKey::Home => Some(NamedKey::Home),
            WKey::End => Some(NamedKey::End),
            WKey::PageUp => Some(NamedKey::PageUp),
            WKey::PageDown => Some(NamedKey::PageDown),
            WKey::Insert => Some(NamedKey::Insert),
            WKey::Delete => Some(NamedKey::Delete),
            WKey::F1 => Some(NamedKey::F(1)),
            WKey::F2 => Some(NamedKey::F(2)),
            WKey::F3 => Some(NamedKey::F(3)),
            WKey::F4 => Some(NamedKey::F(4)),
            WKey::F5 => Some(NamedKey::F(5)),
            WKey::F6 => Some(NamedKey::F(6)),
            WKey::F7 => Some(NamedKey::F(7)),
            WKey::F8 => Some(NamedKey::F(8)),
            WKey::F9 => Some(NamedKey::F(9)),
            WKey::F10 => Some(NamedKey::F(10)),
            WKey::F11 => Some(NamedKey::F(11)),
            WKey::F12 => Some(NamedKey::F(12)),
            WKey::F13 => Some(NamedKey::F(13)),
            WKey::F14 => Some(NamedKey::F(14)),
            WKey::F15 => Some(NamedKey::F(15)),
            WKey::F16 => Some(NamedKey::F(16)),
            WKey::F17 => Some(NamedKey::F(17)),
            WKey::F18 => Some(NamedKey::F(18)),
            WKey::F19 => Some(NamedKey::F(19)),
            WKey::F20 => Some(NamedKey::F(20)),
            WKey::F21 => Some(NamedKey::F(21)),
            WKey::F22 => Some(NamedKey::F(22)),
            WKey::F23 => Some(NamedKey::F(23)),
            WKey::F24 => Some(NamedKey::F(24)),
            WKey::F25 => Some(NamedKey::F(25)),
            WKey::Help => Some(NamedKey::Help),
            WKey::ContextMenu => Some(NamedKey::ContextMenu),
            WKey::Space => {
                if modify_other {
                    if let Some(bytes) = encode_modify_other_char(' ', ms) {
                        return bytes;
                    }
                }
                let tk = TextKey {
                    utf8: " ",
                    unshifted: ' ' as u32,
                    physical: Some(' ' as u32),
                    alt_text_prefix: option_gate(option_as_alt, raw),
                };
                let mt = TermMods {
                    alt: mods.alt_key(),
                    ..ms
                };
                return encode_text_key(&tk, mt, km);
            }
            _ => None,
        };
        if modify_other {
            if let Some(bytes) = key.and_then(|k| encode_named_modify_other(k, mns)) {
                return bytes;
            }
        }
        return key
            .map(|k| encode_named_with(k, mns, app_cursor, km))
            .unwrap_or_default();
    }
    // modifyOtherKeys 상태 2: 수식 키가 붙은 글자 하나를 `CSI 27 ; 수식 ; 코드 ~`로(Ghostty는 Ctrl→C0 바이트보다 먼저 본다)
    if modify_other {
        if let Key::Character(s) = k.logical {
            let mut chars = s.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if let Some(bytes) = encode_modify_other_char(c, ms) {
                    return bytes;
                }
            }
        }
    }
    // 글자 키: Ctrl 제어 바이트 → fixterms CSI u → Alt ESC 앞붙이기 → 글 그대로(`encode_text_key`).
    // Ctrl·Alt 조합은 수식 키를 뺀 글자로 만든다(Option이 만드는 ∫ 같은 글자 대신 b). Ctrl이 바뀐 상태에 없으면 OS가 만든 제어 문자(\x03) 대신 글자 키를 쓴다
    let utf8 = match k.logical {
        Key::Character(s) if m.ctrl || m.alt || ctrl_text_replaced(raw, mods) => s.as_str(),
        _ => k.text.unwrap_or(""),
    };
    let tk = TextKey {
        utf8,
        unshifted: k.unshifted,
        physical: k.physical.and_then(base_layout_codepoint),
        alt_text_prefix: option_gate(option_as_alt, raw),
    };
    // Ctrl 길은 Option도 Alt로 센다. 글자 앞 ESC는 `alt_text_prefix`가 막는다
    let mt = TermMods {
        alt: mods.alt_key(),
        ..ms
    };
    encode_text_key(&tk, mt, km)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_term::{encode_text, encode_text_with};

    #[test]
    fn non_latin_letter_with_cmd_or_ctrl_uses_command_layer() {
        let hangul = Key::Character("ㅅ".into());
        let t = || Some("t".to_string());
        // 한글 2벌식 T 자리: ⌘·⌃가 있으면 ⌘ 층 글자 t로 맞춘다
        assert_eq!(
            layout_trigger_key(&hangul, ModifiersState::SUPER, t),
            Key::Character("t".into())
        );
        assert_eq!(
            layout_trigger_key(&hangul, ModifiersState::CONTROL, t),
            Key::Character("t".into())
        );
        let both = ModifiersState::SUPER | ModifiersState::SHIFT;
        assert_eq!(layout_trigger_key(&hangul, both, t), Key::Character("t".into()));
        // ⌘ 층이 대문자를 주어도 트리거는 소문자로(Shift는 수식 키로 따로 센다)
        let upper = || Some("T".to_string());
        assert_eq!(
            layout_trigger_key(&hangul, ModifiersState::SUPER, upper),
            Key::Character("t".into())
        );
    }

    /// 한글 2벌식에서 확인 막대의 Y·N 자리 키는 자판 글자가 `ㅛ`·`ㅜ`라, ⌘ 층 글자 `y`·`n`으로 읽는다(#721)
    #[test]
    fn answer_char_reads_command_layer_on_non_latin_layouts() {
        let y = || Some("y".to_string());
        assert_eq!(answer_char_of(&Key::Character("ㅛ".into()), y), Some('y'));
        assert_eq!(
            answer_char_of(&Key::Character("ㅜ".into()), || Some("N".to_string())),
            Some('n'),
            "Shift가 붙어 대문자가 와도 소문자로"
        );
        // 라틴 글자 자판(Dvorak 등)은 자판 글자 그대로이고 ⌘ 층을 묻지 않는다
        let never = || -> Option<String> { panic!("⌘ 층을 묻지 않아야 한다") };
        assert_eq!(answer_char_of(&Key::Character("y".into()), never), Some('y'));
        assert_eq!(answer_char_of(&Key::Named(WKey::Enter), never), None);
        // ⌘ 층을 읽지 못하거나 쓸 수 없는 글자면 자판 글자를 그대로 둔다(y·n이 아니라 답이 되지 않는다)
        assert_eq!(answer_char_of(&Key::Character("ㅛ".into()), || None), Some('ㅛ'));
        assert_eq!(
            answer_char_of(&Key::Character("ㅛ".into()), || Some("\u{1b}".to_string())),
            Some('ㅛ')
        );
    }

    #[test]
    fn layout_fallback_keeps_latin_plain_and_unusable_keys() {
        let never = || -> Option<String> { panic!("⌘ 층을 묻지 않아야 한다") };
        // 라틴 글자·숫자·이름 있는 키는 그대로(⌘ 층을 묻지도 않는다)
        for k in [
            Key::Character("t".into()),
            Key::Character("1".into()),
            Key::Named(WKey::Enter),
        ] {
            assert_eq!(layout_trigger_key(&k, ModifiersState::SUPER, never), k);
        }
        let hangul = Key::Character("ㅅ".into());
        // ⌃·⌘ 없이(Shift·Option만) 누른 한글은 그대로: 입력기가 받을 글자다
        assert_eq!(layout_trigger_key(&hangul, ModifiersState::empty(), never), hangul);
        assert_eq!(layout_trigger_key(&hangul, ModifiersState::ALT, never), hangul);
        // ⌘ 층도 라틴 한 글자가 아니면(없음, 제어 문자, 두 글자) 그대로
        for got in [
            None,
            Some("\u{14}".to_string()),
            Some("ㅅ".to_string()),
            Some("tt".to_string()),
        ] {
            assert_eq!(layout_trigger_key(&hangul, ModifiersState::SUPER, || got), hangul);
        }
    }

    #[test]
    fn sided_from_uses_reported_sides_and_falls_back_to_left() {
        let none = (false, (false, false));
        // 오른쪽 Alt만 눌렀다고 알려졌다
        let m = sided_from([none, none, (true, (false, true)), none]);
        assert!(m.has_side(ModKind::Alt, ModSide::Right) && !m.has_side(ModKind::Alt, ModSide::Left));
        // 눌렸는데 좌우를 모르면 왼쪽
        let m = sided_from([(true, (false, false)), none, none, none]);
        assert!(m.has_side(ModKind::Shift, ModSide::Left) && !m.has_side(ModKind::Shift, ModSide::Right));
        // 안 눌렸으면 아무것도 없다
        assert_eq!(sided_from([none; 4]), SidedMods::default());
    }

    #[test]
    fn remapped_state_changes_modifiers_used_for_trigger_and_encoding() {
        let remaps = [
            KeyRemap::parse("ctrl=super").unwrap(),
            KeyRemap::parse("left_alt=ctrl").unwrap(),
        ];
        let ctrl = SidedMods::default().with(ModKind::Ctrl, ModSide::Left);
        let s = remapped_state(&remaps, ctrl);
        assert!(s.super_key() && !s.control_key());
        // 왼쪽 Alt는 Ctrl, 오른쪽 Alt는 그대로
        let s = remapped_state(&remaps, SidedMods::default().with(ModKind::Alt, ModSide::Left));
        assert!(s.control_key() && !s.alt_key());
        let s = remapped_state(&remaps, SidedMods::default().with(ModKind::Alt, ModSide::Right));
        assert!(s.alt_key() && !s.control_key());
        // 바뀐 상태로 키 묶음 트리거와 PTY 바이트가 정해진다: Ctrl이 Super가 되면 Ctrl-C 바이트(0x03)는 나가지 않는다
        let t = trigger_mods(remapped_state(&remaps, ctrl));
        assert!(t.sup && !t.ctrl);
        let bytes = |state| encode_text("c", term_mods(state, true));
        assert_eq!(bytes(remapped_state(&[], ctrl)), vec![0x03]);
        assert_ne!(
            bytes(remapped_state(&remaps, ctrl)),
            vec![0x03],
            "Ctrl이 Super가 되면 Ctrl-C가 아니다"
        );
        // Ctrl이 Alt가 되면 ESC c(Alt-c)
        let remaps = [KeyRemap::parse("ctrl=alt").unwrap()];
        assert_eq!(bytes(remapped_state(&remaps, ctrl)), vec![0x1b, b'c']);
    }

    #[test]
    fn default_modes_use_neither_keypad_application_nor_modify_other_keys() {
        // 기본값(1035 켬, modifyOtherKeys 꺼짐)에서는 `encode`가 키패드·CSI 27 길로 가지 않는다(지금 바이트 그대로)
        let m = bt_term::Modes::default();
        assert!(!m.modify_other_keys && !keypad_application(m.app_keypad, m.key));
        let e = bt_term::Emulator::new(bt_term::GridSize::new(10, 3), bt_core::Theme::default());
        let mut e = e;
        e.feed(b"\x1b=");
        let m = e.snapshot().modes;
        assert!(
            m.app_keypad && !keypad_application(m.app_keypad, m.key),
            "DECKPAM만으로는 응용 모드가 아니다(1035 켬)"
        );
        // 옛 바이트는 그대로
        assert_eq!(encode_text("1", TermMods::default()), b"1");
        assert_eq!(
            encode_text(
                "c",
                TermMods {
                    ctrl: true,
                    ..TermMods::default()
                }
            ),
            vec![0x03]
        );
        assert_eq!(
            encode_named_with(NamedKey::Enter, TermMods::default(), false, KeyModes::default()),
            b"\r"
        );
    }

    #[test]
    fn keypad_key_needs_numpad_location() {
        let ch = |s: &str| Key::Character(s.into());
        for (s, k) in [
            ("0", KeypadKey::Digit(0)),
            ("7", KeypadKey::Digit(7)),
            (".", KeypadKey::Decimal),
            (",", KeypadKey::Decimal),
            ("/", KeypadKey::Divide),
            ("*", KeypadKey::Multiply),
            ("-", KeypadKey::Subtract),
            ("+", KeypadKey::Add),
        ] {
            assert_eq!(keypad_key(&ch(s), KeyLocation::Numpad), Some(k), "{s}");
            // 같은 글자라도 일반 키 위치면 키패드가 아니다
            assert_eq!(keypad_key(&ch(s), KeyLocation::Standard), None, "{s}");
        }
        assert_eq!(
            keypad_key(&Key::Named(WKey::Enter), KeyLocation::Numpad),
            Some(KeypadKey::Enter)
        );
        assert_eq!(keypad_key(&Key::Named(WKey::Enter), KeyLocation::Standard), None);
        assert_eq!(keypad_key(&ch("a"), KeyLocation::Numpad), None);
        assert_eq!(keypad_key(&ch("12"), KeyLocation::Numpad), None);
        assert_eq!(keypad_key(&Key::Named(WKey::ArrowUp), KeyLocation::Numpad), None);
    }

    #[test]
    fn kitty_protocol_ignores_decbkm_and_mode_1036() {
        let mut modes = bt_term::Modes::default();
        modes.key.backarrow_key = true;
        modes.key.alt_esc_prefix = false;
        // kitty가 꺼져 있으면 모드를 그대로 쓴다
        assert!(effective_key_modes(&modes).backarrow_key);
        assert!(!effective_key_modes(&modes).alt_esc_prefix);
        // 켜져 있으면 무시(Ghostty `kitty()`는 backarrow_key_mode·alt_esc_prefix를 읽지 않는다)
        modes.kitty.disambiguate = true;
        let k = effective_key_modes(&modes);
        assert!(!k.backarrow_key && k.alt_esc_prefix);
        // 1035·1039는 그대로 넘긴다
        modes.key.alt_sends_escape = true;
        assert!(effective_key_modes(&modes).alt_sends_escape);
    }

    #[test]
    fn default_modes_keep_option_as_alt_bytes_unchanged() {
        // 기본(1036 켬, 67 꺼짐)에서 Option-as-Alt 바이트는 모드를 넣기 전과 같다
        let km = effective_key_modes(&bt_term::Modes::default());
        let alt = TermMods {
            alt: true,
            ..TermMods::default()
        };
        for t in ["b", "f", "界", "é", "."] {
            assert_eq!(encode_text_with(t, alt, km), encode_text(t, alt), "{t}");
            assert_eq!(encode_text_with(t, alt, km)[0], 0x1b);
        }
        assert_eq!(
            encode_named_with(NamedKey::Backspace, TermMods::default(), false, km),
            vec![0x7f]
        );
        // Option-as-Alt가 꺼지면 Alt가 아니라서 1036이 켜져 있어도 ESC가 없다
        assert!(!option_gate(false, ModifiersState::ALT));
        let m = term_mods(ModifiersState::ALT, option_gate(false, ModifiersState::ALT));
        assert!(!m.alt);
        assert_eq!(encode_text_with("b", m, km), b"b");
    }

    #[test]
    fn option_as_alt_gates_only_the_physical_option_key() {
        let none = ModifiersState::empty();
        let alt = ModifiersState::ALT;
        // 진짜 Option을 누르고 option-as-alt가 꺼져 있으면 Alt가 아니다(OS가 만드는 글자를 그대로 쓴다)
        assert!(!option_gate(false, alt));
        // Option을 누르지 않았는데 Alt가 있으면 key-remap이 만든 것이라 늘 Alt다
        assert!(option_gate(false, none));
        assert!(option_gate(true, alt));
        // Ctrl을 눌렀는데 바뀐 상태에 Ctrl이 없으면 OS가 만든 제어 문자(\x03) 대신 글자를 쓴다
        let ctrl = ModifiersState::CONTROL;
        assert!(ctrl_text_replaced(ctrl, ModifiersState::SUPER));
        assert!(!ctrl_text_replaced(ctrl, ctrl));
        assert!(!ctrl_text_replaced(none, ModifiersState::SUPER));
    }

    #[test]
    fn kitty_physical_keys_follow_ghostty_tables() {
        use KeyCode as C;
        assert_eq!(kitty_key_of(C::Enter), KittyKey::Named(NamedKey::Enter));
        assert_eq!(kitty_key_of(C::ArrowUp), KittyKey::Named(NamedKey::Up));
        assert_eq!(kitty_key_of(C::F13), KittyKey::Named(NamedKey::F(13)));
        assert_eq!(kitty_key_of(C::F25), KittyKey::Named(NamedKey::F(25)));
        assert_eq!(kitty_key_of(C::Numpad7), KittyKey::Keypad(KeypadKey::Digit(7)));
        assert_eq!(kitty_key_of(C::NumpadEnter), KittyKey::Keypad(KeypadKey::Enter));
        assert_eq!(
            kitty_key_of(C::ShiftRight),
            KittyKey::Modifier(ModifierKey::Shift { right: true })
        );
        assert_eq!(
            kitty_key_of(C::SuperLeft),
            KittyKey::Modifier(ModifierKey::Super { right: false })
        );
        assert_eq!(kitty_key_of(C::KeyA), KittyKey::Text, "글자 키는 표에 없다");
        // 기본 배열 키: US 배열 글자(자판이 러시아어여도 KeyA는 a)
        assert_eq!(base_layout_codepoint(C::KeyA), Some('a' as u32));
        assert_eq!(base_layout_codepoint(C::Semicolon), Some(';' as u32));
        assert_eq!(base_layout_codepoint(C::BracketLeft), Some('[' as u32));
        assert_eq!(base_layout_codepoint(C::Numpad1), Some('1' as u32));
        assert_eq!(base_layout_codepoint(C::Tab), Some(9));
        assert_eq!(base_layout_codepoint(C::Enter), None);
        assert_eq!(base_layout_codepoint(C::ArrowUp), None);
        assert_eq!(base_layout_codepoint(C::IntlBackslash), None);
    }

    #[test]
    fn kitty_shift_with_tab_enter_backspace_keeps_shift_like_ghostty() {
        // winit은 이름 키의 `text`에 제어 문자를 넣는다(Tab `\t`, Enter `\r`, Backspace `\u{8}`).
        // Ghostty macOS는 제어 문자로 시작하는 글을 키 글로 넘기지 않아(`keyEventText`) Shift가 남는다
        let named = |k: WKey, code: KeyCode, text: &str| KeyInput {
            pressed: true,
            repeat: false,
            logical: Key::Named(k),
            without_modifiers: Key::Named(k),
            location: KeyLocation::Standard,
            physical: PhysicalKey::Code(code),
            text: Some(text.into()),
        };
        let f1 = bt_term::KittyFlags {
            disambiguate: true,
            ..Default::default()
        };
        let shift = ModifiersState::SHIFT;
        let none = ModifiersState::empty();
        let enc = |i: &KeyInput, m| String::from_utf8(encode_kitty(i, m, m, f1, false)).unwrap();
        let tab = named(WKey::Tab, KeyCode::Tab, "\t");
        let enter = named(WKey::Enter, KeyCode::Enter, "\r");
        let bs = named(WKey::Backspace, KeyCode::Backspace, "\u{8}");
        assert_eq!(enc(&tab, shift), "\x1b[9;2u", "Shift+Tab");
        assert_eq!(enc(&enter, shift), "\x1b[13;2u", "Shift+Enter");
        assert_eq!(enc(&bs, shift), "\x1b[127;2u", "Shift+Backspace");
        // 수식 키가 없으면 옛 바이트 그대로(셸에서 reset을 칠 수 있게)
        assert_eq!(enc(&tab, none), "\t");
        assert_eq!(enc(&enter, none), "\r");
        assert_eq!(enc(&bs, none), "\x7f");
    }

    #[test]
    fn modifier_key_events_carry_their_own_state() {
        let mut m = TermMods::default();
        modifier_key_set(KittyKey::Modifier(ModifierKey::Ctrl { right: false }), &mut m, true);
        assert!(m.ctrl && !m.shift);
        modifier_key_set(KittyKey::Modifier(ModifierKey::Ctrl { right: false }), &mut m, false);
        assert!(!m.ctrl, "뗄 때는 자기 수식 키를 뺀다");
        modifier_key_set(KittyKey::Text, &mut m, true);
        assert_eq!(m, TermMods::default(), "수식 키가 아니면 건드리지 않는다");
    }

    #[test]
    fn kitty_flags_off_by_default_so_old_bytes_are_untouched() {
        // 플래그를 하나도 켜지 않으면 `encode`는 kitty 길로 가지 않는다(`kitty.active()`가 거짓). 그래서 옛 바이트 그대로다
        let m = bt_term::Modes::default();
        assert!(!m.kitty.active());
        assert_eq!(m.kitty, bt_term::KittyFlags::default());
        // 플래그 2·4·16 하나만 켜도 켜진 것이다(Ghostty `kitty_flags.int() != 0`)
        for f in [
            bt_term::KittyFlags {
                report_events: true,
                ..Default::default()
            },
            bt_term::KittyFlags {
                report_alternates: true,
                ..Default::default()
            },
            bt_term::KittyFlags {
                report_associated: true,
                ..Default::default()
            },
        ] {
            assert!(f.active());
        }
    }
}

#[cfg(test)]
#[path = "keys_ghostty_tests.rs"]
mod keys_ghostty_tests;

#[cfg(test)]
#[path = "keys_pc_table_data.rs"]
mod keys_pc_table_data;

#[cfg(test)]
#[path = "keys_pc_table_tests.rs"]
mod keys_pc_table_tests;

#[cfg(test)]
#[path = "keys_option_sides_tests.rs"]
mod keys_option_sides_tests;
