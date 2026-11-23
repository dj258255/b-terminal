//! Ghostty v1.4.0 `src/input/Binding.zig`의 트리거 해석 시험(`parse: triggers` 2885줄, `parse: w3c key names` 2989줄,
//! `parse: plus sign` 3030줄, `parse: equals sign` 3082줄 앞, `parse: text action equals sign` 3082줄,
//! `parse: backwards compatibility with <= 1.1.x` 3120줄, `parse: global triggers` 3158줄 — 모두 같은 입력과 같은 기대)을
//! 우리 해석기(`Keymap::build`·`parse_trigger`)에 옮긴 것(#767).
//!
//! 우리 트리거는 글자 키를 물리 키가 아니라 글자로 맞춘다(ADR-070): Ghostty의 `.physical = .key_a`는 우리 `a`로, `.physical = .digit_0`은
//! 우리 `digit_0`으로, `.unicode = '0'`도 `digit_0`으로 적는다(`0` 글자와 `digit_0`을 한 키로 센다). 그 밖의 기대는 Ghostty 그대로다.

use super::*;

fn mods(s: &str) -> Mods {
    Mods {
        shift: s.contains('s'),
        ctrl: s.contains('c'),
        alt: s.contains('a'),
        sup: s.contains('u'),
    }
}

fn tr(m: &str, key: &str) -> Trigger {
    Trigger {
        mods: mods(m),
        key: key.into(),
    }
}

/// 한 줄만 얹은 키 묶음(기본 묶음은 지운다)
fn build1(line: &str) -> (Keymap, Vec<String>) {
    Keymap::build(&["clear".to_string(), line.to_string()])
}

/// 한 줄이 한 키 묶음이 되면 그 (트리거, 묶음). 경고가 있거나 모양이 다르면 설명
fn one(line: &str) -> Result<(Trigger, Bound), String> {
    let (km, w) = build1(line);
    if let Some(w) = w.first() {
        return Err(format!("경고: {w}"));
    }
    if km.map.len() == 1 && km.seqs.is_empty() && km.global.is_empty() {
        let (t, b) = km.map.iter().next().unwrap();
        return Ok((t.clone(), b.clone()));
    }
    Err(format!(
        "모양 map={} seqs={} global={}",
        km.map.len(),
        km.seqs.len(),
        km.global.len()
    ))
}

fn is_invalid(line: &str) -> bool {
    !build1(line).1.is_empty()
}

/// 기대와 같은 트리거·동작인가
fn is(line: &str, m: &str, key: &str, act: Action) -> bool {
    matches!(one(line), Ok((t, b)) if t == tr(m, key) && b.action == act)
}

fn ign() -> Action {
    Action::Ignore
}

#[test]
fn ghostty_parse_triggers() {
    // single character
    assert!(is("a=ignore", "", "a", ign()), "{:?}", one("a=ignore"));
    // single modifier
    assert!(is("shift+a=ignore", "s", "a", ign()), "{:?}", one("shift+a=ignore"));
    assert!(is("ctrl+a=ignore", "c", "a", ign()), "{:?}", one("ctrl+a=ignore"));
    // multiple modifier
    assert!(
        is("shift+ctrl+a=ignore", "sc", "a", ign()),
        "{:?}",
        one("shift+ctrl+a=ignore")
    );
    // key can come before modifier
    assert!(is("a+shift=ignore", "s", "a", ign()), "{:?}", one("a+shift=ignore"));
    // physical keys(`key_a`): 글자 키는 글자로 맞춘다(ADR-070)
    assert!(
        is("shift+key_a=ignore", "s", "a", ign()),
        "{:?}",
        one("shift+key_a=ignore")
    );
    // unicode keys
    assert!(is("shift+ö=ignore", "s", "ö", ign()), "{:?}", one("shift+ö=ignore"));
    // unconsumed keys
    let (t, b) = one("unconsumed:shift+a=ignore").unwrap();
    assert_eq!(t, tr("s", "a"));
    assert!(b.flags.unconsumed && !b.flags.performable);
    // unconsumed physical keys
    let (t, b) = one("unconsumed:key_a+shift=ignore").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t, tr("s", "a"));
    assert!(b.flags.unconsumed);
    // performable keys
    let (t, b) = one("performable:shift+a=ignore").unwrap();
    assert_eq!(t, tr("s", "a"));
    assert!(b.flags.performable && !b.flags.unconsumed);
    // invalid key
    assert!(is_invalid("foo=ignore"));
    // repeated control
    assert!(is_invalid("shift+shift+a=ignore"));
    // multiple character
    assert!(is_invalid("a+b=ignore"));
}

