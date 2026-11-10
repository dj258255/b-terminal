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
                s.push(cell.c);
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
