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
    c.c == ' ' && c.bg == bg && c.flags.is_empty() && c.zerowidth().is_none()
}

#[test]
fn overwrite_wide_head_clears_spacer_with_pen() {
    let mut t = T::new(10, 2);
    // 굵게·빨간 배경으로 넓은 글자를 쓰고, 기본 펜으로 머리를 덮는다
    t.feed("\x1b[1;41m😀\x1b[0m\x1b[1;1HA");
    assert_eq!(t.cell(0, 0).c, 'A');
    assert!(
        is_blank_with_bg(t.cell(0, 1), Color::Named(NamedColor::Background)),
        "{:?}",
        t.cell(0, 1)
    );
}

#[test]
fn overwrite_wide_head_clears_spacer_with_pen_bg() {
    let mut t = T::new(10, 2);
    // 지금 펜이 파란 배경이면 짝 칸은 파란 배경의 빈 칸(Ghostty `blankCell`)
    t.feed("\x1b[1;41m😀\x1b[0;44m\x1b[1;1HA");
    assert_eq!(t.cell(0, 0).c, 'A');
    assert!(
        is_blank_with_bg(t.cell(0, 1), Color::Named(NamedColor::Blue)),
        "{:?}",
        t.cell(0, 1)
    );
}

#[test]
fn overwrite_wide_tail_clears_head_with_pen() {
    let mut t = T::new(10, 2);
    t.feed("\x1b[1;41m😀\x1b[0m\x1b[1;2HB");
    assert_eq!(t.cell(0, 1).c, 'B');
    assert!(
        is_blank_with_bg(t.cell(0, 0), Color::Named(NamedColor::Background)),
        "{:?}",
        t.cell(0, 0)
    );
    t.feed("\x1b[1;1H\x1b[1;41m😀\x1b[0;44m\x1b[1;2HC");
    assert!(
        is_blank_with_bg(t.cell(0, 0), Color::Named(NamedColor::Blue)),
        "{:?}",
        t.cell(0, 0)
    );
}

// 덮은 뒤 넓은 글자 표시가 남지 않는다
#[test]
fn overwrite_wide_leaves_no_wide_flags() {
    let mut t = T::new(10, 2);
    t.feed("😀\x1b[1;1HA");
    assert!(!t.cell(0, 0).flags.contains(Flags::WIDE_CHAR));
    assert!(!t.cell(0, 1).flags.contains(Flags::WIDE_CHAR_SPACER));
}

// Terminal.zig 7384·7404·7335줄 `grapheme transfer when widening ...`, grapheme.zig 66줄 `graphemeWidthEffect`
// 묶음에 덧붙는 글자가 VS가 아니고 "묶음 안에서 폭 0"이 아니면(폭이 있으면) 묶음은 폭 2로 넓어진다.
// 좁은 바탕 + ZWJ + 이미 넓은 이모지(❤ + ZWJ + 👍)도 넓어져 다음 줄로 넘어가고, 묶음 글자가 함께 옮겨진다.
fn zw(t: &T, line: i32, col: usize) -> Vec<char> {
    t.cell(line, col).zerowidth().map(|z| z.to_vec()).unwrap_or_default()
}

#[test]
fn zwj_narrow_plus_wide_emoji_widens_and_wraps() {
    // 5열: abcd 다음 칸(마지막 칸)에 ❤, ZWJ, 👍
    let mut t = T::new(5, 3);
    t.feed("\x1b[?2027habcd\u{2764}\u{200d}\u{1F44D}");
    // 마지막 칸은 앞 칸 자리 표시가 되고 묶음은 다음 줄 첫 칸에서 넓은 글자가 된다
    assert!(
        t.cell(0, 4).flags.contains(Flags::LEADING_WIDE_CHAR_SPACER),
        "{:?}",
        t.cell(0, 4)
    );
    assert_eq!(t.cell(1, 0).c, '\u{2764}');
    assert!(t.cell(1, 0).flags.contains(Flags::WIDE_CHAR));
    assert!(t.cell(1, 1).flags.contains(Flags::WIDE_CHAR_SPACER));
    assert_eq!(zw(&t, 1, 0), vec!['\u{200d}', '\u{1F44D}']);
}

#[test]
fn zwj_narrow_plus_narrow_pictographic_widens_like_before() {
    // 7335줄: ☺ ZWJ ❤는 이미 넓어졌다. 같은 값이어야 한다
    let mut t = T::new(3, 5);
    t.feed("\x1b[?2027h\x1b[1;3H\u{263A}\u{200d}\u{2764}");
    assert!(t.cell(0, 2).flags.contains(Flags::LEADING_WIDE_CHAR_SPACER));
    assert_eq!(t.cell(1, 0).c, '\u{263A}');
    assert!(t.cell(1, 0).flags.contains(Flags::WIDE_CHAR));
    assert_eq!(zw(&t, 1, 0), vec!['\u{200d}', '\u{2764}']);
}

