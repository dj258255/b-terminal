//! 프롬프트 클릭(#338): 입력 칸 표시(B 뒤에 쓴 칸), `cl=`·`click_events=`가 정한 PTY 바이트.
//! 기준: Ghostty `Surface.maybePromptClick`(gh7/Surface.zig 4300~4420줄), `Screen.promptClickMove`·`promptClickLine`(gh10/Screen.zig 3639~3820줄),
//! `Terminal.semanticPrompt`(2784~2902줄). 격자는 `Term`을 직접 만들고 OSC 133은 `osc133::parse`·`apply`로 넣는다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::semantic::{RowPrompt, SemanticClick};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::osc133::{self, Click, ClickEvents};
use bt_term::prompt_click::{move_bytes, prompt_click, sgr_click};

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
    fn osc(&mut self, data: &str) -> &mut Self {
        let sp = osc133::parse(data).unwrap_or_else(|| panic!("OSC 133;{data}"));
        sp.apply(&mut self.term);
        self
    }
    /// `$ `를 프롬프트로, 뒤를 입력으로 쓴다
    fn prompt(&mut self, opts: &str, text: &str) -> &mut Self {
        self.osc(&format!("A{opts}")).feed("$ ").osc("B").feed(text)
    }
    fn is_input(&self, line: i32, col: usize) -> bool {
        self.term.grid()[Line(line)][Column(col)].flags.contains(Flags::SEMANTIC_INPUT)
    }
    fn input_cols(&self, line: i32) -> Vec<usize> {
        (0..self.term.columns()).filter(|&c| self.is_input(line, c)).collect()
    }
    fn click(&self, col: usize, row: usize) -> Option<Vec<u8>> {
        prompt_click(&self.term, col, row)
    }
}

const LEFT: &[u8] = b"\x1b[D";
const RIGHT: &[u8] = b"\x1b[C";

// ---- 입력 칸 표시 ----

#[test]
fn only_cells_written_after_b_are_input() {
    let mut t = T::new(20, 5);
    t.prompt("", "ls");
    assert_eq!(t.input_cols(0), vec![2, 3], "프롬프트 `$ `는 입력이 아니다");
    t.osc("C").feed("\r\nout");
    assert!(t.input_cols(1).is_empty(), "C 뒤의 출력은 입력이 아니다");
    t.osc("D;0");
    t.osc("A").feed("\r\n$ ");
    assert!(t.input_cols(2).is_empty(), "A 뒤의 프롬프트 글은 입력이 아니다");
}

#[test]
fn input_marks_continue_across_soft_wrap() {
    let mut t = T::new(10, 5);
    t.prompt("", "abcdefghijkl");
    assert_eq!(t.input_cols(0), (2..10).collect::<Vec<_>>());
    assert_eq!(t.input_cols(1), vec![0, 1, 2, 3]);
    assert_eq!(t.term.row_prompt(Line(1)), RowPrompt::Continuation, "줄 넘김으로 이어진 입력 줄은 이어진 프롬프트");
}

#[test]
fn erasing_or_overwriting_clears_the_input_mark() {
    let mut t = T::new(20, 5);
    t.prompt("", "hello");
    t.feed("\x1b[1;5H\x1b[K"); // 5번 칸부터 줄 끝까지 지움
    assert_eq!(t.input_cols(0), vec![2, 3]);
    t.osc("C").feed("\x1b[1;3Hxy"); // 출력 상태로 덮어씀
    assert!(t.input_cols(0).is_empty());
}

// ---- 바이트를 만드는 순수 함수 ----

#[test]
fn move_bytes_follow_decckm() {
    assert_eq!(move_bytes(2, 0, false), b"\x1b[D\x1b[D".to_vec());
    assert_eq!(move_bytes(0, 3, false), b"\x1b[C\x1b[C\x1b[C".to_vec());
    assert_eq!(move_bytes(1, 0, true), b"\x1bOD".to_vec());
    assert_eq!(move_bytes(0, 1, true), b"\x1bOC".to_vec());
    assert!(move_bytes(0, 0, false).is_empty());
}

