//! OSC 5522·모드 5522가 실제 PTY를 지나 세션에서 사건·답·DECRQM·XTSAVE·붙여넣기 이벤트가 되는지(#325, ADR-030).
//! PTY는 `/bin/sh`와 `printf`·`dd`만 쓴다(사용자 셸 설정과 클립보드를 건드리지 않는다)

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::kitty_clip::{Action, ReadRequest, WriteRequest};
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn spawn(script: &str, clipboard_write: bool, limit: Option<usize>) -> (Session, Receiver<SessionEvent>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        clipboard_write,
        clipboard_write_limit_bytes: limit,
        ..Default::default()
    };
    (
        Session::spawn(o, GridSize::new(100, 40), Theme::default(), sink).expect("spawn"),
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

fn run(setup: &str, clipboard_write: bool, limit: Option<usize>) -> (String, Vec<SessionEvent>) {
    let (s, rx) = spawn(&reader(setup), clipboard_write, limit);
    let events = wait_exit(&rx);
    (parse_reply(&s), events)
}

fn requests(events: &[SessionEvent]) -> Vec<Action> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::KittyClipboardRead(r) => Some(Action::Read(r.clone())),
            SessionEvent::KittyClipboardWrite(w) => Some(Action::Write(w.clone())),
            _ => None,
        })
        .collect()
}

// ---------- DECRQM·XTSAVE·초기화 ----------

fn mode_reply(setup: &str) -> String {
    run(&format!("{setup}printf '\\033[?5522$p'"), true, None).0
}

#[test]
fn decrqm_5522_answers_set_or_reset_exactly_once() {
    assert_eq!(mode_reply(""), "\x1b[?5522;2$y", "처음엔 꺼짐(0이 아니다)");
    assert_eq!(mode_reply("printf '\\033[?5522h'; "), "\x1b[?5522;1$y");
    assert_eq!(mode_reply("printf '\\033[?5522h\\033[?5522l'; "), "\x1b[?5522;2$y");
}

#[test]
fn xtsave_and_xtrestore_keep_mode_5522_like_ghostty() {
    // Ghostty modes.zig 표에 5522가 있어 XTSAVE·XTRESTORE가 다룬다
    assert_eq!(
        mode_reply("printf '\\033[?5522h\\033[?5522s\\033[?5522l\\033[?5522r'; "),
        "\x1b[?5522;1$y"
    );
    // 저장한 적이 없으면 기본값(꺼짐)
    assert_eq!(mode_reply("printf '\\033[?5522h\\033[?5522r'; "), "\x1b[?5522;2$y");
}

#[test]
fn full_reset_turns_mode_5522_off() {
    assert_eq!(mode_reply("printf '\\033[?5522h\\033c'; "), "\x1b[?5522;2$y");
}

// ---------- 요청이 사건이 된다 ----------

#[test]
fn write_transaction_becomes_one_event_with_either_terminator() {
    for end in ["\\033\\\\", "\\007"] {
        let setup = format!(
            "printf '\\033]5522;type=write:id=42{end}\\033]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ=={end}\\033]5522;type=wdata{end}'"
        );
        let (_, events) = run(&setup, true, None);
        assert_eq!(
            requests(&events),
            vec![Action::Write(WriteRequest {
                primary: false,
                id: "42".into(),
                pw: String::new(),
                name: String::new(),
                contents: vec![("text/plain".into(), b"Ghostty".to_vec())],
                granted: false,
                bel: end == "\\007",
            })]
        );
    }
}

#[test]
fn read_request_becomes_an_event_and_nothing_is_answered_by_the_session() {
    let (reply, events) = run(
        "printf '\\033]5522;type=read:id=7:loc=primary;dGV4dC9wbGFpbg==\\033\\\\'",
        true,
        None,
    );
    assert_eq!(
        requests(&events),
        vec![Action::Read(ReadRequest {
            primary: true,
            id: "7".into(),
            mimes: vec!["text/plain".into()],
            list: false,
            pw: String::new(),
            name: String::new(),
            granted: false,
            bel: false,
        })]
    );
    assert_eq!(reply, "", "답은 앱이 정책을 본 뒤에 쓴다");
}

#[test]
fn write_denied_by_policy_answers_eperm_with_the_id() {
    let (reply, events) = run(
        "printf '\\033]5522;type=write:id=9\\033\\\\\\033]5522;type=wdata\\033\\\\'",
        false,
        None,
    );
    assert_eq!(reply, "\x1b]5522;type=write:status=EPERM:id=9\x1b\\");
    assert!(requests(&events).is_empty());
}

#[test]
fn write_over_the_limit_answers_efbig_and_never_reaches_the_app() {
    let setup = "printf '\\033]5522;type=write:id=5\\033\\\\\\033]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\\033\\\\\\033]5522;type=wdata\\033\\\\'";
    let (reply, events) = run(setup, true, Some(4));
    assert_eq!(reply, "\x1b]5522;type=write:status=EFBIG:id=5\x1b\\");
    assert!(requests(&events).is_empty());
}

