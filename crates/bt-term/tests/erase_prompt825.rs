//! 칸을 지우거나 밀어도 OSC 133 줄 프롬프트 표시가 남는지(#825).
//! 기준: Ghostty 1.4.0 `Screen.clearCells`는 칸만 비우고 줄의 `semantic_prompt`를 건드리지 않는다.
//! 줄 표시를 지우는 것은 줄째 지우는 `clearRows`(ED 0의 아랫줄, ED 1의 윗줄, ED 2)뿐이다.
//! 격자는 `Term`을 직접 만들어 읽는다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::semantic::RowPrompt;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::osc133;

use RowPrompt::{None as N, Prompt as P};

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
    fn new() -> Self {
        T {
            term: Term::new(Config::default(), &Dims(20, 4), Quiet),
            parser: Processor::new(),
        }
    }
    fn feed(&mut self, s: &str) -> &mut Self {
        self.parser.advance(&mut self.term, s.as_bytes());
        self
    }
    fn osc(&mut self, data: &str) -> &mut Self {
        osc133::parse(data)
            .unwrap_or_else(|| panic!("OSC 133;{data}"))
            .apply(&mut self.term);
        self
    }
    /// 0부터 센 줄 `row`에 프롬프트를 그린다(`A`, 글, `D`)
    fn prompt(&mut self, row: usize, text: &str) -> &mut Self {
        self.feed(&format!("\x1b[{};1H", row + 1)).osc("A").feed(text).osc("D")
    }
    fn marks(&self) -> String {
        (0..self.term.screen_lines() as i32)
            .map(|r| match self.term.row_prompt(Line(r)) {
                N => '.',
                P => 'P',
                RowPrompt::Continuation => 'c',
            })
            .collect()
    }
}

/// 0번 줄에 프롬프트를 그리고 `seq`를 보낸 뒤의 줄 표시
fn after(text: &str, seq: &str) -> String {
    let mut t = T::new();
    t.prompt(0, text).feed(seq);
    t.marks()
}

#[test]
fn el_0_at_column_0_keeps_mark() {
    assert_eq!(after("$ hi", "\r\x1b[K"), "P...");
}

#[test]
fn el_2_keeps_mark() {
    assert_eq!(after("$ hi", "\x1b[2K"), "P...");
}

#[test]
fn el_1_keeps_mark() {
    assert_eq!(after("$ hi", "\x1b[4G\x1b[1K"), "P...");
}

#[test]
fn ed_0_on_cursor_row_keeps_mark() {
    assert_eq!(after("$ hi", "\r\x1b[J"), "P...");
}

#[test]
fn ed_1_on_cursor_row_keeps_mark() {
    assert_eq!(after("$ hi", "\x1b[1J"), "P...");
}

#[test]
fn dch_keeps_mark() {
    assert_eq!(after("$ hi", "\r\x1b[2P"), "P...");
}

#[test]
fn ech_keeps_mark() {
    assert_eq!(after("$ hi", "\r\x1b[2X"), "P...");
}

#[test]
fn ich_keeps_mark() {
    assert_eq!(after("$ hi", "\r\x1b[2@"), "P...");
}

#[test]
fn decsel_and_iso_protected_erase_keep_mark() {
    assert_eq!(after("$ hi", "\r\x1b[?K"), "P...");
    assert_eq!(after("$ hi", "\x1b[1\"q\r\x1b[K\x1b[J"), "P...");
}

#[test]
fn print_over_column_0_keeps_mark() {
    assert_eq!(after("$ hi", "\rX"), "P...");
}

#[test]
fn wide_char_boundary_split_keeps_mark() {
    // 0번 칸의 넓은 글자를 경계에서 가르는 지우기(1번 칸에서 DCH·ECH·EL, 1번 칸의 뒤 칸에서 EL 0)
    for seq in [
        "\x1b[2G\x1b[P",
        "\x1b[2G\x1b[X",
        "\x1b[2G\x1b[K",
        "\x1b[2G\x1b[@",
        "\x1b[1G\x1b[1K",
    ] {
        assert_eq!(after("\u{ac00}hi", seq), "P...", "{seq:?}");
    }
}

#[test]
fn margin_boundary_wide_clear_keeps_mark() {
    // 왼쪽 여백 1에서 IL·DL이 0번 칸의 넓은 글자를 지움
    let mut t = T::new();
    t.prompt(0, "\u{ac00}hi").feed("\x1b[?69h\x1b[2;20s\x1b[1;2H\x1b[L");
    assert_eq!(t.marks().as_bytes()[0], b'P');
}

// ---- 줄째 지우는 연산은 표시를 지운다(`clearRows`) ----

#[test]
fn ed_0_clears_rows_below_but_keeps_cursor_row() {
    let mut t = T::new();
    t.prompt(0, "$ a").prompt(1, "$ b").feed("\x1b[1;1H\x1b[J");
    assert_eq!(t.marks(), "P...");
}

#[test]
fn ed_1_clears_rows_above_but_keeps_cursor_row() {
    let mut t = T::new();
    t.prompt(0, "$ a").prompt(1, "$ b").feed("\x1b[2;3H\x1b[1J");
    assert_eq!(t.marks(), ".P..");
}

#[test]
fn ed_2_and_decaln_clear_all_marks() {
    let mut t = T::new();
    t.prompt(0, "$ a").prompt(1, "$ b").feed("\x1b[?1049h\x1b[?1049l"); // 대체 화면은 따로
    assert_eq!(t.marks(), "PP..");
    // 출력 줄이 아래에 있어 프롬프트로 보지 않으므로 스크롤 없이 지운다
    t.feed("\x1b[4;1Hout\x1b[2J");
    assert_eq!(t.marks(), "....");
    let mut t = T::new();
    t.prompt(0, "$ a").feed("\x1b#8");
    assert_eq!(t.marks(), "....");
}

#[test]
fn protected_ed_keeps_marks_on_rows() {
    // 마지막 보호 방식이 ISO(SPA)면 ED는 `clearRows(protected)`라 Ghostty는 줄 속성을 남긴다
    let mut t = T::new();
    t.prompt(0, "$ a").prompt(1, "$ b").feed("\x1bV\x1b[1;1H\x1b[J");
    assert_eq!(t.marks(), "PP..");
    // DEC 방식(DECSCA)이면 보호 칸을 가리지 않아 일반 ED처럼 아랫줄 표시를 지운다
    let mut t = T::new();
    t.prompt(0, "$ a").prompt(1, "$ b").feed("\x1b[1\"q\x1b[1;1H\x1b[J");
    assert_eq!(t.marks(), "P...");
}
