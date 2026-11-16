//! 크기 바꾸기 둘레(#583·#584). 기준: Ghostty main 7fa26e8b675f `Terminal.zig`의 시험
//! `resize prompt redraw clears prompts that reflow splits or joins`(18714줄), `resize resets synchronized output`(5263줄),
//! `resize rejects zero dimensions before mutation`(5274줄), `init rejects zero dimensions`(5302줄),
//! `resize less cols without reflow cutting wide char tail`(18659줄), `resize grows rows while viewport is scrolled up ...`(19705·19722줄).
//! 바이트는 `Emulator::feed`와 같은 순서로(스캐너가 신호를 나눈 자리마다 VT에 먹이고 신호를 적용) 엔진에 넣는다. PTY는 없다.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_core::Theme;
use bt_term::notify::{Scanner, Signal};
use bt_term::{Emulator, GridSize};

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
    scanner: Scanner,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        let config = Config {
            scrolling_history: 10000,
            ..Config::default()
        };
        T {
            term: Term::new(config, &Dims(cols, rows), VoidListener),
            parser: Processor::new(),
            scanner: Scanner::default(),
        }
    }
    fn feed(&mut self, s: &str) -> &mut Self {
        let bytes = s.as_bytes();
        let mut pos = 0;
        for (end, sig) in self.scanner.feed_at(bytes) {
            let end = end.min(bytes.len());
            self.parser.advance(&mut self.term, &bytes[pos..end]);
            pos = end;
            if let Signal::Semantic(sp) = &sig {
                sp.apply(&mut self.term);
            }
        }
        self.parser.advance(&mut self.term, &bytes[pos..]);
        self
    }
    fn resize(&mut self, cols: usize, rows: usize) -> &mut Self {
        self.term.resize(Dims(cols, rows));
        self
    }
    fn row_text(&self, line: i32) -> String {
        let mut s = String::new();
        for c in 0..self.term.columns() {
            let cell = &self.term.grid()[Line(line)][Column(c)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            s.push(cell.c);
        }
        s.trim_end().to_string()
    }
    /// 스크롤백부터 끝의 빈 줄을 뺀 글(Ghostty `dumpString .screen`)
    fn dump(&self) -> String {
        let h = self.term.history_size() as i32;
        let mut lines: Vec<String> = (-h..self.term.screen_lines() as i32)
            .map(|r| self.row_text(r))
            .collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
    fn cursor(&self) -> (usize, usize) {
        let p = self.term.grid().cursor.point;
        (p.column.0, p.line.0 as usize)
    }
}

// ---- #583: redraw=last 프롬프트 재흐름(Terminal.zig 18714줄) ----

#[test]
fn redraw_last_프롬프트는_넓은_폭에서도_지우고_커서는_줄을_지킨다() {
    let bash_wrapped = "out\r\n\x1b]133;A;redraw=last;cl=line\x07first\r\n\x1b]133;P;k=s\x07abcdefghijklmno";
    let mut t = T::new(12, 6);
    t.feed(bash_wrapped);
    t.resize(20, 6);
    assert_eq!(t.dump(), "out\nfirst\nabcdefghijkl");
    assert_eq!(t.cursor(), (3, 3));
}

#[test]
fn redraw_last_프롬프트는_좁은_폭에서_앞_줄을_남기고_커서는_줄을_지킨다() {
    let bash_wrapped = "out\r\n\x1b]133;A;redraw=last;cl=line\x07first\r\n\x1b]133;P;k=s\x07abcdefghijklmno";
    let mut t = T::new(12, 6);
    t.feed(bash_wrapped);
    t.resize(6, 6);
    assert_eq!(t.dump(), "out\nfirst\nabcdef\nghijkl");
    assert_eq!(t.cursor(), (3, 3));
}

// ---- #584 C11: 자동 줄바꿈을 끄고 열을 줄이면 잘린 넓은 글자 머리를 비운다(Terminal.zig 18659줄) ----

#[test]
fn 자동_줄바꿈_끔_열_줄이기는_잘린_넓은_글자를_비운다() {
    let mut t = T::new(3, 1);
    t.feed("a\u{4e00}\x1b[?7l");
    t.resize(2, 1);
    let cell = &t.term.grid()[Line(0)][Column(1)];
    assert_eq!(
        (
            cell.c,
            cell.flags.contains(Flags::WIDE_CHAR),
            cell.flags.contains(Flags::WIDE_CHAR_SPACER)
        ),
        (' ', false, false)
    );
    assert_eq!(t.dump(), "a");
}

#[test]
fn 자동_줄바꿈_끔_열_줄이기는_꼬리만_잘려도_머리를_비운다() {
    // "ab一"(머리는 2번 칸, 꼬리는 3번 칸)을 3열로 줄이면 꼬리가 잘린다
    let mut t = T::new(4, 1);
    t.feed("ab\u{4e00}\x1b[?7l");
    t.resize(3, 1);
    let cell = &t.term.grid()[Line(0)][Column(2)];
    assert_eq!((cell.c, cell.flags.contains(Flags::WIDE_CHAR)), (' ', false));
    assert_eq!(t.dump(), "ab");
}

#[test]
fn 자동_줄바꿈_켬_열_줄이기는_넓은_글자를_다음_줄로_넘긴다() {
    // 기존 동작(Ghostty `resize less cols to wrap wide char`)이 그대로여야 한다
    let mut t = T::new(4, 2);
    t.feed("ab\u{4e00}");
    t.resize(3, 2);
    assert_eq!(t.dump(), "ab\n\u{4e00}");
}

// ---- #584 C13: 올려 본 화면에서 줄을 늘릴 때 커서가 맨 아래가 아니면 그 자리 유지(Terminal.zig 19705·19722줄, PageList.zig 3084~3100줄) ----

fn scrolled(cursor_to_top: bool) -> T {
    let mut t = T::new(5, 3);
    t.feed("1\r\n2\r\n3\r\n4\r\n5\r\n6");
    if cursor_to_top {
        t.feed("\x1b[1;1H");
    }
    t.term.scroll_display(Scroll::Delta(1));
    t
}

#[test]
fn 줄_늘리기는_커서가_맨_아래가_아니면_올려_본_자리를_지킨다() {
    let mut t = scrolled(true);
    assert!(t.term.grid().display_offset() > 0, "먼저 올려 봐야 한다");
    t.resize(5, 6);
    assert!(
        t.term.grid().display_offset() > 0,
        "올려 본 자리가 맨 아래로 튐: display_offset {}",
        t.term.grid().display_offset()
    );
}

#[test]
fn 줄_늘리기는_커서가_맨_아래가_아니면_스크롤백을_끌어내리지_않는다() {
    let mut t = scrolled(true);
    t.resize(5, 6);
    assert_eq!(t.term.history_size(), 3, "스크롤백 줄 수");
    assert_eq!(t.cursor(), (0, 0), "커서는 그 줄에 남는다");
    let rows: Vec<String> = (0..6).map(|r| t.row_text(r)).collect();
    assert_eq!(rows, ["4", "5", "6", "", "", ""]);
    assert_eq!(t.row_text(-3), "1");
}

#[test]
fn 줄_늘리기는_커서가_맨_아래면_스크롤백을_끌어내린다() {
    // 이웃 시험: 커서가 맨 아래이면 기존대로 끌어내리고 올려 본 자리는 맨 아래로 돌아온다
    let mut t = scrolled(false);
    t.resize(5, 6);
    assert_eq!(t.term.history_size(), 0);
    assert_eq!(t.term.grid().display_offset(), 0);
    assert_eq!(t.cursor(), (1, 5));
    let rows: Vec<String> = (0..6).map(|r| t.row_text(r)).collect();
    assert_eq!(rows, ["1", "2", "3", "4", "5", "6"]);
}

#[test]
fn 줄_늘리기는_스크롤백이_모자라면_빈_줄을_아래에_더한다() {
    // 커서가 맨 아래이고 끌어올 스크롤백이 줄 수보다 적으면 모자란 만큼은 아래에 빈 줄이 생긴다(Ghostty `for (count..rows) grow()`)
    let mut t = T::new(5, 3);
    t.feed("1\r\n2\r\n3\r\n4");
    t.resize(5, 8);
    let rows: Vec<String> = (0..8).map(|r| t.row_text(r)).collect();
    assert_eq!(rows, ["1", "2", "3", "4", "", "", "", ""]);
    assert_eq!(t.cursor(), (1, 3));
}

// ---- #584 C8: 크기를 바꾸면 동기 출력(?2026)을 푼다(Terminal.zig 5263줄) ----

#[test]
fn 크기를_바꾸면_동기_출력을_푼다() {
    let mut e = Emulator::new(GridSize::new(10, 5), Theme::default());
    e.feed(b"\x1b[?2026h");
    e.resize(GridSize::new(10, 5));
    e.feed(b"A");
    assert_eq!(e.snapshot().text().trim_end(), "A");
}

#[test]
fn 크기를_바꾸면_동기_출력에_모아_둔_글은_옛_크기에_반영된다() {
    // Ghostty는 글을 모으지 않고 모드만 풀므로, 이미 온 글은 크기를 바꾸기 전 화면에 있다. 우리는 모은 글을 크기를 바꾸기 전에 먹인다
    let mut e = Emulator::new(GridSize::new(10, 5), Theme::default());
    e.feed(b"\x1b[?2026hAB");
    e.resize(GridSize::new(6, 5));
    assert_eq!(e.snapshot().text().trim_end(), "AB");
}

// ---- #584 C9: 크기 0(Terminal.zig 5274·5302줄) ----

#[test]
fn 엔진_resize는_크기_0을_거절하고_상태를_그대로_둔다() {
    let mut t = T::new(10, 5);
    t.feed("abc");
    t.resize(0, 5);
    assert_eq!((t.term.columns(), t.term.screen_lines()), (10, 5));
    t.resize(10, 0);
    assert_eq!((t.term.columns(), t.term.screen_lines()), (10, 5));
    assert_eq!(t.dump(), "abc");
}

#[test]
fn 엔진_new는_크기_0을_1로_올린다() {
    let t = T::new(0, 0);
    assert_eq!((t.term.columns(), t.term.screen_lines()), (1, 1));
}

#[test]
fn 에뮬레이터_new는_크기_0을_최소_크기로_올린다() {
    let zero = GridSize {
        cols: 0,
        rows: 0,
        cell_width: 0,
        cell_height: 0,
    };
    let mut e = Emulator::new(zero, Theme::default());
    e.feed("글 abc\r\nxyz".as_bytes());
    let screen = e.snapshot();
    let min = GridSize::new(0, 0);
    assert_eq!((screen.cols as u16, screen.rows as u16), (min.cols, min.rows));
}

#[test]
fn 에뮬레이터_resize는_크기_0을_최소_크기로_올린다() {
    let mut e = Emulator::new(GridSize::new(10, 5), Theme::default());
    e.feed(b"abc");
    let zero = GridSize {
        cols: 0,
        rows: 0,
        cell_width: 0,
        cell_height: 0,
    };
    e.resize(zero);
    let min = GridSize::new(0, 0);
    let screen = e.snapshot();
    assert_eq!((screen.cols as u16, screen.rows as u16), (min.cols, min.rows));
    e.feed(b"x");
}
