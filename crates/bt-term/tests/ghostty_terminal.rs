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

// ---- #524: 지우기·삭제가 넓은 글자를 반쪽만 지움 ----

impl T {
    /// Ghostty `plainStringUnwrapped`: 줄 끝 칸에 이어진 줄 표시(WRAPLINE)가 있으면 다음 줄에 이어 붙인다
    fn unwrapped(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        let cols = self.term.columns();
        for r in 0..self.term.screen_lines() {
            let mut prev_wide = false;
            let mut wrapped = false;
            for c in 0..cols {
                let cell = &self.term.grid()[Line(r as i32)][Column(c)];
                let was_wide = prev_wide;
                prev_wide = cell.flags.contains(Flags::WIDE_CHAR);
                if c + 1 == cols && cell.flags.contains(Flags::WRAPLINE) {
                    wrapped = true;
                }
                if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    if !was_wide {
                        cur.push(' ');
                    }
                    continue;
                }
                cur.push(if cell.c == '\t' { ' ' } else { cell.c });
            }
            if !wrapped {
                lines.push(cur.trim_end().to_string());
                cur = String::new();
            }
        }
        if !cur.is_empty() {
            lines.push(cur.trim_end().to_string());
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
    /// 짝 잃은 넓은 글자 반쪽(뒤 칸 없는 앞 칸, 앞 칸 없는 뒤 칸)이 없는지 격자에서 직접 본다.
    /// `plain`은 짝 잃은 뒤 칸을 빈칸으로 그려 주므로 이 검사가 따로 필요하다
    fn assert_no_orphan_wide_cells(&self) {
        let cols = self.term.columns();
        for r in 0..self.term.screen_lines() {
            let row = &self.term.grid()[Line(r as i32)];
            for c in 0..cols {
                let flags = row[Column(c)].flags;
                if flags.contains(Flags::WIDE_CHAR) {
                    assert!(
                        c + 1 < cols && row[Column(c + 1)].flags.contains(Flags::WIDE_CHAR_SPACER),
                        "줄 {r} 칸 {c}: 뒤 칸 없는 넓은 글자"
                    );
                }
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    assert!(
                        c > 0 && row[Column(c - 1)].flags.contains(Flags::WIDE_CHAR),
                        "줄 {r} 칸 {c}: 앞 칸 없는 뒤 칸"
                    );
                }
            }
        }
    }
}

/// Ghostty 시험의 `printString("ABC"); carriageReturn; linefeed; …` 꼴
fn abc_def_ghi(t: &mut T) {
    t.feed("ABC\r\nDEF\r\nGHI");
}

// Ghostty "Terminal: eraseChars wide char boundary conditions" (Terminal.zig 11219줄)
#[test]
fn erase_chars_wide_char_boundary_conditions() {
    let mut t = T::new(8, 1);
    t.feed("😀a😀b😀");
    assert_eq!(t.plain(), "😀a😀b😀");
    t.feed("\x1b[1;2H\x1b[3X");
    assert_eq!(t.plain(), "     b😀");
    t.assert_no_orphan_wide_cells();
}

// Ghostty "Terminal: eraseChars wide char wrap boundary conditions" (11274줄)
#[test]
fn erase_chars_wide_char_wrap_boundary_conditions() {
    let mut t = T::new(8, 3);
    t.feed(".......😀abcde😀......");
    assert_eq!(t.plain(), ".......\n😀abcde\n😀......");
    assert_eq!(t.unwrapped(), ".......😀abcde😀......");
    t.feed("\x1b[2;2H\x1b[3X");
    assert_eq!(t.plain(), ".......\n    cde\n😀......");
    assert_eq!(t.unwrapped(), ".......     cde\n😀......");
    t.assert_no_orphan_wide_cells();
}

// Ghostty "Terminal: deleteChars wide char boundary conditions" (14750줄)
#[test]
fn delete_chars_wide_char_boundary_conditions() {
    let mut t = T::new(8, 1);
    t.feed("😀a😀b😀");
    assert_eq!(t.plain(), "😀a😀b😀");
    t.feed("\x1b[1;2H\x1b[3P");
    assert_eq!(t.plain(), "  b😀");
    t.assert_no_orphan_wide_cells();
}

