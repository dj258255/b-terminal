//! #778: iTerm2 클립보드 쓰기 `OSC 1337 ; Copy=:base64`는 OSC 52 쓰기와 같은 사건(`ClipboardStore`)이 된다.
//! 기준: Ghostty v1.4.0 `osc/parsers/iterm2.zig`의 Copy 시험들, `stream_terminal.zig` `clipboard_write effect callback`
//! (`ESC]1337;Copy=:aVRlcm0y ESC\` → "iTerm2" 쓰기 한 번). 엔진 설정 `clipboard-write`가 꺼져 있으면 닿지 않는다.
//! PTY는 `/bin/sh`와 `printf`만 쓴다.

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn events(script: &str, write: bool, read: bool) -> Vec<SessionEvent> {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        clipboard_write: write,
        clipboard_read: read,
        ..Default::default()
    };
    let _s =
        Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
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

fn stores(ev: &[SessionEvent]) -> Vec<&str> {
    ev.iter()
        .filter_map(|e| match e {
            SessionEvent::ClipboardStore(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn copy_writes_once_when_allowed() {
    let ev = events("printf '\\033]1337;Copy=:aVRlcm0y\\033\\\\'", true, false);
    assert_eq!(stores(&ev), vec!["iTerm2"], "{ev:?}");
}

#[test]
fn copy_follows_clipboard_write_deny() {
    // 꺼져 있으면 OSC 52처럼 닿지 않는다(ask·allow는 앱이 가른다)
    let ev = events("printf '\\033]1337;Copy=:aVRlcm0y\\033\\\\'", false, true);
    assert!(stores(&ev).is_empty(), "{ev:?}");
}

#[test]
fn copy_rejects_what_ghostty_rejects() {
    for body in [
        "Copy",
        "Copy=",
        "Copy=:",
        "Copy=:?",
        "Copy=YWJjMTIz",
        "Copy=c:YWJjMTIz",
        "Copy=:@@@@",
        "Copy=:YWJj;x",
    ] {
        let ev = events(&format!("printf '\\033]1337;{body}\\007'"), true, true);
        assert!(stores(&ev).is_empty(), "{body}: {ev:?}");
    }
}

#[test]
fn copy_never_reads_the_clipboard() {
    // `?`는 OSC 52 읽기처럼 해석되지 않는다. clipboard-read가 켜져 있어도 읽기 사건이 없다
    let ev = events("printf '\\033]1337;Copy=:?\\007'", true, true);
    assert!(
        !ev.iter().any(|e| matches!(e, SessionEvent::ClipboardLoad(_))),
        "{ev:?}"
    );
}

#[test]
fn key_is_case_insensitive_and_other_subcommands_are_ignored() {
    let ev = events("printf '\\033]1337;copy=:YWJjMTIz\\007'", true, false);
    assert_eq!(stores(&ev), vec!["abc123"], "{ev:?}");
    let ev = events(
        "printf '\\033]1337;File=inline=1:YWJj\\007\\033]1337;CurrentDir=/tmp\\007'",
        true,
        true,
    );
    assert!(stores(&ev).is_empty(), "{ev:?}");
}
