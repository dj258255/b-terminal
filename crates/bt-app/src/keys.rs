//! winit 키 이벤트를 PTY 바이트로 바꾼다. 앱 단축키(Cmd-…)는 여기서 거른다

use bt_term::{Mods as TermMods, NamedKey, encode_char_kitty, encode_named, encode_named_kitty, encode_text};

use crate::keymap::{Mods, Trigger, normalize_key};
use bt_core::config::{KeyRemap, ModKind, ModSide, SidedMods, apply_key_remap};
use winit::event::{KeyEvent, Modifiers};
use winit::keyboard::{Key, ModifiersKeyState, ModifiersState, NamedKey as WKey};

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
    let key = match event.key_without_modifiers() {
        Key::Character(s) => normalize_key(&s),
        Key::Named(n) => named_key_name(n)?.to_string(),
        _ => return None,
    };
    Some(Trigger {
        mods: trigger_mods(mods),
        key,
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
        _ => return None,
    })
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

/// 키 이벤트를 PTY 바이트로. 보낼 것이 없으면 빈 벡터. `mods`는 `key-remap`을 적용한 상태, `raw`는 바꾸기 전 상태
pub fn encode(
    event: &KeyEvent,
    mods: ModifiersState,
    raw: ModifiersState,
    modes: bt_term::Modes,
    option_as_alt: bool,
) -> Vec<u8> {
    let app_cursor = modes.app_cursor;
    let kitty = modes.kitty;
    let m = term_mods(mods, option_gate(option_as_alt, raw));
    if let Key::Named(named) = &event.logical_key {
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
            WKey::Space => return encode_text(" ", m),
            _ => None,
        };
        // 프로그램이 kitty 키보드 프로토콜을 켰으면 Shift+Enter 같은 키를 CSI u로(#41)
        if let Some(k) = key {
            if let Some(bytes) = encode_named_kitty(k, m, kitty) {
                return bytes;
            }
        }
        return key.map(|k| encode_named(k, m, app_cursor)).unwrap_or_default();
    }
    if kitty.active() {
        use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
        if let Key::Character(base) = event.key_without_modifiers() {
            let mut chars = base.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if let Some(bytes) = encode_char_kitty(c, m, kitty) {
                    return bytes;
                }
            }
        }
    }
    // Ctrl·Alt 조합은 수식 키를 뺀 글자로 만든다(Option이 만드는 ∫ 같은 글자 대신 b)
    if m.ctrl || m.alt {
        if let Key::Character(s) = &event.logical_key {
            return encode_text(s, m);
        }
    }
    if ctrl_text_replaced(raw, mods) {
        if let Key::Character(s) = &event.logical_key {
            return encode_text(s, TermMods::default());
        }
    }
    match &event.text {
        Some(t) if !t.is_empty() => encode_text(t, TermMods::default()),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