// Ghostty "Terminal: deleteChars wide char wrap boundary conditions" (14815줄)
#[test]
fn delete_chars_wide_char_wrap_boundary_conditions() {
    let mut t = T::new(8, 3);
    t.feed(".......😀abcde😀......");
    assert_eq!(t.plain(), ".......\n😀abcde\n😀......");
    assert_eq!(t.unwrapped(), ".......😀abcde😀......");
    t.feed("\x1b[2;2H\x1b[3P");
    assert_eq!(t.plain(), ".......\n cde\n😀......");
    assert_eq!(t.unwrapped(), ".......  cde\n😀......");
    t.assert_no_orphan_wide_cells();
}

// Ghostty "Terminal: eraseLine right wide character" (15434줄)
#[test]
fn erase_line_right_wide_character() {
    let mut t = T::new(10, 5);
    t.feed("AB橋DE\x1b[1;4H\x1b[0K");
    assert_eq!(t.plain(), "AB");
    t.assert_no_orphan_wide_cells();
}

// Ghostty "Terminal: eraseDisplay below split multi-cell" (17310줄)
#[test]
fn erase_display_below_split_multi_cell() {
    let mut t = T::new(5, 5);
    t.feed("AB橋C\r\nDE橋F\r\nGH橋I");
    t.feed("\x1b[2;4H\x1b[0J");
    assert_eq!(t.plain(), "AB橋C\nDE");
    t.assert_no_orphan_wide_cells();
}

// 이웃: Ghostty "Terminal: eraseChars simple operation" (10972줄)
#[test]
fn neighbor_erase_chars_simple_operation() {
    let mut t = T::new(5, 5);
    t.feed("ABC\x1b[1;1H\x1b[2XX");
    assert_eq!(t.plain(), "X C");
}

// 이웃: Ghostty "Terminal: eraseChars wide character" (11031줄)
#[test]
fn neighbor_erase_chars_wide_character() {
    let mut t = T::new(5, 5);
    t.feed("橋BC\x1b[1;1H\x1b[1XX");
    assert_eq!(t.plain(), "X BC");
    t.assert_no_orphan_wide_cells();
}

// 이웃: Ghostty "Terminal: eraseChars wide char splits proper cell boundaries" (11243줄)
#[test]
fn neighbor_erase_chars_wide_char_splits_proper_cell_boundaries() {
    let mut t = T::new(30, 1);
    t.feed("x食べて下さい");
    assert_eq!(t.plain(), "x食べて下さい");
    t.feed("\x1b[1;6H\x1b[4X");
    assert_eq!(t.plain(), "x食べ    さい");
    t.assert_no_orphan_wide_cells();
}

// 이웃: Ghostty "Terminal: deleteChars" (14393줄)
#[test]
fn neighbor_delete_chars() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE\x1b[1;2H\x1b[2P");
    assert_eq!(t.plain(), "ADE");
}

// 이웃: Ghostty "Terminal: deleteChars split wide character from spacer tail" (14640줄)
#[test]
fn neighbor_delete_chars_split_wide_character_from_spacer_tail() {
    let mut t = T::new(6, 10);
    t.feed("A橋123\x1b[1;3H\x1b[1P");
    assert_eq!(t.plain(), "A 123");
    t.assert_no_orphan_wide_cells();
}

// 이웃: Ghostty "Terminal: deleteChars split wide character tail" (14731줄)
#[test]
fn neighbor_delete_chars_split_wide_character_tail() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[1;4H橋\r\x1b[4P0");
    assert_eq!(t.plain(), "0");
}

// 이웃: Ghostty "Terminal: eraseLine simple erase right" (15334줄)
#[test]
fn neighbor_erase_line_simple_erase_right() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE\x1b[1;3H\x1b[0K");
    assert_eq!(t.plain(), "AB");
}

// 이웃: Ghostty "Terminal: eraseLine simple erase left" (15539줄)
#[test]
fn neighbor_erase_line_simple_erase_left() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE\x1b[1;3H\x1b[1K");
    assert_eq!(t.plain(), "   DE");
}

// 이웃: Ghostty "Terminal: eraseLine left wide character" (15613줄)
#[test]
fn neighbor_erase_line_left_wide_character() {
    let mut t = T::new(10, 5);
    t.feed("AB橋DE\x1b[1;3H\x1b[1K");
    assert_eq!(t.plain(), "    DE");
    t.assert_no_orphan_wide_cells();
}

// 이웃: Ghostty "Terminal: eraseDisplay simple erase below" (17240줄)
#[test]
fn neighbor_erase_display_simple_erase_below() {
    let mut t = T::new(5, 5);
    abc_def_ghi(&mut t);
    t.feed("\x1b[2;2H\x1b[0J");
    assert_eq!(t.plain(), "ABC\nD");
}

// ---- #525: LF·ECH·DCH·EL 뒤의 줄바꿈 대기 ----

// Ghostty "Terminal: linefeed unsets pending wrap" (Terminal.zig 8928줄)
#[test]
fn linefeed_unsets_pending_wrap() {
    let mut t = T::new(5, 80);
    t.feed("hello");
    assert!(t.wrap_pending());
    t.feed("\n");
    assert!(!t.wrap_pending());
}

// Ghostty "Terminal: eraseChars resets pending wrap" (11050줄)
#[test]
fn erase_chars_resets_pending_wrap() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE");
    assert!(t.wrap_pending());
    t.feed("\x1b[1X");
    assert!(!t.wrap_pending());
    t.feed("X");
    assert_eq!(t.plain(), "ABCDX");
}

// Ghostty "Terminal: deleteChars resets pending wrap" (14493줄)
#[test]
fn delete_chars_resets_pending_wrap() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE");
    assert!(t.wrap_pending());
    t.feed("\x1b[1P");
    assert!(!t.wrap_pending());
    t.feed("X");
    assert_eq!(t.plain(), "ABCDX");
}

// Ghostty "Terminal: eraseLine resets pending wrap" (15353줄)
#[test]
fn erase_line_resets_pending_wrap() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE");
    assert!(t.wrap_pending());
    t.feed("\x1b[0K");
    assert!(!t.wrap_pending());
    t.feed("B");
    assert_eq!(t.plain(), "ABCDB");
}

// Ghostty "Terminal: eraseLine left resets wrap" (15558줄)
#[test]
fn erase_line_left_resets_wrap() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE");
    assert!(t.wrap_pending());
    t.feed("\x1b[1K");
    assert!(!t.wrap_pending());
    t.feed("B");
    assert_eq!(t.plain(), "    B");
}

