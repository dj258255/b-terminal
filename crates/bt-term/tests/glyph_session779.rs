//! 글리프 프로토콜(APC 25a1)의 답이 실제 PTY를 지나 프로그램에 돌아오는지(#779).
//! PTY는 `/bin/sh`와 `printf`·`dd`만 쓴다(사용자 셸 설정과 데스크톱 알림을 건드리지 않는다)

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
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

fn run(setup: &str) -> String {
    let (s, rx) = spawn(&reader(setup));
    wait_exit(&rx);
    parse_reply(&s)
}

const ST: &str = "\\033\\\\";

#[test]
fn support_register_query_answer_through_the_pty() {
    // Ghostty `stream_terminal.zig` "glyph protocol APC with write_pty callback"와 같은 요청을 PTY로
    let reply = run(&format!(
        "printf '\\033_25a1;s{ST}\\033_25a1;r;cp=e0a0;AAAAAAAAAAAAAA=={ST}\\033_25a1;q;cp=e0a0{ST}'"
    ));
    assert_eq!(
        reply,
        "\x1b_25a1;s;fmt=glyf\x1b\\\x1b_25a1;r;cp=e0a0;status=0\x1b\\\x1b_25a1;q;cp=e0a0;status=glossary\x1b\\"
    );
}

#[test]
fn full_reset_over_the_pty_clears_the_registration() {
    let reply = run(&format!(
        "printf '\\033_25a1;r;cp=e0a0;reply=0;AAAAAAAAAAAAAA=={ST}\\033c\\033_25a1;q;cp=e0a0{ST}'"
    ));
    assert_eq!(reply, "\x1b_25a1;q;cp=e0a0;status=\x1b\\");
}

#[test]
fn rejected_requests_answer_with_numbers_and_fixed_names_only() {
    // 요청의 cp 글(앞 0, 대문자)과 알 수 없는 옵션은 답에 그대로 돌아가지 않는다
    let reply = run(&format!(
        "printf '\\033_25a1;q;cp=000E0A0;x=hostile{ST}\\033_25a1;r;cp=41;AAAAAAAAAAAAAA=={ST}\\033_25a1;c;cp=41{ST}'"
    ));
    assert_eq!(
        reply,
        "\x1b_25a1;q;cp=e0a0;status=\x1b\\\x1b_25a1;r;cp=41;status=1;reason=out_of_namespace\x1b\\\x1b_25a1;c;status=1;reason=out_of_namespace\x1b\\"
    );
}

#[test]
fn tmux_passthrough_wrapped_glyph_apc_is_not_answered() {
    // tmux 패스스루 안쪽의 APC는 kitty 그래픽처럼 신호로 읽지 않는다(ADR-053, #603). 바깥의 지원 질의 하나만 답한다
    let reply = run(&format!(
        "printf '\\033Ptmux;\\033\\033_25a1;s\\033\\033\\\\\\033\\\\\\033_25a1;s{ST}'"
    ));
    assert_eq!(reply, "\x1b_25a1;s;fmt=glyf\x1b\\");
}

#[test]
fn readonly_pane_still_answers_glyph_queries() {
    // 터미널의 답은 사람의 입력이 아니다(ADR-018 결정 4)
    let (s, rx) = spawn(&reader(&format!("sleep 0.2; printf '\\033_25a1;s{ST}'")));
    s.set_readonly(true);
    wait_exit(&rx);
    assert_eq!(parse_reply(&s), "\x1b_25a1;s;fmt=glyf\x1b\\");
}
