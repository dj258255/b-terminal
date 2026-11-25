//! #815: IL·DL(그리고 같은 길을 가는 SU·SD·RI)로 함께 움직인 줄은 줄 접힘 표시(`WRAPLINE`)와 줄 끝 넓은 글자
//! 자리표시(`LEADING_WIDE_CHAR_SPACER`, Ghostty의 spacer head)를 지키고, 다음 줄이 바뀐 줄만 푼다.
//! 기준: Ghostty main PR #15309(2026-11-23 병합) `src/terminal/Terminal.zig`의 새 시험 다섯 개
//! - `insertLines preserves wrap of rows shifted together`
//! - `deleteLines preserves wrap of rows shifted together`
//! - `insertLines and deleteLines soft wrap state`
//! - `insertLines and deleteLines clear a spacer head with its wrap`
//! - `insertLines and deleteLines keep soft wraps across a page boundary`
//!
//! 그리고 그 PR이 기대를 바꾼 `deleteLines wide character spacer head`.
//!
//! 우리는 줄 접힘을 줄 마지막 칸의 `WRAPLINE`으로 들고 "이어받는 줄" 표시는 따로 없다(윗줄의 `WRAPLINE`이 곧 그것이다).
//! 우리에게는 페이지가 없어 page boundary 시험은 같은 입력의 화면 글과 줄 표시만 견준다.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
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
    fn flags(&self, line: i32, col: usize) -> Flags {
        self.term.grid()[Line(line)][Column(col)].flags
    }
    fn wrapped(&self, line: i32) -> bool {
        self.flags(line, self.term.columns() - 1).contains(Flags::WRAPLINE)
    }
    /// 접힌 줄을 잇고 끝의 빈 줄과 공백을 뗀 화면 글(Ghostty `plainStringUnwrapped`)
    fn unwrapped(&self) -> String {
        let last = self.term.screen_lines() as i32 - 1;
        let s = self.term.bounds_to_string(
            Point::new(Line(0), Column(0)),
            Point::new(Line(last), Column(self.term.columns() - 1)),
        );
        s.trim_end_matches(['\n', ' ']).to_string()
    }
    /// 줄마다 `w`(다음 줄로 이어짐), `c`(윗줄에서 이어받음), `b`(둘 다), `.`(둘 다 아님)
    fn rows(&self) -> String {
        (0..self.term.screen_lines() as i32)
            .map(|y| match (self.wrapped(y), y > 0 && self.wrapped(y - 1)) {
                (true, true) => 'b',
                (true, false) => 'w',
                (false, true) => 'c',
                (false, false) => '.',
            })
            .collect()
    }
}

#[test]
fn insert_lines_preserves_wrap_of_rows_shifted_together() {
    let mut t = T::new(3, 4);
    t.feed("ABCDE");
    t.feed("\x1b[1;3r\x1b[1;1H\x1b[L");
    assert_eq!(t.unwrapped(), "\nABCDE");
}

#[test]
fn delete_lines_preserves_wrap_of_rows_shifted_together() {
    let mut t = T::new(3, 4);
    t.feed("1\r\nABCDE");
    t.feed("\x1b[1;3r\x1b[1;1H\x1b[M");
    assert_eq!(t.unwrapped(), "ABCDE");
}

