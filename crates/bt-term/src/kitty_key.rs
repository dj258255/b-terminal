//! kitty 키보드 프로토콜의 키 인코딩(Ghostty `input/key_encode.zig`의 `kitty()`와 `KittySequence`를 옮긴 것).
//! 플래그가 하나라도 켜져 있으면 키 입력은 모두 이 길로 간다(Ghostty `encode`가 `kitty_flags.int() != 0`이면 `kitty()`).
//! <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>

use crate::input::{KeypadKey, KittyFlags, Mods, NamedKey};

/// 키 이벤트 종류(Ghostty `key.Action`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Repeat,
    Release,
}

/// 수식 키 단독(왼쪽·오른쪽을 가린다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    Shift { right: bool },
    Ctrl { right: bool },
    Alt { right: bool },
    Super { right: bool },
}

/// 눌린 키(자판 배열과 상관없는 물리 키 쪽). 글자 키는 `Text`로 두고 글자 값은 `unshifted`·`base`로 준다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyKey {
    /// 이름 있는 키(F1~F25까지)
    Named(NamedKey),
    Keypad(KeypadKey),
    NumpadEqual,
    NumpadSeparator,
    Modifier(ModifierKey),
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    /// 표에 없는 키. 글자 키는 `unshifted`로 코드를 정한다
    Text,
}

/// 키 이벤트 하나(Ghostty `key.KeyEvent`)
#[derive(Debug, Clone, Copy)]
pub struct KittyKeyEvent<'a> {
    pub action: KeyAction,
    pub key: KittyKey,
    /// 눌려 있는 수식 키(Cmd는 `sup`)
    pub mods: Mods,
    pub caps_lock: bool,
    pub num_lock: bool,
    /// 글자를 만드는 데 쓰인 수식 키(`utf8`이 비어 있으면 뜻이 없다). 유효 수식 키는 `mods`에서 이것을 뺀 것
    pub consumed: Mods,
    /// 한글 같은 조합 중에 눌린 키인가
    pub composing: bool,
    /// 키가 만든 글자
    pub utf8: &'a str,
    /// 수식 키 없이 눌렀을 때의 글자(배열 기준). 없으면 0
    pub unshifted: u32,
    /// 물리 키의 US 배열 글자(기본 배열 키). 글자가 없는 키는 없다
    pub base: Option<u32>,
    /// Alt가 글자를 막는가. macOS에서는 Option을 Alt로 쓸 때만 그렇다(Ghostty `macos_option_as_alt`), 다른 OS는 늘 그렇다
    pub alt_prevents_text: bool,
}

/// 표의 한 줄(Ghostty `kitty.zig`의 `Entry`)
struct Entry {
    code: u32,
    final_byte: u8,
    /// 수식 키 단독이면 true. 플래그 8이 있어야 보낸다
    modifier: bool,
}

fn entry(code: u32, final_byte: u8, modifier: bool) -> Entry {
    Entry {
        code,
        final_byte,
        modifier,
    }
}