#[test]
fn ghostty_parse_w3c_key_names() {
    // Exact match
    assert!(is("KeyA=ignore", "", "a", ign()), "{:?}", one("KeyA=ignore"));
    // Case-sensitive
    assert!(is_invalid("Keya=ignore"));
    // 같은 W3C 이름 몇 개(Ghostty `key.zig` `fromW3C`)
    assert!(is("Digit1=ignore", "", "digit_1", ign()), "{:?}", one("Digit1=ignore"));
    assert!(
        is("ArrowUp=ignore", "", "arrow_up", ign()),
        "{:?}",
        one("ArrowUp=ignore")
    );
    assert!(
        is("BracketLeft=ignore", "", "bracket_left", ign()),
        "{:?}",
        one("BracketLeft=ignore")
    );
    assert!(
        is("NumpadEnter=ignore", "", "numpad_enter", ign()),
        "{:?}",
        one("NumpadEnter=ignore")
    );
    assert!(
        is("ShiftLeft=ignore", "", "shift_left", ign()),
        "{:?}",
        one("ShiftLeft=ignore")
    );
    assert!(is("Enter=ignore", "", "enter", ign()), "{:?}", one("Enter=ignore"));
    assert!(is("F13=ignore", "", "f13", ign()), "{:?}", one("F13=ignore"));
}

#[test]
fn ghostty_parse_plus_sign() {
    assert!(is("+=ignore", "", "+", ign()), "{:?}", one("+=ignore"));
    // Modifier
    assert!(is("ctrl++=ignore", "c", "+", ign()), "{:?}", one("ctrl++=ignore"));
    assert!(is_invalid("++=ignore"));
}

#[test]
fn ghostty_parse_equals_sign() {
    assert!(is("==ignore", "", "equal", ign()), "{:?}", one("==ignore"));
    // Modifier
    assert!(is("ctrl+==ignore", "c", "equal", ign()), "{:?}", one("ctrl+==ignore"));
    assert!(is_invalid("=ignore"));
}

#[test]
fn ghostty_parse_text_action_equals_sign() {
    let text = |s: &str| Action::Text(s.into());
    assert!(is("==text:=", "", "equal", text("=")), "{:?}", one("==text:="));
    assert!(
        is("==text:=hello", "", "equal", text("=hello")),
        "{:?}",
        one("==text:=hello")
    );
    assert!(
        is("ctrl+==text:=hello", "c", "equal", text("=hello")),
        "{:?}",
        one("ctrl+==text:=hello")
    );
    assert!(
        is("=+ctrl=text:=hello", "c", "equal", text("=hello")),
        "{:?}",
        one("=+ctrl=text:=hello")
    );
}

