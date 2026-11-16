//! Ghostty `src/input/function_keys.zig`(main 7fa26e8b675f)의 PC식 함수 키 표 전체를 앱 입구(`encode_legacy`)로 견준다(#571).
//! 기대값은 `keys_pc_table_data.rs`(표를 읽는 스크립트가 만든 자료)이고 손으로 고른 칸이 아니다.
//! 조합: 키 전부 × 수식 16가지(없음 + `modifiers` 표 15) × DECCKM × DECKPAM × 1035 × modifyOtherKeys 2 × DECBKM.
//! 수식 키는 macOS 앱이 넘기는 대로 Option도 Alt, Cmd도 수식(`mods.binding()`)으로 센다.
//!
//! 견주지 않은 키: `numpad_begin`(winit에 대응하는 이름 키가 없고 macOS 키패드 Clear는 Ghostty에서도 다른 키다).
//! `numpad_up` 등 키패드 이동 키는 표가 일반 이동 키와 같아 winit의 이름 키를 키패드 위치로 불러 견준다.

use super::keys_pc_table_data::PC_ROWS;
use super::*;
use std::collections::BTreeMap;
use winit::keyboard::SmolStr;

/// 영향 있는 모드 번호들과 (모드 비트, 수식) -> 기대 바이트
type KeyRows = (Vec<usize>, BTreeMap<(u8, u8), &'static [u8]>);

const FLAG_NAMES: [&str; 5] = ["cursor", "keypad", "ignore", "mok", "decbkm"];

fn mods_of(bits: u8) -> ModifiersState {
    let mut m = ModifiersState::empty();
    if bits & 1 != 0 {
        m |= ModifiersState::SHIFT;
    }
    if bits & 2 != 0 {
        m |= ModifiersState::CONTROL;
    }
    if bits & 4 != 0 {
        m |= ModifiersState::ALT;
    }
    if bits & 8 != 0 {
        m |= ModifiersState::SUPER;
    }
    m
}

/// 표의 키 이름을 winit이 주는 (논리 키, 위치, 글)로. 없으면 견주지 않는 키
fn winit_key(name: &str) -> Option<(Key, KeyLocation, Option<&'static str>)> {
    let std = KeyLocation::Standard;
    let pad = KeyLocation::Numpad;
    let named = |n: WKey| Some((Key::Named(n), std, None));
    let pad_named = |n: WKey| Some((Key::Named(n), pad, None));
    let pad_char = |s: &'static str| Some((Key::Character(SmolStr::new(s)), pad, Some(s)));
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return named(match n {
            1 => WKey::F1,
            2 => WKey::F2,
            3 => WKey::F3,
            4 => WKey::F4,
            5 => WKey::F5,
            6 => WKey::F6,
            7 => WKey::F7,
            8 => WKey::F8,
            9 => WKey::F9,
            10 => WKey::F10,
            11 => WKey::F11,
            12 => WKey::F12,
            13 => WKey::F13,
            14 => WKey::F14,
            15 => WKey::F15,
            16 => WKey::F16,
            17 => WKey::F17,
            18 => WKey::F18,
            19 => WKey::F19,
            20 => WKey::F20,
            21 => WKey::F21,
            22 => WKey::F22,
            23 => WKey::F23,
            24 => WKey::F24,
            25 => WKey::F25,
            _ => return None,
        });
    }
    match name {
        "arrow_up" => named(WKey::ArrowUp),
        "arrow_down" => named(WKey::ArrowDown),
        "arrow_left" => named(WKey::ArrowLeft),
        "arrow_right" => named(WKey::ArrowRight),
        "home" => named(WKey::Home),
        "end" => named(WKey::End),
        "insert" => named(WKey::Insert),
        "delete" => named(WKey::Delete),
        "page_up" => named(WKey::PageUp),
        "page_down" => named(WKey::PageDown),
        "help" => named(WKey::Help),
        "context_menu" => named(WKey::ContextMenu),
        "backspace" => named(WKey::Backspace),
        "tab" => named(WKey::Tab),
        "enter" => named(WKey::Enter),
        "escape" => named(WKey::Escape),
        "numpad_up" => pad_named(WKey::ArrowUp),
        "numpad_down" => pad_named(WKey::ArrowDown),
        "numpad_left" => pad_named(WKey::ArrowLeft),
        "numpad_right" => pad_named(WKey::ArrowRight),
        "numpad_home" => pad_named(WKey::Home),
        "numpad_end" => pad_named(WKey::End),
        "numpad_insert" => pad_named(WKey::Insert),
        "numpad_delete" => pad_named(WKey::Delete),
        "numpad_page_up" => pad_named(WKey::PageUp),
        "numpad_page_down" => pad_named(WKey::PageDown),
        "numpad_enter" => pad_named(WKey::Enter),
        "numpad_0" => pad_char("0"),
        "numpad_1" => pad_char("1"),
        "numpad_2" => pad_char("2"),
        "numpad_3" => pad_char("3"),
        "numpad_4" => pad_char("4"),
        "numpad_5" => pad_char("5"),
        "numpad_6" => pad_char("6"),
        "numpad_7" => pad_char("7"),
        "numpad_8" => pad_char("8"),
        "numpad_9" => pad_char("9"),
        "numpad_decimal" => pad_char("."),
        "numpad_divide" => pad_char("/"),
        "numpad_multiply" => pad_char("*"),
        "numpad_subtract" => pad_char("-"),
        "numpad_add" => pad_char("+"),
        _ => None,
    }
}

