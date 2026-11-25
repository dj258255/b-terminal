//! OSC 133 상태(커서 칸 종류·클릭 방식·본 적 있음)를 주 화면과 대체 화면이 따로 쥐는지(#826).
//! 기준: Ghostty 1.4.0 `Screen.cursor.semantic_content`(180줄)와 `Screen.semantic_prompt.{click,seen}`(84줄)은 화면마다 있다.
//! 모드 47·1047은 화면을 바꿀 때마다, 1049는 들어갈 때만 `cursorCopy`로 커서(칸 종류 포함)를 새 화면에 복사한다
//! (나올 때는 `restoreCursor`라 칸 종류를 건드리지 않는다). 클릭 방식·본 적 있음은 복사하지 않고, 대체 화면은 RIS까지 남는다.
//! 격자는 `Term`을 직접 만들어 읽는다

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::semantic::{Click, RowPrompt, SemanticClick, SemanticContent};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::osc133;

use SemanticContent::{Input, Output, Prompt};

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
    fn marks(&self) -> String {
        (0..self.term.screen_lines() as i32)
            .map(|r| match self.term.row_prompt(Line(r)) {
                RowPrompt::None => 'n',
                RowPrompt::Prompt => 'P',
                RowPrompt::Continuation => 'C',
            })
            .collect()
    }
}

/// 입력 중(`A`, `$ `, `B`)인 주 화면
fn typing() -> T {
    let mut t = T::new();
    t.osc("A").feed("$ ").osc("B");
    t
}

#[test]
fn alt_screen_1049_copies_cursor_content_on_enter_like_ghostty_cursor_copy() {
    // 이슈 본문은 `nnnnn`이라 했으나 Ghostty `cursorCopy`는 `self.cursor = other`라 칸 종류와 줄 끝 지우기 표지까지 복사한다.
    // 입력(B)인 채 들어가면 대체 화면의 `x\r\n`이 다음 줄을 이어진 프롬프트로 표시한다(Terminal.zig `index`의 defer)
    let mut t = typing();
    t.feed("\x1b[?1049h");
    assert_eq!(t.term.semantic_content(), Input);
    t.feed("x\r\ny");
    assert_eq!(t.marks(), "nCnn");
}

#[test]
fn alt_screen_1049_starts_with_fresh_click_and_seen() {
    let mut t = T::new();
    t.osc("A;cl=line").feed("$ ").osc("B");
    assert_eq!(t.term.semantic_click(), SemanticClick::Cl(Click::Line));
    assert!(t.term.semantic_seen());
    t.feed("\x1b[?1049h");
    assert_eq!(t.term.semantic_click(), SemanticClick::None);
    assert!(!t.term.semantic_seen());
    t.feed("\x1b[?1049l");
    assert_eq!(t.term.semantic_click(), SemanticClick::Cl(Click::Line));
    assert!(t.term.semantic_seen());
}

#[test]
fn alt_screen_click_does_not_leak_to_primary() {
    let mut t = T::new();
    t.feed("\x1b[?1049h").osc("A;cl=line").feed("\x1b[?1049l");
    assert_eq!(t.term.semantic_click(), SemanticClick::None);
    assert!(!t.term.semantic_seen());
}

#[test]
fn alt_screen_prompt_does_not_leak_to_primary_content() {
    let mut t = T::new();
    t.osc("A").feed("$ ").osc("D");
    assert_eq!(t.term.semantic_content(), Output);
    t.feed("\x1b[?1049h").osc("A");
    assert_eq!(t.term.semantic_content(), Prompt);
    t.feed("\x1b[?1049l");
    assert_eq!(t.term.semantic_content(), Output);
}

#[test]
fn alt_screen_line_feed_does_not_end_primary_input() {
    // `I`(줄 끝까지만 입력)로 입력 중 대체 화면에서 줄바꿈하면 대체 화면 쪽만 출력으로 돌아간다
    let mut t = T::new();
    t.osc("A").feed("$ ").osc("I");
    assert_eq!(t.term.semantic_content(), Input);
    t.feed("\x1b[?1049h\n");
    assert_eq!(t.term.semantic_content(), Output);
    t.feed("\x1b[?1049l");
    assert_eq!(t.term.semantic_content(), Input);
}

#[test]
fn modes_47_and_1047_copy_cursor_content_both_ways() {
    for (on, off) in [("\x1b[?47h", "\x1b[?47l"), ("\x1b[?1047h", "\x1b[?1047l")] {
        let mut t = typing();
        t.feed(on);
        assert_eq!(t.term.semantic_content(), Input, "{on:?} 들어갈 때 복사");
        t.osc("C");
        assert_eq!(t.term.semantic_content(), Output);
        t.feed(off);
        assert_eq!(t.term.semantic_content(), Output, "{off:?} 나올 때도 복사");
    }
}

#[test]
fn alt_screen_state_persists_between_visits_until_ris() {
    let mut t = T::new();
    t.feed("\x1b[?1049h").osc("A;cl=line").feed("\x1b[?1049l\x1b[?1049h");
    assert_eq!(t.term.semantic_click(), SemanticClick::Cl(Click::Line));
    assert!(t.term.semantic_seen());
    t.feed("\x1b[?1049l\x1bc\x1b[?1049h");
    assert_eq!(t.term.semantic_click(), SemanticClick::None);
    assert!(!t.term.semantic_seen());
}

#[test]
fn alt_screen_seen_does_not_leak_to_primary() {
    // 대체 화면에서 본 표지는 주 화면의 "본 적 있음"이 아니다
    let mut t = T::new();
    t.feed("out\x1b[?1049h").osc("A").feed("\x1b[?1049l");
    assert!(!t.term.semantic_seen());
}

#[test]
fn resize_in_alt_screen_redraws_primary_prompt_by_primary_state() {
    // 대체 화면에서 `C`(출력)여도 크기를 바꿀 때 비우는 것은 주 화면의 프롬프트다(주 화면의 칸 종류가 입력이다)
    use alacritty_terminal::index::Column;
    let mut t = typing();
    t.feed("\x1b[?1049h").osc("C");
    assert_eq!(t.term.semantic_content(), Output);
    t.term.resize(Dims(10, 4));
    t.feed("\x1b[?1049l");
    assert_eq!(t.term.grid()[Line(0)][Column(0)].c, ' ');
    assert_eq!(t.term.row_prompt(Line(0)), RowPrompt::Prompt);
}
