//! 프로그램 상태(OSC 7501)가 실제 PTY를 지나 질의 답과 칸의 기록이 되는지(#618).
//! PTY는 `/bin/sh`와 `printf`·`dd`만 쓴다(사용자 셸 설정과 데스크톱 알림을 건드리지 않는다)

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::program_status::{Alert, State};
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn spawn(script: &str) -> (Session, Receiver<SessionEvent>) {
    let (tx, rx) = channel();
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
    (
        Session::spawn(o, GridSize::new(100, 40), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}")),
        rx,
    )
}

fn wait_exit(rx: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let mut all = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::Exited(_)) => break,
            Ok(e) => all.push(e),
            Err(_) => {}
        }
    }
    all.extend(rx.try_iter());
    all
}

/// 셸이 PTY에서 읽은 바이트(답)를 `R[16진]`으로 찍는다
fn reader(setup: &str) -> String {
    format!(
        "stty raw -echo min 0 time 5; {setup}; sleep 0.3; \
         r=$(dd bs=512 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    )
}

fn parse_reply(s: &Session) -> String {
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    let hex: String = text[start..end].chars().filter(char::is_ascii_hexdigit).collect();
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

fn run(setup: &str) -> (String, Vec<SessionEvent>) {
    let (s, rx) = spawn(&reader(setup));
    let events = wait_exit(&rx);
    (parse_reply(&s), events)
}

#[test]
fn query_is_answered_with_the_same_terminator_and_no_record_contents() {
    let (reply, _) = run("printf '\\033]7501;?\\033\\\\'");
    assert_eq!(reply, "\x1b]7501;?\x1b\\");
    let (reply, _) = run("printf '\\033]7501;?\\007'");
    assert_eq!(reply, "\x1b]7501;?\x07");
}

#[test]
fn a_report_gets_no_reply() {
    // 보고에는 답하지 않는다. 끝에 질의를 한 번 더 보내 그 답만 읽히는지 본다
    let (reply, _) = run("printf '\\033]7501;state=working\\033\\\\\\033]7501;?\\033\\\\'");
    assert_eq!(reply, "\x1b]7501;?\x1b\\");
}

fn status_events(events: &[SessionEvent]) -> Vec<Option<Alert>> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ProgramStatus(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

/// 기록이 `want`가 될 때까지 기다린다(읽기 스레드가 적용하는 것을 기다린다)
fn wait_status(s: &Session, want: Option<State>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let got = s.program_status().map(|x| x.state);
        if got == want {
            return;
        }
        assert!(Instant::now() < deadline, "기록이 {want:?}가 되지 않았다: {got:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_report_becomes_a_record_and_blocked_makes_one_alert() {
    let (s, rx) = spawn(
        "printf '\\033]7501;state=working:progress=40\\033\\\\'; \
         printf '\\033]7501;state=blocked:kind=question:id=a:title=UGxhbg\\033\\\\'; sleep 3",
    );
    wait_status(&s, Some(State::Blocked));
    assert_eq!(s.program_status().unwrap().state, State::Blocked);
    let events: Vec<_> = rx.try_iter().collect();
    let alerts: Vec<_> = status_events(&events).into_iter().flatten().collect();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].state, State::Blocked);
    assert_eq!(alerts[0].title.as_deref(), Some("Plan"));
}

#[test]
fn an_invalid_report_changes_nothing() {
    let (s, rx) = spawn(
        "printf '\\033]7501;state=sleeping\\033\\\\\\033]7501;state=working:id=a b\\033\\\\\\033]7501;state=done:msg=a\\033\\\\'",
    );
    let events = wait_exit(&rx);
    assert_eq!(s.program_status(), None);
    assert!(status_events(&events).is_empty());
}

#[test]
fn a_new_shell_prompt_drops_working_but_keeps_done() {
    let (s, _rx) = spawn(
        "printf '\\033]7501;state=working:id=w\\033\\\\\\033]7501;state=done:id=d\\033\\\\'; \
         sleep 0.3; printf '\\033]133;A\\033\\\\'; sleep 3",
    );
    wait_status(&s, Some(State::Done));
    assert!(s.clear_seen_program_status());
    assert_eq!(s.program_status(), None);
}

#[test]
fn ris_clears_all_records_but_decstr_does_not() {
    let (s, _rx) = spawn(
        "printf '\\033]7501;state=done:id=d\\033\\\\'; sleep 0.5; printf '\\033[!p'; sleep 0.5; \
         printf '\\033]7501;state=working\\033\\\\'; sleep 0.5; printf '\\033c'; sleep 3",
    );
    wait_status(&s, Some(State::Done));
    // DECSTR 뒤에도 done이 남아 있다가 working 보고로 요약이 바뀐다
    wait_status(&s, Some(State::Working));
    // RIS가 모두 지운다
    wait_status(&s, None);
}

#[test]
fn done_survives_process_exit_but_working_does_not() {
    let (s, rx) = spawn("printf '\\033]7501;state=working:id=w\\033\\\\\\033]7501;state=done:id=d\\033\\\\'");
    wait_exit(&rx);
    assert_eq!(s.program_status().map(|x| x.state), Some(State::Done));
}
