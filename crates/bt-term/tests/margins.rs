//! 좌우 여백: DECLRMM(69)과 DECSLRM(`CSI Pl ; Pr s`)(#311).
//! 기준: Ghostty Terminal.zig(`setLeftAndRightMargin` 3346줄과 `scrolling_region.left`·`right`를 쓰는 곳들, 그 시험들),
//! stream.zig 2604~2625줄(`CSI s`: 69가 켜졌으면 DECSLRM, 아니면 DECSC), stream_handler.zig 308~315·700~705줄, modes.zig 69.
//! 격자는 `Term`을 직접 만들어 칸을 읽는다(Ghostty의 `plainString`처럼 줄 끝 공백과 끝의 빈 줄은 뺀다).
//! PTY를 띄우는 시험은 `/bin/sh`와 `printf`만 쓴다

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
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
    /// 줄마다 글(너비 2 글자의 뒤 칸은 건너뜀, 줄 끝 공백 없음), 끝의 빈 줄은 뺌
    fn plain(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for r in 0..self.term.screen_lines() {
            let mut s = String::new();
            for c in 0..self.term.columns() {
                let cell = &self.term.grid()[Line(r as i32)][Column(c)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                // alacritty는 탭이 지나간 빈 칸에 탭 글자를 남긴다(복사용). Ghostty 시험과 맞추려고 공백으로 본다
                s.push(if cell.c == '\t' { ' ' } else { cell.c });
            }
            lines.push(s.trim_end().to_string());
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
    /// (열, 줄) 0부터
    fn cursor(&self) -> (usize, usize) {
        let p = self.term.grid().cursor.point;
        (p.column.0, p.line.0 as usize)
    }
    fn wrap_pending(&self) -> bool {
        self.term.grid().cursor.input_needs_wrap
    }
    fn margins(&self) -> (usize, usize) {
        (self.term.left_margin(), self.term.right_margin())
    }
}

/// Ghostty 시험의 `printString("ABC"); carriageReturn; linefeed; …` 꼴
fn abc_def_ghi(t: &mut T) {
    t.feed("ABC\r\nDEF\r\nGHI");
}

// ---- 상태: DECSLRM과 69 ----

#[test]
fn decslrm_is_ignored_while_mode_69_is_off() {
    // "setLeftAndRightMargin mode 69 unset"
    let mut t = T::new(5, 5);
    abc_def_ghi(&mut t);
    t.feed("\x1b[1;2s\x1b[1;2H");
    assert_eq!(t.margins(), (0, 4));
    t.feed("\x1b[L");
    assert_eq!(t.plain(), "\nABC\nDEF\nGHI");
}

#[test]
fn decslrm_sets_zero_based_margins_and_homes_the_cursor() {
    // "setLeftAndRightMargin left only": (2, 0) -> left 1, right cols - 1
    let mut t = T::new(5, 5);
    t.feed("\x1b[?69h\x1b[3;4H");
    t.feed("\x1b[2s");
    assert_eq!(t.margins(), (1, 4));
    assert_eq!(t.cursor(), (0, 0));
    t.feed("\x1b[3;4H\x1b[2;4s");
    assert_eq!(t.margins(), (1, 3));
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn decslrm_with_no_or_zero_parameters_resets_the_margins_when_69_is_on() {
    // stream_handler.zig: `.left_and_right_margin_ambiguous`(69 켬): setLeftAndRightMargin(0, 0)
    let mut t = T::new(6, 3);
    t.feed("\x1b[?69h\x1b[2;4s");
    assert_eq!(t.margins(), (1, 3));
    t.feed("\x1b[s");
    assert_eq!(t.margins(), (0, 5));
    t.feed("\x1b[2;4s\x1b[;s");
    assert_eq!(t.margins(), (0, 5));
}

#[test]
fn csi_s_saves_the_cursor_when_69_is_off() {
    // stream_handler.zig: 69가 꺼져 있으면 `CSI s`는 SCOSC(커서 저장)
    let mut t = T::new(6, 3);
    t.feed("\x1b[2;3H\x1b[s\x1b[1;1H\x1b[u");
    assert_eq!(t.cursor(), (2, 1));
    assert_eq!(t.margins(), (0, 5));
}

#[test]
fn csi_s_does_not_save_the_cursor_when_69_is_on() {
    let mut t = T::new(6, 3);
    t.feed("\x1b[?69h\x1b[2;3H\x1b[s\x1b[3;5H\x1b[u");
    // 저장한 적 없으니 `CSI u`는 원점(저장 칸 기본값)으로 간다
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn decslrm_ignores_left_not_below_right() {
    // "setLeftAndRightMargin left equal right": 같으면 아무것도 안 한다
    let mut t = T::new(5, 5);
    abc_def_ghi(&mut t);
    t.feed("\x1b[?69h\x1b[2;2s\x1b[1;2H\x1b[L");
    assert_eq!(t.margins(), (0, 4));
    assert_eq!(t.plain(), "\nABC\nDEF\nGHI");
    t.feed("\x1b[4;2s");
    assert_eq!(t.margins(), (0, 4));
}

#[test]
fn decslrm_clamps_the_right_margin_to_the_last_column() {
    let mut t = T::new(5, 3);
    t.feed("\x1b[?69h\x1b[2;99s");
    assert_eq!(t.margins(), (1, 4));
}

#[test]
fn decslrm_with_three_parameters_is_ignored() {
    let mut t = T::new(5, 3);
    t.feed("\x1b[?69h\x1b[2;3;4s");
    assert_eq!(t.margins(), (0, 4));
}

#[test]
fn turning_69_off_resets_the_margins_to_the_full_width() {
    // stream_handler.zig 700~705줄
    let mut t = T::new(6, 3);
    t.feed("\x1b[?69h\x1b[2;4s");
    assert_eq!(t.margins(), (1, 3));
    t.feed("\x1b[?69l");
    assert_eq!(t.margins(), (0, 5));
    // 다시 켜도 이전 값은 남지 않는다
    t.feed("\x1b[?69h");
    assert_eq!(t.margins(), (0, 5));
}

#[test]
fn soft_reset_and_resize_clear_the_margins() {
    // Terminal.zig `softReset`(5918줄): 69와 여백을 되돌린다. 크기를 바꾸면 위아래처럼 여백도 처음으로
    let mut t = T::new(6, 3);
    t.feed("\x1b[?69h\x1b[2;4s");
    t.term.reset_screen_modes_soft();
    assert_eq!(t.margins(), (0, 5));
    t.feed("\x1b[?69h\x1b[2;4s");
    t.term.resize(Dims(8, 3));
    assert_eq!(t.margins(), (0, 7));
}

#[test]
fn ris_clears_the_margins_and_mode() {
    let mut t = T::new(6, 3);
    t.feed("\x1b[?69h\x1b[2;4s\x1bc");
    assert_eq!(t.margins(), (0, 5));
    t.feed("\x1b[2;4s");
    assert_eq!(t.margins(), (0, 5), "RIS가 69도 껐다");
}

// ---- 2단계: 커서 이동, CR, 원점 모드, 탭 ----

#[test]
fn carriage_return_goes_to_the_left_margin_in_origin_mode() {
    // "carriage return origin mode moves to left margin"
    let mut t = T::new(5, 5);
    t.feed("\x1b[?69h\x1b[3;5s\x1b[?6h\x1b[1;1H");
    assert_eq!(t.cursor(), (2, 0));
    t.feed("\x1b[2C\r");
    assert_eq!(t.cursor(), (2, 0));
}

#[test]
fn carriage_return_left_of_the_left_margin_goes_to_zero() {
    // "carriage return left of left margin moves to zero"
    let mut t = T::new(5, 5);
    t.feed("\x1b[?69h\x1b[3;5s\x1b[1;2H\r");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn carriage_return_right_of_the_left_margin_goes_to_the_left_margin() {
    // "carriage return right of left margin moves to left margin"
    let mut t = T::new(5, 5);
    t.feed("\x1b[?69h\x1b[3;5s\x1b[1;4H\r");
    assert_eq!(t.cursor(), (2, 0));
    // 오른쪽 여백 밖(여백이 3~4칸이고 커서가 5번째 칸)에서도 왼쪽 여백보다 오른쪽이면 왼쪽 여백이다
    let mut t = T::new(6, 5);
    t.feed("\x1b[?69h\x1b[3;4s\x1b[1;6H\r");
    assert_eq!(t.cursor(), (2, 0));
}

#[test]
fn carriage_return_clears_the_pending_wrap_and_cnl_cpl_use_it() {
    let mut t = T::new(5, 5);
    t.feed("hello");
    assert!(t.wrap_pending());
    t.feed("\r");
    assert!(!t.wrap_pending());
    // CNL(`CSI E`)은 내려가서 CR이다: 왼쪽 여백으로 간다
    let mut t = T::new(6, 5);
    t.feed("\x1b[?69h\x1b[3;5s\x1b[1;5H\x1b[E");
    assert_eq!(t.cursor(), (2, 1));
    t.feed("\x1b[4G\x1b[F");
    assert_eq!(t.cursor(), (2, 0));
}

#[test]
fn cursor_forward_stops_at_the_right_margin_and_runs_to_the_edge_outside_it() {
    // Terminal.zig `cursorRight`: 여백 안이면 오른쪽 여백까지, 오른쪽 여백 밖이면 화면 끝까지
    let mut t = T::new(10, 3);
    t.feed("\x1b[?69h\x1b[3;5s\x1b[1;4H\x1b[99C");
    assert_eq!(t.cursor(), (4, 0));
    t.feed("\x1b[1;7H\x1b[99C");
    assert_eq!(t.cursor(), (9, 0));
}

#[test]
fn cursor_back_and_backspace_stop_at_the_left_margin_and_run_to_zero_outside_it() {
    // Terminal.zig `cursorLeft`(역방향 줄바꿈이 꺼진 갈래): 왼쪽 여백 안이면 왼쪽 여백까지, 왼쪽 여백 왼쪽이면 0까지
    let mut t = T::new(10, 3);
    t.feed("\x1b[?69h\x1b[3;8s\x1b[1;6H\x1b[99D");
    assert_eq!(t.cursor(), (2, 0));
    t.feed("\x08");
    assert_eq!(t.cursor(), (2, 0));
    t.feed("\x1b[1;2H\x1b[99D");
    assert_eq!(t.cursor(), (0, 0));
    t.feed("\x1b[1;2H\x08");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn cup_is_relative_to_the_margins_in_origin_mode() {
    // "cursorPos relative to origin with left/right": 위 2~3줄(0부터), 좌우 2~4칸
    let mut t = T::new(5, 5);
    t.feed("\x1b[3;4r\x1b[?69h\x1b[3;5s\x1b[?6h\x1b[1;1HX");
    assert_eq!(t.plain(), "\n\n  X");
}

#[test]
fn cup_is_limited_to_the_margins_in_origin_mode() {
    // "cursorPos limits with full scroll region"
    let mut t = T::new(5, 5);
    t.feed("\x1b[3;4r\x1b[?69h\x1b[3;5s\x1b[?6h\x1b[500;500HX");
    assert_eq!(t.plain(), "\n\n\n    X");
}

#[test]
fn cha_and_vpa_are_relative_in_origin_mode() {
    // Ghostty `setCursorCol`·`setCursorRow`는 setCursorPos를 거친다: 원점 모드에서는 여백 기준이다
    let mut t = T::new(10, 6);
    t.feed("\x1b[3;5r\x1b[?69h\x1b[4;8s\x1b[?6h\x1b[2G");
    assert_eq!(t.cursor(), (4, 2), "CHA 2는 왼쪽 여백(3) 다음 칸, 줄은 그대로");
    t.feed("\x1b[3d");
    assert_eq!(t.cursor(), (4, 4), "VPA 3은 위 여백(2)에서 둘 아래, 칸은 그대로");
    t.feed("\x1b[99G");
    assert_eq!(t.cursor().0, 7, "오른쪽 여백에서 멈춘다");
}

#[test]
fn cuu_and_cud_move_by_screen_lines_in_origin_mode() {
    // 원점 모드에서도 CUU·CUD는 지금 줄에서 상대로 움직인다(여백 칸을 다시 더하지 않는다)
    let mut t = T::new(10, 6);
    t.feed("\x1b[3;6r\x1b[?69h\x1b[4;8s\x1b[?6h\x1b[2;2H");
    assert_eq!(t.cursor(), (4, 3));
    t.feed("\x1b[A");
    assert_eq!(t.cursor(), (4, 2));
    t.feed("\x1b[2B");
    assert_eq!(t.cursor(), (4, 4));
}

#[test]
fn decstbm_and_decom_home_the_cursor_to_the_margin_corner_in_origin_mode() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[?69h\x1b[3;5s\x1b[?6h");
    assert_eq!(t.cursor(), (2, 0));
    t.feed("\x1b[2;4r");
    assert_eq!(t.cursor(), (2, 1));
    // 원점 모드를 끄면 Ghostty처럼 화면 원점으로 간다
    t.feed("\x1b[?6l");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn horizontal_tab_stops_at_the_right_margin() {
    // "horizontal tabs with right margin": 여백 2~5, 탭 자리가 여백 안에 없으면 오른쪽 여백까지
    let mut t = T::new(20, 3);
    t.feed("\x1b[?69h\x1b[3;6s");
    t.feed("X\tA");
    assert_eq!(t.plain(), "X    A");
}

#[test]
fn horizontal_tab_right_of_the_right_margin_does_not_move() {
    // Terminal.zig `horizontalTab`: `while x < right`라 여백 오른쪽에서는 움직이지 않는다
    let mut t = T::new(20, 3);
    t.feed("\x1b[?69h\x1b[3;6s\x1b[1;8H\t");
    assert_eq!(t.cursor(), (7, 0));
}

#[test]
fn horizontal_tab_back_stops_at_the_left_margin_in_origin_mode() {
    // "horizontal tabs with left margin in origin mode"
    let mut t = T::new(10, 3);
    t.feed("\x1b[?69h\x1b[3;6s\x1b[?6h\x1b[1;2HX\x1b[ZA");
    assert_eq!(t.plain(), "  AX");
}

#[test]
fn horizontal_tab_back_ignores_the_margin_outside_origin_mode() {
    let mut t = T::new(20, 3);
    t.feed("\x1b[?69h\x1b[3;6s\x1b[1;4H\x1b[Z");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn horizontal_tab_back_with_cursor_before_the_left_margin() {
    // "horizontal tab back with cursor before left margin": 여백 왼쪽 칸에서는 움직이지 않는다
    let mut t = T::new(10, 3);
    t.feed("\x1b7\x1b[?6h\x1b[?69h\x1b[5s\x1b8\x1b[ZX");
    assert_eq!(t.plain(), "X");
}

#[test]
fn cht_stops_at_the_right_margin() {
    let mut t = T::new(30, 3);
    t.feed("\x1b[?69h\x1b[3;12s\x1b[1;1H\x1b[3I");
    assert_eq!(t.cursor(), (11, 0));
}

// ---- DECRQM(PTY) ----

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

#[test]
fn decrqm_reports_mode_69_off_on_off() {
    // 실측(0.24 합친 빌드): `CSI ? 69 $ p`가 0이었다. 이제 끔 2, 켬 1, 다시 끔 2
    let n = "\x1b[?69;2$y".len();
    let q = "\\033[?69$p";
    let read = format!("$(dd bs=1 count={n} 2>/dev/null | od -An -tx1 | tr -d ' \\n')");
    let script = format!(
        "stty raw -echo; printf '{q}'; a={read}; printf '\\033[?69h{q}'; b={read}; printf '\\033[?69l{q}'; c={read}; stty sane; printf 'ans=%s,%s,%s\\n' \"$a\" \"$b\" \"$c\""
    );
    let (s, rx) = sh(&script, GridSize::new(80, 5));
    wait_exit(&rx);
    let text = s.snapshot().text();
    let line = text
        .lines()
        .find(|l| l.starts_with("ans="))
        .unwrap_or_else(|| panic!("답이 없다: {text}"));
    let got: Vec<&str> = line["ans=".len()..].split(',').collect();
    let want = |state: u8| hex(&format!("\x1b[?69;{state}$y"));
    assert_eq!(got, [want(2), want(1), want(2)]);
}

#[test]
fn cursor_position_report_is_relative_to_the_margins_in_origin_mode() {
    // Ghostty stream_handler.zig `cursorReportPos`(854줄): 원점 모드에서는 위 여백·왼쪽 여백 기준으로 답한다
    // CUP 3;4를 보냈으니 원점 기준 3;4로 답한다(절대 좌표면 4;6)
    let want = "\x1b[3;4R";
    let n = want.len();
    let read = format!("$(dd bs=1 count={n} 2>/dev/null | od -An -tx1 | tr -d ' \\n')");
    let script = format!(
        "stty raw -echo; printf '\\033[2;4r\\033[?69h\\033[3;8s\\033[?6h\\033[3;4H\\033[6n'; a={read}; stty sane; printf 'ans=%s\\n' \"$a\""
    );
    let (s, rx) = sh(&script, GridSize::new(80, 8));
    wait_exit(&rx);
    let text = s.snapshot().text();
    // 여백 안에서 찍혀 줄 앞에 공백이 붙는다
    let line = text
        .lines()
        .map(str::trim_start)
        .find(|l| l.starts_with("ans="))
        .unwrap_or_else(|| panic!("답이 없다: {text}"));
    assert_eq!(&line["ans=".len()..], hex(want));
}