#[test]
fn zwj_widening_below_scroll_region() {
    // 7384줄: 3x3, 위아래 여백 1~2, 커서 (3,3)에서 ❤ ZWJ 👍. 여백 밖 맨 아래 줄이라 스크롤 없이 다음 줄(없음)로
    let mut t = T::new(3, 3);
    t.feed("\x1b[?2027h\x1b[1;2r\x1b[3;3H\u{2764}\u{200d}\u{1F44D}");
    assert_eq!(t.cell(2, 0).c, '\u{2764}');
    assert_eq!(zw(&t, 2, 0), vec!['\u{200d}', '\u{1F44D}']);
}

#[test]
fn zwj_widening_scrolls_single_row() {
    // 7404줄: 1행 5열. 위 줄(스크롤백)의 묶음은 그대로 둔다. 여백 유무와 대체 화면 유무 4가지
    for alternate in [false, true] {
        for margins in [false, true] {
            let mut t = T::new(5, 1);
            t.feed("\x1b[?2027h");
            if alternate {
                t.feed("\x1b[?1049h\x1b[?2027h");
            }
            t.feed("\x1b[1;5Ha\u{301}\n");
            if margins {
                t.feed("\x1b[?69h\x1b[3;s");
            }
            t.feed("\x1b[1;5H\u{2764}\u{200d}\u{1F44D}");
            let x = if margins { 2 } else { 0 };
            assert_eq!(t.cell(0, x).c, '\u{2764}', "alt={alternate} margins={margins}");
            assert_eq!(
                zw(&t, 0, x),
                vec!['\u{200d}', '\u{1F44D}'],
                "alt={alternate} margins={margins}"
            );
            assert!(t.cell(0, x).flags.contains(Flags::WIDE_CHAR));
            if !alternate {
                let g = t.term.grid();
                // 맨 위 스크롤백 줄(Ghostty `.screen` y=0)
                let top = -(t.term.history_size() as i32);
                let hist = &g[Line(top)][Column(4)];
                assert_eq!(hist.c, 'a');
                assert_eq!(hist.zerowidth(), Some(&['\u{301}'][..]));
            }
        }
    }
}

// 자리가 있으면 그 자리에서 넓어지고 커서는 두 칸 뒤
#[test]
fn zwj_narrow_plus_wide_emoji_widens_in_place() {
    let mut t = T::new(10, 2);
    t.feed("\x1b[?2027h\u{2764}\u{200d}\u{1F44D}");
    assert!(t.cell(0, 0).flags.contains(Flags::WIDE_CHAR));
    assert!(t.cell(0, 1).flags.contains(Flags::WIDE_CHAR_SPACER));
    assert_eq!(t.cursor(), (0, 2));
}

// 이미 넓은 바탕에 ZWJ와 넓은 이모지가 붙어도 폭은 그대로 2(0.64 #536의 값 유지)
#[test]
fn zwj_wide_base_stays_wide() {
    let mut t = T::new(10, 2);
    t.feed("\x1b[?2027h\u{1F468}\u{200d}\u{1F469}");
    assert!(t.cell(0, 0).flags.contains(Flags::WIDE_CHAR));
    assert_eq!(t.cursor(), (0, 2));
}

// Terminal.zig 13190줄 `deleteLines resets wrap`, 3942~3948줄, 17168줄 `eraseLine on a blank row`의 EL 1 항목
// 우리는 줄 이음을 줄의 마지막 칸 `WRAPLINE`으로 든다. 전체 폭 영역을 DL로 옮긴 줄은 이음 표지를 잃고,
// EL 1(왼쪽 지우기)은 마지막 칸을 지워도 줄 이음 표지를 유지한다(EL 0·EL 2는 지운다).
use alacritty_terminal::index::Point;

fn wrapped(t: &T, line: i32) -> bool {
    t.cell(line, t.term.columns() - 1).flags.contains(Flags::WRAPLINE)
}

fn copy_all(t: &T) -> String {
    let last = t.term.screen_lines() as i32 - 1;
    t.term.bounds_to_string(
        Point::new(Line(0), Column(0)),
        Point::new(Line(last), Column(t.term.columns() - 1)),
    )
}

#[test]
fn delete_lines_resets_wrap() {
    let mut t = T::new(3, 3);
    // "1" 다음 줄에 ABCDEF → ABC(이음) / DEF
    t.feed("1\r\nABCDEF");
    assert!(wrapped(&t, 1));
    t.feed("\x1b[1;2r\x1b[1;1H\x1b[M");
    t.feed("X");
    // 옮겨 온 줄과 지운 줄 모두 이음 표지가 없다
    for line in 0..3 {
        assert!(!wrapped(&t, line), "line {line}");
    }
    assert_eq!(t.cell(0, 0).c, 'X');
    assert_eq!(t.cell(0, 1).c, 'B');
    assert_eq!(t.cell(0, 2).c, 'C');
    assert_eq!(t.cell(2, 0).c, 'D');
    // 복사하면 줄이 이어지지 않는다
    assert_eq!(copy_all(&t), "XBC\n\nDEF");
}