// Ghostty `eraseLine`(4327줄)은 줄바꿈 대기 중에도 커서 칸(마지막 칸)부터 지운다. 대기가 있으면 EL 0을 건너뛰던 것을 고쳤다
#[test]
fn erase_line_right_while_pending_wrap_clears_the_last_cell() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE");
    assert!(t.wrap_pending());
    t.feed("\x1b[K");
    assert_eq!(t.plain(), "ABCD");
    assert_eq!(t.cursor(), (4, 0));
    assert!(!t.wrap_pending());
}

// Ghostty `index`(3017줄)가 줄바꿈 대기를 푼다. IND(`ESC D`)도 같다. 아래 여백 줄에서 스크롤하며 멈추는 경우도 푼다
#[test]
fn index_unsets_pending_wrap() {
    let mut t = T::new(5, 2);
    t.feed("hello\x1bD");
    assert!(!t.wrap_pending());
    assert_eq!(t.cursor(), (4, 1));
    t.feed("x");
    assert!(t.wrap_pending());
    t.feed("\n");
    assert!(!t.wrap_pending());
    assert_eq!(t.plain(), "    x");
}

// Ghostty `eraseDisplay`(4441줄)는 below·above·complete에서 줄바꿈 대기를 푼다(같은 이름의 Ghostty 시험은 없다)
#[test]
fn erase_display_unsets_pending_wrap() {
    for mode in ["0", "1", "2"] {
        let mut t = T::new(5, 5);
        t.feed("ABCDE");
        assert!(t.wrap_pending());
        t.feed(&format!("\x1b[{mode}J"));
        assert!(!t.wrap_pending(), "ED {mode}");
    }
}

// 이웃: Ghostty "Terminal: carriage return unsets pending wrap" (8958줄)
#[test]
fn neighbor_carriage_return_unsets_pending_wrap() {
    let mut t = T::new(5, 80);
    t.feed("hello");
    assert!(t.wrap_pending());
    t.feed("\r");
    assert!(!t.wrap_pending());
}

