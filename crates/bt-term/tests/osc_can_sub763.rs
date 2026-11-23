//! #763: CAN(0x18)·SUB(0x1A)로 끝난(끊긴) OSC는 명령이 아니다.
//! 기준: Ghostty v1.4.0 `src/terminal/osc.zig` 1092~1098줄 `Parser.end`(끝 바이트가 CAN·SUB면 null, "This matches xterm"),
//! 시험 `Parser end with CAN or SUB cancels the command`(1196줄). `Parser.zig` 276줄이 상태를 나갈 때 그 바이트를 `end(c)`에 넘긴다.
//! ESC로 끝난 것(`ESC \`가 아닌 ESC 포함)은 그대로 실행한다. DCS·APC는 바이트와 상관없이 끝낸다(`dcs_unhook`·`apc_end`).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn feed_all(b: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(b)
}

#[test]
fn scanner_drops_osc_cancelled_by_can_or_sub() {
    for end in [0x18u8, 0x1a] {
        for body in [
            &b"9;hello"[..],
            b"99;;hello",
            b"777;notify;t;b",
            b"7;file:///tmp/x",
            b"133;A",
            b"9;4;1;50",
            b"7501;working",
        ] {
            let mut v = b"\x1b]".to_vec();
            v.extend(body);
            v.push(end);
            v.extend(b"\x1b[c");
            assert_eq!(
                feed_all(&v),
                vec![Signal::Da1],
                "{:?} + {end:#x}",
                String::from_utf8_lossy(body)
            );
        }
    }
}

#[test]
fn scanner_still_runs_osc_ended_by_bel_st_or_esc() {
    for end in [&b"\x07"[..], b"\x1b\\", b"\x1b[c"] {
        let mut v = b"\x1b]9;hi".to_vec();
        v.extend(end);
        let got = feed_all(&v);
        assert!(
            matches!(&got[0], Signal::Notify(n) if n.body == "hi"),
            "{end:?}: {got:?}"
        );
    }
}

#[test]
fn parser_still_works_after_a_cancel() {
    // Ghostty 시험의 셋째 단언: 취소 뒤 다음 OSC는 그대로 읽는다
    let got = feed_all(b"\x1b]9;one\x18\x1b]9;two\x07");
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(matches!(&got[0], Signal::Notify(n) if n.body == "two"));
}

fn events(script: &str, opts: impl FnOnce(&mut SpawnOptions)) -> (Session, Vec<SessionEvent>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let mut o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    opts(&mut o);
    let s = Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    (s, collect(&rx))
}

fn collect(rx: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let mut out = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::Exited(_)) => break,
            Ok(e) => out.push(e),
            Err(_) => {}
        }
    }
    out
}

#[test]
fn osc52_cancelled_by_sub_does_not_reach_the_clipboard() {
    // Ghostty 시험의 둘째 단언(`52;c;Zm9v` + SUB). 클립보드 쓰기를 켜도 닿지 않는다
    let (_s, ev) = events("printf '\\033]52;c;Zm9v\\032'; printf '\\033]52;c;YmFy\\030'", |o| {
        o.clipboard_write = true
    });
    assert!(
        !ev.iter().any(|e| matches!(e, SessionEvent::ClipboardStore(_))),
        "{ev:?}"
    );
    // BEL로 끝나면 닿는다(시험이 무엇을 보는지 확인)
    let (_s, ev) = events("printf '\\033]52;c;Zm9v\\007'", |o| o.clipboard_write = true);
    assert!(
        ev.iter()
            .any(|e| matches!(e, SessionEvent::ClipboardStore(t) if t == "foo")),
        "{ev:?}"
    );
}

#[test]
fn title_cancelled_by_can_does_not_change_the_title() {
    // Ghostty 시험의 첫 단언(`2;title` + CAN). 앞서 정한 제목이 남는다
    let (_s, ev) = events("printf '\\033]2;old\\007\\033]2;new\\030'", |_| {});
    let titles: Vec<&str> = ev
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Title(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(titles, vec!["old"]);
}

#[test]
fn notification_and_cwd_cancelled_by_can_do_not_reach_the_app() {
    let (_s, ev) = events(
        "printf '\\033]9;hello\\030\\033]99;;hello\\032\\033]7;file:///tmp/x\\030'",
        |_| {},
    );
    assert!(
        !ev.iter().any(|e| matches!(
            e,
            SessionEvent::Notification(_) | SessionEvent::Cwd { .. } | SessionEvent::CwdReset
        )),
        "{ev:?}"
    );
}
