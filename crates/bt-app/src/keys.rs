//! winit 키 이벤트를 PTY 바이트로 바꾼다. 앱 단축키(Cmd-…)는 여기서 거른다

use bt_term::{Mods as TermMods, NamedKey, encode_char_kitty, encode_named, encode_named_kitty, encode_text};

use crate::keymap::{Mods, Trigger, normalize_key};
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey as WKey};

/// 키 이벤트를 키 묶음 트리거로(Ghostty 키 이름). 수식 키를 뺀 글자를 쓴다(Shift+]도 bracket_right)
pub fn trigger(event: &KeyEvent, mods: ModifiersState) -> Option<Trigger> {
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    let key = match event.key_without_modifiers() {
        Key::Character(s) => normalize_key(&s),
        Key::Named(n) => named_key_name(n)?.to_string(),
        _ => return None,
    };
    Some(Trigger {
        mods: Mods {
            shift: mods.shift_key(),
            ctrl: mods.control_key(),
            alt: mods.alt_key(),
            sup: mods.super_key(),
        },
        key,
    })
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

/// 키 이벤트를 PTY 바이트로. 보낼 것이 없으면 빈 벡터
pub fn encode(event: &KeyEvent, mods: ModifiersState, modes: bt_term::Modes, option_as_alt: bool) -> Vec<u8> {
    let app_cursor = modes.app_cursor;
    let kitty = modes.kitty;
    let m = TermMods {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key() && option_as_alt,
        sup: false,
    };
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
    match &event.text {
        Some(t) if !t.is_empty() => encode_text(t, TermMods::default()),
        _ => Vec::new(),
    }
}
