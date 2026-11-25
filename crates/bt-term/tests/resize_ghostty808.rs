//! 크기를 줄일 때 두 곳(#808)을 Ghostty 1.4.0 `Screen.zig`와 견준다.
//!
//! 1. 1열로 줄이면서 넓은 글자가 있으면 끝나지 않던 것: 시험 `resize less cols to eliminate wide char`(8974줄)와
//!    `... with row space`(9050줄). 1열에는 넓은 글자가 들어가지 않아 지운다(엔진 안에서 막는다. 앱은 `GridSize::clamped`가 열을 2 이상으로
//!    올려 닿지 않던 곳이다).
//! 2. 커서가 맨 위일 때 줄 수를 줄이는 것: Ghostty 시험 `resize less rows no scrollback`(8436줄)은 보이는 줄이 `3IJKL`이지만
//!    우리는 `1ABCD`이다. xterm도 우리와 같아서(`screen.c` `ScreenResize`, 아래) 그대로 둔다(ADR-056 3항: xterm과 같고 Ghostty만 다르면 xterm을 남긴다).
//!    xterm `ScreenResize`는 기본 resizeGravity(SouthWest, `charproc.c`의 resource 기본값)에서 줄이면
//!    `amount = (MaxRows - move_up - 1) - cur_row`를 구하고, 0 이상(커서 아래에 버릴 줄이 충분)이면 위 줄을 스크롤백으로 옮기지 않고
//!    (`move_down_by = 0`) 버퍼를 줄여 아래 줄을 버린다. 5x3에서 커서 (0,0)이면 amount = (3 - 2 - 1) - 0 = 0이라 `1ABCD`가 남는다.
//!    (xterm 소스는 ThomasDickey/xterm-snapshots master의 `screen.c` 2217줄 이하를 읽어 확인했고, 실제 xterm을 돌려 보지는 않았다.)

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use std::sync::mpsc;
use std::time::Duration;

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
    fn new(cols: usize, rows: usize, history: usize) -> Self {
        let cfg = Config {
            scrolling_history: history,
            ..Config::default()
        };
        T {
            term: Term::new(cfg, &Dims(cols, rows), VoidListener),
            parser: Processor::new(),
        }
    }
    fn feed(&mut self, s: &str) {
        self.parser.advance(&mut self.term, s.replace('\n', "\r\n").as_bytes());
    }
    /// 크기를 바꾼다. 끝나지 않으면(옛 엔진은 1열 + 넓은 글자에서 CPU 100%로 돌며 메모리를 먹었다) 2초 뒤 실패로 알린다
    fn resize(self, cols: usize, rows: usize) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut t = self;
            t.term.resize(Dims(cols, rows));
            let _ = tx.send(t);
        });
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("resize({cols}x{rows})가 2초 안에 끝나지 않았다"))
    }
    fn lines(&self, from: i32, to: i32) -> String {
        let g = self.term.grid();
        let mut lines: Vec<String> = Vec::new();
        for r in from..to {
            let mut s = String::new();
            for c in 0..self.term.columns() {
                let cell = &g[Line(r)][Column(c)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
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
    /// 스크롤백부터 활성 영역 끝까지
    fn screen(&self) -> String {
        self.lines(-(self.term.history_size() as i32), self.term.screen_lines() as i32)
    }
    fn viewport(&self) -> String {
        let off = self.term.grid().display_offset() as i32;
        self.lines(-off, self.term.screen_lines() as i32 - off)
    }
    fn cur(&self) -> (usize, usize) {
        let p = self.term.grid().cursor.point;
        (p.column.0, p.line.0 as usize)
    }
    /// 격자에 넓은 글자나 그 자리표시 칸이 남았는가
    fn has_wide(&self) -> bool {
        let g = self.term.grid();
        let wide = Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
        (-(self.term.history_size() as i32)..self.term.screen_lines() as i32)
            .any(|r| (0..self.term.columns()).any(|c| g[Line(r)][Column(c)].flags.intersects(wide)))
    }
}

/// 8974 "resize less cols to eliminate wide char": 2x1에 😀을 쓰고 1열로 줄이면 넓은 글자는 지워지고 칸은 빈 좁은 칸이다
#[test]
fn wide_char_is_deleted_at_one_col() {
    let mut t = T::new(2, 1, 0);
    t.feed("😀");
    assert_eq!(t.screen(), "😀");
    assert!(t.term.grid()[Line(0)][Column(0)].flags.contains(Flags::WIDE_CHAR));
    let t = t.resize(1, 1);
    assert_eq!(t.screen(), "");
    let cell = &t.term.grid()[Line(0)][Column(0)];
    assert_eq!(cell.c, ' ');
    assert!(!t.has_wide(), "넓은 글자 표시가 남지 않았다");
}

/// 9050 "resize less cols to eliminate wide char with row space": 줄이 남아도 1열이면 지운다
#[test]
fn wide_char_is_deleted_at_one_col_with_row_space() {
    let mut t = T::new(2, 2, 0);
    t.feed("😀");
    assert_eq!(t.screen(), "😀");
    let t = t.resize(1, 2);
    assert_eq!(t.screen(), "");
    assert!(!t.has_wide());
}

/// 1열로 줄일 때 넓은 글자가 섞인 글·스크롤백·이어진 줄도 끝나고, 좁은 글자는 남는다
#[test]
fn one_col_resize_with_wide_chars_in_history_and_wraps() {
    let mut t = T::new(3, 3, 100);
    t.feed("x😀\nab😀cd\n한글ok\n😀😀😀\nlast");
    assert!(t.has_wide());
    let t = t.resize(1, 3);
    assert!(!t.has_wide());
    let text = t.screen();
    for ch in ['x', 'a', 'b', 'c', 'd', 'o', 'k', 'l', 'a', 's', 't'] {
        assert!(text.contains(ch), "{ch}가 남아야 한다: {text:?}");
    }
    assert!(!text.contains('😀') && !text.contains('한'));
}

/// 1열로 줄였다가 다시 키워도 끝난다(옛 엔진이 멈추던 입력 그대로)
#[test]
fn one_col_and_back_terminates() {
    let mut t = T::new(4, 2, 100);
    t.feed("😀😀");
    let t = t.resize(1, 2).resize(4, 2);
    assert!(!t.has_wide());
}

/// 8436 "resize less rows no scrollback"와 같은 입력(5x3, 스크롤백 0, 커서 (0,0), 5x1로). Ghostty는 `3IJKL`을 보이지만
/// xterm은 커서 아래 줄을 버려 `1ABCD`가 남는다(파일 머리의 `ScreenResize` 설명). 우리는 xterm과 같다. 커서는 움직이지 않는다
#[test]
fn less_rows_with_cursor_on_top_drops_the_lines_below_like_xterm() {
    let mut t = T::new(5, 3, 0);
    t.feed("1ABCD\n2EFGH\n3IJKL");
    t.feed("\x1b[H");
    let before = t.cur();
    let t = t.resize(5, 1);
    assert_eq!(t.cur(), before);
    assert_eq!(t.viewport(), "1ABCD");
    assert_eq!(t.screen(), "1ABCD");
}

/// 같은 입력에 스크롤백이 있어도 xterm처럼 아래 두 줄을 스크롤백으로 옮기지 않고 버린다
#[test]
fn less_rows_with_cursor_on_top_keeps_no_history_like_xterm() {
    let mut t = T::new(5, 3, 1000);
    t.feed("1ABCD\n2EFGH\n3IJKL");
    t.feed("\x1b[H");
    let t = t.resize(5, 1);
    assert_eq!(t.screen(), "1ABCD");
    assert_eq!(t.term.history_size(), 0);
}

/// 커서가 맨 아래면 xterm도 Ghostty도 위 줄이 스크롤백으로 가고 아래 줄이 남는다(8469 "moving cursor"의 바닥 쪽, 8511 "empty scrollback")
#[test]
fn less_rows_with_cursor_at_bottom_scrolls_the_top_into_history() {
    let mut t = T::new(5, 3, 1000);
    t.feed("1ABCD\n2EFGH\n3IJKL");
    let t = t.resize(5, 1);
    assert_eq!(t.viewport(), "3IJKL");
    assert_eq!(t.screen(), "1ABCD\n2EFGH\n3IJKL");
}