#[test]
fn sgr_click_is_a_left_press_with_one_based_cell() {
    assert_eq!(sgr_click(5, 1), b"\x1b[<0;5;1M".to_vec());
    assert_eq!(sgr_click(80, 24), b"\x1b[<0;80;24M".to_vec());
}

// ---- 셸이 밝히지 않으면 아무것도 보내지 않는다 ----

#[test]
fn nothing_is_sent_unless_the_shell_declared_cl_or_click_events() {
    let mut t = T::new(20, 5);
    t.prompt("", "hello");
    assert_eq!(t.term.semantic_click(), SemanticClick::None);
    assert_eq!(t.click(4, 0), None);
    assert_eq!(t.click(0, 0), None);
}

#[test]
fn click_kind_parsing_priority_and_persistence() {
    let mut t = T::new(20, 5);
    t.osc("A;cl=line");
    assert_eq!(t.term.semantic_click(), SemanticClick::Cl(Click::Line));
    // click_events가 cl보다 앞선다
    t.osc("A;cl=line;click_events=2");
    assert_eq!(t.term.semantic_click(), SemanticClick::ClickEvents(ClickEvents::Relative));
    // 옵션 없는 A는 기억을 지우지 않는다(Ghostty는 옵션이 있을 때만 바꾼다)
    t.osc("A");
    assert_eq!(t.term.semantic_click(), SemanticClick::ClickEvents(ClickEvents::Relative));
    // 틀린 click_events 값은 없는 것으로 보고 cl로 간다
    t.osc("A;click_events=3;cl=m");
    assert_eq!(t.term.semantic_click(), SemanticClick::Cl(Click::Multiple));
}

// ---- cl=line ----

#[test]
fn cl_left_moves_over_input_cells_only() {
    let mut t = T::new(20, 5);
    t.prompt(";cl=line", "hello"); // h2 e3 l4 l5 o6, 커서 7
    assert_eq!(t.click(4, 0), Some(LEFT.repeat(3)), "o·l·l을 지나 l에 선다");
    assert_eq!(t.click(2, 0), Some(LEFT.repeat(5)));
    assert_eq!(t.click(0, 0), Some(LEFT.repeat(5)), "프롬프트 글은 건너뛰고 입력 시작까지만");
    assert_eq!(t.click(7, 0), None, "커서 자리를 누르면 움직이지 않는다");
}

#[test]
fn cl_right_moves_over_input_cells_and_one_more_past_the_end() {
    let mut t = T::new(20, 5);
    t.prompt(";cl=line", "hello");
    t.feed("\x1b[1;3H"); // 커서를 h(2)로
    assert_eq!(t.click(5, 0), Some(RIGHT.repeat(3)));
    assert_eq!(t.click(6, 0), Some(RIGHT.repeat(4)));
    // 글 끝 뒤를 누르면 입력 칸을 모두 지나고, 커서가 입력 칸이면 빈 끝 칸까지 하나 더
    assert_eq!(t.click(15, 0), Some(RIGHT.repeat(5)));
}

#[test]
fn cl_at_end_cursor_cannot_move_right() {
    let mut t = T::new(20, 5);
    t.prompt(";cl=line", "hello"); // 커서는 입력 칸이 아닌 끝 칸
    assert_eq!(t.click(15, 0), None);
}

#[test]
fn cl_counts_a_wide_char_once() {
    let mut t = T::new(20, 5);
    t.prompt(";cl=line", "한글ab"); // 한2-3 글4-5 a6 b7, 커서 8
    assert_eq!(t.click(2, 0), Some(LEFT.repeat(4)));
    assert_eq!(t.click(3, 0), Some(LEFT.repeat(4)), "넓은 글자의 뒤 칸을 눌러도 그 글자");
    assert_eq!(t.click(5, 0), Some(LEFT.repeat(3)));
    t.feed("\x1b[1;3H"); // 한(2)
    assert_eq!(t.click(6, 0), Some(RIGHT.repeat(2)), "한에서 글, a");
}

