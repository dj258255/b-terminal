//! 열 삽입·삭제(DECIC·DECDC), DECBI·DECFI, CTC, HPB·VPB, 문자 집합 바꿈(LS2·LS3·SS2·SS3)과 영국 문자 집합(#295).
//! vendor/vte는 작업 공간 밖이라 자체 테스트가 돌지 않아 여기서 해석과 엔진 결과를 함께 본다.
//! 기준은 Ghostty stream.zig(CSI '}'·'~', CTC 'W', HPB 'j', VPB 'k', ESC 6·9·N·O·n·o)와 Terminal.zig(insertColumns·deleteColumns·backIndex·forwardIndex)

use alacritty_terminal::vte::ansi::{CharsetIndex, Handler, Processor, StandardCharset, TabulationClearMode};
use bt_core::Theme;
use bt_term::{Emulator, GridSize};

#[derive(Debug, PartialEq, Eq)]
enum Ev {
    InsertColumns(usize),
    DeleteColumns(usize),
    BackIndex,
    ForwardIndex,
    SetTab,
    ClearTabs(&'static str),
    Back(usize),
    Up(usize),
    Active(CharsetIndex),
    Single(CharsetIndex),
    Configure(CharsetIndex, StandardCharset),
}

#[derive(Default)]
struct Collect(Vec<Ev>);

impl Handler for Collect {
    fn insert_columns(&mut self, count: usize) {
        self.0.push(Ev::InsertColumns(count));
    }
    fn delete_columns(&mut self, count: usize) {
        self.0.push(Ev::DeleteColumns(count));
    }
    fn back_index(&mut self) {
        self.0.push(Ev::BackIndex);
    }
    fn forward_index(&mut self) {
        self.0.push(Ev::ForwardIndex);
    }
    fn set_horizontal_tabstop(&mut self) {
        self.0.push(Ev::SetTab);
    }
    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        self.0.push(Ev::ClearTabs(match mode {
            TabulationClearMode::Current => "current",
            TabulationClearMode::All => "all",
        }));
    }
    fn move_backward(&mut self, cols: usize) {
        self.0.push(Ev::Back(cols));
    }
    fn move_up(&mut self, lines: usize) {
        self.0.push(Ev::Up(lines));
    }
    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.0.push(Ev::Active(index));
    }
    fn set_single_shift(&mut self, index: CharsetIndex) {
        self.0.push(Ev::Single(index));
    }
    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        self.0.push(Ev::Configure(index, charset));
    }
}

fn parse(bytes: &[u8]) -> Vec<Ev> {
    let mut parser: Processor = Processor::new();
    let mut handler = Collect::default();
    parser.advance(&mut handler, bytes);
    handler.0
}

#[test]
fn decic_and_decdc_parse_with_default_one_and_zero_as_one() {
    assert_eq!(parse(b"\x1b['}"), vec![Ev::InsertColumns(1)]);
    assert_eq!(parse(b"\x1b[0'}"), vec![Ev::InsertColumns(1)]);
    assert_eq!(parse(b"\x1b[3'}"), vec![Ev::InsertColumns(3)]);
    assert_eq!(parse(b"\x1b['~"), vec![Ev::DeleteColumns(1)]);
    assert_eq!(parse(b"\x1b[2'~"), vec![Ev::DeleteColumns(2)]);
    // 매개변수가 둘이면 Ghostty처럼 버린다
    assert_eq!(parse(b"\x1b[1;2'}"), vec![]);
    assert_eq!(parse(b"\x1b[1;2'~"), vec![]);
}

#[test]
fn esc_6_and_9_are_back_and_forward_index() {
    assert_eq!(parse(b"\x1b6"), vec![Ev::BackIndex]);
    assert_eq!(parse(b"\x1b9"), vec![Ev::ForwardIndex]);
}

#[test]
fn ctc_sets_and_clears_tab_stops() {
    assert_eq!(parse(b"\x1b[W"), vec![Ev::SetTab]);
    assert_eq!(parse(b"\x1b[0W"), vec![Ev::SetTab]);
    assert_eq!(parse(b"\x1b[2W"), vec![Ev::ClearTabs("current")]);
    assert_eq!(parse(b"\x1b[5W"), vec![Ev::ClearTabs("all")]);
    // 모르는 값과 매개변수가 둘인 꼴은 버린다
    assert_eq!(parse(b"\x1b[1W"), vec![]);
    assert_eq!(parse(b"\x1b[0;2W"), vec![]);
}

