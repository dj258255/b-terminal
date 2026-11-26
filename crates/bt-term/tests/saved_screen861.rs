//! 창 상태 복원의 지난 화면(#861, ADR-092): 실제 PTY 칸에서 저장한 줄이 셸이 쓰기 전에 스크롤백에 놓이고,
//! 저장 파일에 심은 OSC 52·DCS·BEL이 실행되지 않는다. PTY는 `/bin/sh`와 `printf`만 쓴다

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::saved_screen::{SavedColor, SavedLine, SavedRun};
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

/// 칸을 띄워 끝날 때까지 돌리고, 받은 사건을 돌려준다
fn run(script: &str, restore: Vec<SavedLine>, note: Option<&str>) -> (Session, Vec<String>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        scrollback: 1000,
        restore_lines: restore,
        restore_note: note.map(str::to_string),
        // 클립보드 쓰기를 허용해도(OSC 52가 살아 있어도) 저장 글은 실행되지 않아야 한다
        clipboard_write: true,
        clipboard_read: true,
        ..Default::default()
    };
    let s = Session::spawn(o, GridSize::new(40, 6), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::Exited(_)) => break,
            Ok(e) => events.push(format!("{e:?}")),
            Err(_) => {}
        }
    }
    // 끝난 뒤 남은 사건도 모은다
    while let Ok(e) = rx.try_recv() {
        events.push(format!("{e:?}"));
    }
    (s, events)
}

#[test]
fn restored_lines_sit_in_scrollback_before_the_shell_output() {
    let saved = vec![SavedLine::plain("지난 줄 하나"), SavedLine::plain("지난 줄 둘")];
    let (s, _) = run("printf 'live\\n'", saved, Some("-- 저장 --"));
    let text = s.plain_text(true);
    let a = text.find("지난 줄 하나").unwrap_or_else(|| panic!("{text}"));
    let b = text.find("지난 줄 둘").unwrap();
    let n = text.find("-- 저장 --").unwrap();
    let live = text.find("live").unwrap();
    assert!(a < b && b < n && n < live, "순서: {text}");
}

#[test]
fn hostile_saved_file_triggers_no_events_and_leaves_no_control_bytes() {
    let evil = "\x1b]52;c;ZXZpbA==\x07\x1b]0;pwned\x07\x1b]9;note\x07\x1bP+q\x1b\\\x07x";
    let saved = vec![SavedLine {
        runs: vec![SavedRun {
            t: evil.into(),
            fg: SavedColor::D,
            bg: SavedColor::D,
            f: 0,
        }],
    }];
    let (s, events) = run("printf 'ok\\n'", saved, None);
    let bad: Vec<&String> = events
        .iter()
        .filter(|e| {
            ["ClipboardStore", "ClipboardLoad", "Bell", "Title", "Notification"]
                .iter()
                .any(|k| e.starts_with(k))
        })
        .collect();
    assert!(bad.is_empty(), "저장 글이 사건을 일으켰다: {bad:?}");
    let text = s.plain_text(true);
    assert!(text.contains("ok"));
    assert!(
        !text.chars().any(|c| c.is_control() && c != '\n'),
        "제어 문자가 화면에 남았다: {text:?}"
    );
}

#[test]
fn capture_then_restore_round_trips_through_real_sessions() {
    let (a, _) = run("printf 'first\\n\\033[31mred\\033[0m\\nlast\\n'", vec![], None);
    let lines = a.capture_scrollback(100);
    let texts: Vec<String> = lines.iter().map(SavedLine::text).collect();
    assert!(texts.contains(&"first".to_string()), "{texts:?}");
    let red = lines.iter().find(|l| l.text() == "red").expect("red 줄");
    assert_eq!(red.runs[0].fg, SavedColor::I(1));
    let (b, _) = run("printf 'again\\n'", lines, None);
    let t = b.plain_text(true);
    assert!(
        t.contains("first") && t.contains("red") && t.contains("last") && t.contains("again"),
        "{t}"
    );
}
