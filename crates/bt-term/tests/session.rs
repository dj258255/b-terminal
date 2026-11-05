//! 실제 PTY로 /bin/sh를 띄워 확인한다. 사용자의 로그인 셸 설정이 결과를 바꾸지 않게 /bin/sh와 printf만 쓴다(AGENTS.md)

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

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

fn wait_exit(rx: &Receiver<SessionEvent>) -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(SessionEvent::Exited(code)) = rx.recv_timeout(Duration::from_millis(100)) {
            return code;
        }
    }
    panic!("10초 안에 끝나지 않았다");
}

fn wait_until(session: &Session, mut pred: impl FnMut(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = session.snapshot().text();
        if pred(&text) {
            return text;
        }
        assert!(Instant::now() < deadline, "화면이 기대대로 되지 않았다:\n{text}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn printf_output_lands_in_grid_and_exit_code_is_reported() {
    let (s, rx) = sh("printf 'hello\\n한글 OK\\n'; exit 3", GridSize::new(40, 10));
    let code = wait_exit(&rx);
    assert_eq!(code, Some(3));
    let screen = s.snapshot();
    assert_eq!(screen.row_text(0), "hello");
    assert_eq!(screen.row_text(1), "한글 OK");
    // 한글은 두 칸: '한'(0) 뒤칸(1) '글'(2) 뒤칸(3) ' '(4) 'O'(5)
    assert!(screen.cell(0, 1).flags.contains(bt_term::CellFlags::WIDE));
    assert!(screen.cell(1, 1).flags.contains(bt_term::CellFlags::SPACER));
    assert_eq!(screen.cell(5, 1).ch, 'O');
}

#[test]
fn resize_reaches_the_pty() {
    let (s, _rx) = sh("sleep 0.3; stty size; sleep 5", GridSize::new(40, 10));
    s.resize(GridSize::new(100, 30));
    wait_until(&s, |t| t.contains("30 100"));
}

#[test]
fn input_is_written_to_the_pty() {
    let (s, _rx) = sh(
        "read line; printf 'got:%s\\n' \"$line\"; sleep 5",
        GridSize::new(40, 10),
    );
    s.write("안녕 abc\r".as_bytes());
    wait_until(&s, |t| t.contains("got:안녕 abc"));
}

#[test]
fn environment_identifies_the_terminal() {
    let (s, rx) = sh("printf '%s %s\\n' \"$TERM_PROGRAM\" \"$TERM\"", GridSize::new(40, 5));
    wait_exit(&rx);
    assert_eq!(s.snapshot().row_text(0), "b-terminal xterm-256color");
}

#[test]
fn foreground_process_follows_the_running_child() {
    // Linux의 dash는 마지막 명령을 exec하지 않고 fork한다(작업 제어가 없어 전경 그룹 대표가 sh로 남는다). exec로 못 박는다
    let (s, _rx) = sh("exec sleep 5", GridSize::new(40, 5));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(fg) = s.foreground() {
            if fg.name == "sleep" {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "전경 프로세스가 sleep이 되지 않았다: {:?}",
            s.foreground()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(s.cwd().is_some());
}

#[test]
fn notifications_from_the_pty_reach_the_sink() {
    let (_s, rx) = sh(
        "printf '\\033]777;notify;빌드;끝났습니다\\007'; printf '\\033]7;file://h/tmp\\007'; sleep 1",
        GridSize::new(40, 5),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut note, mut cwd) = (None, None);
    while Instant::now() < deadline && (note.is_none() || cwd.is_none()) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::Notification(n)) => note = Some(n),
            Ok(SessionEvent::Cwd { path, .. }) => cwd = Some(path),
            _ => {}
        }
    }
    let note = note.expect("알림이 오지 않았다");
    assert_eq!((note.title.as_str(), note.body.as_str()), ("빌드", "끝났습니다"));
    assert_eq!(cwd.as_deref(), Some(std::path::Path::new("/tmp")));
}

#[test]
fn mouse_modes_left_on_by_a_dead_program_can_be_reset() {
    let (s, rx) = sh("printf '\\033[?1003h\\033[?1006h'", GridSize::new(20, 5));
    wait_exit(&rx);
    let m = s.snapshot().modes;
    assert!(m.mouse_motion && m.sgr_mouse, "켠 채로 끝났다");
    assert!(s.reset_mouse_modes());
    let m = s.snapshot().modes;
    assert!(!m.mouse_motion && !m.sgr_mouse && !m.mouse_report);
    assert!(!s.reset_mouse_modes(), "이미 꺼졌으면 거짓");
}
