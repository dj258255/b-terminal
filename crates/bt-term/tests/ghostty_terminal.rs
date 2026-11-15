//! Ghostty `src/terminal/Terminal.zig`의 단위 시험을 같은 escape 바이트로 옮긴 것(#523~#526).
//! 기준: ghostty-org/ghostty 35188fe06a655d04c6ffcaf277a966aef80e3cf5(main, 2026-11-15)의 Terminal.zig.
//! 시험 이름 위 주석에 Ghostty 시험 이름과 Terminal.zig 줄 번호를 적었다. "이웃" 시험은 같은 함수를 지키는 것으로, 고치기 전에도 통과했다.
//! 격자는 `Term`을 직접 만들어 읽는다. Ghostty의 `plainString`처럼 줄 끝 공백과 끝의 빈 줄은 뺀다.
//! 스타일·하이퍼링크·셀 내부 단언은 옮기지 않았다(글자와 커서, 줄바꿈 대기만 견준다)

use alacritty_terminal::event::EventListener;
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
    /// 줄마다 글(줄 끝 공백 없음), 끝의 빈 줄은 뺌. 넓은 글자의 뒤 칸과 줄 끝의 앞 칸 자리 표시는 건너뛰고,
    /// 앞 칸 없이 홀로 남은 뒤 칸은 빈칸으로 본다(눈에 보이는 모습)
    fn plain(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for r in 0..self.term.screen_lines() {
            let mut s = String::new();
            let mut prev_wide = false;
            for c in 0..self.term.columns() {
                let cell = &self.term.grid()[Line(r as i32)][Column(c)];
                let was_wide = prev_wide;
                prev_wide = cell.flags.contains(Flags::WIDE_CHAR);
                if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    if !was_wide {
                        s.push(' ');
                    }
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
    fn wrap_pending(&self) -> bool {
        self.term.grid().cursor.input_needs_wrap
    }
}

// ---- #523: 1열 격자의 넓은 글자 ----

// Ghostty "Terminal: print wide char in single-width terminal" (Terminal.zig 6318줄)
#[test]
fn print_wide_char_in_single_width_terminal() {
    let mut t = T::new(1, 80);
    t.feed("😀");
    assert_eq!(t.cursor(), (0, 0));
    assert!(t.wrap_pending());
    let cell = &t.term.grid()[Line(0)][Column(0)];
    assert_eq!(cell.c, ' ');
    assert!(!cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER));
}

// 이웃: 1열에서 넓은 글자 뒤에 글자를 더 찍어도 죽지 않고, 줄바꿈 대기가 풀려 다음 줄에 찍힌다(Ghostty `print`의 같은 갈래)
#[test]
fn print_after_wide_char_in_single_width_terminal_wraps() {
    let mut t = T::new(1, 3);
    t.feed("😀A😀B");
    assert_eq!(t.plain(), "A\n\nB");
}

// 이웃: Ghostty "Terminal: print wide char" (6249줄)
#[test]
fn neighbor_print_wide_char() {
    let mut t = T::new(80, 80);
    t.feed("😀");
    assert_eq!(t.cursor(), (2, 0));
}

// 이웃: Ghostty "Terminal: print wide char at edge creates spacer head" (6272줄)
#[test]
fn neighbor_print_wide_char_at_edge_creates_spacer_head() {
    let mut t = T::new(10, 10);
    t.feed("\x1b[1;10H😀");
    assert_eq!(t.cursor(), (2, 1));
    let head = &t.term.grid()[Line(0)][Column(9)];
    assert!(head.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER));
}

// 이웃: Ghostty "Terminal: print over wide char at 0,0" (6337줄)
#[test]
fn neighbor_print_over_wide_char_at_origin() {
    let mut t = T::new(80, 80);
    t.feed("😀\x1b[0;0HA");
    assert_eq!(t.cursor(), (1, 0));
    assert_eq!(t.plain(), "A");
}
