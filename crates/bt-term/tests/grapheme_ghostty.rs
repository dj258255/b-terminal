//! 글자 묶음 폭(DECSET 2027)에서 VS16·VS15·피부색 수식자가 놓이는 자리를 Ghostty와 견준다(#536).
//! 기준: Ghostty 35188fe `Terminal.zig`의 `print`(1893~2270줄)와 그 시험들(줄 번호는 각 시험에 적는다).
//! 격자 칸의 표시(`WIDE_CHAR`·`WIDE_CHAR_SPACER`·`LEADING_WIDE_CHAR_SPACER`·`WRAPLINE`)를 직접 본다

use alacritty_terminal::event::{Event, EventListener};
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

#[derive(Clone)]
struct Quiet;
impl EventListener for Quiet {
    fn send_event(&self, _: Event) {}
}

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
    /// (줄, 칸) 위치의 글자·표시·덧붙은 글자
    fn cell(&self, line: i32, col: usize) -> (char, Flags, Vec<char>) {
        let c = &self.term.grid()[Line(line)][Column(col)];
        (c.c, c.flags, c.zerowidth().map(|z| z.to_vec()).unwrap_or_default())
    }
    /// 커서 (줄, 칸, 줄바꿈 대기)
    fn cursor(&self) -> (i32, usize, bool) {
        let c = &self.term.grid().cursor;
        (c.point.line.0, c.point.column.0, c.input_needs_wrap)
    }
}

const ON: &str = "\x1b[?2027h";
const OFF: &str = "\x1b[?2027l";
const VS16: &str = "\u{FE0F}";
const VS15: &str = "\u{FE0E}";

/// Ghostty 7183줄: 줄 끝 `#`에 VS16이 오면 `#`이 다음 줄로 옮겨 두 칸이 된다. 옛 칸은 앞 칸 자리 표시가 된다
#[test]
fn vs16_at_line_end_moves_to_next_line() {
    let mut t = T::new(3, 5);
    t.feed(ON).feed("\x1b[2C#").feed(VS16);
    assert_eq!(t.cursor(), (1, 2, false));
    let (c, f, z) = t.cell(0, 2);
    assert_eq!(c, ' ');
    assert!(f.contains(Flags::LEADING_WIDE_CHAR_SPACER), "{f:?}");
    assert!(f.contains(Flags::WRAPLINE), "{f:?}");
    assert!(z.is_empty());
    let (c, f, z) = t.cell(1, 0);
    assert_eq!(c, '#');
    assert!(f.contains(Flags::WIDE_CHAR), "{f:?}");
    assert_eq!(z, vec!['\u{FE0F}']);
    assert!(t.cell(1, 1).1.contains(Flags::WIDE_CHAR_SPACER));
}

/// Ghostty 7485줄: `#`이 끝에서 둘째 칸이면 제자리에서 넓어지고 줄바꿈 대기가 된다
#[test]
fn vs16_in_place_then_pending_wrap() {
    let mut t = T::new(3, 5);
    t.feed(ON).feed("\x1b[1C#").feed(VS16);
    assert_eq!(t.cursor(), (0, 2, true));
    let (c, f, z) = t.cell(0, 1);
    assert_eq!(c, '#');
    assert!(f.contains(Flags::WIDE_CHAR));
    assert_eq!(z, vec!['\u{FE0F}']);
    assert!(t.cell(0, 2).1.contains(Flags::WIDE_CHAR_SPACER));
}

/// Ghostty 7720줄: 데바나가리 `क्‍ष`가 줄 끝에서 마지막 글자로 넓어지면 다음 줄로 옮긴다
#[test]
fn devanagari_widens_onto_next_line() {
    let mut t = T::new(3, 5);
    t.feed(ON).feed("\x1b[2C\u{0915}\u{094D}\u{200D}");
    assert_eq!(t.cursor(), (0, 2, true));
    t.feed("\u{0937}");
    assert_eq!(t.cursor(), (1, 2, false));
    assert!(t.cell(0, 2).1.contains(Flags::LEADING_WIDE_CHAR_SPACER));
    let (c, f, z) = t.cell(1, 0);
    assert_eq!(c, '\u{0915}');
    assert!(f.contains(Flags::WIDE_CHAR));
    assert_eq!(z, vec!['\u{094D}', '\u{200D}', '\u{0937}']);
    assert!(t.cell(1, 1).1.contains(Flags::WIDE_CHAR_SPACER));
}

