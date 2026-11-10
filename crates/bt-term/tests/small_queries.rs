//! 작은 질의·제어: DECXCPR(`CSI ? 6 n`), ED 22(`CSI 22 J`), XTSHIFTESCAPE(`CSI > Ps s`)(#327).
//! 기준: Ghostty stream.zig 2386~2411줄(DSR, `?`면 question)·1905~1925줄(ED)·2630~2650줄(XTSHIFTESCAPE),
//! device_status.zig 110~119줄, stream_handler.zig 891~902줄(DECXCPR 답)·322줄, Terminal.zig 4452~4471줄(`scroll_complete`).
//! 엔진은 `Term`을 직접 만들어 읽는다. PTY를 띄우는 시험은 `/bin/sh`와 `printf`만 쓴다

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

struct Dims(usize, usize);
impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.1
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn columns(&self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy)]
struct Quiet;
impl EventListener for Quiet {}

struct T {
    term: Term<Quiet>,
    parser: Processor,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        T {
            term: Term::new(Config::default(), &Dims(cols, rows), Quiet),
            parser: Processor::new(),
        }
    }
    fn feed(&mut self, s: &str) -> &mut Self {
        self.parser.advance(&mut self.term, s.as_bytes());
        self
    }
    fn row(&self, r: i32) -> String {
        (0..self.term.columns())
            .map(|c| self.term.grid()[Line(r)][Column(c)].c)
            .collect::<String>()
            .trim_end()
            .to_string()
    }
    fn visible_blank(&self) -> bool {
        (0..self.term.screen_lines() as i32).all(|r| self.row(r).is_empty())
    }
}

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

fn wait_exit(rx: &Receiver<SessionEvent>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            return;
        }
    }
    panic!("10초 안에 끝나지 않았다");
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// 보낸 바이트에 대한 답(`want`와 같은 길이)을 PTY에서 읽어 16진수로 돌려준다. 답이 오지 않으면 읽다가 멈추므로 시험이 실패한다
fn answer_to(send: &str, want: &str, rows: u16) -> String {
    let n = want.len();
    let read = format!("$(dd bs=1 count={n} 2>/dev/null | od -An -tx1 | tr -d ' \\n')");
    let script = format!(
        "stty raw -echo min 0 time 20; printf '{send}'; a={read}; stty sane; printf '\\033[?69l\\033[?6l\\033[r\\033[H'; printf 'ans=%s\\n' \"$a\""
    );
    let (s, rx) = sh(&script, GridSize::new(80, rows));
    wait_exit(&rx);
    let text = s.snapshot().text();
    let line = text
        .lines()
        .map(str::trim_start)
        .find(|l| l.starts_with("ans="))
        .unwrap_or_else(|| panic!("답이 없다: {text}"));
    line["ans=".len()..].to_string()
}

#[test]
fn decxcpr_answers_with_row_column_and_page_one() {
    // 실측(0.25.0): 답이 없었다. Ghostty: `ESC [ ? 행 ; 열 ; 1 R`
    let want = "\x1b[?5;7;1R";
    assert_eq!(answer_to("\\033[5;7H\\033[?6n", want, 8), hex(want));
}

#[test]
fn decxcpr_is_relative_to_the_margins_in_origin_mode() {
    // Ghostty `cursorReportPos`: 원점 모드에서는 위 여백·왼쪽 여백 기준(CUP 3;4면 3;4, 절대 좌표면 4;6)
    let want = "\x1b[?3;4;1R";
    let send = "\\033[2;4r\\033[?69h\\033[3;8s\\033[?6h\\033[3;4H\\033[?6n";
    assert_eq!(answer_to(send, want, 8), hex(want));
}

#[test]
fn plain_dsr_6_keeps_its_answer_without_the_page() {
    let want = "\x1b[5;7R";
    assert_eq!(answer_to("\\033[5;7H\\033[6n", want, 8), hex(want));
}

#[test]
fn ed_22_pushes_the_visible_screen_to_scrollback_and_clears_it() {
    let mut t = T::new(10, 4);
    t.feed("one\r\ntwo\r\nthree\r\nfour");
    assert_eq!(t.term.history_size(), 0);
    t.feed("\x1b[22J");
    assert!(t.visible_blank(), "보이는 화면이 비어야 한다");
    // 지운 내용은 스크롤백에 남는다(ED 3으로 지우기 전까지)
    assert!(t.term.history_size() >= 4, "스크롤백 {}줄", t.term.history_size());
    assert_eq!(t.row(-4), "one");
    assert_eq!(t.row(-1), "four");
}

