//! 명령 출력 고르기(#700). 기준: Ghostty v1.4.0 `Screen.selectOutput`, `PageList.highlightSemanticContent(.output)`,
//! `PromptIterator`, `SelectionGesture.dragSelectionOutput`과 그 시험("SelectionGesture output behavior …").
//! 격자는 `Term`을 직접 만들고 OSC 133은 `osc133::parse`·`apply`로 넣는다(prompt_click 시험과 같은 틀)

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::osc133;
use bt_term::output_select::{drag_output, select_output};

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

fn p(line: i32, col: usize) -> Point {
    Point::new(Line(line), Column(col))
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
        osc133::parse(data)
            .unwrap_or_else(|| panic!("OSC 133;{data}"))
            .apply(&mut self.term);
        self
    }
    /// 프롬프트 `$ `, 입력, Enter, 출력 시작(C)
    fn command(&mut self, input: &str) -> &mut Self {
        self.osc("A").feed("$ ").osc("B").feed(input).feed("\r\n").osc("C")
    }
    fn sel(&self, line: i32, col: usize) -> Option<(Point, Point)> {
        select_output(&self.term, p(line, col))
    }
}

/// 두 명령: 각 출력 구간을 고르고, 다음 프롬프트 줄은 넣지 않는다
#[test]
fn selects_the_output_of_the_command_under_the_click() {
    let mut t = T::new(20, 10);
    t.command("ls").feed("a.txt\r\nb.txt\r\n").osc("D;0");
    t.command("pwd").feed("/tmp\r\n").osc("D;0");
    t.osc("A").feed("$ ");
    // 줄 0: $ ls, 1: a.txt, 2: b.txt, 3: $ pwd, 4: /tmp, 5: $
    assert_eq!(t.sel(1, 2), Some((p(1, 0), p(2, 4))));
    assert_eq!(t.sel(2, 0), Some((p(1, 0), p(2, 4))));
    assert_eq!(t.sel(4, 3), Some((p(4, 0), p(4, 3))));
}

/// 누른 칸이 출력 줄의 빈 칸이어도(기본 출력) 그 구간이다. 끝의 빈칸은 넣지 않는다
#[test]
fn an_empty_output_cell_selects_the_block_and_trailing_blanks_are_trimmed() {
    let mut t = T::new(20, 10);
    t.command("ls").feed("a.txt\r\nbb\r\n").osc("D;0");
    t.osc("A").feed("$ ");
    assert_eq!(t.sel(2, 15), Some((p(1, 0), p(2, 1))));
}

/// 프롬프트 칸·입력 칸 위에서는 고르지 않는다(Ghostty `selectOutput`이 null)
#[test]
fn prompt_and_input_cells_are_not_output() {
    let mut t = T::new(20, 10);
    t.command("ls").feed("a.txt\r\n").osc("D;0");
    t.osc("A").feed("$ ");
    assert_eq!(t.sel(0, 0), None, "프롬프트");
    assert_eq!(t.sel(0, 3), None, "입력");
}

/// 위에 프롬프트가 없으면(셸 통합 전의 출력) 화면 맨 위부터 다음 프롬프트 바로 위까지, 끝 빈칸은 뺀다
#[test]
fn output_before_any_prompt_runs_from_the_top_to_the_next_prompt() {
    let mut t = T::new(20, 10);
    t.feed("motd line\r\nsecond\r\n\r\n");
    t.command("ls").feed("x\r\n").osc("D;0");
    assert_eq!(t.sel(1, 0), Some((p(0, 0), p(1, 5))));
}

/// 프롬프트만 있고 다음 프롬프트가 없으면 화면 끝까지가 구간이다
#[test]
fn the_last_command_runs_to_the_end_of_the_screen() {
    let mut t = T::new(20, 6);
    t.command("cat").feed("one\r\ntwo");
    assert_eq!(t.sel(1, 1), Some((p(1, 0), p(2, 2))));
}

/// 출력이 없는 명령 뒤의 프롬프트 사이에는 고를 것이 없다
#[test]
fn a_command_without_output_has_nothing_to_select() {
    let mut t = T::new(20, 6);
    t.command("true").osc("D;0");
    t.osc("A").feed("$ ");
    // 줄 1은 다음 프롬프트 줄이다. 줄 2는 아무것도 없다(다음 프롬프트 아래, 화면 끝까지 빈칸)
    assert_eq!(t.sel(3, 0), None);
}

/// 이어진 프롬프트 줄(k=s)은 같은 프롬프트로 보고 그 아래 출력을 고른다
#[test]
fn continuation_prompt_rows_belong_to_the_prompt() {
    let mut t = T::new(20, 10);
    t.osc("A")
        .feed("$ ")
        .osc("B")
        .feed("echo \\\r\n")
        .osc("A;k=s")
        .feed("> ")
        .osc("B")
        .feed("hi\r\n")
        .osc("C");
    t.feed("hi\r\n").osc("D;0");
    t.osc("A").feed("$ ");
    assert_eq!(t.sel(2, 0), Some((p(2, 0), p(2, 1))));
}

/// 끌기: 누른 구간에서 끈 구간까지. 위로 끌면 시작을, 아래로 끌면 끝을 옮긴다. 끈 자리가 출력이 아니면 그대로
#[test]
fn dragging_extends_by_whole_output_blocks() {
    let mut t = T::new(20, 12);
    t.command("a").feed("1\r\n").osc("D;0");
    t.command("b").feed("2\r\n22\r\n").osc("D;0");
    t.command("c").feed("3\r\n").osc("D;0");
    t.osc("A").feed("$ ");
    // 0 $ a, 1 "1", 2 $ b, 3 "2", 4 "22", 5 $ c, 6 "3", 7 $
    assert_eq!(drag_output(&t.term, p(3, 0), p(6, 0)), Some((p(3, 0), p(6, 0))));
    assert_eq!(drag_output(&t.term, p(3, 0), p(1, 0)), Some((p(1, 0), p(4, 1))));
    assert_eq!(
        drag_output(&t.term, p(3, 0), p(5, 0)),
        Some((p(3, 0), p(4, 1))),
        "프롬프트 위로 끌면 그대로"
    );
    assert_eq!(
        drag_output(&t.term, p(5, 0), p(6, 0)),
        None,
        "누른 자리가 출력이 아니면 없음"
    );
}

/// 넓은 글자의 뒤 칸은 글이 없는 칸이라 끝이 아니다(끝은 넓은 글자의 앞 칸)
#[test]
fn a_trailing_wide_character_ends_at_its_first_cell() {
    let mut t = T::new(20, 6);
    t.command("echo").feed("가나\r\n").osc("D;0");
    t.osc("A").feed("$ ");
    assert_eq!(t.sel(1, 0), Some((p(1, 0), p(1, 2))));
}

/// 격자 밖 좌표(끄는 사이 스크롤백이 지워지거나 크기가 줄어 남은 옛 좌표)는 고르지 않는다. 죽지 않는다
#[test]
fn points_outside_the_grid_select_nothing() {
    let mut t = T::new(20, 6);
    t.command("ls").feed("a\r\n").osc("D;0");
    assert_eq!(t.sel(-3, 0), None, "없는 스크롤백 줄");
    assert_eq!(t.sel(6, 0), None, "화면 아래");
    assert_eq!(t.sel(1, 20), None, "오른쪽 밖");
    assert_eq!(drag_output(&t.term, p(-5, 0), p(1, 0)), None);
    assert_eq!(drag_output(&t.term, p(1, 0), p(1, 40)), Some((p(1, 0), p(1, 0))));
}
