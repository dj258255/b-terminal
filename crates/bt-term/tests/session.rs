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

#[test]
fn kitty_keyboard_query_gets_an_answer_like_claude_code_expects() {
    // 프로그램이 CSI ? u로 묻고, 터미널이 CSI ? <플래그> u로 답하는지(Claude Code가 지원 여부를 이렇게 본다)
    let script = "stty raw -echo; printf '\\033[>1u\\033[?u'; ans=$(dd bs=1 count=5 2>/dev/null | od -An -tx1 | tr -d ' \\n'); stty sane; printf 'ans=%s\\n' \"$ans\"";
    let (s, rx) = sh(script, GridSize::new(60, 5));
    wait_exit(&rx);
    let text = s.snapshot().text();
    assert!(text.contains("ans=1b5b3f3175"), "ESC [ ? 1 u 가 와야 한다: {text}");
}

#[test]
fn osc52_write_reaches_the_app_only_when_allowed() {
    let run = |allow: bool| {
        let (tx, rx) = channel();
        let tx = std::sync::Mutex::new(tx);
        let sink = Arc::new(move |e: SessionEvent| {
            let _ = tx.lock().unwrap().send(e);
        });
        let opts = SpawnOptions {
            program: Some("/bin/sh".into()),
            // "복사됨"을 base64로
            args: vec!["-c".into(), "printf '\\033]52;c;67O17IKs65CoCg==\\007'".into()],
            clipboard_write: allow,
            ..Default::default()
        };
        let _s = Session::spawn(opts, GridSize::new(20, 3), Theme::default(), sink).unwrap();
        let mut got = None;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(SessionEvent::ClipboardStore(t)) => got = Some(t),
                Ok(SessionEvent::Exited(_)) => break,
                _ => {}
            }
        }
        got
    };
    assert_eq!(run(true).as_deref(), Some("복사됨\n"));
    assert_eq!(run(false), None, "꺼져 있으면 클립보드에 닿지 않는다");
}

#[test]
fn shell_integration_marks_become_a_command_record() {
    let script = r#"for i in 1 2 3 4 5 6 7 8; do printf 'filler %s\n' $i; done
printf '\033]133;A\007$ '
printf '\033]633;E;make test\007\033]133;C\007'
printf '\033[32mok\033[0m 12 passed\r\n'
for i in 1 2 3 4 5 6; do printf 'line %s\r\n' $i; done
printf '\033]133;D;3\007\033]133;A\007$ '
sleep 1"#;
    let (s, rx) = sh(script, GridSize::new(40, 4));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut rec = None;
    let mut started = None;
    while Instant::now() < deadline && rec.is_none() {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::CommandStarted(c)) => started = Some(c),
            Ok(SessionEvent::CommandFinished(r)) => rec = Some(r),
            _ => {}
        }
    }
    let rec = rec.expect("명령 기록이 오지 않았다");
    assert_eq!(started, Some(Some("make test".to_string())));
    assert_eq!(rec.command.as_deref(), Some("make test"));
    assert_eq!(rec.exit_code, Some(3));
    assert_eq!(
        rec.output,
        "ok 12 passed\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\n"
    );
    assert!(s.has_shell_integration());
    // 화면이 4줄이라 앞 프롬프트는 스크롤백에 있다. 앞 프롬프트로 가면 화면이 위로 간다
    assert!(s.jump_to_prompt(-1));
    assert!(s.snapshot().display_offset > 0);
    assert!(s.jump_to_prompt(1));
}

/// #126: 단어 고르기의 경계 글자를 설정으로 바꾼다(selection-word-chars)
#[test]
fn word_selection_follows_selection_word_chars() {
    let pick = |chars: Option<&str>| {
        let (tx, rx) = channel();
        let tx = std::sync::Mutex::new(tx);
        let sink = Arc::new(move |e: SessionEvent| {
            let _ = tx.lock().unwrap().send(e);
        });
        let opts = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), "printf 'foo:bar baz'".into()],
            cwd: Some(std::env::temp_dir()),
            word_chars: chars.map(str::to_string),
            ..Default::default()
        };
        let s = Session::spawn(opts, GridSize::new(40, 5), Theme::default(), sink).expect("spawn");
        wait_exit(&rx);
        wait_until(&s, |t| t.contains("baz"));
        s.select_start(1, 0, bt_term::SelectKind::Word);
        s.selection_text()
    };
    assert_eq!(pick(None).as_deref(), Some("foo"), "VT 엔진 기본은 :에서 끊는다");
    assert_eq!(pick(Some(" ")).as_deref(), Some("foo:bar"), "공백만 경계");
}

/// #136: 화면에 보이는 프롬프트 시작 줄(셸 통합 OSC 133;A)을 화면 맨 위 기준으로 돌려준다
#[test]
fn prompt_rows_are_the_visible_prompt_marks() {
    let (s, rx) = sh(
        "printf 'out 1\\n\\033]133;A\\007$ ls\\nfile\\n\\033]133;A\\007$ '",
        GridSize::new(20, 6),
    );
    wait_exit(&rx);
    wait_until(&s, |t| t.contains("file"));
    assert_eq!(s.prompt_rows(), vec![1, 3]);
}

/// #138: 고른 것의 끝점을 키보드로 옮긴다(adjust_selection). 고른 것이 없으면 아무것도 안 한다
#[test]
fn adjust_selection_moves_the_selection_end() {
    use bt_term::SelectionAdjust::*;
    let (s, rx) = sh("printf 'abcdef\\nghijkl\\n'", GridSize::new(20, 5));
    wait_exit(&rx);
    wait_until(&s, |t| t.contains("ghijkl"));
    assert!(!s.adjust_selection(Right), "고른 것이 없으면 거짓");
    s.select_start(0, 0, bt_term::SelectKind::Simple);
    s.select_update(1, 0);
    assert_eq!(s.selection_text().as_deref(), Some("ab"));
    assert!(s.adjust_selection(Right));
    assert_eq!(s.selection_text().as_deref(), Some("abc"));
    assert!(s.adjust_selection(Down));
    assert_eq!(s.selection_text().as_deref(), Some("abcdef\nghi"));
    assert!(s.adjust_selection(BeginningOfLine));
    assert_eq!(s.selection_text().as_deref(), Some("abcdef\ng"));
}

