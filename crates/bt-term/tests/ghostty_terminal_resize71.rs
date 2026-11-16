//! 크기 바꾸기 둘레(#583·#584). 기준: Ghostty main 7fa26e8b675f `Terminal.zig`의 시험
//! `resize prompt redraw clears prompts that reflow splits or joins`(18714줄), `resize resets synchronized output`(5263줄),
//! `resize rejects zero dimensions before mutation`(5274줄), `init rejects zero dimensions`(5302줄),
//! `resize less cols without reflow cutting wide char tail`(18659줄), `resize grows rows while viewport is scrolled up ...`(19705·19722줄).
//! 바이트는 `Emulator::feed`와 같은 순서로(스캐너가 신호를 나눈 자리마다 VT에 먹이고 신호를 적용) 엔진에 넣는다. PTY는 없다.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::notify::{Scanner, Signal};

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