/// 키의 표 줄(Ghostty `kitty.zig`의 `raw_entries`). 표에 없는 키는 없다
fn table_entry(key: KittyKey) -> Option<Entry> {
    Some(match key {
        KittyKey::Named(n) => match n {
            NamedKey::Escape => entry(27, b'u', false),
            NamedKey::Enter => entry(13, b'u', false),
            NamedKey::Tab => entry(9, b'u', false),
            NamedKey::Backspace => entry(127, b'u', false),
            NamedKey::Insert => entry(2, b'~', false),
            NamedKey::Delete => entry(3, b'~', false),
            NamedKey::Left => entry(1, b'D', false),
            NamedKey::Right => entry(1, b'C', false),
            NamedKey::Up => entry(1, b'A', false),
            NamedKey::Down => entry(1, b'B', false),
            NamedKey::PageUp => entry(5, b'~', false),
            NamedKey::PageDown => entry(6, b'~', false),
            NamedKey::Home => entry(1, b'H', false),
            NamedKey::End => entry(1, b'F', false),
            NamedKey::F(n) => match n {
                1 => entry(1, b'P', false),
                2 => entry(1, b'Q', false),
                3 => entry(13, b'~', false),
                4 => entry(1, b'S', false),
                5 => entry(15, b'~', false),
                6 => entry(17, b'~', false),
                7 => entry(18, b'~', false),
                8 => entry(19, b'~', false),
                9 => entry(20, b'~', false),
                10 => entry(21, b'~', false),
                11 => entry(23, b'~', false),
                12 => entry(24, b'~', false),
                13..=25 => entry(57376 + (n as u32 - 13), b'u', false),
                _ => return None,
            },
            // 표에 없는 키(Ghostty `kitty.zig`에도 없다)
            NamedKey::Help | NamedKey::ContextMenu => return None,
        },
        KittyKey::CapsLock => entry(57358, b'u', true),
        KittyKey::ScrollLock => entry(57359, b'u', false),
        KittyKey::NumLock => entry(57360, b'u', true),
        KittyKey::PrintScreen => entry(57361, b'u', false),
        KittyKey::Pause => entry(57362, b'u', false),
        KittyKey::Keypad(k) => match k {
            KeypadKey::Digit(n) => entry(57399 + n.min(9) as u32, b'u', false),
            KeypadKey::Decimal => entry(57409, b'u', false),
            KeypadKey::Divide => entry(57410, b'u', false),
            KeypadKey::Multiply => entry(57411, b'u', false),
            KeypadKey::Subtract => entry(57412, b'u', false),
            KeypadKey::Add => entry(57413, b'u', false),
            KeypadKey::Enter => entry(57414, b'u', false),
        },
        KittyKey::NumpadEqual => entry(57415, b'u', false),
        KittyKey::NumpadSeparator => entry(57416, b'u', false),
        KittyKey::Modifier(m) => match m {
            ModifierKey::Shift { right: false } => entry(57441, b'u', true),
            ModifierKey::Shift { right: true } => entry(57447, b'u', true),
            ModifierKey::Ctrl { right: false } => entry(57442, b'u', true),
            ModifierKey::Ctrl { right: true } => entry(57448, b'u', true),
            ModifierKey::Super { right: false } => entry(57444, b'u', true),
            ModifierKey::Super { right: true } => entry(57450, b'u', true),
            ModifierKey::Alt { right: false } => entry(57443, b'u', true),
            ModifierKey::Alt { right: true } => entry(57449, b'u', true),
        },
        KittyKey::Text => return None,
    })
}

fn is_control(cp: u32) -> bool {
    cp < 0x20 || cp == 0x7f
}

/// 글이 제어 문자 한 바이트인가(Ghostty `isControlUtf8`)
fn is_control_utf8(s: &str) -> bool {
    s.len() == 1 && is_control(s.as_bytes()[0] as u32)
}

fn any_mod(m: Mods) -> bool {
    m.shift || m.ctrl || m.alt || m.sup
}

/// 수식 키 칸의 수(Ghostty `KittyMods.seqInt`): 1 + shift·alt·ctrl·super·hyper·meta·caps·num
fn mods_param(m: Mods, caps: bool, num: bool) -> u32 {
    1 + m.shift as u32 + 2 * m.alt as u32 + 4 * m.ctrl as u32 + 8 * m.sup as u32 + 64 * caps as u32 + 128 * num as u32
}

fn unset(mods: Mods, consumed: Mods) -> Mods {
    Mods {
        shift: mods.shift && !consumed.shift,
        ctrl: mods.ctrl && !consumed.ctrl,
        alt: mods.alt && !consumed.alt,
        sup: mods.sup && !consumed.sup,
    }
}

