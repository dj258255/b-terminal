//! 키패드 응용 모드(DECKPAM·1035)와 modifyOtherKeys(`CSI > 4 ; 2 m`, #312): 모드 상태와 바이트.
//! 기대 바이트는 Ghostty `function_keys.zig`·`key_encode.zig` 테스트 값을 그대로 옮긴 것이다. PTY는 `/bin/sh`와 `printf`만 쓴다

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{
    Emulator, GridSize, KeyModes, KeypadKey, Mods, NamedKey, Session, SessionEvent, SpawnOptions, encode_keypad,
    encode_modify_other_char, encode_named_modify_other, keypad_application,
};

fn emu() -> Emulator {
    Emulator::new(GridSize::new(20, 5), Theme::default())
}

fn mods(shift: bool, ctrl: bool, alt: bool, sup: bool) -> Mods {
    Mods { shift, ctrl, alt, sup }
}

fn none() -> Mods {
    Mods::default()
}

fn ctrl() -> Mods {
    mods(false, true, false, false)
}

fn alt() -> Mods {
    mods(false, false, true, false)
}

fn shift() -> Mods {
    mods(true, false, false, false)
}

fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

fn mok(e: &mut Emulator) -> bool {
    e.snapshot().modes.modify_other_keys
}

#[test]
fn modify_other_keys_is_off_by_default() {
    assert!(!mok(&mut emu()));
}

#[test]
fn csi_gt_4_2_m_turns_state_2_on_and_others_turn_it_off() {
    // Ghostty stream.zig 2323~2360줄·stream_handler.zig setModifyKeyFormat: `4 ; 2`만 켠다
    let mut e = emu();
    e.feed(b"\x1b[>4;2m");
    assert!(mok(&mut e), "CSI > 4 ; 2 m");
    for off in [
        &b"\x1b[>4;0m"[..],
        b"\x1b[>4;1m",
        b"\x1b[>4m",
        b"\x1b[>m",
        b"\x1b[>0m",
        b"\x1b[>1m",
        b"\x1b[>2m",
        b"\x1b[>4n",
    ] {
        e.feed(b"\x1b[>4;2m");
        assert!(mok(&mut e));
        e.feed(off);
        assert!(!mok(&mut e), "{:?}는 끈다", String::from_utf8_lossy(off));
    }
    // 지원하지 않는 첫 매개변수와 세 매개변수 이상은 버린다(상태 그대로)
    e.feed(b"\x1b[>4;2m\x1b[>3m");
    assert!(mok(&mut e), "첫 매개변수 3은 버린다");
    e.feed(b"\x1b[>4;2;1m");
    assert!(mok(&mut e), "매개변수 셋은 버린다");
}

#[test]
fn ris_and_decstr_turn_modify_other_keys_off() {
    let mut e = emu();
    e.feed(b"\x1b[>4;2m\x1b[!p");
    assert!(!mok(&mut e), "DECSTR(Ghostty softReset)");
    e.feed(b"\x1b[>4;2m\x1bc");
    assert!(!mok(&mut e), "RIS");
}

#[test]
fn deckpam_and_1035_decide_keypad_application() {
    let mut e = emu();
    let m = e.snapshot().modes;
    assert!(!m.app_keypad && m.key.ignore_keypad_with_numlock);
    e.feed(b"\x1b=");
    let m = e.snapshot().modes;
    assert!(m.app_keypad);
    assert!(
        !keypad_application(m.app_keypad, m.key),
        "1035 기본 켬이라 DECKPAM만으로는 숫자 키패드"
    );
    e.feed(b"\x1b[?1035l");
    let m = e.snapshot().modes;
    assert!(keypad_application(m.app_keypad, m.key), "1035를 끄면 응용 모드");
    e.feed(b"\x1b>");
    let m = e.snapshot().modes;
    assert!(!keypad_application(m.app_keypad, m.key), "DECKPNM");
    let off = KeyModes {
        ignore_keypad_with_numlock: false,
        ..KeyModes::default()
    };
    assert!(!keypad_application(false, off));
}

#[test]
fn keypad_table_matches_ghostty_function_keys() {
    // function_keys.zig 121~145줄: (키, 응용 모드 끝글자, 숫자 모드 글자)
    let table = [
        (KeypadKey::Digit(0), "p", "0"),
        (KeypadKey::Digit(1), "q", "1"),
        (KeypadKey::Digit(2), "r", "2"),
        (KeypadKey::Digit(3), "s", "3"),
        (KeypadKey::Digit(4), "t", "4"),
        (KeypadKey::Digit(5), "u", "5"),
        (KeypadKey::Digit(6), "v", "6"),
        (KeypadKey::Digit(7), "w", "7"),
        (KeypadKey::Digit(8), "x", "8"),
        (KeypadKey::Digit(9), "y", "9"),
        (KeypadKey::Decimal, "n", "."),
        (KeypadKey::Divide, "o", "/"),
        (KeypadKey::Multiply, "j", "*"),
        (KeypadKey::Subtract, "m", "-"),
        (KeypadKey::Add, "k", "+"),
        (KeypadKey::Enter, "M", "\r"),
    ];
    for (k, suffix, normal) in table {
        assert_eq!(
            s(encode_keypad(k, none(), true)),
            format!("\x1bO{suffix}"),
            "{k:?} 응용"
        );
        assert_eq!(s(encode_keypad(k, none(), false)), normal, "{k:?} 숫자");
        // 숫자 모드는 수식 키와 상관없이 그 글자(Ghostty 테스트 "keypad 1 with modify other state 2")
        assert_eq!(s(encode_keypad(k, ctrl(), false)), normal, "{k:?} 숫자+Ctrl");
        // 응용 모드의 수식 키: 번호 = 1 + shift + 2 alt + 4 ctrl + 8 super
        assert_eq!(s(encode_keypad(k, shift(), true)), format!("\x1bO2{suffix}"));
        assert_eq!(s(encode_keypad(k, alt(), true)), format!("\x1bO3{suffix}"));
        assert_eq!(s(encode_keypad(k, ctrl(), true)), format!("\x1bO5{suffix}"));
        assert_eq!(
            s(encode_keypad(k, mods(true, true, true, true), true)),
            format!("\x1bO16{suffix}")
        );
        assert_eq!(
            s(encode_keypad(k, mods(false, false, false, true), true)),
            format!("\x1bO9{suffix}")
        );
    }
    // key_encode.zig 2502~2529줄: Ctrl+numpad 1 + 응용 모드
    assert_eq!(s(encode_keypad(KeypadKey::Digit(1), ctrl(), true)), "\x1bO5q");
    // 2479줄: keypad enter 숫자 모드
    assert_eq!(encode_keypad(KeypadKey::Enter, none(), false), b"\r");
}