/// #147: 스크롤백을 만든 뒤 위로 올려 두고, 셸이 새 출력을 내게 한다. 맨 아래로 돌아왔는지(display_offset 0) 돌려준다
fn offset_after_new_output(scroll_on_output: bool) -> usize {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let script = "i=0; while [ $i -lt 30 ]; do printf 'row%s\\n' $i; i=$((i+1)); done; \
                  read x; printf 'tail1\\ntail2\\ntail3\\n'; read y";
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        scroll_on_output,
        ..Default::default()
    };
    let s = Session::spawn(opts, GridSize::new(40, 5), Theme::default(), sink).expect("spawn");
    wait_until(&s, |t| t.contains("row29"));
    s.scroll(10);
    assert!(s.snapshot().display_offset > 0, "위로 올라가 있어야 한다");
    // 스크롤 자리를 바꾸지 않는 쓰기(write)로 read를 풀어 새 출력을 낸다
    s.write(b"\n");
    // 새 출력이 읽기 스레드에서 처리될 때까지 기다린다
    std::thread::sleep(Duration::from_millis(500));
    let off = s.snapshot().display_offset;
    s.write(b"\n");
    let _ = wait_exit(&rx);
    off
}

#[test]
fn new_output_keeps_scroll_position_unless_scroll_on_output() {
    assert!(
        offset_after_new_output(false) > 0,
        "output가 꺼져 있으면 보던 자리 그대로"
    );
    assert_eq!(offset_after_new_output(true), 0, "output가 켜져 있으면 맨 아래로");
}

#[test]
fn write_input_scrolls_to_bottom_only_when_asked() {
    let (s, _rx) = sh(
        "i=0; while [ $i -lt 30 ]; do printf 'row%s\\n' $i; i=$((i+1)); done; read x",
        GridSize::new(40, 5),
    );
    wait_until(&s, |t| t.contains("row29"));
    s.scroll(10);
    s.write_input(b"", true);
    assert!(s.snapshot().display_offset > 0, "빈 입력은 아무것도 하지 않는다");
    s.write_input(b"a", false);
    assert!(s.snapshot().display_offset > 0, "scroll이 꺼져 있으면 그대로");
    s.write_input(b"b", true);
    assert_eq!(s.snapshot().display_offset, 0, "scroll이 켜져 있으면 맨 아래로");
}

// ---- 제목·색 보고(#151) ----

/// 질의를 찍고 PTY로 돌아온 바이트를 16진수로 화면에 적는 /bin/sh 스크립트. 답이 없으면 `R[]`
fn query_script(query: &str) -> String {
    format!(
        "stty raw -echo min 0 time 5; printf '{query}'; sleep 0.3; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    )
}

fn hex_of(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// 질의를 보내고 돌아온 답을 16진수 문자열로 받는다
fn reply_to(query: &str, opts: impl FnOnce(&mut SpawnOptions)) -> String {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let mut o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), query_script(query)],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    opts(&mut o);
    let s = Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).expect("spawn");
    wait_exit(&rx);
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    text[start..end].to_string()
}

fn bg_hex(fmt: &str) -> String {
    let b = Theme::default().background;
    let body = match fmt {
        "16" => format!("{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}", b.r, b.g, b.b),
        _ => format!("{:02x}/{:02x}/{:02x}", b.r, b.g, b.b),
    };
    hex_of(&format!("\x1b]11;rgb:{body}\x07"))
}

#[test]
fn osc_color_query_default_is_16_bit() {
    assert_eq!(reply_to("\\033]11;?\\007", |_| {}), bg_hex("16"));
}

#[test]
fn osc_color_query_8_bit() {
    let got = reply_to("\\033]11;?\\007", |o| {
        o.osc_color_report_format = bt_core::config::OscColorReportFormat::Bit8
    });
    assert_eq!(got, bg_hex("8"));
}

#[test]
fn osc_color_query_none_gets_no_reply() {
    let got = reply_to("\\033]11;?\\007", |o| {
        o.osc_color_report_format = bt_core::config::OscColorReportFormat::None
    });
    assert_eq!(got, "");
    // 팔레트(OSC 4)·글자색(OSC 10) 질의도 답하지 않는다
    let got = reply_to("\\033]4;1;?\\007\\033]10;?\\007", |o| {
        o.osc_color_report_format = bt_core::config::OscColorReportFormat::None
    });
    assert_eq!(got, "");
}

#[test]
fn title_report_is_off_by_default() {
    // 출력이 정한 제목이 셸 입력으로 돌아오면 명령이 될 수 있다
    assert_eq!(reply_to("\\033]2;rm -rf x\\007\\033[21t", |_| {}), "");
}

#[test]
fn title_report_on_replies_with_title() {
    let got = reply_to("\\033]2;hello\\007\\033[21t", |o| o.title_report = true);
    assert_eq!(got, hex_of("\x1b]lhello\x1b\\"));
}

#[test]
fn feed_local_draws_after_the_process_exited() {
    // 명령이 끝난 칸에 안내를 그린다(wait-after-command, #178). PTY로는 가지 않는다
    let (s, rx) = sh("printf 'bye\\n'; exit 2", GridSize::new(40, 10));
    assert_eq!(wait_exit(&rx), Some(2));
    s.feed_local("\r\n끝났습니다 (2)\r\n".as_bytes());
    let text = s.snapshot().text();
    assert!(text.contains("bye") && text.contains("끝났습니다 (2)"), "{text}");
}

#[test]
fn title_report_strips_control_chars() {
    // 제목 안의 제어 문자(0x01, 0x7f)는 지우고 답한다
    let got = reply_to("\\033]2;a\\001b\\177c\\007\\033[21t", |o| o.title_report = true);
    assert_eq!(got, hex_of("\x1b]labc\x1b\\"));
}

