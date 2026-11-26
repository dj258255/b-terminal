//! 커서·고름 색의 바탕이 되는 지금 글자색과 OSC 12 커서색(#838).
//! 기준: Ghostty 1.4.0 renderer/generic.zig 2789~2830줄(커서색은 OSC 12 → cursor-color → 지금 글자색),
//! termio/stream_handler.zig colorOperation(OSC 10·11·12 설정, 110·111·112 되돌리기)

use bt_core::{Rgb, Theme};
use bt_term::{Emulator, GridSize};

fn emu(theme: Theme) -> Emulator {
    Emulator::new(GridSize::new(10, 2), theme)
}

#[test]
fn snapshot_carries_current_foreground_and_osc_12_cursor() {
    let t = Theme::default();
    let mut e = emu(t.clone());
    let s = e.snapshot();
    assert_eq!((s.foreground, s.cursor_color), (t.foreground, None));
    // OSC 10은 지금 글자색을, OSC 12는 커서색을 바꾼다
    e.feed(b"\x1b]10;#ffff00\x07\x1b]12;red\x07");
    let s = e.snapshot();
    assert_eq!(s.foreground, Rgb::new(255, 255, 0));
    assert_eq!(s.cursor_color, Some(Rgb::new(255, 0, 0)));
    // OSC 112는 커서색만, OSC 110은 글자색을 되돌린다
    e.feed(b"\x1b]112\x07");
    let s = e.snapshot();
    assert_eq!((s.foreground, s.cursor_color), (Rgb::new(255, 255, 0), None));
    e.feed(b"\x1b]110\x07");
    assert_eq!(e.snapshot().foreground, t.foreground);
}

#[test]
fn reversed_screen_swaps_the_snapshot_foreground_but_not_the_cursor_color() {
    let t = Theme::default();
    let mut e = emu(t.clone());
    e.feed(b"\x1b]12;#0000ff\x07\x1b[?5h");
    let s = e.snapshot();
    assert_eq!(s.foreground, t.background, "반전하면 기본 글자색과 바탕색이 맞바뀐다");
    assert_eq!(s.cursor_color, Some(Rgb::new(0, 0, 255)));
}
