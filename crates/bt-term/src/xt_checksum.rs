//! DECRQCRA(화면 사각 영역의 체크섬 묻기, `CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y`)와
//! XTCHECKSUM(`CSI Ps # y`, 체크섬 계산법 고르기)(#259).
//!
//! 계산은 Ghostty `src/terminal/xt_checksum.zig`(그리고 그 기준인 xterm `xtermCheckRect`)를 그대로 옮겼다.
//! alacritty 격자와 다른 점은 아래 주석과 ADR-021에 적었다.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags as CellFlags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use bt_core::config::XtChecksumExtension;

/// XTCHECKSUM의 비트(xterm의 `CSBITS`). 0은 DEC 체크섬이고, 초기화(RIS·DECSTR)가 설정의 기본으로 되돌리는 값이다
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// 비트 0: 합을 뒤집지 않는다
    pub positive: bool,
    /// 비트 1: 칸마다 VT100 표시(굵게·밑줄·반전·숨김)를 더하지 않는다
    pub no_attributes: bool,
    /// 비트 2: 빈칸을 빼지 않는다. DEC 체크섬은 사각형 첫 칸이 아닌 일반 빈칸을 더하지 않는다
    pub no_trim: bool,
    /// 비트 3: 한 번도 쓰지 않은 칸을 건너뛰지 않고 빈칸으로 센다
    pub undrawn: bool,
    /// 비트 4: DEC 8비트 값 대신 코드포인트 전체를 쓴다
    pub full: bool,
}

impl Flags {
    /// `CSI Ps # y`의 Ps. 알지 못하는 윗 비트는 xterm처럼 버린다
    pub fn from_bits(bits: u16) -> Self {
        Self {
            positive: bits & 1 != 0,
            no_attributes: bits & 2 != 0,
            no_trim: bits & 4 != 0,
            undrawn: bits & 8 != 0,
            full: bits & 16 != 0,
        }
    }

    /// 설정 `vt-xt-checksum-extension`(켜짐이 DEC 쪽: negate·attributes·trim)을 비트로 바꾼다
    pub fn from_config(c: &XtChecksumExtension) -> Self {
        Self {
            positive: !c.negate,
            no_attributes: !c.attributes,
            no_trim: !c.trim,
            undrawn: c.undrawn,
            full: c.full,
        }
    }
}

/// DECRQCRA 요청. 좌표는 1부터 세고 0은 생략이다. 쪽 번호(Pg)는 한 쪽뿐이라 무시한다
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Request {
    pub id: u16,
    pub top: u16,
    pub left: u16,
    pub bottom: u16,
    pub right: u16,
}

/// 사각형(0부터, 양 끝 포함): (왼쪽 열, 위 줄, 오른쪽 열, 아래 줄)
type Rect = (usize, usize, usize, usize);

impl Request {
    /// 요청을 사각형으로 푼다. 원점 모드(DECOM)면 좌표가 (위 여백, 왼쪽 여백)부터 시작한다. 화면 밖은 화면 안으로 자른다.
    /// 사각형이 비면 `None`
    fn resolve(self, rows: usize, cols: usize, margin: (usize, usize)) -> Option<Rect> {
        if rows == 0 || cols == 0 {
            return None;
        }
        let limit = |v: u16, default: usize, margin: usize, max: usize| {
            let n = if v == 0 { default } else { v as usize };
            n.saturating_add(margin).clamp(1, max)
        };
        let top = limit(self.top, 1, margin.0, rows);
        let left = limit(self.left, 1, margin.1, cols);
        let bottom = limit(self.bottom, rows, margin.0, rows);
        let right = limit(self.right, cols, margin.1, cols);
        if top > bottom || left > right {
            return None;
        }
        Some((left - 1, top - 1, right - 1, bottom - 1))
    }
}

/// 한 칸의 값
enum Value {
    /// 세지 않는다
    Skip,
    /// 한 번도 쓰지 않은 칸
    Undrawn,
    /// 속성을 더하기 전 글자 값
    Glyph(u16),
}

/// 쓴 적 없는 칸으로 본다. alacritty 격자에는 "쓴 적 없음" 표시가 없어서, 빈칸에 색도 속성도 없는 칸을 그렇게 본다.
/// 그래서 `printf ' '`로 찍은 평범한 빈칸도 여기에 든다(Ghostty는 찍은 빈칸을 쓴 칸으로 센다)
fn is_undrawn(cell: &Cell) -> bool {
    cell.flags.contains(CellFlags::LEADING_WIDE_CHAR_SPACER)
        || (cell.c == ' '
            && cell.flags.difference(CellFlags::WRAPLINE).is_empty()
            && cell.fg == Color::Named(NamedColor::Foreground)
            && cell.bg == Color::Named(NamedColor::Background)
            && cell.zerowidth().is_none_or(<[char]>::is_empty))
}