/// Ghostty 8563줄: 여백 오른쪽 밖 맨 끝 칸에서 넓어지면 다음 줄의 여백 안으로 간다
#[test]
fn vs16_outside_right_margin_wraps_onto_margin() {
    let mut t = T::new(5, 3);
    t.feed(ON).feed("\x1b[?69h\x1b[1;2s\x1b[1;5H\u{261D}");
    assert_eq!(t.cursor(), (0, 4, true));
    t.feed(VS16);
    assert_eq!(t.cursor(), (1, 1, true));
    let (c, f, _) = t.cell(1, 0);
    assert_eq!(c, '\u{261D}');
    assert!(f.contains(Flags::WIDE_CHAR));
    assert!(t.cell(1, 1).1.contains(Flags::WIDE_CHAR_SPACER));
}

/// Ghostty 8335줄: 자동 줄바꿈이 꺼져 있으면 넓히지 않고 VS16도 붙이지 않는다
#[test]
fn vs16_at_edge_without_wraparound_changes_nothing() {
    let mut t = T::new(5, 5);
    t.feed(ON).feed("\x1b[?7lAAAA\u{2764}").feed(VS16);
    assert_eq!(t.cursor().0, 0);
    assert_eq!(t.cursor().1, 4);
    let (c, f, z) = t.cell(0, 4);
    assert_eq!(c, '\u{2764}');
    assert!(!f.contains(Flags::WIDE_CHAR));
    assert!(z.is_empty(), "VS16이 붙었다: {z:?}");
}

/// Ghostty 6925줄: VS15가 기본 두 칸인 이모지를 한 칸으로 만든다
#[test]
fn vs15_narrows_wide_emoji() {
    let mut t = T::new(5, 5);
    t.feed(ON).feed("\u{2614}");
    assert_eq!(t.cursor(), (0, 2, false));
    t.feed(VS15);
    assert_eq!(t.cursor(), (0, 1, false));
    let (c, f, z) = t.cell(0, 0);
    assert_eq!(c, '\u{2614}');
    assert!(!f.contains(Flags::WIDE_CHAR), "{f:?}");
    assert_eq!(z, vec!['\u{FE0E}']);
    assert!(!t.cell(0, 1).1.contains(Flags::WIDE_CHAR_SPACER));
}

/// Ghostty 7065줄: 줄바꿈 대기 중 VS15는 대기를 풀고 커서는 그 칸에 남는다
#[test]
fn vs15_with_pending_wrap_clears_it() {
    let mut t = T::new(4, 5);
    t.feed(ON).feed("\u{1F34B}\u{2614}");
    assert_eq!(t.cursor(), (0, 3, true));
    t.feed(VS15);
    assert_eq!(t.cursor(), (0, 3, false));
    assert!(!t.cell(0, 2).1.contains(Flags::WIDE_CHAR));
    assert!(!t.cell(0, 3).1.contains(Flags::WIDE_CHAR_SPACER));
    // 앞 글자는 그대로 넓다
    assert!(t.cell(0, 0).1.contains(Flags::WIDE_CHAR));
}

/// Ghostty 7000줄: VS15를 받지 않는 이모지(🧠)는 넓은 채 그대로, VS15는 버린다
#[test]
fn invalid_vs15_after_wide_emoji_is_ignored() {
    let mut t = T::new(80, 5);
    t.feed(ON).feed("\u{1F9E0}").feed(VS15);
    assert_eq!(t.cursor(), (0, 2, false));
    let (_, f, z) = t.cell(0, 0);
    assert!(f.contains(Flags::WIDE_CHAR));
    assert!(z.is_empty(), "{z:?}");
}