#[test]
fn ed_22_clears_the_pending_wrap() {
    let mut t = T::new(5, 3);
    t.feed("abcde");
    assert!(t.term.grid().cursor.input_needs_wrap);
    t.feed("\x1b[22J");
    assert!(!t.term.grid().cursor.input_needs_wrap);
    // 다음 글자는 줄을 바꾸지 않고 같은 자리에 찍힌다
    let line = t.term.grid().cursor.point.line;
    t.feed("x");
    assert_eq!(t.term.grid().cursor.point.line, line);
}

#[test]
fn ed_22_on_a_blank_screen_adds_no_scrollback() {
    let mut t = T::new(10, 4);
    t.feed("\x1b[22J");
    assert_eq!(t.term.history_size(), 0);
}

#[test]
fn decsed_22_scrolls_like_ed_22_even_over_protected_cells() {
    // #339: Ghostty stream.zig 1896~1925줄은 `CSI ? 22 J`도 `scroll_complete`로 보내고,
    // Terminal.zig 4452줄 `scrollClear`는 보호 표시를 보지 않는다
    let mut t = T::new(10, 4);
    t.feed("one\r\n\x1b[1\"qtwo\x1b[0\"q\r\nthree");
    t.feed("\x1b[?22J");
    assert!(t.visible_blank(), "보이는 화면이 비어야 한다(보호 칸 포함)");
    assert!(t.term.history_size() >= 3, "스크롤백 {}줄", t.term.history_size());
    assert_eq!(t.row(-3), "one");
    assert_eq!(t.row(-2), "two");
}

#[test]
fn decsed_22_with_extra_parameters_is_dropped() {
    let mut t = T::new(10, 4);
    t.feed("one\r\ntwo");
    t.feed("\x1b[?22;1J");
    assert_eq!(
        t.row(0),
        "one",
        "매개변수가 둘이면 버린다(Ghostty: params.len이 1이 아니면 버림)"
    );
    assert_eq!(t.term.history_size(), 0);
}

#[test]
fn ed_2_and_ed_3_still_work_next_to_ed_22() {
    let mut t = T::new(10, 3);
    t.feed("a\r\nb\x1b[2J");
    assert!(t.visible_blank());
    t.feed("\x1b[3J");
    assert_eq!(t.term.history_size(), 0);
}

#[test]
fn xtshiftescape_sets_and_clears_the_program_choice() {
    let mut t = T::new(10, 3);
    assert_eq!(t.term.mouse_shift_capture(), None);
    t.feed("\x1b[>1s");
    assert_eq!(t.term.mouse_shift_capture(), Some(true));
    t.feed("\x1b[>0s");
    assert_eq!(t.term.mouse_shift_capture(), Some(false));
    t.feed("\x1b[>1s\x1b[>s");
    // 매개변수가 없으면 거짓(Ghostty stream.zig 2634줄)
    assert_eq!(t.term.mouse_shift_capture(), Some(false));
}

#[test]
fn xtshiftescape_drops_other_values_and_extra_parameters() {
    let mut t = T::new(10, 3);
    t.feed("\x1b[>1s");
    // 2, 1;1은 버린다. 값은 그대로다
    t.feed("\x1b[>2s\x1b[>1;1s\x1b[>0;0s");
    assert_eq!(t.term.mouse_shift_capture(), Some(true));
}

#[test]
fn xtshiftescape_does_not_disturb_save_cursor_or_xtsave() {
    // `CSI > s`는 DECSC가 아니고 `CSI ? s`(XTSAVE)도 아니다
    let mut t = T::new(10, 3);
    t.feed("\x1b[2;3H\x1b[>1s\x1b[1;1H\x1b[u");
    // 저장한 적이 없으니 CSI u는 원점이다(저장 위치 2;3으로 가지 않는다)
    assert_eq!(t.term.grid().cursor.point.line, Line(0));
}

#[test]
fn full_reset_forgets_the_program_choice() {
    let mut t = T::new(10, 3);
    t.feed("\x1b[>1s");
    assert_eq!(t.term.mouse_shift_capture(), Some(true));
    t.feed("\x1bc");
    assert_eq!(t.term.mouse_shift_capture(), None);
}
