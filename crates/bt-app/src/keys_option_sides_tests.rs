//! `macos-option-as-alt = left|right`가 글자 키에서 좌우를 가르는지(#572). Ghostty `key_encode.zig`(main 7fa26e8b675f)의
//! 글자 키 앞 ESC(566~569줄), modifyOtherKeys 2의 Alt 수식(403~407줄), kitty 연결 글자(299~307줄)가 모두 같은 판정을 쓴다.
//! 앱은 눌린 Option의 좌우(`sided_mods`)와 설정으로 `option_counts_as_alt`를 구해 인코더에 넘기므로 그 길 그대로 시험한다.

use super::*;
use bt_core::config::OptionAsAlt as Setting;
use bt_term::{KittyFlags, KittyKeyEvent};
use winit::keyboard::SmolStr;

fn option(side: ModSide) -> SidedMods {
    SidedMods::default().with(ModKind::Alt, side)
}

/// winit이 주는 모양: Option이 Alt로 쓰이면 글자는 `b` 그대로, 아니면 macOS가 만든 `∫`(수식 키 없는 글자는 `b`)
fn legacy(setting: Setting, side: ModSide, modify_other: bool) -> Vec<u8> {
    let sided = option(side);
    let as_alt = option_counts_as_alt(setting, sided);
    let text = if as_alt { "b" } else { "∫" };
    let logical = Key::Character(SmolStr::new(text));
    let k = LegacyKey {
        logical: &logical,
        location: KeyLocation::Standard,
        physical: Some(KeyCode::KeyB),
        text: Some(text),
        unshifted: 'b' as u32,
        mods: to_state(sided),
        raw: to_state(sided),
    };
    let modes = bt_term::Modes {
        modify_other_keys: modify_other,
        ..bt_term::Modes::default()
    };
    encode_legacy(&k, modes, as_alt)
}

/// kitty 연결 글자(플래그 16과 8): Option이 글을 막는지는 `option_gate`가 정한다
fn kitty(setting: Setting, side: ModSide) -> Vec<u8> {
    let sided = option(side);
    let alt_prevents_text = option_gate(option_counts_as_alt(setting, sided), to_state(sided));
    let ev = KittyKeyEvent {
        action: KeyAction::Press,
        key: KittyKey::Text,
        mods: TermMods {
            alt: true,
            ..TermMods::default()
        },
        caps_lock: false,
        num_lock: false,
        consumed: TermMods {
            shift: true,
            alt: !alt_prevents_text,
            ..TermMods::default()
        },
        composing: false,
        utf8: "∫",
        unshifted: 'b' as u32,
        base: Some('b' as u32),
        alt_prevents_text,
    };
    encode_kitty_key(
        &ev,
        KittyFlags {
            disambiguate: true,
            all_as_escapes: true,
            report_associated: true,
            ..KittyFlags::default()
        },
    )
}

const SETTINGS: [(Setting, bool, bool); 4] = [
    // (설정, 왼쪽 Option이 Alt인가, 오른쪽 Option이 Alt인가)
    (Setting::None, false, false),
    (Setting::Left, true, false),
    (Setting::Right, false, true),
    (Setting::Both, true, true),
];

#[test]
fn option_counts_as_alt_follows_side() {
    for (setting, left, right) in SETTINGS {
        assert_eq!(
            option_counts_as_alt(setting, option(ModSide::Left)),
            left,
            "{setting:?} 왼쪽"
        );
        assert_eq!(
            option_counts_as_alt(setting, option(ModSide::Right)),
            right,
            "{setting:?} 오른쪽"
        );
    }
    // 양쪽을 다 누르면 오른쪽으로 센다(Ghostty macOS 앱)
    let both = option(ModSide::Left).with(ModKind::Alt, ModSide::Right);
    assert!(!option_counts_as_alt(Setting::Left, both));
    assert!(option_counts_as_alt(Setting::Right, both));
}

#[test]
fn letter_key_prefix_esc_only_for_the_chosen_option() {
    for (setting, left, right) in SETTINGS {
        for (side, alt) in [(ModSide::Left, left), (ModSide::Right, right)] {
            let want: &[u8] = if alt { b"\x1bb" } else { "∫".as_bytes() };
            assert_eq!(legacy(setting, side, false), want, "{setting:?} {side:?}");
        }
    }
}

#[test]
fn modify_other_keys_2_alt_only_for_the_chosen_option() {
    for (setting, left, right) in SETTINGS {
        for (side, alt) in [(ModSide::Left, left), (ModSide::Right, right)] {
            let want: &[u8] = if alt { b"\x1b[27;3;98~" } else { "∫".as_bytes() };
            assert_eq!(legacy(setting, side, true), want, "{setting:?} {side:?}");
        }
    }
}

#[test]
fn kitty_associated_text_kept_only_when_option_is_not_alt() {
    for (setting, left, right) in SETTINGS {
        for (side, alt) in [(ModSide::Left, left), (ModSide::Right, right)] {
            let out = String::from_utf8(kitty(setting, side)).unwrap();
            // Alt로 쓰이는 Option은 글을 막는다(연결 글자 없음). 아니면 `;3;8747`처럼 `∫`(U+222B)의 코드가 붙는다
            assert_eq!(out.contains("8747"), !alt, "{setting:?} {side:?}: {out:?}");
        }
    }
}