#[test]
fn modify_other_char_matches_ghostty_tests() {
    // key_encode.zig 2329~2392줄
    let c = |ch, m| encode_modify_other_char(ch, m).map(s);
    assert_eq!(c('H', mods(true, true, false, false)).as_deref(), Some("\x1b[27;6;72~"));
    assert_eq!(c('p', ctrl()).as_deref(), Some("\x1b[27;5;112~"));
    assert_eq!(c('8', alt()).as_deref(), Some("\x1b[27;3;56~"));
    // 수식 키가 없으면 보내지 않는다(그대로 글자)
    assert_eq!(c('p', none()), None);
    // Shift만: 0x40~0x7F(xterm IsControlInput)와 공백만. 나머지는 옛 꼴
    assert_eq!(c('A', shift()).as_deref(), Some("\x1b[27;2;65~"));
    assert_eq!(c(' ', shift()).as_deref(), Some("\x1b[27;2;32~"));
    assert_eq!(c('!', shift()), None);
    assert_eq!(c('가', shift()), None);
    // Shift 말고 다른 수식 키가 있으면 어떤 글자든
    assert_eq!(c('1', ctrl()).as_deref(), Some("\x1b[27;5;49~"));
    assert_eq!(c('가', alt()).as_deref(), Some("\x1b[27;3;44032~"));
    assert_eq!(
        c('x', mods(false, false, false, true)).as_deref(),
        Some("\x1b[27;9;120~")
    );
    assert_eq!(c('x', mods(true, true, true, true)).as_deref(), Some("\x1b[27;16;120~"));
}

#[test]
fn modify_other_named_keys_follow_function_keys_table() {
    let some = |k, m| encode_named_modify_other(k, m).map(s);
    // Enter: 수식 키가 있으면 모두 `27;번호;13`
    assert_eq!(some(NamedKey::Enter, shift()).as_deref(), Some("\x1b[27;2;13~"));
    assert_eq!(some(NamedKey::Enter, alt()).as_deref(), Some("\x1b[27;3;13~"));
    assert_eq!(some(NamedKey::Enter, ctrl()).as_deref(), Some("\x1b[27;5;13~"));
    assert_eq!(some(NamedKey::Enter, none()), None);
    // Tab: Shift도 `27;2;9`(상태 2의 set_other 항목)
    assert_eq!(some(NamedKey::Tab, shift()).as_deref(), Some("\x1b[27;2;9~"));
    assert_eq!(some(NamedKey::Tab, alt()).as_deref(), Some("\x1b[27;3;9~"));
    assert_eq!(some(NamedKey::Tab, ctrl()).as_deref(), Some("\x1b[27;5;9~"));
    // Escape
    assert_eq!(some(NamedKey::Escape, alt()).as_deref(), Some("\x1b[27;3;27~"));
    assert_eq!(some(NamedKey::Escape, shift()).as_deref(), Some("\x1b[27;2;27~"));
    // Backspace: Ctrl만 옛 꼴(0x08), 나머지는 `27;번호;127`
    assert_eq!(some(NamedKey::Backspace, ctrl()), None);
    assert_eq!(some(NamedKey::Backspace, shift()).as_deref(), Some("\x1b[27;2;127~"));
    assert_eq!(some(NamedKey::Backspace, alt()).as_deref(), Some("\x1b[27;3;127~"));
    assert_eq!(
        some(NamedKey::Backspace, mods(true, true, false, false)).as_deref(),
        Some("\x1b[27;6;127~")
    );
    assert_eq!(
        some(NamedKey::Backspace, mods(false, false, false, true)).as_deref(),
        Some("\x1b[27;9;127~")
    );
    // 다른 키는 상관없다
    assert_eq!(some(NamedKey::Up, ctrl()), None);
}

fn run_sh(script: String, size: GridSize) -> Session {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let sess = Session::spawn(o, size, Theme::default(), sink).expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            break;
        }
    }
    sess
}

#[test]
fn xtqmodkeys_gets_no_reply_like_ghostty() {
    // Ghostty stream.zig는 `CSI ? 4 m`을 무시한다(답하지 않는다)
    let script = "stty raw -echo min 0 time 5; printf '\\033[>4;2m\\033[?4m'; sleep 0.3; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
        .to_string();
    let sess = run_sh(script, GridSize::new(80, 6));
    let text = sess.snapshot().text();
    assert!(text.contains("R[]"), "답이 오면 안 된다:\n{text}");
}

#[test]
fn session_modes_follow_the_pty_output() {
    let sess = run_sh(r"printf '\033[>4;2m\033=\033[?1035l'".to_string(), GridSize::new(40, 4));
    let m = sess.snapshot().modes;
    assert!(m.modify_other_keys && m.app_keypad && !m.key.ignore_keypad_with_numlock);
}