#[test]
fn cl_uses_ss3_when_decckm_is_on() {
    let mut t = T::new(20, 5);
    t.prompt(";cl=line", "hello");
    t.feed("\x1b[?1h");
    assert_eq!(t.click(5, 0), Some(b"\x1bOD".repeat(2)));
}

#[test]
fn cl_follows_a_soft_wrapped_input_line() {
    let mut t = T::new(10, 5);
    t.prompt(";cl=line", "abcdefghijkl"); // 윗줄 a..h(2~9), 아랫줄 i..l(0~3), 커서 (1,4)
    assert_eq!(t.click(2, 0), Some(LEFT.repeat(12)));
    assert_eq!(t.click(1, 1), Some(LEFT.repeat(3)));
    t.feed("\x1b[1;6H"); // d(0행 5번)
    assert_eq!(t.click(2, 1), Some(RIGHT.repeat(7)), "윗줄 e~h 넷, 아랫줄 i·j·k 셋");
}

#[test]
fn cl_ignores_clicks_above_the_prompt() {
    let mut t = T::new(20, 5);
    t.feed("old\r\n");
    t.prompt(";cl=line", "hello"); // 프롬프트는 1행
    assert_eq!(t.click(3, 0), None);
    assert_eq!(t.click(4, 1), Some(LEFT.repeat(3)));
}

#[test]
fn nothing_is_sent_when_the_cursor_is_not_at_a_prompt_or_on_the_alt_screen() {
    for opts in [";cl=line", ";click_events=1"] {
        let mut t = T::new(20, 5);
        t.prompt(opts, "hello");
        t.osc("C").feed("\r\nout");
        assert_eq!(t.click(2, 0), None, "{opts}: 명령 출력 줄에 커서가 있으면 프롬프트가 아니다");
        assert_eq!(t.click(2, 1), None, "{opts}");
        // 대체 화면 안에서 프롬프트 표지를 다시 보내도 프롬프트가 아니다
        let mut t = T::new(20, 5);
        t.feed("\x1b[?1049h");
        t.prompt(opts, "hello");
        assert_eq!(t.click(4, 0), None, "{opts}: 대체 화면");
    }
}

#[test]
fn cl_before_b_has_nothing_to_move() {
    let mut t = T::new(20, 5);
    t.osc("A;cl=line").feed("$ ");
    assert_eq!(t.click(0, 0), None, "커서가 프롬프트 글에 있고 입력 칸이 아니다");
}

// ---- click_events ----

#[test]
fn click_events_1_sends_absolute_screen_cell() {
    let mut t = T::new(20, 5);
    t.feed("old\r\n");
    t.prompt(";click_events=1", "hello");
    assert_eq!(t.click(4, 1), Some(b"\x1b[<0;5;2M".to_vec()), "x는 칸+1, y는 화면 줄+1");
    assert_eq!(t.click(15, 3), Some(b"\x1b[<0;16;4M".to_vec()), "프롬프트 뒤 어디든 셸이 판단한다");
    assert_eq!(t.click(4, 0), None, "프롬프트보다 위는 보내지 않는다");
}

#[test]
fn click_events_2_sends_row_relative_to_the_prompt() {
    let mut t = T::new(10, 6);
    t.feed("old\r\n");
    t.prompt(";click_events=2", "abcdefghijkl"); // 프롬프트 1행, 이어진 줄 2행
    assert_eq!(t.click(3, 1), Some(b"\x1b[<0;4;1M".to_vec()), "프롬프트 줄이 1");
    assert_eq!(t.click(3, 2), Some(b"\x1b[<0;4;2M".to_vec()));
    assert_eq!(t.click(0, 4), Some(b"\x1b[<0;1;4M".to_vec()));
}

#[test]
fn click_events_sends_even_when_cl_would_not_move() {
    let mut t = T::new(20, 5);
    t.osc("A;click_events=1").feed("$ "); // B 전: 프롬프트 글 위
    assert_eq!(t.click(0, 0), Some(b"\x1b[<0;1;1M".to_vec()));
}
