//! 키 입력 모드(DECBKM 67, 1035, 1036, 1039, #301): 모드 상태와 DECRQM 답. PTY는 `/bin/sh`와 `printf`만 쓴다

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{Emulator, GridSize, Session, SessionEvent, SpawnOptions};

fn emu() -> Emulator {
    Emulator::new(GridSize::new(20, 5), Theme::default())
}

fn reply_to(query: &str) -> String {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let script = format!(
        "stty raw -echo min 0 time 5; printf '{query}'; sleep 0.3; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    );
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let s = Session::spawn(o, GridSize::new(80, 6), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            break;
        }
    }
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    let hex = &text[start..end];
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn defaults_match_ghostty_modes() {
    let k = emu().snapshot().modes.key;
    assert!(!k.backarrow_key, "67 기본 꺼짐");
    assert!(k.ignore_keypad_with_numlock, "1035 기본 켬");
    assert!(k.alt_esc_prefix, "1036 기본 켬");
    assert!(!k.alt_sends_escape, "1039 기본 꺼짐");
}

#[test]
fn decset_and_decrst_change_each_mode() {
    let mut e = emu();
    e.feed(b"\x1b[?67h\x1b[?1035l\x1b[?1036l\x1b[?1039h");
    let k = e.snapshot().modes.key;
    assert!(k.backarrow_key && !k.ignore_keypad_with_numlock && !k.alt_esc_prefix && k.alt_sends_escape);
    e.feed(b"\x1b[?67l\x1b[?1035h\x1b[?1036h\x1b[?1039l");
    let k = e.snapshot().modes.key;
    assert!(!k.backarrow_key && k.ignore_keypad_with_numlock && k.alt_esc_prefix && !k.alt_sends_escape);
}

#[test]
fn ris_restores_defaults_but_decstr_keeps_them() {
    let mut e = emu();
    e.feed(b"\x1b[?67h\x1b[?1036l");
    // DECSTR는 Ghostty softReset의 모드 목록에 없다
    e.feed(b"\x1b[!p");
    let k = e.snapshot().modes.key;
    assert!(k.backarrow_key && !k.alt_esc_prefix);
    // RIS는 기본으로
    e.feed(b"\x1bc");
    let k = e.snapshot().modes.key;
    assert!(!k.backarrow_key && k.alt_esc_prefix);
}

#[test]
fn decrqm_reports_each_mode() {
    // 기본: 67 꺼짐(2), 1035 켬(1), 1036 켬(1), 1039 꺼짐(2)
    assert_eq!(reply_to("\\033[?67$p"), "\x1b[?67;2$y");
    assert_eq!(reply_to("\\033[?1035$p"), "\x1b[?1035;1$y");
    assert_eq!(reply_to("\\033[?1036$p"), "\x1b[?1036;1$y");
    assert_eq!(reply_to("\\033[?1039$p"), "\x1b[?1039;2$y");
    // 바꾼 뒤
    assert_eq!(reply_to("\\033[?67h\\033[?67$p"), "\x1b[?67;1$y");
    assert_eq!(reply_to("\\033[?1035l\\033[?1035$p"), "\x1b[?1035;2$y");
    assert_eq!(reply_to("\\033[?1036l\\033[?1036$p"), "\x1b[?1036;2$y");
    assert_eq!(reply_to("\\033[?1039h\\033[?1039$p"), "\x1b[?1039;1$y");
}
