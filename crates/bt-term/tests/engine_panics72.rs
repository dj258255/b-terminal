//! 퍼징으로 찾은 엔진 패닉(#587)과 Ghostty `setTopAndBottomMargin` 규칙.
//! 기준: Ghostty Terminal.zig `setTopAndBottomMargin`(3335~3343줄): top = max(1, top), bottom = min(rows, bottom 또는 rows),
//! top >= bottom이면 아무것도 하지 않고(커서도 그대로), 아니면 영역을 정하고 커서를 (1,1)로 옮긴다.

use bt_core::Theme;
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

fn one_line(seq: &str) {
    let mut e = emu(10, 5);
    e.resize(GridSize::new(10, 1));
    e.feed(seq.as_bytes());
    let s = e.snapshot();
    assert!(s.display_offset <= s.history);
}

#[test]
fn single_line_decstbm_then_decom_then_cursor_up() {
    one_line("\x1b[2;4r\x1b[?6h\x1b[10A");
}

#[test]
fn single_line_decom_then_decstbm_then_cursor_up() {
    one_line("\x1b[?6h\x1b[2;4r\x1b[10A");
}

#[test]
fn single_line_decstbm_then_decom_then_home_then_cursor_down() {
    one_line("\x1b[2;4r\x1b[?6h\x1b[H\x1b[10B");
}

#[test]
fn margin_bottom_beyond_rows_is_clamped() {
    // 5줄 화면에서 `2;99r`은 2~5줄 영역이다. 영역 밖(1줄)에서 SU 한 번은 1줄을 건드리지 않는다.
    let mut e = emu(10, 5);
    e.feed(b"\x1b[1;1Hr1\x1b[2;1Hr2\x1b[3;1Hr3\x1b[4;1Hr4\x1b[5;1Hr5");
    e.feed(b"\x1b[2;99r\x1b[S");
    let s = e.snapshot();
    assert_eq!(s.row_text(0).trim_end(), "r1");
    assert_eq!(s.row_text(1).trim_end(), "r3");
    assert_eq!(s.row_text(3).trim_end(), "r5");
    assert_eq!(s.row_text(4).trim_end(), "");
}

#[test]
fn margin_ignored_after_clamp_leaves_cursor() {
    // 1줄 화면: `2;4r`은 top 2, bottom min(1, 4) = 1이라 top >= bottom이다. 무시하고 커서는 움직이지 않는다.
    let mut e = emu(10, 1);
    e.feed(b"abc");
    let before = e.snapshot();
    e.feed(b"\x1b[2;4r");
    let after = e.snapshot();
    assert_eq!((after.cursor.col, after.cursor.row), (before.cursor.col, before.cursor.row));
    assert_eq!(after.cursor.col, 3);
}

#[test]
fn margin_valid_moves_cursor_home() {
    let mut e = emu(10, 5);
    e.feed(b"\x1b[4;5H\x1b[2;4r");
    let s = e.snapshot();
    assert_eq!((s.cursor.col, s.cursor.row), (0, 0));
}

#[test]
fn margin_top_equal_bottom_ignored() {
    let mut e = emu(10, 5);
    e.feed(b"\x1b[4;5H\x1b[3;3r");
    let s = e.snapshot();
    assert_eq!((s.cursor.col, s.cursor.row), (4, 3));
}

/// #587(퍼징): 줄 수 2 이상에서도 같은 `damage_line` 패닉이 났다. 원인은 영역이 아니라 DCH(`CSI P`)의 넓은 글자 갈라짐이다.
/// 커서 줄 0의 첫 칸이 넓은 글자인데 바로 위 줄이 스크롤백(음수 줄)이면 `above.0 as usize`가 usize::MAX가 돼 범위 밖이었다.
#[test]
fn dch_with_wide_char_at_top_of_screen_and_history_above() {
    // 줄 끝 칸에 못 들어가 다음 줄로 넘어간 넓은 글자(앞줄 끝에 자리 표지)가 화면 맨 윗줄 첫 칸이 되고,
    // 그 앞줄은 스크롤백인 상태에서 그 칸에서 DCH를 한다(앱처럼 snapshot으로 손상 기록을 비운다)
    let mut e = emu(10, 3);
    e.feed("123456789한\r\n\n".as_bytes());
    e.snapshot();
    e.feed(b"\x1b[1;1H");
    e.snapshot();
    e.feed(b"\x1b[2P");
    let s = e.snapshot();
    assert!(s.display_offset <= s.history);
}
