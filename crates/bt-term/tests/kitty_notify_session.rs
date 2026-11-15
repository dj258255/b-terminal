//! kitty 알림(OSC 99)이 실제 PTY를 지나 알림 사건 하나가 되고, 지원 질의의 답이 PTY로 돌아오는지(#561).
//! PTY는 `/bin/sh`와 `printf`·`dd`만 쓴다(사용자 셸 설정과 데스크톱 알림을 건드리지 않는다)

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Notification, Session, SessionEvent, SpawnOptions};

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

fn notifications(events: &[SessionEvent]) -> Vec<Notification> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Notification(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn query_is_answered_through_the_pty_with_the_same_terminator() {
    let (reply, events) = run("printf '\\033]99;i=probe-1:p=?;\\033\\\\'");
    assert_eq!(reply, "\x1b]99;i=probe-1:p=?;o=always:p=title,body,?\x1b\\");
    assert!(notifications(&events).is_empty(), "질의는 알림을 내지 않는다");
    let (reply, _) = run("printf '\\033]99;i=probe-1:p=?;\\007'");
    assert_eq!(reply, "\x1b]99;i=probe-1:p=?;o=always:p=title,body,?\x07");
    let (reply, _) = run("printf '\\033]99;p=?;\\033\\\\'");
    assert_eq!(reply, "\x1b]99;p=?;o=always:p=title,body,?\x1b\\");
}

#[test]
fn query_with_a_hostile_id_gets_no_reply() {
    // 공백·`/`·`;` 같은 글자가 든 i, 아주 긴 i는 답하지 않는다(PTY 입력 주입 방지). 끝에 알려진 질의를 한 번 더 보내
    // 그 답만 읽히는지 본다
    let long = "a".repeat(300);
    let setup = format!(
        "printf '\\033]99;i=a b:p=?;\\033\\\\\\033]99;i=a/b:p=?;\\033\\\\\\033]99;i={long}:p=?;\\033\\\\\\033]99;i=ok:p=?;\\033\\\\'"
    );
    let (reply, _) = run(&setup);
    assert_eq!(reply, "\x1b]99;i=ok:p=?;o=always:p=title,body,?\x1b\\");
}

#[test]
fn spec_example_makes_one_notification_through_the_pty() {
    let (_, events) =
        run("printf '\\033]99;i=1:d=0;Hello world\\033\\\\'; printf '\\033]99;i=1:p=body;This is cool\\033\\\\'");
    assert_eq!(
        notifications(&events),
        vec![Notification {
            title: "Hello world".into(),
            body: "This is cool".into()
        }]
    );
}

#[test]
fn base64_hangul_notification_through_the_pty() {
    // "빌드 끝"을 미리 base64로 인코딩한 값(65OcIOuBnQ== 앞부분 포함)
    let (_, events) = run("printf '\\033]99;e=1;67mM65OcIOuBnQ==\\033\\\\'");
    assert_eq!(
        notifications(&events),
        vec![Notification {
            title: "빌드 끝".into(),
            body: String::new()
        }]
    );
}