#[test]
fn delete_lines_resets_wrap_on_all_shifted_rows() {
    // 4줄 중 위 두 줄이 이어진 줄이고, DL이 그 줄을 위로 올린다
    let mut t = T::new(3, 4);
    t.feed("ABCDEFGHI\r\nJ");
    assert!(wrapped(&t, 0) && wrapped(&t, 1));
    t.feed("\x1b[1;1H\x1b[M");
    for line in 0..4 {
        assert!(!wrapped(&t, line), "line {line}");
    }
    // DL 한 번에 영역을 다 지워도 마찬가지
    let mut t = T::new(3, 3);
    t.feed("ABCDEFGHI\x1b[1;1H\x1b[5M");
    for line in 0..3 {
        assert!(!wrapped(&t, line), "line {line}");
    }
}

#[test]
fn delete_lines_outside_region_leaves_wrap_alone() {
    // 커서가 영역 밖이면 DL은 아무것도 하지 않으므로 이음 표지도 그대로
    let mut t = T::new(3, 4);
    t.feed("ABCDEF\x1b[3;4r\x1b[1;1H\x1b[M");
    assert!(wrapped(&t, 0));
}

#[test]
fn delete_lines_keeps_rows_above_cursor() {
    // 커서 줄 위의 줄(영역 안이어도 옮기지 않는 줄)은 표지가 그대로
    let mut t = T::new(3, 4);
    t.feed("ABCDEFGHI\x1b[2;1H\x1b[M");
    assert!(wrapped(&t, 0));
    assert!(!wrapped(&t, 1));
    assert!(!wrapped(&t, 2));
}

#[test]
fn delete_lines_with_left_right_margins_keeps_wrap() {
    // 좌우 여백이 있으면 줄 이음 표지는 줄에 남는다(Ghostty 3942줄은 전체 폭일 때만 지움)
    let mut t = T::new(10, 3);
    t.feed("0123456789abc");
    assert!(wrapped(&t, 0));
    t.feed("\x1b[?69h\x1b[2;9s\x1b[1;2H\x1b[M");
    assert!(wrapped(&t, 0));
    assert!(!wrapped(&t, 1));
}

#[test]
fn erase_line_left_keeps_wrap() {
    // 17168줄: EL 1은 줄 표지를 유지, EL 0·EL 2는 지운다. 마지막 칸을 지워도 유지한다
    for (cursor, seq, keeps) in [
        ("\x1b[1;5H", "\x1b[1K", true), // 커서가 마지막 칸: 마지막 칸까지 지운다
        ("\x1b[1;3H", "\x1b[1K", true),
        ("\x1b[1;5H", "\x1b[1;1H\x1b[0K", false),
        ("\x1b[1;3H", "\x1b[0K", false),
        ("\x1b[1;3H", "\x1b[2K", false),
    ] {
        let mut t = T::new(5, 3);
        t.feed("ABCDEF");
        assert!(wrapped(&t, 0));
        t.feed(cursor);
        t.feed(seq);
        assert_eq!(wrapped(&t, 0), keeps, "{cursor:?} {seq:?}");
    }
}

#[test]
fn erase_line_left_wrap_with_protected_last_cell() {
    // 보호 칸 건너뛰기(DECSEL)에서도 마지막 칸을 지울 때 표지를 유지한다
    let mut t = T::new(5, 3);
    t.feed("ABCDEF\x1b[1;5H\x1b[?1K");
    assert!(wrapped(&t, 0));
    assert_eq!(t.cell(0, 4).c, ' ');
}

// Terminal.zig 17168줄: 이어진 줄의 마지막 칸을 새로 써도 줄 이음 표지는 남는다(`print`는 `row.wrap`을 건드리지 않는다).
// EL 0·2와 ECH는 지운다(`cursorResetWrap`, 4298줄).
#[test]
fn overwriting_last_cell_keeps_row_wrap() {
    for (seq, keeps) in [
        ("\x1b[1K", true),
        ("\x1b[0K", false),
        ("\x1b[2K", false),
        ("\x1b[X", false),
    ] {
        let mut t = T::new(5, 3);
        t.feed("abcdefghijkl\x1b[2;5HX");
        assert!(wrapped(&t, 1), "X를 쓴 뒤");
        t.feed(seq);
        assert_eq!(wrapped(&t, 1), keeps, "{seq:?}");
    }
}

#[test]
fn autowrap_marks_row_wrap() {
    let mut t = T::new(5, 3);
    t.feed("abcdef");
    assert!(wrapped(&t, 0));
    assert!(!wrapped(&t, 1));
}
