//! OSC 133 확장: 줄의 프롬프트 표시, fresh-line, 프롬프트에서 ED 2, 크기가 바뀔 때 다시 그리기(#329).
//! 기준: Ghostty Terminal.zig `semanticPrompt`(2784~2902줄)·`semanticPromptFreshLine`·`index`(3023~3050줄)·`printWrap`(2450~2480줄)·
//! `eraseDisplay`(4423~4535줄), Screen.zig `cursorSetSemanticContent`·`clearPromptForRedraw`.
//! 격자는 `Term`을 직접 만들어 읽고, OSC 133은 스캐너를 거치지 않고 `osc133::parse`·`apply`로 그 자리에서 넣는다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::semantic::{RowPrompt, SemanticContent};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::osc133;

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
    /// `133;` 뒤의 글을 읽어 엔진에 적용한다
    fn osc(&mut self, data: &str) -> &mut Self {
        let sp = osc133::parse(data).unwrap_or_else(|| panic!("OSC 133;{data}"));
        sp.apply(&mut self.term);
        self
    }
    fn resize(&mut self, cols: usize, rows: usize) -> &mut Self {
        self.term.resize(Dims(cols, rows));
        self
    }
    /// 보이는 줄마다 글(줄 끝 공백 없음), 끝의 빈 줄은 뺌
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
    /// 스크롤백 줄의 글(맨 위부터)
    fn history_text(&self) -> Vec<String> {
        let h = self.term.grid().history_size() as i32;
        (-h..0)
            .map(|l| {
                (0..self.term.columns())
                    .map(|c| self.term.grid()[Line(l)][Column(c)].c)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }
    fn history(&self) -> usize {
        self.term.grid().history_size()
    }
    /// (열, 줄) 0부터
    fn cursor(&self) -> (usize, usize) {
        let p = self.term.grid().cursor.point;
        (p.column.0, p.line.0 as usize)
    }
    fn mark(&self, row: i32) -> RowPrompt {
        self.term.row_prompt(Line(row))
    }
    fn marks(&self) -> Vec<RowPrompt> {
        (0..self.term.screen_lines() as i32).map(|r| self.mark(r)).collect()
    }
}

use RowPrompt::{Continuation as C, None as N, Prompt as P};

// ---- 줄의 프롬프트 표시 ----

#[test]
fn a_marks_the_cursor_row_as_a_prompt_and_prompt_content_follows() {
    // Ghostty "Terminal: semantic prompt"
    let mut t = T::new(10, 5);
    t.feed("out\r\n");
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
    t.osc("A").feed("$ ");
    assert_eq!(t.marks(), vec![N, P, N, N, N]);
    assert_eq!(t.term.semantic_content(), SemanticContent::Prompt);
    assert!(t.term.semantic_seen());
}

#[test]
fn nothing_is_seen_or_marked_without_osc_133() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n$ ");
    assert!(!t.term.semantic_seen());
    assert_eq!(t.marks(), vec![N; 5]);
}

#[test]
fn k_selects_prompt_or_continuation_for_the_row() {
    // Screen.cursorSetSemanticContent: initial·right는 prompt, continuation·secondary는 prompt_continuation
    for (opt, want) in [("k=i", P), ("k=r", P), ("k=c", C), ("k=s", C)] {
        let mut t = T::new(10, 5);
        t.osc(&format!("A;{opt}"));
        assert_eq!(t.mark(0), want, "A;{opt}");
        let mut t = T::new(10, 5);
        t.osc(&format!("P;{opt}"));
        assert_eq!(t.mark(0), want, "P;{opt}");
    }
    let mut t = T::new(10, 5);
    t.osc("P");
    assert_eq!(t.mark(0), P, "k가 없으면 initial");
    assert_eq!(t.term.semantic_content(), SemanticContent::Prompt);
}

#[test]
fn n_is_a_with_the_same_options() {
    let mut t = T::new(10, 5);
    t.feed("out").osc("N;aid=7;k=s");
    assert_eq!(t.cursor(), (0, 1), "fresh-line");
    assert_eq!(t.mark(1), C);
    assert_eq!(t.term.semantic_content(), SemanticContent::Prompt);
}

#[test]
fn linefeed_while_in_prompt_or_input_marks_the_next_row_as_continuation() {
    // Terminal.index의 defer 블록과 시험 "semantic prompt continuations"
    let mut t = T::new(10, 6);
    t.osc("A").feed("$ one\r\n> two");
    assert_eq!(t.marks(), vec![P, C, N, N, N, N]);
    t.osc("B").feed("\r\nthree");
    assert_eq!(t.marks()[2], C, "입력 중 줄바꿈도 이어진 줄");
    assert_eq!(t.term.semantic_content(), SemanticContent::Input);
}