// ---- VT 제한 키(#176): enquiry-response, vt-kam-allowed, vt-window-resize-allowed, image-storage-limit ----

#[test]
fn enquiry_gets_no_answer_by_default() {
    assert_eq!(reply_to("\\005", |_| {}), "");
}

#[test]
fn enquiry_answers_with_the_configured_text() {
    let got = reply_to("\\005", |o| o.enquiry_response = "b-term".into());
    assert_eq!(got, hex_of("b-term"));
    // 두 번 물으면 두 번 답한다
    let got = reply_to("\\005\\005", |o| o.enquiry_response = "ok".into());
    assert_eq!(got, hex_of("okok"));
}

#[test]
fn enquiry_byte_inside_a_string_sequence_is_not_a_question() {
    // OSC 제목 안의 0x05는 글자일 뿐이다
    let got = reply_to("\\033]2;a\\005b\\007", |o| o.enquiry_response = "NO".into());
    assert_eq!(got, "");
}

/// 스크립트가 READY를 찍은 뒤 사람이 `x`를 친 것처럼 쓰고, 스크립트가 받은 바이트를 16진수로 돌려준다
fn typed_after(setup: &str, opts: impl FnOnce(&mut SpawnOptions)) -> String {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let script = format!(
        "stty raw -echo min 0 time 15; printf '{setup}'; printf READY; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    );
    let mut o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    opts(&mut o);
    let s = Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).expect("spawn");
    wait_until(&s, |t| t.contains("READY"));
    s.write_input(b"x", false);
    wait_exit(&rx);
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    text[start..end].to_string()
}

#[test]
fn kam_is_ignored_unless_allowed() {
    // 허용하지 않으면 키보드 잠금 요청을 무시한다: 친 글이 간다
    assert_eq!(typed_after("\\033[2h", |_| {}), "78");
}

#[test]
fn kam_locks_the_keyboard_when_allowed_and_unlocks() {
    let allow = |o: &mut SpawnOptions| o.vt_kam_allowed = true;
    assert_eq!(typed_after("\\033[2h", allow), "", "잠기면 키 입력이 PTY에 가지 않는다");
    assert_eq!(typed_after("\\033[2h\\033[2l", allow), "78", "CSI 2 l로 푼다");
    assert_eq!(typed_after("\\033[2h\\033c", allow), "78", "초기화(RIS)가 푼다");
    // 비공개 모드(?2)와 다른 모드(4)는 KAM이 아니다
    assert_eq!(typed_after("\\033[?2h\\033[4h", allow), "78");
}

/// 프로그램이 `printf`로 찍은 것에 세션이 낸 창 크기 요청 사건을 모은다
fn resize_events(script: &str, allow: bool) -> Vec<(u16, u16)> {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        vt_window_resize_allowed: allow,
        ..Default::default()
    };
    let _s = Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).expect("spawn");
    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SessionEvent::WindowResize { rows, cols }) => got.push((rows, cols)),
            Ok(SessionEvent::Exited(_)) => break,
            _ => {}
        }
    }
    got
}

#[test]
fn window_resize_request_reaches_the_app_only_when_allowed() {
    let script = "printf '\\033[8;30;100t'";
    assert_eq!(resize_events(script, true), vec![(30, 100)]);
    assert_eq!(resize_events(script, false), vec![], "꺼져 있으면 앱에 알리지 않는다");
}

#[test]
fn window_resize_request_forms() {
    // 0이나 빠진 쪽은 0으로 알린다(그대로 두라는 뜻). 다른 CSI t와 뒤섞이지 않는다
    let script = "printf '\\033[8;0;120t\\033[8;40t\\033[8;;90t\\033[14t\\033[18t\\033[22;0t'";
    assert_eq!(resize_events(script, true), vec![(0, 120), (40, 0), (0, 90)]);
}

#[test]
fn kitty_graphics_query_follows_image_storage_limit() {
    let q = "\\033_Ga=q,i=1,s=1,v=1,f=24;AAAA\\033\\\\";
    assert_eq!(reply_to(q, |_| {}), hex_of("\x1b_Gi=1;OK\x1b\\"));
    // 0이면 그림 프로토콜을 쓰지 않는다: 질의에도 답하지 않는다
    assert_eq!(reply_to(q, |o| o.image_storage_limit = Some(0)), "");
    assert_eq!(
        reply_to(q, |o| o.image_storage_limit = Some(1_000_000)),
        hex_of("\x1b_Gi=1;OK\x1b\\")
    );
}

fn sh_with(script: &str, opts: impl FnOnce(&mut SpawnOptions)) -> (Session, Receiver<SessionEvent>) {
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
    (
        Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).expect("spawn"),
        rx,
    )
}

#[test]
fn env_option_reaches_the_child_and_beats_built_in_variables() {
    let (s, rx) = sh_with("printf '%s|%s|%s' \"$FOO\" \"$TERM_PROGRAM\" \"$BT_TAB_ID\"", |o| {
        o.env = vec![
            ("BT_TAB_ID".into(), "7".into()),
            ("FOO".into(), "bar".into()),
            ("TERM_PROGRAM".into(), "mine".into()),
        ];
    });
    wait_exit(&rx);
    assert_eq!(s.snapshot().row_text(0), "bar|mine|7");
}

#[test]
fn term_option_sets_term_and_empty_keeps_the_default() {
    let (s, rx) = sh_with("printf '%s' \"$TERM\"", |o| o.term = "screen-256color".into());
    wait_exit(&rx);
    assert_eq!(s.snapshot().row_text(0), "screen-256color");
    let (s, rx) = sh_with("printf '%s' \"$TERM\"", |_| {});
    wait_exit(&rx);
    assert_eq!(s.snapshot().row_text(0), "xterm-256color");
    // env의 TERM이 term보다 이긴다(Ghostty처럼)
    let (s, rx) = sh_with("printf '%s' \"$TERM\"", |o| {
        o.term = "screen".into();
        o.env = vec![("TERM".into(), "from-env".into())];
    });
    wait_exit(&rx);
    assert_eq!(s.snapshot().row_text(0), "from-env");
}

