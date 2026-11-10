//! 마우스 보고 형식과 이벤트 모드(#300): DECSET 9(X10), 1005(UTF-8), 1006(SGR), 1015(urxvt), 1016(SGR 픽셀).
//! 기준: Ghostty stream_handler.zig 775~815줄 — 형식은 하나만 있고 마지막에 켠 것이 이긴다.
//! 어느 형식을 끄든 X10 형식으로 돌아간다(Ghostty 동작 그대로)

use bt_core::Theme;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_term::{Emulator, GridSize, MouseFormat, Session, SessionEvent, SpawnOptions};

fn sh(script: &str, size: GridSize) -> (Session, Receiver<SessionEvent>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    (Session::spawn(opts, size, Theme::default(), sink).expect("spawn"), rx)
}

fn wait_exit(rx: &Receiver<SessionEvent>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            return;
        }
    }
    panic!("10초 안에 끝나지 않았다");
}

fn emu() -> Emulator {
    Emulator::new(GridSize::new(20, 5), Theme::default())
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// 실제 PTY에서 DECRQM(`CSI ? mode $ p`)을 끈 채·켠 뒤·다시 끈 뒤에 묻고 답 바이트를 읽는다.
/// 답은 PTY 입력으로 오므로 Session으로 해야 한다(Emulator는 답을 버린다)
fn decrqm_run(mode: u16) -> Vec<String> {
    let reply = |state: u8| format!("\x1b[?{mode};{state}$y");
    let n = reply(2).len();
    let q = format!("\\033[?{mode}$p");
    let read = format!("$(dd bs=1 count={n} 2>/dev/null | od -An -tx1 | tr -d ' \\n')");
    let script = format!(
        "stty raw -echo; printf '{q}'; a={read}; printf '\\033[?{mode}h{q}'; b={read}; printf '\\033[?{mode}l{q}'; c={read}; stty sane; printf 'ans=%s,%s,%s\\n' \"$a\" \"$b\" \"$c\""
    );
    let (s, rx) = sh(&script, GridSize::new(80, 5));
    wait_exit(&rx);
    let text = s.snapshot().text();
    let line = text
        .lines()
        .find(|l| l.starts_with("ans="))
        .unwrap_or_else(|| panic!("답이 없다: {text}"))
        .to_string();
    line["ans=".len()..].split(',').map(str::to_string).collect()
}

#[test]
fn decrqm_answers_the_new_mouse_modes_instead_of_unknown() {
    for mode in [9u16, 1005, 1006, 1015, 1016] {
        let want = |state: u8| hex(&format!("\x1b[?{mode};{state}$y"));
        assert_eq!(
            decrqm_run(mode),
            [want(2), want(1), want(2)],
            "모드 {mode}: 끔·켬·다시 끔"
        );
    }
}

#[test]
fn the_last_format_turned_on_wins() {
    let mut e = emu();
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::X10);
    e.feed(b"\x1b[?1006h");
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::Sgr);
    e.feed(b"\x1b[?1015h");
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::Urxvt);
    e.feed(b"\x1b[?1016h");
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::SgrPixels);
    e.feed(b"\x1b[?1005h");
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::Utf8);
}

#[test]
fn turning_off_any_format_goes_back_to_x10_like_ghostty() {
    // stream_handler.zig: `.mouse_format_* => flags.mouse_format = if (enabled) .x else .x10`
    let mut e = emu();
    e.feed(b"\x1b[?1016h\x1b[?1006l");
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::X10);
    e.feed(b"\x1b[?1015h\x1b[?1015l");
    assert_eq!(e.snapshot().modes.mouse_format, MouseFormat::X10);
}

#[test]
fn x10_event_mode_replaces_the_other_event_modes() {
    let mut e = emu();
    e.feed(b"\x1b[?1003h");
    assert!(e.snapshot().modes.mouse_motion);
    e.feed(b"\x1b[?9h");
    let m = e.snapshot().modes;
    assert!(m.mouse_x10 && !m.mouse_motion);
    e.feed(b"\x1b[?1000h");
    let m = e.snapshot().modes;
    assert!(!m.mouse_x10 && m.mouse_report);
    e.feed(b"\x1b[?9h\x1b[?9l");
    let m = e.snapshot().modes;
    assert!(!m.mouse_x10 && !m.mouse_report && !m.mouse_drag && !m.mouse_motion);
}