#[test]
fn ghostty_parse_backwards_compatibility() {
    // simple, for sanity
    assert!(is("zero=ignore", "", "digit_0", ign()), "{:?}", one("zero=ignore"));
    assert!(
        is("physical:zero=ignore", "", "digit_0", ign()),
        "{:?}",
        one("physical:zero=ignore")
    );
    // duplicates
    assert!(is_invalid("zero+one=ignore"));
    // 1.1.x 이름 표 전체(Ghostty `backwards_compatible_keys`): 이름 → 우리 트리거 키
    let table: &[(&str, &str)] = &[
        ("zero", "digit_0"),
        ("one", "digit_1"),
        ("two", "digit_2"),
        ("three", "digit_3"),
        ("four", "digit_4"),
        ("five", "digit_5"),
        ("six", "digit_6"),
        ("seven", "digit_7"),
        ("eight", "digit_8"),
        ("nine", "digit_9"),
        ("plus", "+"),
        ("apostrophe", "quote"),
        ("grave_accent", "backquote"),
        ("left_bracket", "bracket_left"),
        ("right_bracket", "bracket_right"),
        ("up", "arrow_up"),
        ("down", "arrow_down"),
        ("left", "arrow_left"),
        ("right", "arrow_right"),
        ("kp_0", "numpad_0"),
        ("kp_9", "numpad_9"),
        ("kp_add", "numpad_add"),
        ("kp_subtract", "numpad_subtract"),
        ("kp_multiply", "numpad_multiply"),
        ("kp_divide", "numpad_divide"),
        ("kp_decimal", "numpad_decimal"),
        ("kp_enter", "numpad_enter"),
        ("kp_equal", "numpad_equal"),
        ("kp_separator", "numpad_separator"),
        ("kp_left", "numpad_left"),
        ("kp_right", "numpad_right"),
        ("kp_up", "numpad_up"),
        ("kp_down", "numpad_down"),
        ("kp_page_up", "numpad_page_up"),
        ("kp_page_down", "numpad_page_down"),
        ("kp_home", "numpad_home"),
        ("kp_end", "numpad_end"),
        ("kp_insert", "numpad_insert"),
        ("kp_delete", "numpad_delete"),
        ("kp_begin", "numpad_begin"),
        ("left_shift", "shift_left"),
        ("right_shift", "shift_right"),
        ("left_control", "control_left"),
        ("right_control", "control_right"),
        ("left_alt", "alt_left"),
        ("right_alt", "alt_right"),
        ("left_super", "meta_left"),
        ("right_super", "meta_right"),
    ];
    for (old, new) in table {
        assert!(
            is(&format!("{old}=ignore"), "", new, ign()),
            "{old}: {:?}",
            one(&format!("{old}=ignore"))
        );
        // `physical:` 접두사(zero~nine, apostrophe 등은 물리 키)
        let phys = format!("physical:{old}=ignore");
        let want = match *old {
            "plus" => continue, // `physical:plus`는 Ghostty 표에 없다
            _ => *new,
        };
        assert!(is(&phys, "", want, ign()), "{phys}: {:?}", one(&phys));
    }
}

#[test]
fn ghostty_parse_global_triggers() {
    // global keys
    let (km, w) = build1("global:shift+a=ignore");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(km.global, vec![(tr("s", "a"), Action::Ignore)]);
    // global physical keys
    let (km, w) = build1("global:key_a+shift=ignore");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(km.global, vec![(tr("s", "a"), Action::Ignore)]);
    // global unconsumed keys(받기만 한다)
    let (km, w) = build1("unconsumed:global:a+shift=ignore");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(km.global, vec![(tr("s", "a"), Action::Ignore)]);
    // global sequences not allowed
    assert!(is_invalid("global:a>b=ignore"));
}

/// 접두사와 수식 키를 겹쳐 쓰면 Ghostty는 오류다(`parseFlags`, `Trigger.parse`)
#[test]
fn ghostty_rejects_repeated_prefixes() {
    for line in [
        "all:all:a=ignore",
        "global:global:a=ignore",
        "unconsumed:unconsumed:a=ignore",
        "performable:performable:a=ignore",
    ] {
        assert!(is_invalid(line), "{line}");
    }
    // 겹치지 않는 접두사는 그대로 된다
    assert!(!is_invalid("all:unconsumed:a=ignore"));
}

/// Ghostty는 수식 키 이름의 대소문자와 `+` 둘레의 공백을 가린다(`SHIFT`, ` shift `는 키 이름이 될 수 없어 오류)
#[test]
fn ghostty_is_strict_about_case_and_spaces() {
    assert!(is_invalid("SHIFT+a=ignore"));
    assert!(is_invalid("shift + a=ignore"));
    assert!(is_invalid("Shift+a=ignore"));
}

/// 유니코드 글자 키는 소문자로 맞춘다(Ghostty `foldedCodepoint`): 눌림 `ö`와 묶음 `ctrl+Ö`가 맞는다
#[test]
fn unicode_keys_fold_to_lowercase() {
    assert_eq!(parse_trigger("ctrl+Ö").unwrap(), tr("c", "ö"));
    assert_eq!(normalize_key("Ö"), "ö");
    assert_eq!(normalize_key("A"), "a");
}

/// 눌러서 만들 수 없는 이름도 읽고, 모르는 이름은 경고한다
#[test]
fn key_names_are_checked() {
    for ok in [
        "f13",
        "f25",
        "caps_lock",
        "print_screen",
        "pause",
        "context_menu",
        "media_play_pause",
        "numpad_5",
        "kp_enter",
    ] {
        assert!(!is_invalid(&format!("{ok}=ignore")), "{ok}");
    }
    for bad in ["f26", "foo", "Keya", "arrowup", "a1"] {
        assert!(is_invalid(&format!("{bad}=ignore")), "{bad}");
    }
}