#[test]
fn hpb_is_cursor_left_and_vpb_is_cursor_up() {
    assert_eq!(parse(b"\x1b[j"), vec![Ev::Back(1)]);
    assert_eq!(parse(b"\x1b[3j"), vec![Ev::Back(3)]);
    assert_eq!(parse(b"\x1b[k"), vec![Ev::Up(1)]);
    assert_eq!(parse(b"\x1b[4k"), vec![Ev::Up(4)]);
}

#[test]
fn locking_shifts_and_single_shifts_and_british_parse() {
    assert_eq!(parse(b"\x1bn"), vec![Ev::Active(CharsetIndex::G2)]);
    assert_eq!(parse(b"\x1bo"), vec![Ev::Active(CharsetIndex::G3)]);
    assert_eq!(parse(b"\x1bN"), vec![Ev::Single(CharsetIndex::G2)]);
    assert_eq!(parse(b"\x1bO"), vec![Ev::Single(CharsetIndex::G3)]);
    assert_eq!(
        parse(b"\x1b(A"),
        vec![Ev::Configure(CharsetIndex::G0, StandardCharset::British)]
    );
    assert_eq!(
        parse(b"\x1b)A"),
        vec![Ev::Configure(CharsetIndex::G1, StandardCharset::British)]
    );
    // GR로 부르는 LS1R~LS3R은 Ghostty처럼 받기만 한다(Terminal.zig 2279줄: GR은 아직 글자에 쓰지 않음)
    assert_eq!(parse(b"\x1b~\x1b}\x1b|"), vec![]);
}

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

fn rows(e: &mut Emulator) -> Vec<String> {
    let s = e.snapshot();
    (0..s.rows).map(|r| s.row_text(r)).collect()
}

fn fill(e: &mut Emulator) {
    e.feed(b"abcdefgh\r\nijklmnop\r\nqrstuvwx");
}

#[test]
fn decic_inserts_blank_columns_in_every_row_and_keeps_the_cursor() {
    let mut e = emu(8, 3);
    fill(&mut e);
    e.feed(b"\x1b[2;3H\x1b[2'}");
    assert_eq!(rows(&mut e), vec!["ab  cdef", "ij  klmn", "qr  stuv"]);
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (1, 2));
}

#[test]
fn decdc_deletes_columns_and_fills_blanks_from_the_right() {
    let mut e = emu(8, 3);
    fill(&mut e);
    e.feed(b"\x1b[2;3H\x1b[2'~");
    assert_eq!(rows(&mut e), vec!["abefgh", "ijmnop", "qruvwx"]);
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (1, 2));
}

#[test]
fn decic_and_decdc_only_touch_the_scroll_region_rows() {
    let mut e = emu(8, 4);
    e.feed(b"abcdefgh\r\nijklmnop\r\nqrstuvwx\r\nyz012345");
    e.feed(b"\x1b[2;3r\x1b[2;3H\x1b['}");
    assert_eq!(rows(&mut e), vec!["abcdefgh", "ij klmno", "qr stuvw", "yz012345"]);
    e.feed(b"\x1b[2'~");
    assert_eq!(rows(&mut e), vec!["abcdefgh", "ijlmno", "qrtuvw", "yz012345"]);
}

#[test]
fn decic_and_decdc_do_nothing_when_the_cursor_is_outside_the_scroll_region() {
    let mut e = emu(8, 4);
    e.feed(b"abcdefgh\r\nijklmnop\r\nqrstuvwx\r\nyz012345");
    // 영역(2~3줄) 밖인 1줄과 4줄
    e.feed(b"\x1b[2;3r\x1b[1;3H\x1b['}\x1b['~\x1b[4;3H\x1b['}\x1b['~");
    assert_eq!(rows(&mut e), vec!["abcdefgh", "ijklmnop", "qrstuvwx", "yz012345"]);
}

#[test]
fn decic_count_larger_than_the_rest_blanks_to_the_edge() {
    let mut e = emu(8, 2);
    e.feed(b"abcdefgh\x1b[1;6H\x1b[99'}");
    assert_eq!(rows(&mut e)[0], "abcde");
    e.feed(b"\x1b[1;1H\x1b[99'~");
    assert_eq!(rows(&mut e)[0], "");
}

#[test]
fn decic_clears_pending_wrap_so_the_next_char_overwrites_the_last_column() {
    let mut e = emu(4, 2);
    e.feed(b"abcd\x1b['}");
    e.feed(b"Z");
    assert_eq!(rows(&mut e), vec!["abcZ", ""]);
}