fn value(cell: &Cell, flags: Flags) -> Value {
    // 넓은 글자의 오른쪽 반. xterm은 여기에 8비트 밖의 자리표시를 두므로 DEC 체크섬은 ESC로 센다
    if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
        return if flags.full { Value::Skip } else { Value::Glyph(0x1B) };
    }
    if is_undrawn(cell) {
        return Value::Undrawn;
    }
    let cp = cell.c as u32;
    if flags.full {
        return Value::Glyph(cp as u16);
    }
    // xterm `xtermCharSetDec`의 DEC 8비트 값(ASCII 문자 집합). 글자 집합 변환 뒤의 코드포인트만 있으므로
    // DEC 선 그리기는 보낸 글자가 아니라 8비트 밖의 글자로 센다
    Value::Glyph(match cp {
        0x7F | 0xFF => 0,
        0x20..=0x7E | 0x80..=0x9F | 0xA1..=0xFE => (cp & 0x7F) as u16,
        _ => 0x1B,
    })
}

/// xterm이 칸마다 더하는 VT100 표시. 깜박임·DECSCA 보호 표시는 격자에 있지만(#287·#294) 원본을 보고 맞추지 않아 0x40·0x04는 더하지 않는다
fn attributes(cell: &Cell) -> u16 {
    let f = cell.flags;
    let mut n = 0;
    if f.contains(CellFlags::HIDDEN) {
        n += 0x08;
    }
    if f.intersects(CellFlags::ALL_UNDERLINES) {
        n += 0x10;
    }
    if f.contains(CellFlags::INVERSE) {
        n += 0x20;
    }
    if f.contains(CellFlags::BOLD) {
        n += 0x80;
    }
    n
}

/// 합에는 더하지 않지만 빈칸을 빼지 않게 하는 속성
fn extended(cell: &Cell) -> bool {
    cell.flags
        .intersects(CellFlags::DIM | CellFlags::ITALIC | CellFlags::STRIKEOUT | CellFlags::DOUBLE_UNDERLINE)
}

fn sum_rect<L: EventListener>(term: &Term<L>, rect: Option<Rect>, flags: Flags) -> u16 {
    // 빈 사각형에도 답은 가고, 합은 0이다
    let Some((x0, y0, x1, y1)) = rect else { return 0 };
    let grid = term.grid();
    let mut sum: u16 = 0;
    // DEC 체크섬은 센 첫 칸이 아닌 일반 빈칸을 더하지 않는다
    let mut first = true;
    for y in y0..=y1 {
        let row = &grid[Line(y as i32)];
        for x in x0..=x1 {
            let cell = &row[Column(x)];
            let mut ch: u16 = match value(cell, flags) {
                Value::Skip => continue,
                Value::Undrawn if flags.no_trim || flags.undrawn => b' ' as u16,
                Value::Undrawn => continue,
                Value::Glyph(v) => v,
            };
            if !flags.no_attributes {
                ch = ch.wrapping_add(attributes(cell));
            }
            if flags.no_trim {
                sum = sum.wrapping_add(ch);
                // xterm은 결합 문자를 DEC 모드의 빈칸을 빼지 않는 합에만 더한다
                if !flags.full {
                    for &c in cell.zerowidth().unwrap_or(&[]) {
                        sum = sum.wrapping_add(c as u32 as u16);
                    }
                }
            } else if first || ch != b' ' as u16 || extended(cell) {
                sum = sum.wrapping_add(ch);
            }
            first = flags.no_trim;
        }
        if !flags.no_trim {
            first = false;
        }
    }
    if flags.positive { sum } else { 0u16.wrapping_sub(sum) }
}

/// 지금 격자(보이는 화면, 스크롤백이 아님)에서 요청한 사각형의 체크섬을 구한다
pub fn compute<L: EventListener>(term: &Term<L>, req: Request, flags: Flags) -> u16 {
    let margin = if term.mode().contains(TermMode::ORIGIN) {
        // 왼쪽 여백(DECSLRM)은 alacritty에 없어 0이다
        (term.scroll_region().start.0.max(0) as usize, 0)
    } else {
        (0, 0)
    };
    let rect = req.resolve(term.screen_lines(), term.columns(), margin);
    sum_rect(term, rect, flags)
}

