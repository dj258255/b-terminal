//! 크기를 바꿀 때(#537) 줄바꿈 대기·저장 커서·자동 줄바꿈 끔의 다시 흘리기를 Ghostty와 견준다.
//! 기준: Ghostty 35188fe `Screen.zig` `resize`(2100~2300줄), `Terminal.zig` `resize`(4912줄~)와
//! 시험 18974·19014·19031줄, `resize with wraparound off/on`(18846·18867줄).
//! 비교 글은 Ghostty `plainString`처럼 스크롤백까지 읽는다. 보이는 화면만 읽은 값은 따로 본다.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

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

struct T {
    term: Term<VoidListener>,
    parser: Processor,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        T {
            term: Term::new(Config::default(), &Dims(cols, rows), VoidListener),
            parser: Processor::new(),
        }
    }
    fn feed(&mut self, s: &str) -> &mut Self {
        self.parser.advance(&mut self.term, s.as_bytes());
        self
    }
    fn resize(&mut self, cols: usize, rows: usize) -> &mut Self {
        self.term.resize(Dims(cols, rows));
        self
    }
    fn rows_text(&self, from: i32) -> String {
        let g = self.term.grid();
        let mut lines: Vec<String> = Vec::new();
        for r in from..self.term.screen_lines() as i32 {
            let mut s = String::new();
            for c in 0..self.term.columns() {
                let cell = &g[Line(r)][Column(c)];
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
        // 위쪽 빈 줄은 Ghostty `plainString`처럼 남긴다
        lines.join("\n")
    }
    /// 스크롤백부터 읽는다(Ghostty `plainString`)
    fn all(&self) -> String {
        self.rows_text(-(self.term.history_size() as i32))
    }
    fn visible(&self) -> String {
        self.rows_text(0)
    }
    fn pending(&self) -> bool {
        self.term.grid().cursor.input_needs_wrap
    }
}

// ---- 1: 줄바꿈 대기 + 넓힘 / 시험 19031줄 ----

/// (넣은 글, 새 열 수, 복원 뒤 대기, 기대 글)
const CASES: &[(&str, usize, bool, &str)] = &[
    ("ABCD", 6, false, "ABCDX"),
    ("ABCD", 3, false, "ABC\nDX"),
    ("ABCD", 2, true, "AB\nCD\nX"),
    ("ABCD", 4, true, "ABCD\nX"),
    ("ABCDEFGH", 6, false, "ABCDEF\nGHX"),
    ("AB界", 6, false, "AB界X"),
    ("ABC", 6, false, "ABCX"),
];

#[test]
fn resize_pending_wrap_live_and_saved_cursors() {
    for (text, cols, pending, expected) in CASES {
        for restore in [false, true] {
            let mut t = T::new(4, 5);
            t.feed(text);
            if restore {
                t.feed("\x1b7");
            }
            t.resize(*cols, 6);
            if restore {
                t.feed("\x1b8");
            }
            let label = format!("{text:?} -> {cols}열 restore={restore}");
            assert_eq!(t.pending(), *pending, "{label}: 대기");
            t.feed("X");
            assert_eq!(t.all(), *expected, "{label}: 글");
        }
    }
}

// ---- 2: 시험 18974줄 ----

#[test]
fn resize_reflow_and_saved_cursor_pending_wrap() {
    let mut t = T::new(2, 3);
    t.feed("1A2B");
    assert_eq!(t.all(), "1A\n2B");
    t.feed("\x1b7");
    t.resize(5, 3);
    t.feed("\x1b8");
    assert_eq!(t.all(), "1A2B");
    t.feed("X");
    assert_eq!(t.all(), "1A2BX");
}

// ---- 3: 시험 19014줄 ----

#[test]
fn saved_cursor_survives_repeated_widening() {
    let mut t = T::new(4, 5);
    t.feed("abc\r\nAAA|");
    t.feed("\x1b7");
    t.resize(5, 5);
    t.resize(6, 5);
    t.feed("\x1b8X");
    assert_eq!(t.all(), "abc\nAAA|X");
}

// ---- 5: 자동 줄바꿈 끔은 다시 흘리지 않는다 / 시험 18846줄 ----

#[test]
fn resize_with_wraparound_off_does_not_reflow() {
    let mut t = T::new(4, 2);
    t.feed("\x1b[?7l0123");
    t.resize(2, 2);
    assert_eq!(t.all(), "01");
}

#[test]
fn resize_with_wraparound_on() {
    let mut t = T::new(4, 2);
    t.feed("0123");
    t.resize(2, 2);
    assert_eq!(t.all(), "01\n23");
}

#[test]
fn resize_with_wraparound_off_widen_keeps_pending_wrap_at_edge() {
    // 다시 흘리지 않고 넓히면 커서는 줄 끝이 아니게 되므로 대기가 풀려 한 칸 오른쪽에서 이어진다
    let mut t = T::new(4, 2);
    t.feed("\x1b[?7l0123");
    t.resize(6, 2);
    assert!(!t.pending());
}

// ---- 6: 열을 줄여 줄이 늘 때 커서 아래 빈 줄을 먼저 쓴다 ----
// Ghostty `Screen: resize less cols with reflow but row space`(Screen.zig 8652줄): 아래에 빈 줄이 있으면
// 위 줄을 스크롤백으로 밀지 않는다. 보이는 화면(스크롤백 제외)으로 견준다

#[test]
fn narrow_wraparound_on_uses_blank_rows_below_cursor() {
    let mut t = T::new(4, 2);
    t.feed("0123");
    t.resize(2, 2);
    assert_eq!(t.visible(), "01\n23");
    assert_eq!(t.term.history_size(), 0);
    assert_eq!(t.all(), "01\n23");
}

#[test]
fn narrow_abcd_5_to_6_lines() {
    let mut t = T::new(4, 5);
    t.feed("ABCD");
    t.resize(3, 6);
    t.feed("X");
    assert_eq!(t.visible(), "ABC\nDX");
    assert_eq!(t.all(), "ABC\nDX");
}

#[test]
fn narrow_with_row_space_keeps_top_row_and_cursor() {
    // 시험 8652줄: 5열 3줄에 `1ABCD`, 커서를 D에 두고 3열로 -> `1AB`/`CD`, 커서는 (1, 1)
    let mut t = T::new(5, 3);
    t.feed("1ABCD\x1b[1;5H");
    assert!(!t.pending());
    t.resize(3, 3);
    assert_eq!(t.visible(), "1AB\nCD");
    assert_eq!(t.term.history_size(), 0);
    let p = t.term.grid().cursor.point;
    assert_eq!((p.column.0, p.line.0), (1, 1));
}

#[test]
fn narrow_without_row_space_still_scrolls_top_off() {
    // 시험 7494·8692줄: 줄이 모자라면 위가 스크롤백으로 밀린다
    let mut t = T::new(5, 3);
    t.feed("3IJKL\r\n4ABCD\r\n5EFGH");
    t.resize(3, 3);
    assert_eq!(t.visible(), "CD\n5EF\nGH");
    assert_eq!(t.all(), "3IJ\nKL\n4AB\nCD\n5EF\nGH");
}

#[test]
fn narrow_trim_stops_at_saved_cursor_row() {
    // 저장 커서가 아래 빈 줄에 있으면 그 줄은 없애지 않고, 저장 커서는 같은 줄에 남는다
    let mut t = T::new(4, 4);
    t.feed("ABCD\x1b[3;2H\x1b7");
    t.resize(2, 4);
    t.feed("\x1b8Z");
    assert_eq!(t.visible(), "AB\nCD\n\n Z");
}

#[test]
fn alt_screen_never_reflows_and_primary_follows_wraparound() {
    // 대체 화면은 다시 흘리지 않는다
    let mut t = T::new(4, 2);
    t.feed("\x1b[?1049h0123");
    t.resize(2, 2);
    assert_eq!(t.visible(), "01");
}