#[test]
fn startup_input_is_written_to_the_pty() {
    let (s, _rx) = sh_with("read x; printf '[%s]' \"$x\"; sleep 5", |o| {
        o.input = b"hello\n".to_vec()
    });
    wait_until(&s, |t| t.contains("[hello]"));
}

#[test]
fn big_startup_input_does_not_block_spawn() {
    // 아무도 읽지 않는 큰 입력이 칸 띄우기를 붙잡으면 안 된다
    let t = Instant::now();
    let (_s, _rx) = sh_with("sleep 3", |o| o.input = vec![b'a'; 1 << 20]);
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

/// 100줄을 찍고 끝나는 칸(5줄 화면): 스크롤백이 쌓인다
fn long_output() -> Session {
    let (s, rx) = sh(
        "i=1; while [ $i -le 60 ]; do printf 'L%d\\n' $i; i=$((i+1)); done",
        GridSize::new(20, 5),
    );
    wait_exit(&rx);
    wait_until(&s, |t| t.contains("L60"));
    s
}

fn top_row(s: &Session) -> String {
    s.snapshot().row_text(0)
}

#[test]
fn scroll_to_row_counts_from_the_top_of_the_scrollback() {
    let s = long_output();
    s.scroll_to_row(0);
    assert_eq!(top_row(&s), "L1", "0은 스크롤백 맨 위");
    s.scroll_to_row(10);
    assert_eq!(top_row(&s), "L11");
    s.scroll_to_row(0);
    s.scroll_to_row(30);
    assert_eq!(top_row(&s), "L31", "위에서 아래로도 간다");
    s.scroll_to_row(1_000_000);
    assert!(s.snapshot().text().contains("L60"), "범위를 넘으면 맨 아래");
}

#[test]
fn scroll_to_selection_shows_the_selection_and_does_nothing_without_one() {
    let s = long_output();
    s.scroll(1000);
    assert_eq!(top_row(&s), "L1");
    assert!(!s.scroll_to_selection(), "고른 것이 없으면 거짓");
    assert_eq!(top_row(&s), "L1", "고른 것이 없으면 움직이지 않는다");

    s.scroll_to_row(26);
    assert_eq!(top_row(&s), "L27");
    s.select_start(0, 0, bt_term::SelectKind::Simple);
    s.select_update(2, 0);
    s.scroll(1000);
    assert_eq!(top_row(&s), "L1");
    assert!(s.scroll_to_selection());
    assert_eq!(top_row(&s), "L27", "고른 줄이 화면 맨 위에 온다");

    // 이미 보이면 움직이지 않는다
    s.scroll(1);
    let before = top_row(&s);
    assert!(s.scroll_to_selection());
    assert_eq!(top_row(&s), before);
}

#[test]
fn reset_is_ris_screen_scrollback_and_modes() {
    let (s, rx) = sh(
        "printf '\\033[?1h\\033[31mhello\\n\\n\\n\\n\\n\\n\\n'",
        GridSize::new(20, 5),
    );
    wait_exit(&rx);
    assert!(s.modes().app_cursor, "전제: 커서 키 모드가 켜져 있다");
    s.scroll(1000);
    assert_eq!(top_row(&s), "hello", "전제: hello가 스크롤백에 있다");
    s.scroll_to_bottom();
    s.select_start(0, 0, bt_term::SelectKind::Simple);
    s.reset();
    let screen = s.snapshot();
    assert!(!screen.text().contains("hello"), "화면이 비었다");
    assert_eq!((screen.cursor.col, screen.cursor.row), (0, 0), "커서가 맨 위로");
    assert!(!s.modes().app_cursor, "모드가 기본으로");
    assert_eq!(s.selection_text(), None, "고른 것이 풀렸다");
    s.scroll(1000);
    assert!(!s.snapshot().text().contains("hello"), "스크롤백도 비었다");
}

// ---- 읽기 전용 칸(#209, ADR-018): 사람의 입력은 PTY에 한 바이트도 가지 않는다 ----

#[test]
fn readonly_pane_sends_no_input_to_the_pty() {
    let (s, _rx) = sh(
        "read line; printf 'got:%s\\n' \"$line\"; sleep 5",
        GridSize::new(40, 10),
    );
    s.set_readonly(true);
    assert!(s.readonly());
    // 키·붙여넣기(write_input)와 마우스·초점·text: 동작(write)이 모두 막힌다
    s.write_input("abc\r".as_bytes(), false);
    s.write("def\r".as_bytes());
    std::thread::sleep(Duration::from_millis(400));
    let text = s.snapshot().text();
    assert!(!text.contains("got:"), "읽기 전용인데 입력이 갔다:\n{text}");
    assert!(
        !text.contains("abc") && !text.contains("def"),
        "tty 되울림도 없어야 한다(바이트가 가지 않았다는 증거):\n{text}"
    );
    // 풀면 다시 간다
    s.set_readonly(false);
    assert!(!s.readonly());
    s.write("ghi\r".as_bytes());
    wait_until(&s, |t| t.contains("got:ghi"));
}

#[test]
fn readonly_pane_still_answers_terminal_queries() {
    // 프로그램이 물은 것(DSR 5n)에 대한 터미널의 답은 사람의 입력이 아니다. 막으면 묻고 기다리는 프로그램이 멎는다
    let (s, _rx) = sh(
        "stty raw -echo; printf '\\033[5n'; dd bs=4 count=1 2>/dev/null | od -An -c; sleep 5",
        GridSize::new(60, 10),
    );
    s.set_readonly(true);
    wait_until(&s, |t| t.contains("033") && t.contains('n'));
}

#[test]
fn scroll_metrics_report_rows_history_and_offset_for_the_scrollbar() {
    let s = long_output();
    let (rows, history, offset) = s.scroll_metrics();
    assert_eq!(rows, s.size().rows as usize);
    assert!(history > 0, "스크롤백이 쌓였다");
    assert_eq!(offset, 0, "맨 아래");
    s.scroll_to_row(0);
    assert_eq!(s.scroll_metrics().2, history, "맨 위는 offset == history");
    // 막대가 쓰는 화면 맨 위 줄 = history - offset
    s.scroll_to_row(10);
    let (_, h, o) = s.scroll_metrics();
    assert_eq!(h - o, 10);
}

// ---- 색 구성표 보고(#232): CSI ? 996 n, 모드 2031 ----

#[test]
fn color_scheme_query_answers_dark_by_default() {
    assert_eq!(reply_to("\\033[?996n", |_| {}), hex_of("\x1b[?997;1n"));
}

#[test]
fn color_scheme_query_answers_light_when_the_pane_is_light() {
    assert_eq!(
        reply_to("\\033[?996n", |o| o.color_scheme_light = true),
        hex_of("\x1b[?997;2n")
    );
}

#[test]
fn color_scheme_query_is_answered_each_time() {
    assert_eq!(
        reply_to("\\033[?996n\\033[?996n", |_| {}),
        hex_of("\x1b[?997;1n\x1b[?997;1n")
    );
}

/// 셸이 `setup`을 찍고 READY를 보인 뒤 읽은 바이트를 16진수로 돌려준다. `act`는 READY 뒤에 칸에 대고 한다
/// 답이 여러 번 따로 쓰일 수 있어(모드 2031 알림 둘) 한 번만 읽지 않고, 한 바이트씩 읽다가
/// 1.5초(`time 15`) 동안 오는 것이 없으면 끝낸다. 한 번만 읽으면 둘째 답을 놓쳐 병렬 실행에서 흔들렸다
fn notified_after(setup: &str, act: impl FnOnce(&Session)) -> String {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let script = format!(
        "stty raw -echo min 0 time 15; printf '{setup}'; printf READY; \
         r=$(dd bs=1 count=256 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    );
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let s = Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).expect("spawn");
    wait_until(&s, |t| t.contains("READY"));
    act(&s);
    wait_exit(&rx);
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    text[start..end].to_string()
}

#[test]
fn mode_2031_reports_a_look_change_but_not_when_turned_on() {
    // 켠 직후에는 보고하지 않는다(Ghostty와 같다). 어두움에서 밝음으로 바뀌면 `CSI ? 997 ; 2 n`
    let got = notified_after("\\033[?2031h", |s| {
        assert!(s.color_scheme_updates());
        s.set_color_scheme(false);
    });
    assert_eq!(got, hex_of("\x1b[?997;2n"));
}

#[test]
fn mode_2031_report_is_sent_once_per_change() {
    let got = notified_after("\\033[?2031h", |s| {
        s.set_color_scheme(true); // 처음부터 어두움이라 같은 값
        s.set_color_scheme(false);
        s.set_color_scheme(false); // 같은 값이 또
        s.set_color_scheme(true);
    });
    assert_eq!(got, hex_of("\x1b[?997;2n\x1b[?997;1n"));
}

#[test]
fn mode_2031_is_silent_when_off() {
    assert_eq!(notified_after("", |s| s.set_color_scheme(false)), "");
    let got = notified_after("\\033[?2031h\\033[?2031l", |s| {
        assert!(!s.color_scheme_updates());
        s.set_color_scheme(false);
    });
    assert_eq!(got, "");
}

#[test]
fn reset_turns_mode_2031_off() {
    let got = notified_after("\\033[?2031h\\033c", |s| {
        assert!(!s.color_scheme_updates());
        s.set_color_scheme(false);
    });
    assert_eq!(got, "");
}

#[test]
fn mode_2031_report_reaches_a_readonly_pane() {
    // 터미널의 답이라 읽기 전용 칸에서도 막지 않는다(ADR-018 결정 4)
    let got = notified_after("\\033[?2031h", |s| {
        s.set_readonly(true);
        s.set_color_scheme(false);
    });
    assert_eq!(got, hex_of("\x1b[?997;2n"));
}

// ---- OSC 22 마우스 포인터 모양(#239) ----

fn wait_shape(s: &Session, want: Option<bt_term::MouseShape>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while s.mouse_shape() != want {
        assert!(
            Instant::now() < deadline,
            "모양이 {want:?}가 되지 않았다: {:?}",
            s.mouse_shape()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn osc22_sets_the_pane_mouse_shape_and_empty_name_or_ris_resets_it() {
    use bt_term::MouseShape as M;
    let (s, _rx) = sh(
        "read go; printf '\\033]22;crosshair\\033\\\\'; printf A; sleep 1; \
         printf '\\033]22;banana\\007'; printf B; sleep 1; \
         printf '\\033]22;\\007'; printf C; sleep 1; \
         printf '\\033]22;wait\\007'; printf D; sleep 1; \
         printf '\\033c'; printf E; sleep 5",
        GridSize::new(80, 10),
    );
    // 셸은 줄 하나를 받기 전에는 아무것도 찍지 않는다: 처음 모양은 경쟁 없이 본다(#269)
    assert_eq!(s.mouse_shape(), None);
    s.write(b"\n");
    wait_shape(&s, Some(M::Crosshair));
    // 모르는 이름은 모양을 바꾸지 않는다
    wait_until(&s, |t| t.contains('B'));
    assert_eq!(s.mouse_shape(), Some(M::Crosshair));
    // 빈 이름은 되돌린다
    wait_shape(&s, None);
    // 다시 정하고, 초기화(RIS)로 되돌린다
    wait_shape(&s, Some(M::Wait));
    wait_shape(&s, None);
}

#[test]
fn session_reset_clears_the_mouse_shape() {
    use bt_term::MouseShape as M;
    let (s, _rx) = sh("printf '\\033]22;text\\007'; sleep 5", GridSize::new(80, 10));
    wait_shape(&s, Some(M::Text));
    s.reset();
    assert_eq!(s.mouse_shape(), None);
}

// ---- 마우스 보고 모드를 켜고 끌 때의 모양(#250) ----

#[test]
fn toggling_mouse_reporting_resets_the_program_shape_like_ghostty() {
    use bt_term::MouseShape as M;
    // Ghostty는 모드를 켜고 끌 때 모양을 default·text로 되돌린다(stream_handler.zig). b-terminal의 기본은 화살표라 둘 다 "정한 것 없음"
    let (s, _rx) = sh(
        "printf '\\033]22;crosshair\\007'; printf A; sleep 1; \
         printf '\\033[?1000h'; printf B; sleep 1; \
         printf '\\033]22;wait\\007'; printf C; sleep 1; \
         printf '\\033[?1000l'; printf D; sleep 1; \
         printf '\\033]22;help\\007'; printf '\\033[?1006h'; printf E; sleep 1; \
         printf '\\033[?1002h\\033]22;move\\007'; printf F; sleep 5",
        GridSize::new(80, 10),
    );
    wait_shape(&s, Some(M::Crosshair));
    // 모드를 켜면 프로그램이 앞서 정한 모양이 사라진다
    wait_shape(&s, None);
    // 켠 뒤에 정한 모양은 남는다
    wait_shape(&s, Some(M::Wait));
    // 끄면 다시 사라진다
    wait_shape(&s, None);
    // 모양을 정한 뒤 마우스 보고가 아닌 모드(SGR 형식)를 켜는 것은 모양을 건드리지 않는다
    wait_shape(&s, Some(M::Help));
    wait_until(&s, |t| t.contains('E'));
    assert_eq!(s.mouse_shape(), Some(M::Help));
    // 같은 덩어리에서 모드를 켠 뒤 정한 모양은 남는다(모드가 먼저, 신호가 나중)
    wait_shape(&s, Some(M::Move));
}

#[test]
fn password_input_follows_the_ptys_termios() {
    // 되울림을 끈 줄 입력(stty -echo, 암호 프롬프트와 같은 상태)
    let (s, _rx) = sh("printf a; stty -echo; printf b; sleep 8", GridSize::new(40, 10));
    wait_until(&s, |t| t.contains("ab"));
    assert_eq!(s.password_input(), Some(true));

    // 보통 셸 상태
    let (s, _rx) = sh("printf a; sleep 8", GridSize::new(40, 10));
    wait_until(&s, |t| t.contains('a'));
    assert_eq!(s.password_input(), Some(false));

    // 되울림도 줄 단위도 끈 원시 입력(vim 같은 것)은 암호가 아니다
    let (s, _rx) = sh("printf a; stty -echo -icanon; printf b; sleep 8", GridSize::new(40, 10));
    wait_until(&s, |t| t.contains("ab"));
    assert_eq!(s.password_input(), Some(false));

    // 다시 되울림을 켜면 풀린다
    let (s, _rx) = sh(
        "stty -echo; printf a; stty echo; printf b; sleep 8",
        GridSize::new(40, 10),
    );
    wait_until(&s, |t| t.contains("ab"));
    assert_eq!(s.password_input(), Some(false));
}

// ---- DCS 질의: DECRQSS, XTGETTCAP(#265) ----

fn none(_: &mut SpawnOptions) {}

#[test]
fn decrqss_sgr_answers_with_the_cursor_attributes() {
    // 굵게 + 빨강 + 물결 밑줄 뒤에 SGR을 묻는다
    assert_eq!(
        checksum_reply("\\033[1;31;4:3m", "\\033P$qm\\033\\\\", false, none),
        hex_of("\x1bP1$r0;1;4:3;31m\x1b\\")
    );
    // 속성이 없으면 0m
    assert_eq!(
        checksum_reply("", "\\033P$qm\\033\\\\", false, none),
        hex_of("\x1bP1$r0m\x1b\\")
    );
}

#[test]
fn decrqss_cursor_style_and_scroll_region() {
    // DECSCUSR 6(가는 막대, 깜박이지 않음)
    assert_eq!(
        checksum_reply("\\033[6 q", "\\033P$q q\\033\\\\", false, none),
        hex_of("\x1bP1$r6 q\x1b\\")
    );
    // DECSTBM: 화면은 10행(READY와 질의를 합쳐도 SGR은 영향이 없다)
    assert_eq!(
        checksum_reply("\\033[3;7r", "\\033P$qr\\033\\\\", false, none),
        hex_of("\x1bP1$r3;7r\x1b\\")
    );
}

#[test]
fn decrqss_unknown_gets_dcs_zero_and_too_long_gets_nothing() {
    assert_eq!(
        checksum_reply("", "\\033P$qz\\033\\\\", false, none),
        hex_of("\x1bP0$r\x1b\\")
    );
    // DECSLRM은 지원하지 않는다
    assert_eq!(
        checksum_reply("", "\\033P$qs\\033\\\\", false, none),
        hex_of("\x1bP0$r\x1b\\")
    );
    // 세 글자부터는 잘못된 명령이라 답이 없다
    assert_eq!(checksum_reply("", "\\033P$q\" q\\033\\\\", false, none), "");
}

#[test]
fn xtgettcap_answers_known_names_only() {
    // TN(544E)은 자식에게 준 TERM이다(기본 xterm-256color)
    // (답이 한 줄(80열)을 넘으면 화면 글에 줄바꿈이 끼므로 지우고 비교한다)
    assert_eq!(
        checksum_reply("", "\\033P+q544E\\033\\\\", false, none).replace('\n', ""),
        hex_of(&format!(
            "\x1bP1+r544E={}\x1b\\",
            hex_of("xterm-256color").to_uppercase()
        ))
    );
    // 모르는 이름(who)은 답이 없고, 아는 이름(Tc)만 답한다. 두 이름을 한 번에 물어도 된다
    assert_eq!(
        checksum_reply("", "\\033P+q77686f;5463\\033\\\\", false, none),
        hex_of("\x1bP1+r5463\x1b\\")
    );
    assert_eq!(checksum_reply("", "\\033P+q1B5D3532\\033\\\\", false, none), "");
}

#[test]
fn dcs_answers_still_come_from_a_readonly_pane() {
    // 읽기 전용 칸에서도 터미널의 답은 막지 않는다(ADR-018 결정 4)
    assert_eq!(
        checksum_reply("", "\\033P$qm\\033\\\\", true, none),
        hex_of("\x1bP1$r0m\x1b\\")
    );
}

// ---- DECRQCRA 체크섬 보고(#259) ----

/// 셸이 `setup`을 찍고 READY를 보인 뒤(`readonly`면 칸을 읽기 전용으로 하고 1초 뒤) `query`를 찍고, 돌아온 바이트를 16진수로 돌려준다.
/// 답이 여러 번 따로 올 수 있어 한 바이트씩 읽다가 1.5초 동안 오는 것이 없으면 끝낸다(`dd bs=1 count=256`, #244)
fn checksum_reply(setup: &str, query: &str, readonly: bool, opts: impl FnOnce(&mut SpawnOptions)) -> String {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let script = format!(
        "stty raw -echo min 0 time 15; printf '{setup}'; printf READY; sleep 1; printf '{query}'; \
         r=$(dd bs=1 count=256 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    );
    let mut o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    opts(&mut o);
    let s = Session::spawn(o, GridSize::new(80, 10), Theme::default(), sink).expect("spawn");
    wait_until(&s, |t| t.contains("READY"));
    if readonly {
        s.set_readonly(true);
    }
    wait_exit(&rx);
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    text[start..end].to_string()
}

fn xt_on(o: &mut SpawnOptions) {
    o.vt_xt_checksum_report = true;
}

/// 첫 줄 `abc`(97+98+99=294)의 DEC 체크섬: 합을 뒤집어 0x10000 - 294 = 0xFEDA
const ABC_DEC: &str = "FEDA";
const ABC_ROW: &str = "\\033[1;1;1;1;1;3*y";

fn dcs(id: u16, sum: &str) -> String {
    hex_of(&format!("\x1bP{id}!~{sum}\x1b\\"))
}

#[test]
fn checksum_report_is_off_by_default() {
    // 한 칸씩 물어 화면 전체를 읽어 갈 수 있는 길이라 Ghostty처럼 기본이 꺼짐이다
    assert_eq!(checksum_reply("abc", ABC_ROW, false, |_| {}), "");
    // XTCHECKSUM도 무시하고, 켜지 않았으니 아무 답도 없다
    assert_eq!(checksum_reply("abc\\033[1#y", ABC_ROW, false, |_| {}), "");
}

#[test]
fn checksum_report_answers_with_the_dec_checksum_when_on() {
    assert_eq!(checksum_reply("abc", ABC_ROW, false, xt_on), dcs(1, ABC_DEC));
    // 요청 번호(첫째)는 그대로 돌려주고, 쪽 번호(둘째)는 무시한다. 좌표를 생략하면 화면 전체다
    assert_eq!(
        checksum_reply("abc", "\\033[7;9;1;1;1;3*y", false, xt_on),
        dcs(7, ABC_DEC)
    );
    // (화면 첫 줄에는 READY도 있어 abcREADY = 667이다)
    assert_eq!(
        checksum_reply("abc", "\\033[2*y", false, xt_on),
        dcs(2, &format!("{:04X}", 0x10000 - 667))
    );
    // 비어 있는 사각형은 0000
    assert_eq!(
        checksum_reply("abc", "\\033[3;1;1;3;1;2*y", false, xt_on),
        dcs(3, "0000")
    );
    // 두 번 물으면 두 번 답한다
    assert_eq!(
        checksum_reply("abc", &format!("{ABC_ROW}{ABC_ROW}"), false, xt_on),
        format!("{}{}", dcs(1, ABC_DEC), dcs(1, ABC_DEC))
    );
}

#[test]
fn xtchecksum_changes_the_variant_until_reset() {
    // CSI 1 # y: 합을 뒤집지 않는다. 294 = 0x0126
    assert_eq!(checksum_reply("abc\\033[1#y", ABC_ROW, false, xt_on), dcs(1, "0126"));
    // CSI 0 # y와 CSI # y는 DEC 체크섬으로 돌린다
    assert_eq!(
        checksum_reply("abc\\033[1#y\\033[0#y", ABC_ROW, false, xt_on),
        dcs(1, ABC_DEC)
    );
    assert_eq!(
        checksum_reply("abc\\033[1#y\\033[#y", ABC_ROW, false, xt_on),
        dcs(1, ABC_DEC)
    );
    // 소프트 리셋(DECSTR)은 화면을 지우지 않고 계산법을 설정의 기본으로 되돌린다
    assert_eq!(
        checksum_reply("abc\\033[1#y\\033[!p", ABC_ROW, false, xt_on),
        dcs(1, ABC_DEC)
    );
    // 초기화(RIS)도 되돌린다(화면도 지워지므로 다시 찍는다)
    assert_eq!(
        checksum_reply("abc\\033[1#y\\033cabc", ABC_ROW, false, xt_on),
        dcs(1, ABC_DEC)
    );
    // 모르는 윗 비트는 버린다: 33은 하위 다섯 비트로 1
    assert_eq!(checksum_reply("abc\\033[33#y", ABC_ROW, false, xt_on), dcs(1, "0126"));
}

#[test]
fn xt_checksum_extension_setting_is_the_variant_after_reset() {
    // esctest의 `no-negate,no-attributes,no-trim,full`: 뒤집지 않고 속성도 안 더한다
    let esctest = |o: &mut SpawnOptions| {
        o.vt_xt_checksum_report = true;
        o.vt_xt_checksum_extension = bt_core::config::XtChecksumExtension {
            negate: false,
            attributes: false,
            trim: false,
            undrawn: false,
            full: true,
        };
    };
    // 첫 줄은 abcREADY(667)이고, 빼지 않으니 나머지 72칸(쓰지 않은 칸)도 빈칸(32)으로 센다
    let sum = format!("{:04X}", 667 + 72 * 32u32);
    assert_eq!(
        checksum_reply("abc", "\\033[1;1;1;1;1;80*y", false, esctest),
        dcs(1, &sum)
    );
    // 프로그램이 DEC로 바꿨다가 소프트 리셋하면 설정의 값으로 돌아온다
    assert_eq!(
        checksum_reply("abc\\033[0#y\\033[!p", "\\033[1;1;1;1;1;3*y", false, esctest),
        dcs(1, "0126")
    );
}

#[test]
fn checksum_counts_attributes_wide_chars_and_the_screen_position() {
    // 굵게 a: 97 + 0x80 = 225, 뒤집어 0xFF1F
    assert_eq!(
        checksum_reply("\\033[1ma", "\\033[1;1;1;1;1;1*y", false, xt_on),
        dcs(1, "FF1F")
    );
    // 둘째 줄의 b만 물으면 b만 센다
    assert_eq!(
        checksum_reply("a\\r\\nb", "\\033[1;1;2;1;2;1*y", false, xt_on),
        dcs(1, &format!("{:04X}", 0x10000 - 98))
    );
    // 넓은 글자는 두 칸 모두 ESC(0x1B)로 센다
    assert_eq!(
        checksum_reply("\\346\\251\\213", "\\033[1;1;1;1;1;2*y", false, xt_on),
        dcs(1, &format!("{:04X}", 0x10000 - 0x36))
    );
}

#[test]
fn checksum_reply_reaches_a_readonly_pane() {
    // 터미널의 답이라 읽기 전용 칸에서도 막지 않는다(ADR-018 결정 4)
    assert_eq!(checksum_reply("abc", ABC_ROW, true, xt_on), dcs(1, ABC_DEC));
}

// ---- CSI 질의(#264): XTVERSION, DA1·DA2, CSI 16·19 t, 모드 2048 ----

#[test]
fn xtversion_answers_with_the_name_and_version() {
    assert_eq!(
        reply_to("\\033[>q", |_| {}),
        hex_of(&format!("\x1bP>|b-terminal {}\x1b\\", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn primary_device_attributes_follow_clipboard_write_and_are_answered_once() {
    // 답이 하나뿐이어야 한다: VT 엔진의 옛 답(`?6c`)이 겹쳐 나오면 둘이 이어 붙는다
    assert_eq!(reply_to("\\033[c", |_| {}), hex_of("\x1b[?62;22c"));
    assert_eq!(
        reply_to("\\033[c", |o| o.clipboard_write = true),
        hex_of("\x1b[?62;22;52c")
    );
    assert_eq!(reply_to("\\033[0c", |_| {}), hex_of("\x1b[?62;22c"));
}

#[test]
fn secondary_device_attributes_are_ghosttys_and_answered_once() {
    assert_eq!(reply_to("\\033[>c", |_| {}), hex_of("\x1b[>1;10;0c"));
    assert_eq!(reply_to("\\033[>0c", |_| {}), hex_of("\x1b[>1;10;0c"));
}

#[test]
fn tertiary_device_attributes_get_no_answer() {
    assert_eq!(reply_to("\\033[=c", |_| {}), "");
}

#[test]
fn size_reports_cover_cell_and_screen_size_and_keep_the_old_two() {
    // 80x10 칸, 칸 하나 8x16 픽셀
    assert_eq!(reply_to("\\033[16t", |_| {}), hex_of("\x1b[6;16;8t"));
    assert_eq!(reply_to("\\033[19t", |_| {}), hex_of("\x1b[9;10;80t"));
    assert_eq!(reply_to("\\033[14t", |_| {}), hex_of("\x1b[4;160;640t"));
    assert_eq!(reply_to("\\033[18t", |_| {}), hex_of("\x1b[8;10;80t"));
    // 매개변수가 더 붙으면 답하지 않는다
    assert_eq!(reply_to("\\033[16;1t", |_| {}), "");
}

#[test]
fn decrqm_2048_says_reset_then_set() {
    assert_eq!(reply_to("\\033[?2048$p", |_| {}), hex_of("\x1b[?2048;2$y"));
    // 켜는 순간 크기를 한 번 보내고, 그 뒤 DECRQM은 켜짐이다
    assert_eq!(
        reply_to("\\033[?2048h\\033[?2048$p", |_| {}),
        hex_of("\x1b[48;10;80;160;640t\x1b[?2048;1$y")
    );
    assert_eq!(
        reply_to("\\033[?2048h\\033[?2048l\\033[?2048$p", |_| {}),
        hex_of("\x1b[48;10;80;160;640t\x1b[?2048;2$y")
    );
}

#[test]
fn mode_2048_reports_each_size_change_but_not_when_off() {
    let got = notified_after("\\033[?2048h", |s| {
        s.resize(GridSize::new(100, 30));
        s.resize(GridSize::new(100, 30)); // 같은 크기는 보고하지 않는다
        s.resize(GridSize::new(90, 20));
    });
    // 16진수 줄이 80열을 넘어 화면에서 줄바꿈된다
    let got = got.replace('\n', "");
    assert_eq!(
        got,
        hex_of("\x1b[48;10;80;160;640t\x1b[48;30;100;480;800t\x1b[48;20;90;320;720t")
    );
    assert_eq!(notified_after("", |s| s.resize(GridSize::new(100, 30))), "");
    let got = notified_after("\\033[?2048h\\033[?2048l", |s| s.resize(GridSize::new(100, 30)));
    assert_eq!(got, hex_of("\x1b[48;10;80;160;640t"));
}

#[test]
fn reset_turns_mode_2048_off() {
    let got = notified_after("\\033[?2048h\\033c", |s| s.resize(GridSize::new(100, 30)));
    assert_eq!(got, hex_of("\x1b[48;10;80;160;640t"));
    let got = notified_after("\\033[?2048h", |s| {
        s.reset();
        s.resize(GridSize::new(100, 30));
    });
    assert_eq!(got, hex_of("\x1b[48;10;80;160;640t"));
}