#[test]
fn insert_lines_and_delete_lines_soft_wrap_state() {
    let fill = "\x1b[H\
        aaaaaaaaaaAAA\r\n\
        bbbbbbbbbbBBB\r\n\
        ccccccccccCCC\r\n\
        ddddddddddDDD";
    let cases: &[(&str, &str)] = &[
        ("", "wcwcwcwc"),
        // 함께 움직이는 줄은 상태를 지키고, 이어받는 줄이 영역 밖으로 밀리면 풀린다
        ("\x1b[3;1H\x1b[L", "wc.wcwc."),
        ("\x1b[3;1H\x1b[3L", "wc...wc."),
        ("\x1b[3;1H\x1b[99L", "wc......"),
        ("\x1b[3;1H\x1b[2M", "wcwcwc.."),
        ("\x1b[3;1H\x1b[99M", "wc......"),
        // 쌍 사이에 넣으면 쌍이 갈린다
        ("\x1b[2;1H\x1b[L", "...wcwc."),
        ("\x1b[3;6r\x1b[4;1H\x1b[L", "wc....wc"),
        // 쌍의 한쪽을 지우면 다른 쪽은 혼자 남는다
        ("\x1b[3;1H\x1b[M", "wc.wcwc."),
        ("\x1b[2;1H\x1b[M", ".wcwcwc."),
        ("\x1b[4;1H\x1b[2M", "wc..wc.."),
        ("\x1b[3;6r\x1b[4;1H\x1b[M", "wc.wc.wc"),
        // 영역 양 끝 밖에 쌍이 있는 영역. 안의 쌍은 온전히 움직인다
        ("\x1b[3;6r\x1b[3;1H\x1b[L", "wc.wc.wc"),
        ("\x1b[3;6r\x1b[3;1H\x1b[M", "wc.wc.wc"),
        // 위·아래 경계가 쌍을 가르는 영역. 밖의 줄은 이어지던 줄을 잃는다
        ("\x1b[2;5r\x1b[2;1H\x1b[L", "...wc.wc"),
        ("\x1b[2;5r\x1b[2;1H\x1b[M", ".wc...wc"),
        // SU·SD·RI도 같은 길이다
        ("\x1b[2;5r\x1b[S", ".wc...wc"),
        ("\x1b[2;5r\x1b[T", "...wc.wc"),
        ("\x1b[2;5r\x1b[2;1H\x1bM", "...wc.wc"),
        // 좌우 여백은 줄이 아니라 칸을 옮기므로 모든 줄이 상태를 지킨다(xterm과 같다)
        ("\x1b[?69h\x1b[3;8s\x1b[3;3H\x1b[L", "wcwcwcwc"),
        ("\x1b[?69h\x1b[3;8s\x1b[3;3H\x1b[M", "wcwcwcwc"),
    ];
    for (ops, want) in cases {
        let mut t = T::new(10, 8);
        t.feed(fill);
        t.feed(ops);
        assert_eq!(&t.rows(), want, "ops {ops:?}");
    }
}

#[test]
fn insert_lines_and_delete_lines_clear_a_spacer_head_with_its_wrap() {
    for op in ["\x1b[L", "\x1b[M"] {
        let mut t = T::new(5, 4);
        // AAAA* (이어짐, * 는 자리표시 칸) / 😀B
        t.feed("AAAA\u{1F600}B");
        assert!(t.wrapped(0));
        assert!(t.flags(0, 4).contains(Flags::LEADING_WIDE_CHAR_SPACER));
        t.feed("\x1b[2;1H");
        t.feed(op);
        assert!(!t.wrapped(0), "{op:?}");
        assert!(!t.flags(0, 4).contains(Flags::LEADING_WIDE_CHAR_SPACER), "{op:?}");
    }
}

/// `deleteLines wide character spacer head`(1.4.0에서 12454줄): 입력 `AAAAABBBB😀CCC`(5x3)에서 맨 위 줄을 지운다.
/// 1.4.0은 `BBBB*`의 자리표시를 빈 칸으로 만들고 이음을 풀었다. main은 두 줄이 함께 움직이므로 지킨다
#[test]
fn delete_lines_wide_character_spacer_head_moves_together() {
    let mut t = T::new(5, 3);
    t.feed("AAAAABBBB\u{1F600}CCC");
    t.feed("\x1b[1;1H\x1b[M");
    assert!(t.wrapped(0));
    assert!(t.flags(0, 4).contains(Flags::LEADING_WIDE_CHAR_SPACER));
    assert_eq!(t.unwrapped(), "BBBB\u{1F600}CCC");
}

/// 우리에게 페이지는 없다. 같은 입력의 화면 글과 줄 표시만 견준다
#[test]
fn insert_lines_and_delete_lines_keep_soft_wraps_across_a_page_boundary() {
    let mut t = T::new(5, 7);
    t.feed("\x1b[2;1HaaaaaAA\r\nbbbbbBB\r\ncccccCC");
    t.feed("\x1b[2;1H\x1b[L");
    assert_eq!(t.unwrapped(), "\n\naaaaaAA\nbbbbbBB\nccccc");
    for (y, wrap) in [false, false, true, false, true, false, false].into_iter().enumerate() {
        assert_eq!(t.wrapped(y as i32), wrap, "IL 줄 {y}");
        assert_eq!(y > 0 && t.wrapped(y as i32 - 1), y == 3 || y == 5, "IL 이어받음 {y}");
    }
    t.feed("\x1b[M");
    assert_eq!(t.unwrapped(), "\naaaaaAA\nbbbbbBB\nccccc");
    for (y, wrap) in [false, true, false, true, false, false, false].into_iter().enumerate() {
        assert_eq!(t.wrapped(y as i32), wrap, "DL 줄 {y}");
        assert_eq!(y > 0 && t.wrapped(y as i32 - 1), y == 2 || y == 4, "DL 이어받음 {y}");
    }
}