/// 미국 배열에서 Cmd+= 와 Cmd+Shift+=(글꼴 키우기)가 기본 묶음으로 계속 듣는다. 키 사건은 수식 키를 뺀 글자 `=`와 Shift로 온다
#[cfg(target_os = "macos")]
#[test]
fn default_font_size_keys_still_fire_on_us_layout() {
    let km = Keymap::build(&[]).0;
    let up = Action::FontBigger(1.0);
    assert_eq!(
        km.lookup(&Trigger {
            mods: mods("u"),
            key: normalize_key("=")
        }),
        Some(&up)
    );
    assert_eq!(
        km.lookup(&Trigger {
            mods: mods("us"),
            key: normalize_key("=")
        }),
        Some(&up)
    );
    // `+`가 그대로 오는 자판(독일어 등)
    assert_eq!(
        km.lookup(&Trigger {
            mods: mods("u"),
            key: normalize_key("+")
        }),
        Some(&up)
    );
}

/// 눌러서 만들 수 있는 이름: winit이 주는 이름 있는 키가 Ghostty 이름으로 간다
#[test]
fn pressed_named_keys_reach_ghostty_names() {
    use winit::keyboard::{Key, ModifiersState, NamedKey as W};
    for (k, name) in [
        (W::F13, "f13"),
        (W::F25, "f25"),
        (W::CapsLock, "caps_lock"),
        (W::PrintScreen, "print_screen"),
        (W::Pause, "pause"),
        (W::ContextMenu, "context_menu"),
        (W::MediaPlayPause, "media_play_pause"),
        (W::AudioVolumeUp, "audio_volume_up"),
    ] {
        let t = crate::keys::trigger_of(&Key::Named(k), ModifiersState::empty()).unwrap();
        assert_eq!(t.key, name);
        // 설정에 쓴 같은 이름이 그 키에 맞는다
        let km = build1(&format!("{name}=ignore")).0;
        assert_eq!(km.lookup(&t), Some(&Action::Ignore), "{name}");
    }
    // 글자: 눌림 `Ö`·`+`·`=`
    let t = |s: &str| crate::keys::trigger_of(&Key::Character(s.into()), ModifiersState::CONTROL).unwrap();
    assert_eq!(t("Ö"), tr("c", "ö"));
    assert_eq!(t("+"), tr("c", "+"));
    assert_eq!(t("="), tr("c", "equal"));
}

/// 숫자 패드 키는 물리 이름(`kp_1`)으로도 맞춘다. 물리 묶음이 있으면 그것이, 없으면 글자 묶음이 듣는다
#[test]
fn numpad_keys_match_physical_names_and_letters() {
    use winit::keyboard::{KeyCode, PhysicalKey};
    let alt = crate::keys::numpad_trigger(PhysicalKey::Code(KeyCode::Numpad1), Mods::default());
    assert_eq!(alt, Some(tr("", "numpad_1")));
    assert_eq!(
        crate::keys::numpad_trigger(PhysicalKey::Code(KeyCode::Digit1), Mods::default()),
        None
    );
    let press = |km: &Keymap| {
        let mut sq = Sequencer::default();
        sq.feed_with_alt(km, Some(tr("", "digit_1")), alt.clone(), b"1".to_vec(), &mut |_| true)
    };
    // 물리 묶음만 있다
    assert_eq!(
        press(&build1("kp_1=text:phys").0).action,
        Some(Action::Text("phys".into()))
    );
    // 글자 묶음만 있다
    assert_eq!(
        press(&build1("1=text:char").0).action,
        Some(Action::Text("char".into()))
    );
    // 둘 다 있으면 물리 이름이 먼저다
    let (km, w) = Keymap::build(&["clear".into(), "1=text:char".into(), "kp_1=text:phys".into()]);
    assert!(w.is_empty());
    assert_eq!(press(&km).action, Some(Action::Text("phys".into())));
    // 숫자 패드가 아닌 `1`은 물리 묶음에 맞지 않는다
    let mut sq = Sequencer::default();
    let out = sq.feed_with_alt(
        &build1("kp_1=text:phys").0,
        Some(tr("", "digit_1")),
        None,
        b"1".to_vec(),
        &mut |_| true,
    );
    assert_eq!(out.action, None);
    assert_eq!(out.send, b"1");
}