/// 키 하나를 kitty 프로토콜 바이트로. 보낼 것이 없으면 빈 벡터(Ghostty `kitty()`)
pub fn encode_kitty_key(ev: &KittyKeyEvent, flags: KittyFlags) -> Vec<u8> {
    let named = |n| ev.key == KittyKey::Named(n);

    // 뗌은 플래그 2가 있을 때만. Enter·Backspace·Tab의 뗌은 플래그 8이 없으면 없다
    if ev.action == KeyAction::Release {
        if !flags.report_events {
            return Vec::new();
        }
        if !flags.all_as_escapes && (named(NamedKey::Enter) || named(NamedKey::Backspace) || named(NamedKey::Tab)) {
            return Vec::new();
        }
    }

    // 글이 있으면 글을 만드는 데 쓰인 수식 키는 빼고 본다(`effectiveMods`)
    let effective = if ev.utf8.is_empty() {
        ev.mods
    } else {
        unset(ev.mods, ev.consumed)
    };
    let binding_empty = !any_mod(effective);

    // 표의 키는 표대로, 아닌 키는 수식 키 없는 글자로 `u`
    let entry = table_entry(ev.key).or_else(|| {
        (ev.unshifted > 0).then_some(Entry {
            code: ev.unshifted,
            final_byte: b'u',
            modifier: false,
        })
    });

    if ev.composing {
        // 조합 중에는 수식 키만 보낸다
        if !entry.as_ref().is_some_and(|e| e.modifier) {
            return Vec::new();
        }
    } else {
        // IME 확정은 Enter 키와 글로 온다. 그 글을 그대로 보낸다(Backspace는 보내지 않는다)
        if !ev.utf8.is_empty() && (named(NamedKey::Enter) || named(NamedKey::Backspace)) && !is_control_utf8(ev.utf8) {
            if named(NamedKey::Backspace) {
                return Vec::new();
            }
            return ev.utf8.as_bytes().to_vec();
        }
        // 모든 키를 이스케이프로(8)가 아니면 Enter·Tab·Backspace와 수식 키 없는 글자는 옛 꼴 그대로
        if !flags.all_as_escapes {
            if binding_empty {
                if named(NamedKey::Enter) {
                    return b"\r".to_vec();
                }
                if named(NamedKey::Tab) {
                    return b"\t".to_vec();
                }
                if named(NamedKey::Backspace) {
                    return vec![0x7f];
                }
            }
            if !ev.utf8.is_empty()
                && binding_empty
                && ev.action != KeyAction::Release
                && !ev.utf8.chars().any(|c| is_control(c as u32))
            {
                return ev.utf8.as_bytes().to_vec();
            }
        }
    }

    let Some(entry) = entry else {
        // 표에 없고 글자도 모르는 키: 글만 있으면 글을 그대로(뗌은 글을 넣지 않는다)
        if ev.action == KeyAction::Release || ev.utf8.is_empty() {
            return Vec::new();
        }
        return ev.utf8.as_bytes().to_vec();
    };

    // 수식 키 단독은 플래그 8이 있어야 보낸다
    if entry.modifier && !flags.all_as_escapes {
        return Vec::new();
    }

    let mods = mods_param(ev.mods, ev.caps_lock, ev.num_lock);
    let event_code = if flags.report_events {
        Some(match ev.action {
            KeyAction::Press => 1,
            KeyAction::Repeat => 2,
            KeyAction::Release => 3,
        })
    } else {
        None
    };

    // 특수 꼴(`ESC [ 1 ; 수식 : 이벤트 끝글자`): 글자 코드 없는 방향 키·Home·End·F1~F4
    if entry.final_byte != b'u' && entry.final_byte != b'~' {
        let fin = entry.final_byte as char;
        return match event_code {
            Some(e) => format!("\x1b[1;{mods}:{e}{fin}"),
            None if mods > 1 => format!("\x1b[1;{mods}{fin}"),
            None => format!("\x1b[{fin}"),
        }
        .into_bytes();
    }

    // 대체 키: 코드 뒤에 `:shifted:base`
    let mut alt: [Option<u32>; 2] = [None, None];
    if flags.report_alternates && !is_control(entry.code) {
        let mut chars = ev.utf8.chars();
        if let Some(cp1) = chars.next() {
            let cp1 = cp1 as u32;
            if cp1 != entry.code && ev.mods.shift {
                alt[0] = Some(cp1);
            }
            let has_cp2 = chars.next().is_some();
            if let Some(base) = ev.base {
                if base != entry.code && cp1 != base && !has_cp2 {
                    alt[1] = Some(base);
                }
            }
        } else if let Some(base) = ev.base {
            if base != entry.code {
                alt[1] = Some(base);
            }
        }
    }

    // 연결 글자: 이벤트가 뗌이 아니고 글을 막는 수식 키가 없을 때
    let associated = flags.report_associated
        && event_code != Some(3)
        && !((ev.mods.alt && ev.alt_prevents_text) || ev.mods.ctrl || ev.mods.sup);

    let mut out = format!("\x1b[{}", entry.code);
    if let Some(shifted) = alt[0] {
        out.push_str(&format!(":{shifted}"));
    }
    if let Some(base) = alt[1] {
        if alt[0].is_none() {
            out.push_str(&format!("::{base}"));
        } else {
            out.push_str(&format!(":{base}"));
        }
    }
    // 이벤트 번호는 누름이면 생략(`encodeFull`)
    let mut emitted_prior = false;
    match event_code {
        Some(e) if e != 1 => {
            out.push_str(&format!(";{mods}:{e}"));
            emitted_prior = true;
        }
        _ if mods > 1 => {
            out.push_str(&format!(";{mods}"));
            emitted_prior = true;
        }
        _ => {}
    }
    if associated {
        let mut count = 0;
        for c in ev.utf8.chars() {
            if is_control(c as u32) {
                continue;
            }
            if count == 0 {
                if !emitted_prior {
                    out.push(';');
                }
                out.push(';');
            } else {
                out.push(':');
            }
            out.push_str(&(c as u32).to_string());
            count += 1;
        }
    }
    out.push(entry.final_byte as char);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
        sup: false,
    };
    const SHIFT: Mods = Mods { shift: true, ..NONE };
    const CTRL: Mods = Mods { ctrl: true, ..NONE };
    const ALT: Mods = Mods { alt: true, ..NONE };

    fn flags(bits: &str) -> KittyFlags {
        KittyFlags {
            disambiguate: bits.contains('1'),
            report_events: bits.contains('2'),
            report_alternates: bits.contains('4'),
            all_as_escapes: bits.contains('8'),
            report_associated: bits.contains('g'),
        }
    }
    /// 1·2·4·8과 연결 글자(16)를 모두 켠 것
    fn all() -> KittyFlags {
        flags("1248g")
    }

    fn ev(key: KittyKey) -> KittyKeyEvent<'static> {
        KittyKeyEvent {
            action: KeyAction::Press,
            key,
            mods: NONE,
            caps_lock: false,
            num_lock: false,
            consumed: NONE,
            composing: false,
            utf8: "",
            unshifted: 0,
            base: None,
            alt_prevents_text: true,
        }
    }
    /// 글자 키 하나(Ghostty 시험의 `.key_a` 같은 것). `base`는 US 배열 글자
    fn letter(base: char, utf8: &str, unshifted: char) -> KittyKeyEvent<'_> {
        KittyKeyEvent {
            utf8,
            unshifted: unshifted as u32,
            base: Some(base as u32),
            ..ev(KittyKey::Text)
        }
    }
    fn named(k: NamedKey) -> KittyKeyEvent<'static> {
        ev(KittyKey::Named(k))
    }
    fn s(v: Vec<u8>) -> String {
        String::from_utf8(v).unwrap()
    }
    fn enc(e: KittyKeyEvent, f: KittyFlags) -> String {
        s(encode_kitty_key(&e, f))
    }
    fn lshift() -> KittyKey {
        KittyKey::Modifier(ModifierKey::Shift { right: false })
    }
    fn lctrl() -> KittyKey {
        KittyKey::Modifier(ModifierKey::Ctrl { right: false })
    }

    // ---- Ghostty key_encode.zig 1250줄~ 의 kitty 시험을 같은 입력·같은 바이트로 옮긴 것 ----

    #[test]
    fn plain_text_and_repeat_with_disambiguate() {
        assert_eq!(enc(letter('a', "abcd", 'a'), flags("1")), "abcd");
        let rep = KittyKeyEvent {
            action: KeyAction::Repeat,
            ..letter('a', "a", 'a')
        };
        assert_eq!(enc(rep, flags("1")), "a");
    }

    #[test]
    fn enter_backspace_tab_keep_legacy_bytes_and_no_release_without_report_all() {
        let f = flags("1");
        assert_eq!(enc(named(NamedKey::Enter), f), "\r");
        assert_eq!(enc(named(NamedKey::Backspace), f), "\x7f");
        assert_eq!(enc(named(NamedKey::Tab), f), "\t");
        let rel = |k| KittyKeyEvent {
            action: KeyAction::Release,
            ..named(k)
        };
        for k in [NamedKey::Enter, NamedKey::Backspace, NamedKey::Tab] {
            assert_eq!(enc(rel(k), flags("12")), "", "{k:?} 뗌은 report_all 없이는 없다");
        }
        assert_eq!(enc(rel(NamedKey::Enter), flags("128")), "\x1b[13;1:3u");
        assert_eq!(enc(rel(NamedKey::Backspace), flags("128")), "\x1b[127;1:3u");
        assert_eq!(enc(rel(NamedKey::Tab), flags("128")), "\x1b[9;1:3u");
    }

    #[test]
    fn modified_enter_backspace_tab_are_csi_u() {
        let f = flags("1");
        let m = |k, mods, consumed| KittyKeyEvent {
            mods,
            consumed,
            ..named(k)
        };
        assert_eq!(enc(m(NamedKey::Backspace, SHIFT, NONE), f), "\x1b[127;2u");
        // macOS가 Option을 소비한 것으로 표시해도 글이 없으면 수식 키가 그대로 유효하다
        assert_eq!(enc(m(NamedKey::Backspace, ALT, ALT), f), "\x1b[127;3u");
        assert_eq!(enc(m(NamedKey::Enter, SHIFT, NONE), f), "\x1b[13;2u");
        assert_eq!(enc(m(NamedKey::Tab, SHIFT, NONE), f), "\x1b[9;2u");
        assert_eq!(
            enc(m(NamedKey::Tab, SHIFT, NONE), flags("14")),
            "\x1b[9;2u",
            "Shift+Tab과 대체 키"
        );
    }

    #[test]
    fn all_flags_enter_ctrl_and_release_of_ctrl() {
        assert_eq!(enc(named(NamedKey::Enter), all()), "\x1b[13u");
        let c = KittyKeyEvent {
            mods: CTRL,
            ..ev(lctrl())
        };
        assert_eq!(enc(c, all()), "\x1b[57442;5u");
        let r = KittyKeyEvent {
            action: KeyAction::Release,
            ..c
        };
        assert_eq!(enc(r, all()), "\x1b[57442;5:3u");
    }

    #[test]
    fn delete_and_arrows_ignore_control_utf8() {
        let del = KittyKeyEvent {
            utf8: "\x7f",
            ..named(NamedKey::Delete)
        };
        assert_eq!(enc(del, flags("1")), "\x1b[3~");
        assert_eq!(enc(del, flags("148")), "\x1b[3~", "대체 키가 켜져도 제어 문자는 뺀다");
        let up = KittyKeyEvent {
            utf8: "\x1e",
            ..named(NamedKey::Up)
        };
        assert_eq!(enc(up, flags("1")), "\x1b[A");
    }

    #[test]
    fn composing_sends_only_modifier_keys() {
        let a = KittyKeyEvent {
            mods: SHIFT,
            composing: true,
            ..letter('a', "", '\0')
        };
        assert_eq!(enc(a, flags("1")), "");
        let sh = KittyKeyEvent {
            mods: SHIFT,
            composing: true,
            ..ev(lshift())
        };
        assert_eq!(enc(sh, flags("18")), "\x1b[57441;2u");
    }

    #[test]
    fn composed_text_without_key_is_sent_as_is_and_never_on_release() {
        let t = KittyKeyEvent {
            utf8: "û",
            ..ev(KittyKey::Text)
        };
        assert_eq!(enc(t, all()), "û");
        let bang = KittyKeyEvent {
            mods: SHIFT,
            utf8: "!",
            ..ev(KittyKey::Text)
        };
        for report_all in [false, true] {
            let f = KittyFlags {
                all_as_escapes: report_all,
                ..flags("124g")
            };
            let rel = KittyKeyEvent {
                action: KeyAction::Release,
                ..bang
            };
            assert_eq!(enc(rel, f), "", "뗌은 글을 넣지 않는다(report_all={report_all})");
        }
        let rep = KittyKeyEvent {
            action: KeyAction::Repeat,
            ..bang
        };
        assert_eq!(enc(rep, flags("124g")), "!");
    }

    #[test]
    fn alternates_shifted_and_base() {
        let f = flags("14");
        let shift_a = KittyKeyEvent {
            mods: SHIFT,
            ..letter('a', "A", 'a')
        };
        assert_eq!(enc(shift_a, f), "\x1b[97:65;2u");
        // 배열이 기본 키와 다르면(unshifted 65, base 97) 세 번째 값이 붙는다(Ghostty의 가상 입력 시험)
        let odd = KittyKeyEvent {
            mods: SHIFT,
            ..letter('a', "A", 'A')
        };
        assert_eq!(enc(odd, f), "\x1b[65::97;2u");
        // Shift+;
        let colon = KittyKeyEvent {
            mods: SHIFT,
            ..letter(';', ":", ';')
        };
        assert_eq!(enc(colon, flags("148g")), "\x1b[59:58;2;58u");
    }

    #[test]
    fn alternates_with_caps_lock_and_non_us_layouts() {
        let f = flags("148g");
        let caps_j = KittyKeyEvent {
            caps_lock: true,
            ..letter('j', "J", 'j')
        };
        assert_eq!(enc(caps_j, f), "\x1b[106;65;74u");
        // 러시아 배열: 세미콜론 자리의 ч
        let ru = letter(';', "ч", 'ч');
        assert_eq!(enc(ru, f), "\x1b[1095::59;;1095u");
        let ru_shift = KittyKeyEvent {
            mods: SHIFT,
            ..letter(';', "Ч", 'ч')
        };
        assert_eq!(enc(ru_shift, f), "\x1b[1095:1063:59;2;1063u");
        let ru_caps = KittyKeyEvent {
            caps_lock: true,
            ..letter(';', "Ч", 'ч')
        };
        assert_eq!(enc(ru_caps, f), "\x1b[1095::59;65;1063u");
        // 헝가리 배열 Ctrl+ő 뗌
        let hu = KittyKeyEvent {
            action: KeyAction::Release,
            mods: CTRL,
            ..letter('[', "", 'ő')
        };
        assert_eq!(enc(hu, flags("1248g")), "\x1b[337::91;5:3u");
    }

    #[test]
    fn modifier_keys_need_report_all() {
        let sh = ev(lshift());
        assert_eq!(enc(sh, flags("14")), "");
        assert_eq!(enc(sh, flags("18")), "\x1b[57441u");
    }

    #[test]
    fn associated_text_on_macos_option() {
        let f = flags("148g");
        // Option을 Alt로 쓰지 않는다: 글이 따라온다
        let opt = KittyKeyEvent {
            mods: ALT,
            alt_prevents_text: false,
            ..letter('w', "∑", 'w')
        };
        assert_eq!(enc(opt, f), "\x1b[119;3;8721u");
        // Option을 Alt로 쓴다: 글은 없다
        let alt = KittyKeyEvent {
            alt_prevents_text: true,
            ..opt
        };
        assert_eq!(enc(alt, f), "\x1b[119;3u");
        // 수식 키가 없으면 글이 따라온다
        let none = KittyKeyEvent { mods: NONE, ..alt };
        assert_eq!(enc(none, f), "\x1b[119;;8721u");
    }

    #[test]
    fn associated_text_and_modifiers() {
        let f = flags("148g");
        let ctrl_j = KittyKeyEvent {
            mods: CTRL,
            ..letter('j', "j", 'j')
        };
        assert_eq!(enc(ctrl_j, f), "\x1b[106;5u", "Ctrl이 있으면 글은 없다");
        let shift_j = KittyKeyEvent {
            mods: SHIFT,
            ..letter('j', "J", 'j')
        };
        assert_eq!(enc(shift_j, f), "\x1b[106:74;2;74u");
        let rel = KittyKeyEvent {
            action: KeyAction::Release,
            ..shift_j
        };
        assert_eq!(enc(rel, flags("1248g")), "\x1b[106:74;2:3u", "뗌에는 글이 없다");
    }

    #[test]
    fn enter_and_backspace_with_text_and_keypad() {
        let en = KittyKeyEvent {
            utf8: "A",
            unshifted: 0x0d,
            ..named(NamedKey::Enter)
        };
        assert_eq!(enc(en, flags("148")), "A", "죽은 키 상태에서 Enter와 함께 온 글");
        let bs = KittyKeyEvent {
            utf8: "A",
            ..named(NamedKey::Backspace)
        };
        assert_eq!(enc(bs, all()), "");
        assert_eq!(
            enc(named(NamedKey::Backspace), all()),
            "\x1b[127u",
            "DECBKM은 보지 않는다"
        );
        let kp = KittyKeyEvent {
            utf8: "1",
            base: Some('1' as u32),
            ..ev(KittyKey::Keypad(KeypadKey::Digit(1)))
        };
        assert_eq!(enc(kp, all()), "\x1b[57400;;49u");
    }

    // ---- 이벤트 종류(플래그 2), 시퀀스 꼴 ----

    #[test]
    fn event_types_in_text_and_special_forms() {
        let rel = |e: KittyKeyEvent<'static>| KittyKeyEvent {
            action: KeyAction::Release,
            ..e
        };
        let rep = |e: KittyKeyEvent<'static>| KittyKeyEvent {
            action: KeyAction::Repeat,
            ..e
        };
        // 글자 키: 뗌 `;1:3u`, 수식 키가 있으면 `;5:3u`
        assert_eq!(enc(rel(letter('a', "", 'a')), flags("12")), "\x1b[97;1:3u");
        assert_eq!(
            enc(
                rel(KittyKeyEvent {
                    mods: CTRL,
                    ..letter('a', "", 'a')
                }),
                flags("12")
            ),
            "\x1b[97;5:3u"
        );
        assert_eq!(
            enc(
                rep(KittyKeyEvent {
                    mods: CTRL,
                    ..letter('a', "a", 'a')
                }),
                flags("12")
            ),
            "\x1b[97;5:2u"
        );
        // 플래그 2만(1 없이)이어도 이 길로 온다. 글자 키 누름·반복은 글 그대로, 뗌은 CSI u
        assert_eq!(enc(letter('a', "a", 'a'), flags("2")), "a");
        assert_eq!(enc(rep(letter('a', "a", 'a')), flags("2")), "a");
        assert_eq!(enc(rel(letter('a', "a", 'a')), flags("2")), "\x1b[97;1:3u");
        // 플래그 2가 없으면 뗌은 없고 반복은 누름과 같다
        assert_eq!(enc(rel(letter('a', "a", 'a')), flags("1")), "");
        // 플래그 8: 글자도 CSI u. 누름은 `;` 없이, 반복·뗌은 이벤트 번호
        assert_eq!(enc(letter('a', "a", 'a'), flags("8")), "\x1b[97u");
        assert_eq!(enc(letter('a', "a", 'a'), flags("28")), "\x1b[97u");
        assert_eq!(enc(rep(letter('a', "a", 'a')), flags("28")), "\x1b[97;1:2u");
        assert_eq!(enc(rel(letter('a', "a", 'a')), flags("28")), "\x1b[97;1:3u");
        // `~` 꼴과 `ABCDHF`·F1~F4 꼴
        assert_eq!(enc(rel(named(NamedKey::Delete)), flags("12")), "\x1b[3;1:3~");
        assert_eq!(
            enc(
                rel(KittyKeyEvent {
                    mods: SHIFT,
                    ..named(NamedKey::Up)
                }),
                flags("12")
            ),
            "\x1b[1;2:3A"
        );
        assert_eq!(enc(rel(named(NamedKey::F(1))), flags("12")), "\x1b[1;1:3P");
        // Ghostty는 특수 꼴에서 누름에도 `:1`을 붙인다(encodeSpecial)
        assert_eq!(enc(named(NamedKey::Up), flags("12")), "\x1b[1;1:1A");
        assert_eq!(
            enc(named(NamedKey::Up), flags("1")),
            "\x1b[A",
            "플래그 2가 없으면 이벤트 번호가 없다"
        );
        assert_eq!(
            enc(
                KittyKeyEvent {
                    mods: CTRL,
                    ..named(NamedKey::Left)
                },
                flags("1")
            ),
            "\x1b[1;5D"
        );
    }

    #[test]
    fn named_key_codes_follow_ghostty_table() {
        let f = flags("1");
        for (k, want) in [
            (NamedKey::Escape, "\x1b[27u"),
            (NamedKey::Insert, "\x1b[2~"),
            (NamedKey::PageUp, "\x1b[5~"),
            (NamedKey::PageDown, "\x1b[6~"),
            (NamedKey::Home, "\x1b[H"),
            (NamedKey::End, "\x1b[F"),
            (NamedKey::Down, "\x1b[B"),
            (NamedKey::Right, "\x1b[C"),
            (NamedKey::F(1), "\x1b[P"),
            (NamedKey::F(2), "\x1b[Q"),
            (NamedKey::F(3), "\x1b[13~"),
            (NamedKey::F(4), "\x1b[S"),
            (NamedKey::F(5), "\x1b[15~"),
            (NamedKey::F(12), "\x1b[24~"),
            (NamedKey::F(13), "\x1b[57376u"),
            (NamedKey::F(25), "\x1b[57388u"),
        ] {
            assert_eq!(enc(named(k), f), want, "{k:?}");
        }
        assert_eq!(enc(ev(KittyKey::NumLock), flags("18")), "\x1b[57360u");
        assert_eq!(
            enc(ev(KittyKey::NumLock), flags("1")),
            "",
            "수식 키로 센 잠금 키는 report_all이 필요하다"
        );
        assert_eq!(enc(ev(KittyKey::PrintScreen), f), "\x1b[57361u");
        assert_eq!(
            enc(ev(KittyKey::Modifier(ModifierKey::Super { right: true })), flags("18")),
            "\x1b[57450u"
        );
        assert_eq!(enc(ev(KittyKey::Keypad(KeypadKey::Enter)), flags("18")), "\x1b[57414u");
        assert_eq!(enc(ev(KittyKey::NumpadEqual), flags("18")), "\x1b[57415u");
    }

    // ---- 기존 b-terminal 시험(Shift+Enter 등)을 새 인코더로 옮긴 것 ----

    #[test]
    fn shift_enter_and_ctrl_letters_like_before() {
        let f = flags("1");
        assert_eq!(
            enc(
                KittyKeyEvent {
                    mods: SHIFT,
                    ..named(NamedKey::Enter)
                },
                f
            ),
            "\x1b[13;2u"
        );
        assert_eq!(enc(named(NamedKey::Enter), f), "\r", "1단계의 맨 Enter는 CR 그대로");
        assert_eq!(enc(named(NamedKey::Enter), flags("18")), "\x1b[13u");
        assert_eq!(enc(named(NamedKey::Escape), f), "\x1b[27u");
        assert_eq!(
            enc(
                KittyKeyEvent {
                    mods: CTRL,
                    ..letter('c', "c", 'c')
                },
                f
            ),
            "\x1b[99;5u"
        );
        let ctrl_shift = Mods {
            ctrl: true,
            shift: true,
            ..NONE
        };
        assert_eq!(
            enc(
                KittyKeyEvent {
                    mods: ctrl_shift,
                    consumed: SHIFT,
                    ..letter('c', "C", 'c')
                },
                f
            ),
            "\x1b[99;6u",
            "글자는 소문자 코드"
        );
        // Shift만 붙은 글자는 글 그대로(Shift는 글자를 만드는 데 쓰였다)
        assert_eq!(
            enc(
                KittyKeyEvent {
                    mods: SHIFT,
                    consumed: SHIFT,
                    ..letter('a', "A", 'a')
                },
                f
            ),
            "A"
        );
        assert_eq!(enc(letter('a', "a", 'a'), flags("18")), "\x1b[97u");
        assert_eq!(
            enc(
                KittyKeyEvent {
                    mods: ALT,
                    ..letter('a', "a", 'a')
                },
                f
            ),
            "\x1b[97;3u"
        );
    }
}
