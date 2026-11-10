//! XTSAVE·XTRESTORE(`CSI ? Pm s`·`CSI ? Pm r`, #313). 기준: Ghostty stream.zig 2560~2630줄(해석), stream_handler.zig 297~304줄(복원은 setMode),
//! modes.zig `save`·`restore`(모드마다 마지막 저장 값 하나, 저장하지 않은 모드는 모드의 기본값으로). PTY는 `/bin/sh`와 `printf`만 쓴다

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{Emulator, GridSize, Session, SessionEvent, SpawnOptions};

fn emu() -> Emulator {
    Emulator::new(GridSize::new(20, 5), Theme::default())
}

fn reply_to(query: &str) -> String {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let script = format!(
        "stty raw -echo min 0 time 5; printf '{query}'; sleep 0.3; \
         r=$(dd bs=256 count=1 2>/dev/null | od -An -v -tx1 | tr -d ' \\n'); printf 'R[%s]\\n' \"$r\""
    );
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let s = Session::spawn(o, GridSize::new(80, 6), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            break;
        }
    }
    let text = s.snapshot().text();
    let start = text.find("R[").unwrap_or_else(|| panic!("답 줄이 없다:\n{text}")) + 2;
    let end = text[start..].find(']').expect("닫는 ]") + start;
    // 긴 답은 화면에서 줄이 접혀 줄바꿈이 끼므로 16진 숫자만 모은다
    let hex: String = text[start..end].chars().filter(char::is_ascii_hexdigit).collect();
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

fn decrqm(setup: &str, mode: u16) -> String {
    reply_to(&format!("{setup}\\033[?{mode}$p"))
}

fn state(setup: &str, mode: u16) -> char {
    let r = decrqm(setup, mode);
    // 2031·2033·2048을 켜면 알림이 먼저 나올 수 있다. 마지막 DECRPM(ESC [ ? mode ; 상태 $ y)만 본다
    let tail = &r[r.rfind("\x1b[?").unwrap_or_else(|| panic!("답이 없다: {r:?}"))..];
    assert!(tail.starts_with(&format!("\x1b[?{mode};")), "{r:?}");
    tail.trim_end_matches("$y").chars().last().unwrap()
}

#[test]
fn save_change_restore_returns_the_saved_value() {
    // 25: 기본 켬. 저장하고 끄고 복원하면 켬
    assert_eq!(state("\\033[?25s\\033[?25l\\033[?25r", 25), '1');
    // 1000: 기본 꺼짐. 켠 채로 저장, 끄고, 복원하면 켬
    assert_eq!(state("\\033[?1000h\\033[?1000s\\033[?1000l\\033[?1000r", 1000), '1');
    // 저장하지 않고 바꾼 값은 복원이 되돌리지 못한다(복원은 저장 값이다): 저장 뒤 켜진 모드가 꺼짐으로 돌아간다
    assert_eq!(state("\\033[?1004s\\033[?1004h\\033[?1004r", 1004), '2');
}

#[test]
fn only_the_last_save_of_a_mode_is_kept() {
    // 꺼짐 저장 뒤 켜고 다시 저장하면 켬이 남는다
    assert_eq!(state("\\033[?7l\\033[?7s\\033[?7h\\033[?7s\\033[?7l\\033[?7r", 7), '1');
    // 복원은 저장을 지우지 않는다: 두 번 복원해도 같은 값
    assert_eq!(
        state(
            "\\033[?2004h\\033[?2004s\\033[?2004l\\033[?2004r\\033[?2004l\\033[?2004r",
            2004
        ),
        '1'
    );
}

#[test]
fn restoring_an_unsaved_mode_gives_the_ghostty_default() {
    // modes.zig: 저장 표도 모드의 기본값으로 시작한다. 기본 켬인 모드
    assert_eq!(state("\\033[?25l\\033[?25r", 25), '1');
    assert_eq!(state("\\033[?7l\\033[?7r", 7), '1');
    assert_eq!(state("\\033[?1007l\\033[?1007r", 1007), '1');
    assert_eq!(state("\\033[?1035l\\033[?1035r", 1035), '1');
    assert_eq!(state("\\033[?1036l\\033[?1036r", 1036), '1');
    // 기본 꺼짐인 모드
    assert_eq!(state("\\033[?1000h\\033[?1000r", 1000), '2');
    assert_eq!(state("\\033[?2004h\\033[?2004r", 2004), '2');
}

