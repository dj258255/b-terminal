//! #792: DCS 통과·무시 상태에서 0x80~0xFF는 모두 자료다(0x9C도 8비트 ST가 아니다).
//! 기준: Ghostty v1.4.0 `src/terminal/parse_table.zig` 시험 `dcs_passthrough: high bytes are payload data`(412줄),
//! `dcs_ignore: high bytes are ignored payload data`(425줄), `dcs_passthrough: ESC, CAN, and SUB still exit`(436줄).
//! 원본 주석: 원시 0x9C는 UTF-8 연속 바이트와 구별되지 않는다(`Ü`는 0xC3 0x9C). 한글 대부분도 0x9C를 품는다(`한` = ED 95 9C).
//! 끝은 `ESC \`(7비트 ST)와 CAN·SUB·ESC다.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{Emulator, GridSize, Session, SessionEvent, SpawnOptions};

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

fn screen_of(bytes: &[u8]) -> (String, (usize, usize)) {
    let mut e = Emulator::new(GridSize::new(40, 4), Theme::default());
    e.feed(bytes);
    let s = e.snapshot();
    (s.text(), (s.cursor.col, s.cursor.row))
}

#[test]
fn every_high_byte_is_dcs_passthrough_data_in_the_engine() {
    // 통과 상태(`tmux;`로 연 DCS)에서 0x80~0xFF 한 바이트가 DCS를 끝내거나 뒤 글을 바꾸지 않는다
    for c in 0x80u8..=0xff {
        let b = [b"\x1bPtmux;x".as_slice(), &[c], b"y\x1b\\after"].concat();
        assert_eq!(screen_of(&b), ("after".into(), (5, 0)), "{c:#04x}");
        // 무시 상태(`$` 뒤 `0`)도 같다
        let b = [b"\x1bP$0x".as_slice(), &[c], b"y\x1b\\after"].concat();
        assert_eq!(screen_of(&b), ("after".into(), (5, 0)), "무시 {c:#04x}");
    }
}

#[test]
fn hangul_inside_dcs_does_not_leak_to_the_screen() {
    // tmux 제어 모드(`DCS 1000p`)의 `%output`이나 통과 안의 날 한글. "한" = ED 95 9C
    assert_eq!(
        screen_of("\x1bP1000p%output %1 한글\n\x1b\\after".as_bytes()),
        ("after".into(), (5, 0))
    );
    assert_eq!(
        screen_of("\x1bPtmux;한글\x1b\\after".as_bytes()),
        ("after".into(), (5, 0))
    );
}

#[test]
fn esc_can_sub_still_end_the_dcs() {
    // 436줄 시험: CAN·SUB·ESC는 그대로 끝낸다
    for end in [&b"\x18"[..], b"\x1a", b"\x1b\\", b"\x1b[0m"] {
        let b = [b"\x1bPtmux;\x9cx".as_slice(), end, b"after"].concat();
        let (text, _) = screen_of(&b);
        assert!(text.ends_with("after"), "{end:?}: {text:?}");
        assert!(!text.contains('x'), "{end:?}: {text:?}");
    }
}

#[test]
fn tmux_wrapped_hangul_notification_is_unchanged() {
    // 이슈 재현 줄: 안쪽(겹친 ESC 뒤) 글은 OSC 문자열이라 0x9C가 자료였고 전에도 깨지지 않았다.
    // 안쪽 알림은 신호를 내지 않는 것이 설계다(ADR-053). 고친 뒤에도 같다
    let b = "\x1bPtmux;\x1b\x1b]9;한글 알림\x07\x1b\\after".as_bytes();
    assert_eq!(feed_all(b), vec![]);
    assert_eq!(screen_of(b), ("after".into(), (5, 0)));
    // 바깥(겹치지 않은) OSC의 한글 알림은 그대로 한 번 난다
    let got = feed_all("\x1b]9;한글 알림\x07".as_bytes());
    assert!(
        matches!(&got[..], [Signal::Notify(n)] if n.body == "한글 알림"),
        "{got:?}"
    );
}

#[test]
fn scanner_keeps_the_dcs_open_over_9c_and_other_high_bytes() {
    // DECRQSS 본문에 높은 바이트가 끼어도 `ESC \`까지 한 DCS다. 뒤의 ENQ는 DCS가 끝난 뒤에만 답한다
    for c in 0x80u8..=0xff {
        let b = [b"\x1bP$qm".as_slice(), &[c], b"\x05"].concat();
        assert_eq!(feed_all(&b), vec![], "{c:#04x} ENQ는 자료");
        let b = [b"\x1bP$qm".as_slice(), &[c], b"\x1b\\\x05"].concat();
        let got = feed_all(&b);
        assert_eq!(got.last(), Some(&Signal::Enquiry), "{c:#04x}: {got:?}");
    }
    // 질의가 아닌 DCS도 0x9C로 끝나지 않는다
    assert_eq!(feed_all(b"\x1bPq\x9c\x05"), vec![]);
    assert_eq!(feed_all(b"\x1bP1$r\x9c\x05"), vec![]);
    assert_eq!(feed_all(b"\x1bPq\x9c\x1b\\\x05"), vec![Signal::Enquiry]);
}

// ---- PTY 세션: /bin/sh와 printf만 ----

fn run(script: &str) -> (Session, Vec<SessionEvent>) {
    let (tx, rx): (_, Receiver<SessionEvent>) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let s = Session::spawn(o, GridSize::new(40, 4), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    let mut ev = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::Exited(_)) => break,
            Ok(e) => ev.push(e),
            Err(_) => {}
        }
    }
    (s, ev)
}

#[test]
fn session_hangul_in_dcs_does_not_leak() {
    // 날 한글이 든 DCS(tmux 제어 모드의 모양)
    let (s, _) = run("printf '\\033P1000p%%output %%1 한글\\n\\033\\\\after'");
    assert_eq!(s.snapshot().text(), "after");
}

#[test]
fn session_tmux_wrapped_hangul_notification_stays_quiet_and_clean() {
    // 이슈 재현 줄 그대로. 알림도 화면 찌꺼기도 없다
    let (s, ev) = run("printf '\\033Ptmux;\\033\\033]9;한글 알림\\007\\033\\\\after'");
    assert!(!ev.iter().any(|e| matches!(e, SessionEvent::Notification(_))), "{ev:?}");
    assert_eq!(s.snapshot().text(), "after");
}

#[test]
fn session_plain_hangul_notification_still_notifies_once() {
    let (_s, ev) = run("printf '\\033]9;한글 알림\\007'");
    let n: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Notification(n) => Some(n),
            _ => None,
        })
        .collect();
    assert_eq!(n.len(), 1, "{ev:?}");
}