#[test]
fn decbi_moves_left_or_inserts_a_column_at_the_left_edge() {
    let mut e = emu(8, 2);
    e.feed(b"abcdefgh\r\nijklmnop");
    // 가운데에서는 왼쪽으로 한 칸
    e.feed(b"\x1b[1;4H\x1b6");
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (0, 2));
    // 왼쪽 끝에서는 영역 전체가 오른쪽으로 한 칸 밀린다
    e.feed(b"\x1b[1;1H\x1b6");
    assert_eq!(rows(&mut e), vec![" abcdefg", " ijklmno"]);
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (0, 0));
}

#[test]
fn decfi_moves_right_or_deletes_the_left_column_at_the_right_edge() {
    let mut e = emu(8, 2);
    e.feed(b"abcdefgh\r\nijklmnop");
    e.feed(b"\x1b[1;4H\x1b9");
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (0, 4));
    // 오른쪽 끝에서는 왼쪽 끝 열이 지워지고 영역이 왼쪽으로 한 칸 밀린다
    e.feed(b"\x1b[1;8H\x1b9");
    assert_eq!(rows(&mut e), vec!["bcdefgh", "jklmnop"]);
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (0, 7));
}

#[test]
fn decbi_and_decfi_at_the_screen_edge_outside_the_region_do_nothing_to_cells() {
    let mut e = emu(8, 4);
    e.feed(b"abcdefgh\r\nijklmnop\r\nqrstuvwx\r\nyz012345");
    // 영역(2~3줄) 밖인 1줄과 4줄의 양쪽 끝
    e.feed(b"\x1b[2;3r\x1b[1;1H\x1b6\x1b[1;8H\x1b9\x1b[4;1H\x1b6\x1b[4;8H\x1b9");
    assert_eq!(rows(&mut e), vec!["abcdefgh", "ijklmnop", "qrstuvwx", "yz012345"]);
}

#[test]
fn ctc_sets_tab_stop_at_the_cursor_and_clears_it() {
    let mut e = emu(20, 1);
    // 3번 칸(0부터)에 탭 정지를 넣고 처음에서 탭
    e.feed(b"\x1b[1;4H\x1b[W\r\t");
    assert_eq!(e.snapshot().cursor.col, 3);
    // 2W로 지우면 다음 기본 정지(8)로 간다
    e.feed(b"\x1b[2W\r\t");
    assert_eq!(e.snapshot().cursor.col, 8);
    // 5W는 모두 지운다. 끝 칸까지 간다
    e.feed(b"\x1b[5W\r\t");
    assert_eq!(e.snapshot().cursor.col, 19);
}

#[test]
fn hpb_and_vpb_move_the_cursor_like_cub_and_cuu() {
    let mut e = emu(10, 5);
    e.feed(b"\x1b[4;6H\x1b[2j");
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (3, 3));
    e.feed(b"\x1b[2k");
    let c = e.snapshot().cursor;
    assert_eq!((c.row, c.col), (1, 3));
}

#[test]
fn british_charset_maps_hash_to_pound() {
    let mut e = emu(10, 1);
    e.feed(b"\x1b(A#a\x1b(B#");
    assert_eq!(rows(&mut e)[0], "\u{a3}a#");
}

#[test]
fn ls2_and_ls3_lock_the_g2_and_g3_sets_into_gl() {
    let mut e = emu(10, 1);
    e.feed(b"\x1b*A\x1b+0");
    e.feed(b"\x1bn##\x1bo q\x1b(B");
    // LS2: G2(영국)로 '#'이 두 번 파운드, LS3: G3(선 그림)로 ' '은 그대로, 'q'는 가로선
    assert_eq!(rows(&mut e)[0], "\u{a3}\u{a3} \u{2500}");
    // SI(0x0f)는 G0으로 돌아온다
    e.feed(b"\x0f#");
    assert_eq!(rows(&mut e)[0], "\u{a3}\u{a3} \u{2500}#");
}

#[test]
fn ss2_applies_to_exactly_one_character() {
    let mut e = emu(10, 1);
    e.feed(b"\x1b*A\x1bN###");
    assert_eq!(rows(&mut e)[0], "\u{a3}##");
    // SS3도 한 글자
    e.feed(b"\x1b+0\x1bOqq");
    assert_eq!(rows(&mut e)[0], "\u{a3}##\u{2500}q");
}

#[test]
fn lsr_sequences_change_nothing_on_screen() {
    let mut e = emu(10, 1);
    e.feed(b"\x1b*A\x1b~\x1b}\x1b|ab#");
    assert_eq!(rows(&mut e)[0], "ab#");
}