/// Ghostty 2176줄·7593줄·6658줄: 이모지가 아닌 글자 뒤의 VS16은 버린다. 2027을 끈 경우도 같다
#[test]
fn vs16_after_non_emoji_is_dropped() {
    for pre in [ON, OFF] {
        let mut t = T::new(80, 80);
        t.feed(pre).feed("x").feed(VS16);
        assert_eq!(t.cursor(), (0, 1, false), "{pre:?}");
        let (c, f, z) = t.cell(0, 0);
        assert_eq!(c, 'x');
        assert!(!f.contains(Flags::WIDE_CHAR));
        assert!(z.is_empty(), "{pre:?}: VS16이 붙었다: {z:?}");
    }
}

/// Ghostty 7625줄: `x` VS16 `y`는 좁은 두 칸
#[test]
fn invalid_vs16_then_char() {
    let mut t = T::new(80, 80);
    t.feed(ON).feed("x").feed(VS16).feed("y");
    assert_eq!(t.cursor(), (0, 2, false));
    assert!(t.cell(0, 0).2.is_empty());
    assert_eq!(t.cell(0, 1).0, 'y');
}

/// Ghostty 7827줄: 버린 VS16 뒤의 결합 문자는 앞 글자에 붙는다
#[test]
fn invalid_vs16_then_combining() {
    let mut t = T::new(80, 80);
    t.feed(ON).feed("n").feed(VS16).feed("\u{0303}");
    assert_eq!(t.cursor(), (0, 1, false));
    assert_eq!(t.cell(0, 0).2, vec!['\u{0303}']);
}

/// Ghostty 6616줄: 2027을 끄면 ❤ VS16은 좁은 채 VS16이 붙는다
#[test]
fn vs16_off_mode_keeps_heart_narrow() {
    let mut t = T::new(5, 5);
    t.feed(OFF).feed("\u{2764}").feed(VS16);
    let (_, f, z) = t.cell(0, 0);
    assert!(!f.contains(Flags::WIDE_CHAR));
    assert_eq!(z, vec!['\u{FE0F}']);
    assert_eq!(t.cursor(), (0, 1, false));
}

/// Ghostty 6793줄·6764줄: 키패드 `#`은 VS16으로 넓어지고 VS15로 좁은 채 붙는다
#[test]
fn keypad_hash_variation_selectors() {
    let mut t = T::new(80, 80);
    t.feed(ON).feed("#").feed(VS16);
    assert_eq!(t.cursor(), (0, 2, false));
    assert!(t.cell(0, 0).1.contains(Flags::WIDE_CHAR));
    let mut t = T::new(80, 80);
    t.feed(ON).feed("#").feed(VS15);
    assert_eq!(t.cursor(), (0, 1, false));
    assert_eq!(t.cell(0, 0).2, vec!['\u{FE0E}']);
}

/// Ghostty 6851줄: 따옴표 뒤의 피부색은 따옴표에 붙지 않고 두 칸 글자가 된다
#[test]
fn skin_tone_next_to_non_base_is_new_cell() {
    let mut t = T::new(80, 80);
    t.feed(ON).feed("\"\u{1F3FF}\"");
    assert_eq!(t.cursor(), (0, 4, false));
    let (c, f, z) = t.cell(0, 0);
    assert_eq!(c, '"');
    assert!(z.is_empty(), "{z:?}");
    assert!(!f.contains(Flags::WIDE_CHAR));
    let (c, f, _) = t.cell(0, 1);
    assert_eq!(c, '\u{1F3FF}');
    assert!(f.contains(Flags::WIDE_CHAR));
    assert!(t.cell(0, 2).1.contains(Flags::WIDE_CHAR_SPACER));
    assert_eq!(t.cell(0, 3).0, '"');
}

/// Ghostty 6822줄: 바탕 이모지 뒤의 피부색은 그 칸에 붙어 두 칸
#[test]
fn skin_tone_after_base_joins() {
    let mut t = T::new(80, 80);
    t.feed(ON).feed("\u{1F44B}\u{1F3FF}");
    assert_eq!(t.cursor(), (0, 2, false));
    let (c, f, z) = t.cell(0, 0);
    assert_eq!(c, '\u{1F44B}');
    assert!(f.contains(Flags::WIDE_CHAR));
    assert_eq!(z, vec!['\u{1F3FF}']);
}
