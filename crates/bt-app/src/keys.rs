//! winit 키 이벤트를 PTY 바이트로 바꾼다. 앱 단축키(Cmd-…)는 여기서 거른다

use bt_term::{Mods, NamedKey, encode_named, encode_text};
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey as WKey};

/// 앱이 처리하는 단축키
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Copy,
    Paste,
    FontBigger,
    FontSmaller,
    FontReset,
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    /// Cmd-1 … Cmd-9
    GotoTab(usize),
    ClearScrollback,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTop,
    ScrollBottom,
}

/// 키 이벤트가 단축키인가. macOS는 Cmd(Super), 다른 OS는 Ctrl+Shift
pub fn shortcut(event: &KeyEvent, mods: ModifiersState) -> Option<Shortcut> {
    let primary = if cfg!(target_os = "macos") {
        mods.super_key()
    } else {
        mods.control_key() && mods.shift_key()
    };
    if mods.shift_key() && !primary {
        return match &event.logical_key {
            Key::Named(WKey::PageUp) => Some(Shortcut::ScrollPageUp),
            Key::Named(WKey::PageDown) => Some(Shortcut::ScrollPageDown),
            Key::Named(WKey::Home) => Some(Shortcut::ScrollTop),
            Key::Named(WKey::End) => Some(Shortcut::ScrollBottom),
            _ => None,
        };
    }
    if !primary {
        return None;
    }
    let key = match &event.logical_key {
        Key::Character(s) => s.to_lowercase(),
        Key::Named(WKey::ArrowUp) => return Some(Shortcut::ScrollPageUp),
        Key::Named(WKey::ArrowDown) => return Some(Shortcut::ScrollPageDown),
        _ => return None,
    };
    Some(match key.as_str() {
        "c" => Shortcut::Copy,
        "v" => Shortcut::Paste,
        "=" | "+" => Shortcut::FontBigger,
        "-" => Shortcut::FontSmaller,
        "0" => Shortcut::FontReset,
        "t" => Shortcut::NewTab,
        "w" => Shortcut::CloseTab,
        "]" | "}" => Shortcut::NextTab,
        "[" | "{" => Shortcut::PrevTab,
        "k" => Shortcut::ClearScrollback,
        d if d.len() == 1 && ("1"..="9").contains(&d) => Shortcut::GotoTab(d.parse::<usize>().unwrap_or(1) - 1),
        _ => return None,
    })
}

/// 키 이벤트를 PTY 바이트로. 보낼 것이 없으면 빈 벡터
pub fn encode(event: &KeyEvent, mods: ModifiersState, app_cursor: bool, option_as_alt: bool) -> Vec<u8> {
    let m = Mods {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key() && option_as_alt,
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
        return key.map(|k| encode_named(k, m, app_cursor)).unwrap_or_default();
    }
    // Ctrl·Alt 조합은 수식 키를 뺀 글자로 만든다(Option이 만드는 ∫ 같은 글자 대신 b)
    if m.ctrl || m.alt {
        if let Key::Character(s) = &event.logical_key {
            return encode_text(s, m);
        }
    }
    match &event.text {
        Some(t) if !t.is_empty() => encode_text(t, Mods::default()),
        _ => Vec::new(),
    }
}