#[test]
fn input_ended_at_eol_stops_marking_after_one_linefeed() {
    // I: 줄 끝까지가 입력이다. 줄바꿈하면 출력으로 돌아가고 새 줄은 표시하지 않는다
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ").osc("I").feed("ls\r\nfile");
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
    assert_eq!(t.marks(), vec![P, N, N, N, N]);
}

#[test]
fn soft_wrap_marks_only_prompt_content_rows_as_continuation() {
    // printWrap: 프롬프트 글이 넘어가면 이어진 줄, 입력 글이 넘어가면 표시하지 않는다(상태는 그대로)
    let mut t = T::new(5, 5);
    t.osc("A").feed("abcdefgh");
    assert_eq!(t.marks(), vec![P, C, N, N, N]);
    assert_eq!(t.term.semantic_content(), SemanticContent::Prompt);

    let mut t = T::new(5, 5);
    t.osc("A").feed("$ ").osc("B").feed("abcdefgh");
    assert_eq!(t.marks(), vec![P, N, N, N, N]);
    assert_eq!(t.term.semantic_content(), SemanticContent::Input);
}

#[test]
fn soft_wrap_keeps_the_clear_eol_rule() {
    let mut t = T::new(5, 5);
    t.osc("A").feed("$ ").osc("I").feed("abcdefgh");
    assert_eq!(
        t.term.semantic_content(),
        SemanticContent::Input,
        "줄 넘김은 상태를 지우지 않는다"
    );
    t.feed("\r\nx");
    assert_eq!(t.term.semantic_content(), SemanticContent::Output, "줄바꿈은 지운다");
}

#[test]
fn c_at_column_zero_unprompts_the_row() {
    // fish: 입력 뒤 줄바꿈 없이 0번 칸에서 C를 받으면 그 줄은 프롬프트가 아니다
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ls\r\n");
    assert_eq!(t.mark(1), C);
    t.osc("C");
    assert_eq!(t.mark(1), N);
    assert_eq!(t.mark(0), P);
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
}

#[test]
fn c_away_from_column_zero_keeps_the_mark() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ls").osc("C");
    assert_eq!(t.mark(0), P);
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
}

#[test]
fn d_goes_back_to_output() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$").osc("B");
    assert_eq!(t.term.semantic_content(), SemanticContent::Input);
    t.osc("D;0");
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
}

#[test]
fn the_mark_survives_overwriting_the_row() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ abc\rxyz\r");
    assert_eq!(t.mark(0), P);
    assert!(t.plain().starts_with("xyz"));
}

#[test]
fn marks_scroll_into_history_with_the_row() {
    let mut t = T::new(10, 3);
    t.osc("A").feed("$\r\n\r\n\r\n\r\n");
    assert_eq!(t.history(), 2);
    assert_eq!(t.term.row_prompt(Line(-2)), P, "프롬프트 줄은 스크롤백으로 갔다");
    assert_eq!(t.term.row_prompt(Line(-1)), C);
}

#[test]
fn reset_forgets_semantic_state() {
    let mut t = T::new(10, 5);
    t.osc("A;redraw=0").feed("$ ");
    assert!(t.term.semantic_seen());
    t.feed("\x1bc");
    assert!(!t.term.semantic_seen());
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
    assert_eq!(t.term.semantic_redraw(), osc133::Redraw::True);
}

#[test]
fn redraw_option_is_remembered_and_a_without_it_keeps_the_last_value() {
    let mut t = T::new(10, 5);
    assert_eq!(t.term.semantic_redraw(), osc133::Redraw::True, "기본값");
    t.osc("A;redraw=0");
    assert_eq!(t.term.semantic_redraw(), osc133::Redraw::False);
    t.osc("A");
    assert_eq!(t.term.semantic_redraw(), osc133::Redraw::False, "옵션이 없으면 그대로");
    t.osc("A;redraw=last");
    assert_eq!(t.term.semantic_redraw(), osc133::Redraw::Last);
    t.osc("A;redraw=1");
    assert_eq!(t.term.semantic_redraw(), osc133::Redraw::True);
}

// ---- fresh-line ----

#[test]
fn l_does_nothing_at_column_zero() {
    let mut t = T::new(10, 5);
    t.feed("ab\r\n").osc("L");
    assert_eq!(t.cursor(), (0, 1));
}