// 이웃: Ghostty "Terminal: eraseChars resets wrap" (11069줄). 줄바꿈 대기가 아니라 이어진 줄 표시를 푸는 시험
#[test]
fn neighbor_erase_chars_resets_wrap() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE123\x1b[1;1H\x1b[1XX");
    assert_eq!(t.plain(), "XBCDE\n123");
}

// 이웃: Ghostty "Terminal: eraseLine resets wrap" (15372줄)
#[test]
fn neighbor_erase_line_resets_wrap() {
    let mut t = T::new(5, 5);
    t.feed("ABCDE123\x1b[1;1H\x1b[0KX");
    assert_eq!(t.plain(), "X\n123");
}

// ---- #526: ED 1, CUU·CUD 여백, DECALN 여백, LNM ----

// Ghostty "Terminal: eraseDisplay simple erase above" (Terminal.zig 17431줄)
#[test]
fn erase_display_simple_erase_above() {
    let mut t = T::new(5, 5);
    abc_def_ghi(&mut t);
    t.feed("\x1b[2;2H\x1b[1J");
    assert_eq!(t.plain(), "\n  F\nGHI");
}

// Ghostty "Terminal: eraseDisplay above split multi-cell" (17500줄)
#[test]
fn erase_display_above_split_multi_cell() {
    let mut t = T::new(5, 5);
    t.feed("AB橋C\r\nDE橋F\r\nGH橋I");
    t.feed("\x1b[2;3H\x1b[1J");
    assert_eq!(t.plain(), "\n    F\nGH橋I");
    t.assert_no_orphan_wide_cells();
}

// Ghostty "Terminal: cursorUp below top scroll margin" (12239줄)
#[test]
fn cursor_up_below_top_scroll_margin() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[2;4r\x1b[3;1HA\x1b[5AX");
    assert_eq!(t.plain(), "\n X\nA");
}

// Ghostty "Terminal: cursorDown above bottom scroll margin" (12668줄)
#[test]
fn cursor_down_above_bottom_scroll_margin() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[1;3rA\x1b[10BX");
    assert_eq!(t.plain(), "A\n\n X");
}

// Ghostty "Terminal: decaln reset margins" (13701줄)
#[test]
fn decaln_reset_margins() {
    let mut t = T::new(3, 3);
    t.feed("\x1b[?6h\x1b[2;3r\x1b#8\x1b[1T");
    assert_eq!(t.plain(), "\nEEE\nEEE");
}

// 이웃: DECALN은 좌우 여백과 커서도 처음으로 돌린다(Terminal.zig `decaln` 4586줄: 여백 넷을 모두 초기화하고 setCursorPos(1, 1))
#[test]
fn decaln_resets_left_right_margins_and_homes_cursor() {
    let mut t = T::new(4, 3);
    t.feed("\x1b[?69h\x1b[2;3s\x1b[2;2H\x1b#8");
    assert_eq!((t.term.left_margin(), t.term.right_margin()), (0, 3));
    assert_eq!(t.cursor(), (0, 0));
    assert!(!t.wrap_pending());
    assert_eq!(t.plain(), "EEEE\nEEEE\nEEEE");
}

// Ghostty "Terminal: linefeed mode automatic carriage return" (8942줄)
#[test]
fn linefeed_mode_automatic_carriage_return() {
    let mut t = T::new(10, 10);
    t.feed("\x1b[20h123456\nX");
    assert_eq!(t.plain(), "123456\nX");
}

// Ghostty `linefeed`(2530줄)는 LF·VT·FF가 부른다. LNM이 켜졌을 때 셋 모두 CR을 한다
#[test]
fn linefeed_mode_applies_to_lf_vt_and_ff_but_not_ind() {
    for byte in ["\n", "\x0b", "\x0c"] {
        let mut t = T::new(10, 10);
        t.feed("\x1b[20hab");
        t.feed(byte);
        assert_eq!(t.cursor(), (0, 1), "{byte:?}");
    }
    // IND(ESC D)는 Ghostty `index`라 LNM과 상관없이 CR을 하지 않는다
    let mut t = T::new(10, 10);
    t.feed("\x1b[20hab\x1bD");
    assert_eq!(t.cursor(), (2, 1));
    // LNM이 꺼져 있으면 LF는 CR을 하지 않는다
    let mut t = T::new(10, 10);
    t.feed("ab\n");
    assert_eq!(t.cursor(), (2, 1));
}

// 이웃: Ghostty "Terminal: cursorUp basic" (12221줄)
#[test]
fn neighbor_cursor_up_basic() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[3;1HA\x1b[10AX");
    assert_eq!(t.plain(), " X\n\nA");
}

// 이웃: Ghostty "Terminal: cursorUp above top scroll margin" (12258줄)
#[test]
fn neighbor_cursor_up_above_top_scroll_margin() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[3;5r\x1b[3;1HA\x1b[2;1H\x1b[10AX");
    assert_eq!(t.plain(), "X\n\nA");
}

// 이웃: Ghostty "Terminal: cursorDown below bottom scroll margin" (12686줄)
#[test]
fn neighbor_cursor_down_below_bottom_scroll_margin() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[1;3rA\x1b[4;1H\x1b[10BX");
    assert_eq!(t.plain(), "A\n\n\n\nX");
}