#[test]
fn several_modes_in_one_sequence_and_unknown_ones_are_skipped() {
    let setup = "\\033[?7;2004;1004;25s\\033[?7l\\033[?2004h\\033[?1004h\\033[?25l\\033[?7;99999;2004;1004;25r";
    assert_eq!(state(setup, 7), '1');
    assert_eq!(state(setup, 2004), '2');
    assert_eq!(state(setup, 1004), '2');
    assert_eq!(state(setup, 25), '1');
}

#[test]
fn ansi_form_and_plain_forms_are_not_xtsave() {
    // `CSI s`는 커서 저장, `CSI r`은 DECSTBM, 중간 바이트가 다른 것(`CSI > 7 s`)은 모드를 건드리지 않는다
    assert_eq!(state("\\033[?7l\\033[s\\033[r", 7), '2');
    assert_eq!(state("\\033[?7s\\033[?7l\\033[>7s\\033[>7r", 7), '2');
}

#[test]
fn restore_runs_the_same_side_effects_as_decset() {
    // 대체 화면(1049): 주 화면에서 저장, 대체 화면으로 가서 쓰고, 복원하면 주 화면과 글자가 돌아온다
    let mut e = emu();
    e.feed(b"MAIN\x1b[?1049s\x1b[?1049hALT");
    assert!(e.snapshot().text().contains("ALT"), "대체 화면");
    e.feed(b"\x1b[?1049r");
    let t = e.snapshot().text();
    assert!(t.contains("MAIN") && !t.contains("ALT"), "복원 뒤 주 화면: {t}");
    // 대체 화면에서 저장한 값(켬)으로 복원하면 다시 대체 화면
    let mut e = emu();
    e.feed(b"MAIN\x1b[?1049h\x1b[?1049s\x1b[?1049lX\x1b[?1049r");
    assert!(!e.snapshot().text().contains("MAIN"), "대체 화면으로 다시 들어감");
    // 원점 모드(6)를 복원하면 커서가 홈으로 간다(DECSET과 같은 길)
    let mut e = emu();
    e.feed(b"\x1b[?6h\x1b[?6s\x1b[?6l\x1b[3;3H\x1b[?6r");
    let s = e.snapshot();
    assert_eq!((s.cursor.col, s.cursor.row), (0, 0), "원점 모드 복원은 커서를 홈으로");
}

#[test]
fn ris_clears_the_saved_values() {
    // RIS 뒤에는 저장이 없다: 7을 꺼 둔 채 저장했어도 복원하면 기본(켬)
    assert_eq!(state("\\033[?7l\\033[?7s\\033c\\033[?7l\\033[?7r", 7), '1');
}

#[test]
fn scanner_modes_are_saved_and_restored_too() {
    // 2048(크기 알림), 2031(색 구성표 알림), 2033(가시성 보고)은 세션의 스캐너가 쥔다
    assert_eq!(state("\\033[?2048h\\033[?2048s\\033[?2048l\\033[?2048r", 2048), '1');
    assert_eq!(state("\\033[?2031h\\033[?2031s\\033[?2031l\\033[?2031r", 2031), '1');
    assert_eq!(state("\\033[?2033h\\033[?2033s\\033[?2033l\\033[?2033r", 2033), '1');
    // 저장하지 않은 것을 복원하면 기본(꺼짐)
    assert_eq!(state("\\033[?2031h\\033[?2031r", 2031), '2');
    // 꺼 둔 채 저장하고 켠 뒤 복원하면 꺼짐
    assert_eq!(state("\\033[?2033s\\033[?2033h\\033[?2033r", 2033), '2');
    // 한 줄에 여러 모드
    let setup = "\\033[?2031;7s\\033[?2031h\\033[?7l\\033[?2031;7r";
    assert_eq!(state(setup, 2031), '2');
    assert_eq!(state(setup, 7), '1');
}