/// DECRQCRA 답: `DCS Pi ! ~ XXXX ST`. 대문자 16진수 네 자리
pub fn encode(id: u16, sum: u16) -> Vec<u8> {
    format!("\x1bP{id}!~{sum:04X}\x1b\\").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    #[derive(Clone, Copy)]
    struct Null;
    impl EventListener for Null {}

    struct Size(usize, usize);
    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.0
        }
        fn screen_lines(&self) -> usize {
            self.0
        }
        fn columns(&self) -> usize {
            self.1
        }
    }

    /// 10열 5행 터미널에 바이트를 먹인다(Ghostty 테스트와 같은 크기)
    fn term(bytes: &str) -> Term<Null> {
        let mut t = Term::new(Config::default(), &Size(5, 10), Null);
        let mut p: Processor = Processor::new();
        p.advance(&mut t, bytes.as_bytes());
        t
    }

    const POS: Flags = Flags {
        positive: true,
        no_attributes: false,
        no_trim: false,
        undrawn: false,
        full: false,
    };

    fn row1() -> Request {
        Request {
            top: 1,
            bottom: 1,
            ..Request::default()
        }
    }

    // Ghostty "encode"
    #[test]
    fn encode_reply_shape() {
        assert_eq!(encode(65535, 0xFFFF), b"\x1bP65535!~FFFF\x1b\\");
        assert_eq!(encode(1, 0x1A), b"\x1bP1!~001A\x1b\\");
    }

    // Ghostty "selection defaults and clamping"
    #[test]
    fn selection_defaults_and_clamping() {
        assert_eq!(Request::default().resolve(5, 10, (0, 0)), Some((0, 0, 9, 4)));
        let req = Request {
            top: 2,
            left: 3,
            bottom: 99,
            right: 99,
            ..Request::default()
        };
        assert_eq!(req.resolve(5, 10, (0, 0)), Some((2, 1, 9, 4)));
        let inverted_rows = Request {
            top: 3,
            bottom: 2,
            ..Request::default()
        };
        assert_eq!(inverted_rows.resolve(5, 10, (0, 0)), None);
        let inverted_cols = Request {
            left: 3,
            right: 2,
            ..Request::default()
        };
        assert_eq!(inverted_cols.resolve(5, 10, (0, 0)), None);
    }

    // Ghostty "selection origin mode"(위 여백 1, 왼쪽 여백 2)
    #[test]
    fn selection_origin_mode() {
        assert_eq!(Request::default().resolve(5, 10, (1, 2)), Some((2, 1, 9, 4)));
        let one = Request {
            top: 1,
            left: 1,
            bottom: 1,
            right: 1,
            ..Request::default()
        };
        assert_eq!(one.resolve(5, 10, (1, 2)), Some((2, 1, 2, 1)));
    }

    #[test]
    fn origin_mode_uses_the_scroll_region_top_of_the_real_grid() {
        // DECSTBM 2~4행, DECOM 켬: 1행은 화면의 2행이다
        let t = term("abc\r\ndef\r\nghi\x1b[2;4r\x1b[?6h");
        let one = Request {
            top: 1,
            left: 1,
            bottom: 1,
            right: 3,
            ..Request::default()
        };
        assert_eq!(compute(&t, one, POS), b'd' as u16 + b'e' as u16 + b'f' as u16);
        // DECOM을 끄면 1행은 화면 맨 위다
        let t = term("abc\r\ndef\r\nghi\x1b[2;4r");
        assert_eq!(compute(&t, one, POS), b'a' as u16 + b'b' as u16 + b'c' as u16);
    }

    // Ghostty "DEC trims blanks and skips undrawn cells"
    #[test]
    fn dec_trims_blanks_and_skips_undrawn_cells() {
        let t = term("a b");
        let r = row1();
        assert_eq!(
            compute(&t, r, Flags::default()),
            0u16.wrapping_sub(b'a' as u16 + b'b' as u16)
        );
        assert_eq!(compute(&t, r, POS), b'a' as u16 + b'b' as u16);
        // 빼지 않으면 빈칸과 쓰지 않은 칸을 빈칸으로 센다
        let no_trim = Flags { no_trim: true, ..POS };
        assert_eq!(compute(&t, r, no_trim), b'a' as u16 + b'b' as u16 + 32 * 8);
        // 쓰지 않은 칸을 빈칸으로 세면 곧바로 빠진다
        let undrawn = Flags { undrawn: true, ..POS };
        assert_eq!(compute(&t, r, undrawn), b'a' as u16 + b'b' as u16);
    }

    // Ghostty "DEC counts a leading space". 다른 점: 찍은 첫 빈칸이 쓰지 않은 칸과 같아 센 첫 칸이 되지 못한다
    #[test]
    fn leading_space_differs_from_ghostty_because_written_blank_is_undrawn() {
        let t = term(" a\r\n b");
        // Ghostty는 ' ' + 'a' + 'b'. 여기서는 첫 빈칸을 건너뛰어 'a'가 센 첫 칸이 되므로 ' '가 빠진다
        assert_eq!(compute(&t, Request::default(), POS), b'a' as u16 + b'b' as u16);
        // 빼지 않는 계산(esctest가 쓰는 쪽)은 Ghostty와 같다
        let no_trim = Flags { no_trim: true, ..POS };
        assert_eq!(
            compute(&t, Request::default(), no_trim),
            b'a' as u16 + b'b' as u16 + 32 * 48
        );
        // 속성이 붙은 빈칸은 쓴 칸이라 Ghostty와 같다(첫 칸이라 셈)
        let t = term("\x1b[7m \x1b[0ma");
        assert_eq!(compute(&t, row1(), POS), b' ' as u16 + 0x20 + b'a' as u16);
    }

    // Ghostty "empty rectangle"
    #[test]
    fn empty_rectangle_is_zero() {
        let t = term("abc");
        let req = Request {
            left: 3,
            right: 2,
            ..Request::default()
        };
        assert_eq!(compute(&t, req, Flags::default()), 0);
    }

    // Ghostty "rectangle bounds"
    #[test]
    fn rectangle_bounds() {
        let t = term("abc\r\ndef\r\nghi");
        let req = Request {
            top: 2,
            left: 2,
            bottom: 3,
            right: 3,
            ..Request::default()
        };
        assert_eq!(
            compute(&t, req, POS),
            (b'e' as u16 + b'f' as u16 + b'h' as u16 + b'i' as u16)
        );
    }

    // Ghostty "attributes". 깜박임(0x40)과 DECSCA 보호(0x04)는 합에 더하지 않는다
    #[test]
    fn attributes_added_per_cell() {
        // a: 굵게+밑줄, ' ': 반전, b: 숨김
        let t = term("\x1b[1;4ma\x1b[0;7m \x1b[0;8mb");
        let sum = (b'a' as u16 + 0x80 + 0x10) + (b' ' as u16 + 0x20) + (b'b' as u16 + 0x08);
        assert_eq!(compute(&t, row1(), POS), sum);
        let no_attr = Flags {
            no_attributes: true,
            ..POS
        };
        assert_eq!(compute(&t, row1(), no_attr), b'a' as u16 + b'b' as u16);
    }

    // Ghostty "extended attributes keep a space"
    #[test]
    fn italic_keeps_a_space() {
        let t = term("a\x1b[3m ");
        assert_eq!(compute(&t, row1(), POS), b'a' as u16 + b' ' as u16);
    }

    // Ghostty "DEC 8-bit values"
    #[test]
    fn dec_8bit_values() {
        let t = term("é€");
        assert_eq!(compute(&t, row1(), POS), 0x69 + 0x1B);
        let full = Flags { full: true, ..POS };
        assert_eq!(compute(&t, row1(), full), 0xE9u16.wrapping_add(0x20AC));
    }

    // Ghostty "wide characters"
    #[test]
    fn wide_characters() {
        let t = term("橋");
        assert_eq!(compute(&t, row1(), POS), 0x1B + 0x1B);
        let full = Flags { full: true, ..POS };
        assert_eq!(compute(&t, row1(), full), 0x6A4B);
    }

    // Ghostty "combining marks"(첫 두 칸만)
    #[test]
    fn combining_marks() {
        let t = term("e\u{301}");
        let req = Request {
            top: 1,
            bottom: 1,
            right: 1,
            ..Request::default()
        };
        assert_eq!(compute(&t, req, POS), b'e' as u16);
        let no_trim = Flags { no_trim: true, ..POS };
        assert_eq!(compute(&t, req, no_trim), b'e' as u16 + 0x301);
        let both = Flags {
            no_trim: true,
            full: true,
            ..POS
        };
        assert_eq!(compute(&t, req, both), b'e' as u16);
    }

    #[test]
    fn flags_from_bits_and_config() {
        assert_eq!(Flags::from_bits(0), Flags::default());
        assert_eq!(
            Flags::from_bits(0b11111),
            Flags {
                positive: true,
                no_attributes: true,
                no_trim: true,
                undrawn: true,
                full: true
            }
        );
        // 윗 비트는 버린다
        assert_eq!(Flags::from_bits(0b100000), Flags::default());
        assert_eq!(Flags::from_config(&XtChecksumExtension::default()), Flags::default());
        // esctest의 `no-negate,no-attributes,no-trim,full`(xterm checksumExtension 23)
        let esctest = XtChecksumExtension {
            negate: false,
            attributes: false,
            trim: false,
            undrawn: false,
            full: true,
        };
        assert_eq!(Flags::from_config(&esctest), Flags::from_bits(23));
    }
}