#[test]
fn l_moves_to_the_start_of_the_next_line_otherwise() {
    let mut t = T::new(10, 5);
    t.feed("ab").osc("L");
    assert_eq!(t.cursor(), (0, 1));
    assert_eq!(t.marks(), vec![N; 5], "L은 표시를 남기지 않는다");
    assert_eq!(t.term.semantic_content(), SemanticContent::Output);
}

#[test]
fn l_at_the_bottom_scrolls() {
    let mut t = T::new(10, 2);
    t.feed("a\r\nb").osc("L");
    assert_eq!(t.cursor(), (0, 1));
    assert_eq!(t.history(), 1);
    assert_eq!(t.plain(), "b");
}

#[test]
fn l_with_a_pending_wrap_goes_to_the_next_line() {
    let mut t = T::new(5, 5);
    t.feed("abcde").osc("L");
    assert_eq!(t.cursor(), (0, 1));
    assert_eq!(t.plain(), "abcde");
}

#[test]
fn a_does_a_fresh_line_before_marking() {
    // 출력이 줄바꿈 없이 끝났어도 프롬프트는 새 줄에서 시작한다
    let mut t = T::new(10, 5);
    t.feed("output").osc("A").feed("$ ");
    assert_eq!(t.plain(), "output\n$");
    assert_eq!(t.marks(), vec![N, P, N, N, N]);
}

#[test]
fn a_at_column_zero_stays_on_the_row() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n").osc("A").feed("$");
    assert_eq!(t.plain(), "out\n$");
    assert_eq!(t.marks(), vec![N, P, N, N, N]);
}

#[test]
fn fresh_line_treats_a_cursor_left_of_the_left_margin_as_at_column_zero_of_the_screen() {
    // semanticPromptFreshLine: 커서가 왼쪽 여백 왼쪽이면 0번 칸을, 아니면 왼쪽 여백을 "줄의 처음"으로 본다
    let mut t = T::new(10, 5);
    t.feed("\x1b[?69h\x1b[3;8s");
    // 왼쪽 여백 칸(0부터 2): 처음이라 움직이지 않는다
    t.feed("\x1b[1;3H").osc("L");
    assert_eq!(t.cursor(), (2, 0));
    // 왼쪽 여백 오른쪽: 왼쪽 여백으로 가서 다음 줄
    t.feed("\x1b[1;5H").osc("L");
    assert_eq!(t.cursor(), (2, 1));
    // 왼쪽 여백 왼쪽(1번 칸): 0번 칸이 아니므로 줄을 바꾼다. 줄 처음으로 가는 것은 CR의 규칙을 따른다
    t.feed("\x1b[3;2H").osc("L");
    assert_eq!(t.cursor(), (0, 3));
    // 0번 칸: 움직이지 않는다
    t.feed("\x1b[5;1H").osc("L");
    assert_eq!(t.cursor(), (0, 4));
}

#[test]
fn fresh_line_in_input_state_marks_the_new_row_as_continuation() {
    // semanticPromptFreshLine은 index()를 거치므로 입력 중이면 새 줄이 이어진 줄이 된다
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ").osc("B").feed("ab").osc("L");
    assert_eq!(t.cursor(), (0, 1));
    assert_eq!(t.mark(1), C);
}

// ---- 프롬프트에서 ED 2 ----

#[test]
fn ed2_at_a_prompt_scrolls_the_screen_into_history_first() {
    // "Clearing at a prompt": 마지막 글 줄이 프롬프트면 지우기 전에 화면을 스크롤백으로 올린다
    let mut t = T::new(10, 5);
    t.feed("out1\r\nout2\r\n").osc("A").feed("$ ").osc("B");
    t.feed("\x1b[H\x1b[2J");
    assert_eq!(t.plain(), "");
    assert_eq!(t.history_text(), vec!["out1", "out2", "$"]);
}

#[test]
fn ed2_ignores_blank_rows_below_the_prompt() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ").osc("B");
    t.feed("\x1b[5;1H\x1b[2J");
    assert_eq!(t.history_text(), vec!["$"]);
}

#[test]
fn ed2_after_the_prompt_continuation_rows_also_scrolls() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("line1\r\n").osc("A;k=s").feed("> ").osc("B");
    t.feed("\x1b[2J");
    assert_eq!(t.history_text(), vec!["line1", ">"]);
}

#[test]
fn ed2_with_command_output_below_the_prompt_only_clears() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ls").osc("B").osc("C").feed("\r\nfile1\r\nfile2");
    t.feed("\x1b[H\x1b[2J");
    assert_eq!(t.plain(), "");
    assert_eq!(t.history(), 0);
}

