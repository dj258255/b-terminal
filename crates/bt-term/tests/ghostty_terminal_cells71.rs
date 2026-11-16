//! 0.71 칸 내용 조사(#577)에서 나온 차이를 Ghostty와 견준다(#579~#582).
//! 기준: Ghostty 7fa26e8b675f `Terminal.zig`·`page.zig`. 줄 번호는 각 시험 위에 적는다.

use alacritty_terminal::event::VoidListener;
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

struct T {
    term: Term<VoidListener>,
    parser: Processor,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        T {
            term: Term::new(Config::default(), &Dims(cols, rows), VoidListener),
            parser: Processor::new(),
        }
    }
    fn feed(&mut self, s: &str) -> &mut Self {
        self.parser.advance(&mut self.term, s.as_bytes());
        self
    }
    fn cell(&self, line: i32, col: usize) -> &alacritty_terminal::term::cell::Cell {
        &self.term.grid()[Line(line)][Column(col)]
    }
    fn cursor(&self) -> (i32, usize) {
        let p = self.term.grid().cursor.point;
        (p.line.0, p.column.0)
    }
}

// Terminal.zig 6216줄 `caps zero-width codepoints attached to one cell`, page.zig 92·1712줄
// 한 칸에 덧붙는 글자는 64개까지만 둔다. 묶음 모드가 꺼져 있을 때(그 시험과 같다)와 켜져 있을 때 모두.
#[test]
fn zero_width_cap_without_grapheme_mode() {
    let mut t = T::new(5, 2);
    t.feed("\x1b[?2027l");
    t.feed("A");
    t.feed(&"\u{301}".repeat(64 * 4));
    assert_eq!(t.cell(0, 0).zerowidth().map(|z| z.len()), Some(64));
    // 그 뒤 일반 글자는 다음 칸에 정상으로 놓인다
    t.feed("B");
    assert_eq!(t.cell(0, 1).c, 'B');
    assert_eq!(t.cell(0, 1).zerowidth(), None);
}

#[test]
fn zero_width_cap_with_grapheme_mode() {
    let mut t = T::new(5, 2);
    t.feed("\x1b[?2027h");
    t.feed("A");
    t.feed(&"\u{301}".repeat(64 * 4));
    assert_eq!(t.cell(0, 0).zerowidth().map(|z| z.len()), Some(64));
    t.feed("B");
    assert_eq!(t.cell(0, 1).c, 'B');
    assert_eq!(t.cell(0, 0).zerowidth().map(|z| z.len()), Some(64));
}

// 큰 입력도 시험 시간 안에 끝나고 상한을 넘지 않는다(빌드가 debug여도 수 초 안)
#[test]
fn zero_width_cap_huge_input() {
    for mode in ["\x1b[?2027l", "\x1b[?2027h"] {
        let mut t = T::new(5, 2);
        t.feed(mode);
        t.feed("A");
        t.feed(&"\u{301}".repeat(100_000));
        assert_eq!(t.cell(0, 0).zerowidth().map(|z| z.len()), Some(64));
    }
}

// 위치가 어긋나는 길: 줄 끝 대기 상태에서 덧붙는 글자도 같은 상한
#[test]
fn zero_width_cap_at_pending_wrap() {
    let mut t = T::new(3, 2);
    t.feed("abc");
    t.feed(&"\u{301}".repeat(200));
    assert_eq!(t.cell(0, 2).zerowidth().map(|z| z.len()), Some(64));
    assert_eq!(t.cursor(), (0, 2));
}

// Terminal.zig 6434·6460줄 `print over wide char with bold`·`with bg color`, 2310~2349줄 `writeCell`
// 넓은 글자의 머리나 꼬리를 덮으면 짝이 되는 칸은 `clearCells`로 지금 펜(배경색 포함)의 빈 칸이 된다.
// 옛 속성(굵게·옛 배경)은 남지 않는다.
use alacritty_terminal::vte::ansi::{Color, NamedColor};

fn is_blank_with_bg(c: &alacritty_terminal::term::cell::Cell, bg: Color) -> bool {
    c.c == ' '
        && c.bg == bg
        && c.flags.is_empty()
        && c.zerowidth().is_none()
}

#[test]
fn overwrite_wide_head_clears_spacer_with_pen() {
    let mut t = T::new(10, 2);
    // 굵게·빨간 배경으로 넓은 글자를 쓰고, 기본 펜으로 머리를 덮는다
    t.feed("\x1b[1;41m😀\x1b[0m\x1b[1;1HA");
    assert_eq!(t.cell(0, 0).c, 'A');
    assert!(is_blank_with_bg(t.cell(0, 1), Color::Named(NamedColor::Background)), "{:?}", t.cell(0, 1));
}

#[test]
fn overwrite_wide_head_clears_spacer_with_pen_bg() {
    let mut t = T::new(10, 2);
    // 지금 펜이 파란 배경이면 짝 칸은 파란 배경의 빈 칸(Ghostty `blankCell`)
    t.feed("\x1b[1;41m😀\x1b[0;44m\x1b[1;1HA");
    assert_eq!(t.cell(0, 0).c, 'A');
    assert!(is_blank_with_bg(t.cell(0, 1), Color::Named(NamedColor::Blue)), "{:?}", t.cell(0, 1));
}

#[test]
fn overwrite_wide_tail_clears_head_with_pen() {
    let mut t = T::new(10, 2);
    t.feed("\x1b[1;41m😀\x1b[0m\x1b[1;2HB");
    assert_eq!(t.cell(0, 1).c, 'B');
    assert!(is_blank_with_bg(t.cell(0, 0), Color::Named(NamedColor::Background)), "{:?}", t.cell(0, 0));
    t.feed("\x1b[1;1H\x1b[1;41m😀\x1b[0;44m\x1b[1;2HC");
    assert!(is_blank_with_bg(t.cell(0, 0), Color::Named(NamedColor::Blue)), "{:?}", t.cell(0, 0));
}

// 덮은 뒤 넓은 글자 표시가 남지 않는다
#[test]
fn overwrite_wide_leaves_no_wide_flags() {
    let mut t = T::new(10, 2);
    t.feed("😀\x1b[1;1HA");
    assert!(!t.cell(0, 0).flags.contains(Flags::WIDE_CHAR));
    assert!(!t.cell(0, 1).flags.contains(Flags::WIDE_CHAR_SPACER));
}
