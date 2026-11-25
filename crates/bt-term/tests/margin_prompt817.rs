//! 좌우 여백 안 IL·DL이 OSC 133 줄 표시(프롬프트·이어진 프롬프트)를 칸이 아니라 줄 단위로 다루는지(#817).
//! 기준: Ghostty main PR #15327(2026-11-23)의 시험 `Terminal: insertLines and deleteLines in margins keep semantic prompt`(네 경우)와
//! `Page cloneRowFrom partial combines semantic prompt`·`Page moveCells combines semantic prompt`(아홉 조합, 부분 옮기기는 받는 줄과 보낸 줄의 표시를
//! `combine`: 프롬프트 > 이어짐 > 없음). 줄 전체를 옮기면 보낸 줄 표시, 칸만 지운 줄은 표시를 그대로 둔다.
//! kitty 유니코드 자리표시 시험(`insertLines in margins keeps kitty placeholder flag`)은 우리 엔진에 줄 단위 자리표시 표시가 없어 옮기지 않았다.
//! 격자는 `Term`을 직접 만들어 읽는다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::semantic::RowPrompt;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::osc133;

use RowPrompt::{Continuation as C, None as N, Prompt as P};

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
        osc133::parse(data)
            .unwrap_or_else(|| panic!("OSC 133;{data}"))
            .apply(&mut self.term);
        self
    }
    /// 0부터 센 줄 `row`에 표시를 달고 "$ "를 찍는다
    fn mark(&mut self, row: usize, mark: RowPrompt) -> &mut Self {
        self.feed(&format!("\x1b[{};1H", row + 1));
        match mark {
            N => self.feed("$ "),
            P => self.osc("A").feed("$ ").osc("C"),
            C => self.osc("A;k=s").feed("> ").osc("C"),
        }
    }
    fn marks(&self) -> Vec<RowPrompt> {
        (0..self.term.screen_lines() as i32)
            .map(|r| self.term.row_prompt(Line(r)))
            .collect()
    }
}

fn show(marks: &[RowPrompt]) -> String {
    marks
        .iter()
        .map(|m| match m {
            N => '.',
            P => 'P',
            C => 'c',
        })
        .collect()
}

/// 5줄 10열, 줄마다 표시를 달고, 여백을 켜고, 커서를 3;3에 두고 `seq`를 보낸 뒤의 줄 표시
fn run(marks: [RowPrompt; 5], margins: &str, seq: &str) -> String {
    let mut t = T::new(10, 5);
    for (row, m) in marks.into_iter().enumerate() {
        t.mark(row, m);
    }
    t.feed("\x1b[?69h").feed(margins).feed("\x1b[3;3H").feed(seq);
    show(&t.marks())
}

fn only(row: usize, m: RowPrompt) -> [RowPrompt; 5] {
    let mut a = [N; 5];
    a[row] = m;
    a
}

// ---- Ghostty `insertLines and deleteLines in margins keep semantic prompt` ----

#[test]
fn il_margins_3_10_keeps_prompt_in_place() {
    assert!(run(only(3, P), "\x1b[3;10s", "\x1b[L").as_bytes()[3] == b'P');
}

#[test]
fn il_margins_1_8_moves_prompt_down() {
    assert!(run(only(2, P), "\x1b[1;8s", "\x1b[L").as_bytes()[3] == b'P');
}

#[test]
fn dl_margins_3_10_keeps_prompt_in_place() {
    assert!(run(only(2, P), "\x1b[3;10s", "\x1b[M").as_bytes()[2] == b'P');
}

#[test]
fn dl_margins_1_8_moves_prompt_up() {
    assert!(run(only(3, P), "\x1b[1;8s", "\x1b[M").as_bytes()[2] == b'P');
}

// ---- 이슈 #817 표의 여섯 입력(Ghostty main 열) ----

#[test]
fn issue_table_six_inputs() {
    assert_eq!(run(only(3, P), "\x1b[3;10s", "\x1b[L"), "...PP");
    assert_eq!(run(only(2, P), "\x1b[1;8s", "\x1b[L"), "..PP.");
    assert_eq!(run(only(2, P), "\x1b[3;10s", "\x1b[L"), "..PP.");
    assert_eq!(run(only(2, P), "\x1b[1;8s", "\x1b[M"), "..P..");
    assert_eq!(run(only(3, P), "\x1b[1;8s", "\x1b[M"), "..PP.");
    assert_eq!(run([N, N, C, P, N], "\x1b[1;8s", "\x1b[L"), "..cPP");
}

// ---- `combine` 아홉 조합: 받는 줄(3)에 보낸 줄(2)의 칸 일부가 온다. 보낸 줄은 칸만 지워져 표시를 그대로 둔다 ----

#[test]
fn partial_move_combines_marks_in_all_nine_pairs() {
    let all = [N, P, C];
    for dst in all {
        for src in all {
            let want = if dst == P || src == P {
                P
            } else if dst == C || src == C {
                C
            } else {
                N
            };
            for margins in ["\x1b[3;10s", "\x1b[1;8s"] {
                // IL: 줄 2 -> 줄 3, 줄 3 -> 줄 4
                let got = run([N, N, src, dst, N], margins, "\x1b[L");
                let want_il = vec![N, N, src, want, dst];
                assert_eq!(got, show(&want_il), "IL dst={dst:?} src={src:?} {margins:?}");
                // DL: 줄 3 -> 줄 2, 줄 4 -> 줄 3
                let got = run([N, N, dst, src, N], margins, "\x1b[M");
                let want_dl = vec![N, N, want, src, N];
                assert_eq!(got, show(&want_dl), "DL dst={dst:?} src={src:?} {margins:?}");
            }
        }
    }
}

// ---- 칸만 지운 줄은 표시를 남기고, 칸 표시(넓은 글자)는 건드리지 않는다 ----

#[test]
fn blanked_rows_keep_their_marks_and_others_cells_are_untouched() {
    // 줄 4가 프롬프트면 DL이 한 줄 올려도 줄 4(비워짐)는 표시를 남기고 줄 3은 합쳐서 받는다
    assert_eq!(run(only(4, P), "\x1b[1;8s", "\x1b[M"), "...PP");
    // 0번 칸에 넓은 글자가 있어도 표시와 따로 움직인다
    let mut t = T::new(10, 5);
    t.mark(3, P);
    t.feed("\x1b[4;1H\u{d55c}").feed("\x1b[?69h\x1b[3;10s\x1b[3;3H\x1b[L");
    assert_eq!(show(&t.marks()), "...PP");
    use alacritty_terminal::index::Column;
    use alacritty_terminal::term::cell::Flags;
    assert!(t.term.grid()[Line(3)][Column(0)].flags.contains(Flags::WIDE_CHAR));
}