#[test]
fn the_fixed_one_mib_cap_applies_even_when_the_limit_is_unlimited() {
    // OSC 52와 같은 규칙: clipboard-write-limit-bytes = unlimited(None)여도 한 번에 1MiB를 넘지 못한다
    // 5456글자(= 4092바이트) 조각을 300번: 1.2MB. 명령줄에 다 담으면 exec이 못 하므로 셸 반복문으로 만든다
    let setup = "printf '\\033]5522;type=write:id=5\\033\\\\'; b=$(head -c 5456 /dev/zero | tr '\\0' Q); i=0; \
                 while [ $i -lt 300 ]; do printf '\\033]5522;type=wdata:mime=dGV4dC9wbGFpbg==;%s\\033\\\\' \"$b\"; i=$((i+1)); done; \
                 printf '\\033]5522;type=wdata\\033\\\\'";
    let (reply, events) = run(setup, true, None);
    assert_eq!(reply, "\x1b]5522;type=write:status=EFBIG:id=5\x1b\\");
    assert!(requests(&events).is_empty());
}

#[test]
fn an_oversized_osc_is_dropped_before_collecting_and_the_next_one_still_works() {
    let setup = "printf '\\033]5522;type=read:id=1;'; head -c 9000 /dev/zero | tr '\\0' A; printf '\\033\\\\'; \
                 printf '\\033]5522;type=read:id=2;dGV4dC9wbGFpbg==\\033\\\\'";
    let (_, events) = run(setup, true, None);
    let reqs = requests(&events);
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    let Action::Read(r) = &reqs[0] else { panic!() };
    assert_eq!(r.id, "2");
}

#[test]
fn malformed_packets_are_ignored_silently() {
    let setup = "printf '\\033]5522;type=read:bare;dGV4dC9wbGFpbg==\\033\\\\\\033]5522;type=bobr\\033\\\\\\033]5522;;\\033\\\\\\033]5522;type=wdata\\033\\\\'";
    let (reply, events) = run(setup, true, None);
    assert!(requests(&events).is_empty());
    assert_eq!(reply, "");
}

// ---------- 붙여넣기 이벤트 ----------

fn wait_mode(s: &Session, on: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while s.kitty_paste_mode() != on {
        assert!(Instant::now() < deadline, "모드 5522가 {on}이 되지 않았다");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn paste_event_is_sent_only_while_mode_5522_is_on_and_carries_a_fresh_otp() {
    let avail = vec!["text/plain".to_string()];
    // 켜진 동안: 목록과 일회용 암호가 PTY 입력으로 간다
    let script = "stty raw -echo min 0 time 30; printf '\\033[?5522h'; \
                  r=$(dd bs=1024 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\"; sleep 1";
    let (s, rx) = spawn(script, true, None);
    wait_mode(&s, true);
    assert!(s.kitty_paste_event(&avail));
    let _ = wait_exit(&rx);
    let text = parse_reply(&s);
    let packets: Vec<&str> = text.split("\x1b\\").filter(|p| !p.is_empty()).collect();
    assert_eq!(packets.len(), 3, "{text:?}");
    let pw_of = |p: &str| {
        let b = p
            .split(":pw=")
            .nth(1)
            .expect("pw")
            .split(';')
            .next()
            .unwrap()
            .to_string();
        String::from_utf8(bt_term::kitty_clip::b64_decode(b.as_bytes()).unwrap()).unwrap()
    };
    let otp = pw_of(packets[0]);
    assert_eq!(otp.len(), 22);
    assert!(packets.iter().all(|p| pw_of(p) == otp), "모든 패킷에 같은 암호");
    assert!(packets[0].starts_with("\x1b]5522;type=read:status=OK:pw="));
    assert!(packets[1].contains("status=DATA:mime=Lg==:pw="));
    assert!(packets[1].ends_with(";dGV4dC9wbGFpbgo="));
    assert!(packets[2].contains("status=DONE"));
    // 요청 글은 하나도 섞이지 않는다
    assert!(!text.contains("clipboard"), "{text:?}");
}

#[test]
fn paste_event_is_not_sent_while_mode_5522_is_off() {
    let (s, rx) = spawn(&reader("sleep 0.2"), true, None);
    assert!(!s.kitty_paste_mode());
    assert!(!s.kitty_paste_event(&["text/plain".to_string()]));
    let _ = wait_exit(&rx);
    assert_eq!(parse_reply(&s), "", "아무것도 나가지 않았다");
}

#[test]
fn each_paste_event_mints_a_different_password() {
    let avail = vec!["text/plain".to_string()];
    let script = "stty raw -echo min 0 time 30; printf '\\033[?5522h'; \
                  sleep 0.7; r=$(dd bs=2048 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\"; sleep 1";
    let (s, rx) = spawn(script, true, None);
    wait_mode(&s, true);
    assert!(s.kitty_paste_event(&avail));
    assert!(s.kitty_paste_event(&avail));
    let _ = wait_exit(&rx);
    let text = parse_reply(&s);
    let pws: Vec<&str> = text
        .split(":pw=")
        .skip(1)
        .map(|p| p.split([';', '\x1b']).next().unwrap())
        .collect();
    assert!(pws.len() >= 6, "{text:?}");
    assert_eq!(pws[0], pws[1]);
    assert_ne!(pws[0], pws[3], "이벤트마다 암호가 다르다");
}
