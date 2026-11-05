//! PTY 없이 바이트를 해석해 화면 값을 확인한다

use bt_core::{Rgb, Theme};
use bt_term::{CellFlags, CursorShape, Emulator, GridSize};

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

#[test]
fn cursor_moves_and_colors_resolve() {
    let mut e = emu(20, 5);
    e.feed(b"\x1b[31mred\x1b[0m \x1b[38;2;1;2;3mrgb\x1b[0m");
    let s = e.snapshot();
    let t = Theme::default();
    assert_eq!(s.row_text(0), "red rgb");
    assert_eq!(s.cell(0, 0).fg, t.palette[1]);
    assert_eq!(s.cell(4, 0).fg, Rgb::new(1, 2, 3));
    assert_eq!((s.cursor.col, s.cursor.row), (7, 0));
    assert!(s.cursor.visible);
}

#[test]
fn inverse_video_swaps_colors_for_tui_selection() {
    let mut e = emu(10, 2);
    e.feed(b"\x1b[7msel\x1b[0m");
    let s = e.snapshot();
    let t = Theme::default();
    assert_eq!(s.cell(0, 0).bg, t.foreground);
    assert_eq!(s.cell(0, 0).fg, t.background);
}

#[test]
fn alt_screen_and_hidden_cursor_and_shape() {
    let mut e = emu(10, 3);
    e.feed(b"main\x1b[?1049h\x1b[?25lalt");
    let s = e.snapshot();
    assert!(s.modes.alt_screen);
    // 1049는 커서 자리를 그대로 두고 화면만 바꾼다
    assert_eq!(s.row_text(0), "    alt");
    assert!(!s.cursor.visible);
    e.feed(b"\x1b[?1049l\x1b[?25h\x1b[6 q");
    let s = e.snapshot();
    assert_eq!(s.row_text(0), "main");
    assert_eq!(s.cursor.shape, CursorShape::Beam);
}

#[test]
fn scrollback_keeps_history_and_scroll_moves_view() {
    let mut e = emu(10, 3);
    for i in 0..10 {
        e.feed(format!("line{i}\r\n").as_bytes());
    }
    let s = e.snapshot();
    assert!(s.history >= 7);
    assert_eq!(s.row_text(1), "line9");
    e.scroll(3);
    let s = e.snapshot();
    assert_eq!(s.display_offset, 3);
    assert_eq!(s.row_text(0), "line5");
    assert!(!s.cursor.visible, "스크롤백을 보는 동안 커서는 화면 밖이다");
}

#[test]
fn wide_emoji_and_korean_take_two_cells() {
    let mut e = emu(10, 2);
    e.feed("가😀a".as_bytes());
    let s = e.snapshot();
    assert!(s.cell(0, 0).flags.contains(CellFlags::WIDE));
    assert!(s.cell(1, 0).flags.contains(CellFlags::SPACER));
    assert!(s.cell(2, 0).flags.contains(CellFlags::WIDE));
    assert_eq!(s.cell(4, 0).ch, 'a');
}

#[test]
fn bracketed_paste_and_app_cursor_modes_are_reported() {
    let mut e = emu(10, 2);
    e.feed(b"\x1b[?2004h\x1b[?1h");
    let m = e.modes();
    assert!(m.bracketed_paste);
    assert!(m.app_cursor);
}

#[test]
fn kitty_keyboard_mode_is_pushed_popped_and_reported() {
    let mut e = emu(10, 2);
    e.feed(b"\x1b[>1u");
    assert!(e.modes().kitty.disambiguate, "CSI > 1 u로 켠다");
    assert!(!e.modes().kitty.all_as_escapes);
    e.feed(b"\x1b[<u");
    assert!(!e.modes().kitty.active(), "CSI < u로 되돌린다");
}