#[test]
fn ed2_without_any_osc_133_only_clears() {
    // 표지를 받은 적이 없는 화면에는 프롬프트 줄이 없다(Ghostty `semantic_prompt.seen`)
    let mut t = T::new(10, 5);
    t.feed("out1\r\n$ ").feed("\x1b[H\x1b[2J");
    assert_eq!(t.plain(), "");
    assert_eq!(t.history(), 0);
}

#[test]
fn ed2_on_the_alternate_screen_only_clears() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ").osc("B");
    t.feed("\x1b[?1049h").feed("full\x1b[2J");
    assert_eq!(t.history(), 0);
    t.feed("\x1b[?1049l");
    assert_eq!(t.plain(), "$");
}

#[test]
fn ed2_with_iso_protection_also_scrolls_at_a_prompt() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n").osc("A").feed("$ ").osc("B");
    t.feed("\x1bV\x1b[2J");
    assert_eq!(t.history_text(), vec!["out", "$"]);
}

#[test]
fn ed0_ed1_and_ed3_are_not_changed() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n").osc("A").feed("$ ").osc("B");
    t.feed("\x1b[J");
    assert_eq!(t.history(), 0);
    t.feed("\x1b[1J");
    assert_eq!(t.history(), 0);
}

// ---- 크기가 바뀔 때 다시 그리기 ----

#[test]
fn resize_clears_the_prompt_rows_so_the_shell_can_redraw_them() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n").osc("A").feed("$ ").osc("B").feed("ls");
    t.resize(8, 5);
    assert_eq!(t.plain(), "out");
    assert_eq!(t.mark(1), P, "줄의 표시는 남는다");
}

#[test]
fn resize_clears_a_multiline_prompt_from_its_first_row() {
    let mut t = T::new(10, 6);
    t.feed("out\r\n")
        .osc("A")
        .feed("line1\r\n")
        .osc("A;k=s")
        .feed("> ")
        .osc("B")
        .feed("ls");
    t.resize(8, 6);
    assert_eq!(t.plain(), "out");
}

#[test]
fn resize_with_redraw_last_clears_only_the_cursor_row() {
    // bash: 마지막 줄만 다시 그린다
    let mut t = T::new(10, 6);
    t.feed("out\r\n")
        .osc("A;redraw=last")
        .feed("line1\r\n")
        .osc("A;k=s")
        .feed("> ")
        .osc("B")
        .feed("ls");
    t.resize(8, 6);
    assert_eq!(t.plain(), "out\nline1");
}

#[test]
fn resize_with_redraw_false_clears_nothing() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n").osc("A;redraw=0").feed("$ ").osc("B").feed("ls");
    t.resize(8, 5);
    assert_eq!(t.plain(), "out\n$ ls");
}

#[test]
fn resize_while_a_command_is_running_clears_nothing() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n")
        .osc("A")
        .feed("$ ls")
        .osc("B")
        .osc("C")
        .feed("\r\nfile");
    t.resize(8, 5);
    assert_eq!(t.plain(), "out\n$ ls\nfile");
}

#[test]
fn resize_without_osc_133_clears_nothing() {
    let mut t = T::new(10, 5);
    t.feed("out\r\n$ ls");
    t.resize(8, 5);
    assert_eq!(t.plain(), "out\n$ ls");
}

#[test]
fn resize_that_changes_nothing_clears_nothing() {
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ls").osc("B");
    t.resize(10, 5);
    assert_eq!(t.plain(), "$ ls");
}

#[test]
fn resize_does_not_clear_the_primary_prompt_while_the_alternate_screen_runs() {
    // Ghostty는 주 화면을 줄이면서 주 화면의 커서 상태를 본다. 대체 화면의 내용은 건드리지 않는다
    let mut t = T::new(10, 5);
    t.osc("A").feed("$ ").osc("B").feed("\x1b[?1049h\x1b[Hfull");
    t.resize(8, 5);
    assert_eq!(t.plain(), "full");
    t.feed("\x1b[?1049l");
    assert_eq!(t.plain(), "", "돌아온 주 화면의 프롬프트 줄은 셸이 다시 그린다");
}

#[test]
fn resize_reflowed_prompt_rows_are_cleared_from_the_first_wrapped_row() {
    // 줄이면 프롬프트가 두 줄로 감긴다. 첫 줄부터 지운다
    let mut t = T::new(10, 5);
    t.feed("out\r\n").osc("A").feed("$ ").osc("B").feed("abcdefghijkl");
    t.resize(6, 5);
    // 줄이면서 엔진이 "out"을 스크롤백으로 올렸다. 프롬프트의 감긴 세 줄이 모두 비워진다
    assert_eq!(t.history_text(), vec!["out"]);
    assert_eq!(t.plain(), "");
    assert_eq!(t.mark(0), P);
}