fn run_key(
    key: &(Key, KeyLocation, Option<&'static str>),
    mods: ModifiersState,
    flags: u8,
    option_as_alt: bool,
    text: Option<&str>,
) -> Vec<u8> {
    let k = LegacyKey {
        logical: &key.0,
        location: key.1,
        physical: None,
        text: text.or(key.2),
        unshifted: key.2.and_then(|s| s.chars().next()).map_or(0, |c| c as u32),
        mods,
        raw: mods,
    };
    let modes = bt_term::Modes {
        app_cursor: flags & 1 != 0,
        app_keypad: flags & 2 != 0,
        modify_other_keys: flags & 8 != 0,
        key: KeyModes {
            backarrow_key: flags & 16 != 0,
            ignore_keypad_with_numlock: flags & 4 != 0,
            alt_esc_prefix: true,
            alt_sends_escape: false,
        },
        ..bt_term::Modes::default()
    };
    encode_legacy(&k, modes, option_as_alt)
}

fn show(b: &[u8]) -> String {
    b.iter()
        .map(|&c| match c {
            0x1b => "ESC ".to_string(),
            0x20..=0x7e => (c as char).to_string(),
            _ => format!("\\x{c:02x}"),
        })
        .collect()
}

#[test]
fn pc_style_function_key_table_matches_ghostty() {
    // 키 이름 -> (영향 있는 모드 이름, (모드 비트, 수식) -> 기대 바이트)
    let mut table: BTreeMap<&str, KeyRows> = BTreeMap::new();
    for &(name, rel, bits, mods, seq) in PC_ROWS {
        let e = table.entry(name).or_insert_with(|| {
            let idx = rel
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|n| FLAG_NAMES.iter().position(|f| f == &n).unwrap())
                .collect();
            (idx, BTreeMap::new())
        });
        e.1.insert((bits, mods), seq);
    }
    let (mut checked, mut skipped, mut bad) = (0usize, Vec::new(), Vec::new());
    for (name, (rel, rows)) in &table {
        let Some(key) = winit_key(name) else {
            skipped.push(*name);
            continue;
        };
        let mask: u8 = rel.iter().map(|i| 1u8 << i).sum();
        for mods in 0..16u8 {
            for flags in 0..32u8 {
                let want = rows[&(flags & mask, mods)];
                // 이름 키에서 Option은 option-as-alt와 상관없이 Alt다. 둘 다 같아야 한다
                let a = run_key(&key, mods_of(mods), flags, true, None);
                let b = run_key(&key, mods_of(mods), flags, false, None);
                checked += 1;
                if a != want || b != want {
                    bad.push(format!(
                        "{name} mods={mods:#06b} flags={flags:#07b}({}) want [{}] got [{}]/[{}]",
                        FLAG_NAMES
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| flags & (1 << i) != 0)
                            .map(|(_, n)| *n)
                            .collect::<Vec<_>>()
                            .join("+"),
                        show(want),
                        show(&a),
                        show(&b)
                    ));
                }
            }
        }
    }
    assert_eq!(skipped, ["numpad_begin"], "견주지 않는 키는 이것뿐이다");
    let mut per_key: BTreeMap<&str, usize> = BTreeMap::new();
    for b in &bad {
        *per_key.entry(b.split(' ').next().unwrap()).or_default() += 1;
    }
    assert!(
        bad.is_empty(),
        "{checked}칸 중 다른 칸 {}개, 키별 {per_key:?}. 앞 60개:\n{}",
        bad.len(),
        bad.iter().take(60).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// Ghostty는 Enter·Escape·Backspace에 글이 있어도 제어 글자(`\r`·ESC·DEL)면 PC식 칸을 쓴다(key_encode.zig 357~380줄, `isControlUtf8`).
/// macOS 앱이 넘기는 글은 제어 글자 그대로(Control만 뺀 글)이고 winit도 같은 글을 주므로, 글이 있든 없든 바이트가 같아야 한다
#[test]
fn enter_escape_backspace_ignore_control_text_like_ghostty() {
    for (name, text) in [("enter", "\r"), ("escape", "\u{1b}"), ("backspace", "\u{7f}")] {
        let key = winit_key(name).unwrap();
        for mods in 0..16u8 {
            for flags in [0u8, 8, 16, 24] {
                assert_eq!(
                    run_key(&key, mods_of(mods), flags, true, Some(text)),
                    run_key(&key, mods_of(mods), flags, true, None),
                    "{name} mods={mods} flags={flags}"
                );
            }
        }
    }
}

fn bytes(name: &str, mods: u8, flags: u8) -> Vec<u8> {
    run_key(&winit_key(name).unwrap(), mods_of(mods), flags, true, None)
}

const SHIFT_B: u8 = 1;
const CTRL_B: u8 = 2;
const ALT_B: u8 = 4;
const SUPER_B: u8 = 8;

// 원인 1: Enter·Tab·Escape의 수식 키 칸(function_keys.zig "Everything else"). 기본 모드(modifyOtherKeys 꺼짐)에서도 CSI 27이다
#[test]
fn enter_tab_escape_with_modifiers_use_csi_27_in_default_mode() {
    assert_eq!(bytes("enter", SHIFT_B, 0), b"\x1b[27;2;13~");
    assert_eq!(bytes("enter", CTRL_B, 0), b"\x1b[27;5;13~");
    assert_eq!(bytes("enter", CTRL_B | ALT_B, 0), b"\x1b[27;7;13~");
    assert_eq!(bytes("enter", ALT_B, 0), b"\x1b\r", "Alt 혼자는 옛 꼴 그대로");
    assert_eq!(bytes("tab", CTRL_B, 0), b"\x1b[27;5;9~");
    assert_eq!(bytes("tab", SHIFT_B, 0), b"\x1b[Z", "Shift+Tab은 옛 꼴 그대로");
    assert_eq!(bytes("tab", ALT_B, 0), b"\x1b\t");
    assert_eq!(bytes("escape", SHIFT_B, 0), b"\x1b[27;2;27~");
    assert_eq!(bytes("escape", CTRL_B, 0), b"\x1b[27;5;27~");
    assert_eq!(bytes("escape", ALT_B, 0), b"\x1b\x1b");
}

// 원인 2: Cmd(Super)도 PC식 함수 키 표의 수식 키다(`mods.binding()`, 수식 번호 9~16)
#[test]
fn command_counts_as_modifier_for_named_keys() {
    assert_eq!(bytes("f1", SUPER_B, 0), b"\x1b[1;9P");
    assert_eq!(bytes("arrow_up", SUPER_B, 0), b"\x1b[1;9A");
    assert_eq!(bytes("arrow_up", SUPER_B | SHIFT_B, 0), b"\x1b[1;10A");
    assert_eq!(
        bytes("arrow_up", SUPER_B, 1),
        b"\x1b[1;9A",
        "커서 응용 모드여도 수식 키가 있으면 CSI"
    );
    assert_eq!(bytes("delete", SUPER_B, 0), b"\x1b[3;9~");
    assert_eq!(bytes("enter", SUPER_B, 0), b"\x1b[27;9;13~");
    assert_eq!(bytes("backspace", SUPER_B, 0), b"\x7f");
}

// 원인 3: DECBKM은 표의 마지막 두 칸(수식 키 없음, Ctrl 혼자)에만 있다
#[test]
fn decbkm_swaps_only_plain_and_ctrl_backspace() {
    const DECBKM: u8 = 16;
    assert_eq!(bytes("backspace", 0, DECBKM), b"\x08");
    assert_eq!(bytes("backspace", CTRL_B, DECBKM), b"\x7f");
    assert_eq!(bytes("backspace", SHIFT_B, DECBKM), b"\x7f");
    assert_eq!(bytes("backspace", ALT_B, DECBKM), b"\x1b\x7f");
    assert_eq!(bytes("backspace", CTRL_B | SHIFT_B, DECBKM), b"\x08");
    assert_eq!(bytes("backspace", CTRL_B | ALT_B | SHIFT_B, DECBKM), b"\x1b\x08");
}

// 원인 4: 키패드 키는 응용 모드가 아니어도 키패드 칸(숫자 모드는 수식 키와 상관없이 그 글자)을 쓴다
#[test]
fn keypad_keys_ignore_modifiers_in_numeric_mode_by_default() {
    assert_eq!(bytes("numpad_2", CTRL_B, 0), b"2", "Ctrl+키패드 2는 NUL이 아니다");
    assert_eq!(
        bytes("numpad_enter", SHIFT_B, 0),
        b"\r",
        "키패드 Enter는 CSI 27로 바뀌지 않는다"
    );
    assert_eq!(bytes("numpad_enter", CTRL_B, 0), b"\r");
    assert_eq!(bytes("numpad_0", SUPER_B, 0), b"0");
}
