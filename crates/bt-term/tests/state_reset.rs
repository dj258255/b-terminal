//! 상태 되돌리기 셋(#530 DECSTR, #531 DECSC의 원점 모드, #532 DECST8C).
//! 기준: Ghostty 35188fe `Terminal.zig`의 `softReset`(5918줄)·`saveCursor`(2703줄)·`restoreCursor`(2719줄)·`tabReset`(3001줄)과 그 시험들.
//! DECSTR(`CSI ! p`)은 해석기가 아니라 bt-term 스캐너 신호가 처리하므로, 실제 길(`Emulator::feed`·세션 읽기 스레드)처럼
//! 스캐너를 지나게 먹인다. PTY를 띄우는 시험은 `/bin/sh`와 `printf`만 쓴다

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{CharsetIndex, Color, NamedColor, Processor, StandardCharset};
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

/// 터미널이 PTY에 쓰는 답(DECRQM 등)을 모은다
#[derive(Clone, Default)]
struct Quiet {
    out: Arc<Mutex<String>>,
}
impl EventListener for Quiet {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(s) = event {
            self.out.lock().unwrap().push_str(&s);
        }
    }
}

struct T {
    term: Term<Quiet>,
    parser: Processor,
    scanner: bt_term::notify::Scanner,
    replies: Arc<Mutex<String>>,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        let quiet = Quiet::default();
        let replies = quiet.out.clone();
        T {
            term: Term::new(Config::default(), &Dims(cols, rows), quiet),
            parser: Processor::new(),
            scanner: Default::default(),
            replies,
        }
    }
    /// 실제 길처럼 DECSTR은 스캐너 신호를 거쳐 `kitty_color::soft_reset`이 처리한다
    fn feed(&mut self, s: &str) -> &mut Self {
        let bytes = s.as_bytes();
        let mut pos = 0;
        for (end, sig) in self.scanner.feed_at(bytes) {
            let end = end.min(bytes.len());
            self.parser.advance(&mut self.term, &bytes[pos..end]);
            pos = end;
            if let bt_term::Signal::SoftReset = sig {
                bt_term::kitty_color::soft_reset(&mut self.term);
            }
        }
        self.parser.advance(&mut self.term, &bytes[pos..]);
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
    /// DECRQM으로 묻는다: 0 모름, 1 켬, 2 끔
    fn decrqm(&mut self, private: bool, n: u16) -> u8 {
        self.replies.lock().unwrap().clear();
        let q = if private {
            format!("\x1b[?{n}$p")
        } else {
            format!("\x1b[{n}$p")
        };
        self.feed(&q);
        let r = self.replies.lock().unwrap().clone();
        let prefix = if private {
            format!("\x1b[?{n};")
        } else {
            format!("\x1b[{n};")
        };
        let v = r
            .strip_prefix(prefix.as_str())
            .and_then(|s| s.strip_suffix("$y"))
            .unwrap_or_else(|| panic!("DECRQM 답이 이상하다: {r:?}"));
        v.parse().unwrap()
    }
    fn on(&mut self, private: bool, n: u16) -> bool {
        self.decrqm(private, n) == 1
    }
    fn template(&self) -> &alacritty_terminal::term::cell::Cell {
        &self.term.grid().cursor.template
    }
}

// ---- #531: DECSC가 원점 모드를 저장 ----

#[test]
fn save_cursor_keeps_origin_mode() {
    // 이슈 재현 바이트: DECSC 뒤 원점 모드를 끄고 DECRC하면 저장할 때의 값(켬)으로 돌아온다
    let mut t = T::new(10, 6);
    t.feed("\x1b[2;4r\x1b[?6h\x1b[3;4H\x1b7\x1b[?6l\x1b8");
    assert!(t.on(true, 6));
    assert_eq!(t.cursor(), (3, 3));
}

#[test]
fn save_cursor_with_origin_off_turns_origin_off_on_restore() {
    let mut t = T::new(10, 6);
    t.feed("\x1b[2;4r\x1b[3;4H\x1b7\x1b[?6h\x1b8");
    assert!(!t.on(true, 6));
}

#[test]
fn restore_cursor_without_a_save_resets_origin_and_goes_home() {
    // Ghostty `restoreCursor`: 저장한 것이 없으면 원점 모드를 끄고 처음 자리, 기본 스타일로
    let mut t = T::new(10, 6);
    t.feed("\x1b[?6h\x1b[2;4r\x1b[3;3H\x1b[1m\x1b8");
    assert!(!t.on(true, 6));
    assert_eq!(t.cursor(), (0, 0));
    assert!(!t.template().flags.contains(Flags::BOLD));
}

#[test]
fn save_cursor_origin_mode_ghostty_test() {
    // Ghostty "Terminal: saveCursor origin mode"(15189줄): 원점 켠 채 저장, 여백을 바꾸고 복원하면 저장한 절대 자리(0,0)에 X
    let mut t = T::new(10, 5);
    t.feed("\x1b[?6h\x1b7\x1b[?69h\x1b[3;5s\x1b[2;4r\x1b8X");
    assert_eq!(t.plain(), "X");
    assert!(t.on(true, 6));
}

#[test]
fn save_cursor_is_per_screen() {
    // 화면마다 저장한 커서가 따로다(Ghostty `screens.active.saved_cursor`): 대체 화면의 저장이 원점 모드를 끌고 가지 않는다
    let mut t = T::new(10, 6);
    t.feed("\x1b[?47h\x1b[?6h\x1b7\x1b[?6l\x1b8");
    assert!(t.on(true, 6));
}

