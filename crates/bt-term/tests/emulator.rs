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

/// #40: 프로그램이 DECSCUSR로 깜빡이는 커서를 고르면 화면 값에 보인다(1 깜빡이는 네모, 2 멈춘 네모, 5 깜빡이는 막대)
#[test]
fn decscusr_blinking_reaches_the_screen() {
    let mut e = bt_term::Emulator::new(bt_term::GridSize::new(10, 2), bt_core::Theme::default());
    assert!(!e.snapshot().cursor.blinking, "설정 기본은 멈춤");
    e.feed(b"\x1b[1 q");
    assert!(e.snapshot().cursor.blinking);
    e.feed(b"\x1b[2 q");
    assert!(!e.snapshot().cursor.blinking);
    e.feed(b"\x1b[5 q");
    let s = e.snapshot();
    assert!(s.cursor.blinking);
    assert_eq!(s.cursor.shape, bt_term::CursorShape::Beam);
}

/// #66: 스크롤백까지 맨 아래에서 위로 N줄을 글로 꺼낸다. 접힌 줄은 잇고 넓은 글자의 뒤 칸은 뺀다
#[test]
fn tail_text_reads_scrollback_and_joins_wrapped_lines() {
    let mut e = bt_term::Emulator::new(bt_term::GridSize::new(10, 3), bt_core::Theme::default());
    for i in 0..20 {
        e.feed(format!("line {i}\r\n").as_bytes());
    }
    e.feed("한글이 길게 접히는 줄\r\nend".as_bytes());
    let all = e.tail_text(100);
    assert!(all.starts_with("line 0\nline 1\n"), "{all}");
    assert!(
        all.ends_with("한글이 길게 접히는 줄\nend"),
        "접힌 줄을 이어 한 줄로: {all:?}"
    );
    assert_eq!(e.tail_text(1), "end");
    // 화면(3줄)보다 많이: 스크롤백에서 온다
    let five = e.tail_text(5);
    assert!(five.contains("line 19"), "{five:?}");
}

/// #76: kitty 그래픽 바이트가 VT 엔진을 거쳐 커서 자리에 놓이고, 커서가 그림 아래·오른쪽으로 가며, OK로 답한다.
/// 스크롤하면 그림이 따라 올라가고, 화면을 지우면 사라진다
#[test]
fn kitty_graphics_place_scroll_and_clear() {
    fn b64(data: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for ch in data.chunks(3) {
            let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                s.push(if i <= ch.len() {
                    T[(n >> (18 - 6 * i) & 63) as usize] as char
                } else {
                    '='
                });
            }
        }
        s
    }
    let size = bt_term::GridSize {
        cols: 20,
        rows: 6,
        cell_width: 10,
        cell_height: 20,
    };
    let mut e = bt_term::Emulator::new(size, bt_core::Theme::default());
    e.feed(b"ab");
    // 25x30px RGB: 3칸 x 2줄
    let px = vec![9u8; 25 * 30 * 3];
    e.feed(format!("\x1b_Ga=T,f=24,s=25,v=30,i=4;{}\x1b\\", b64(&px)).as_bytes());
    assert_eq!(e.take_replies(), b"\x1b_Gi=4;OK\x1b\\");
    let s = e.snapshot();
    assert_eq!(s.images.len(), 1);
    let im = &s.images[0];
    assert_eq!(
        (im.row, im.col, im.cols, im.rows, im.width, im.height),
        (0, 2, 3, 2, 25, 30)
    );
    assert_eq!((s.cursor.row, s.cursor.col), (1, 5), "그림의 마지막 줄, 그림 오른쪽 칸");
    // 다섯 줄 내려 스크롤: 그림이 화면 위로 밀려 일부(또는 전부) 가려진다
    e.feed(b"\r\n\r\n\r\n\r\n\r\n");
    let s = e.snapshot();
    assert!(
        s.images.is_empty() || s.images[0].row < 0,
        "{:?}",
        s.images.first().map(|i| i.row)
    );
    // 다시 놓고 화면 지우기(clear가 보내는 ED 2·3)
    e.feed(b"\x1b_Ga=p,i=4,q=2\x1b\\");
    assert_eq!(e.snapshot().images.len(), 1);
    e.feed(b"\x1b[H\x1b[2J\x1b[3J");
    assert!(e.snapshot().images.is_empty(), "화면을 지우면 그림도 지운다");
}

/// 보안 검토(#76): 화면 크기 그림을 같은 자리에 수백 번 놓아도 화면 값에는 중복을 빼고 화면 칸 수의 몇 배까지만 담는다
#[test]
fn kitty_graphics_visible_images_are_deduped_and_budgeted() {
    let size = bt_term::GridSize {
        cols: 20,
        rows: 6,
        cell_width: 10,
        cell_height: 20,
    };
    let mut e = bt_term::Emulator::new(size, bt_core::Theme::default());
    e.feed(b"\x1b_Ga=t,f=24,s=1,v=1,i=1,q=2;AAAA\x1b\\");
    let mut many = String::new();
    for _ in 0..500 {
        // 커서를 왼쪽 위로 되돌려 같은 자리에, 화면 전체(20칸 x 6줄)로
        many.push_str("\x1b[H\x1b_Ga=p,i=1,c=20,r=6,C=1,q=2\x1b\\");
    }
    // 자리를 조금씩 달리한 것도 많이
    for col in 0..19 {
        many.push_str(&format!("\x1b[1;{}H\x1b_Ga=p,i=1,c=20,r=6,C=1,q=2\x1b\\", col + 1));
    }
    e.feed(many.as_bytes());
    let s = e.snapshot();
    // 화면에 보이는(잘린) 넓이로 센다
    let area: u32 = s
        .images
        .iter()
        .map(|i| (i.col as u32 + i.cols).min(20).saturating_sub(i.col as u32) * i.rows.min(6))
        .sum();
    assert!(area <= 4 * 20 * 6, "그릴 칸 {area}개, 그림 {}개", s.images.len());
    assert!(!s.images.is_empty());
}

/// 버그: read --lines가 화면 아래쪽 빈 줄까지 세면 글이 위쪽에만 있을 때 빈 글이 된다. 마지막 글 줄(또는 커서 줄)부터 센다
#[test]
fn tail_text_counts_from_the_last_written_line() {
    let mut e = bt_term::Emulator::new(bt_term::GridSize::new(20, 10), bt_core::Theme::default());
    e.feed(b"one\r\ntwo\r\nthree\r\n$ ");
    assert_eq!(e.tail_text(2), "three\n$");
    assert_eq!(e.tail_text(100), "one\ntwo\nthree\n$");
}

#[test]
fn kitty_color_set_query_and_reset_work_without_a_pty() {
    let mut e = emu(20, 3);
    let t = Theme::default();
    e.feed(b"\x1b]21;background=#ff0000;5=#010203\x1b\\");
    assert_eq!(e.snapshot().background, Rgb::new(255, 0, 0));
    e.feed(b"\x1b]21;background=?;5=?\x07");
    assert_eq!(e.take_replies(), b"\x1b]21;background=rgb:ff/00/00;5=rgb:01/02/03\x07");
    e.feed(b"\x1b]21;background=;5=\x1b\\\x1b]21;background=?\x1b\\");
    assert_eq!(e.snapshot().background, t.background);
    let b = t.background;
    let want = format!("\x1b]21;background=rgb:{:02x}/{:02x}/{:02x}\x1b\\", b.r, b.g, b.b);
    assert_eq!(e.take_replies(), want.as_bytes());
}

#[test]
fn ris_and_soft_reset_put_the_palette_back_but_not_foreground_or_background() {
    // Ghostty(Terminal.zig fullReset·softReset)는 팔레트만 되돌린다. OSC 4와 OSC 21이 같은 자리라 둘 다 따른다
    for reset in [&b"\x1bc"[..], &b"\x1b[!p"[..]] {
        let mut e = emu(20, 3);
        let t = Theme::default();
        e.feed(b"\x1b]21;5=#010203;background=#ff0000\x1b\\\x1b]4;6;rgb:aa/bb/cc\x07");
        e.feed(reset);
        e.feed(b"\x1b]21;5=?;background=?\x1b\\");
        let p = t.palette[5];
        let want = format!(
            "\x1b]21;5=rgb:{:02x}/{:02x}/{:02x};background=rgb:ff/00/00\x1b\\",
            p.r, p.g, p.b
        );
        assert_eq!(e.take_replies(), want.as_bytes(), "{reset:?}");
        // OSC 4로 바꾼 6번도 돌아왔다(그리기 쪽 값)
        e.feed(b"\x1b[46m \x1b[0m");
        assert_eq!(e.snapshot().cell(0, 0).bg, t.palette[6], "{reset:?}");
    }
}

#[test]
fn kitty_color_over_the_item_limit_changes_nothing() {
    let mut e = emu(20, 3);
    let mut req = String::from("\x1b]21;background=#ff0000");
    for _ in 0..600 {
        req.push_str(";0=?");
    }
    req.push_str("\x1b\\");
    e.feed(req.as_bytes());
    assert_eq!(e.snapshot().background, Theme::default().background);
    assert!(e.take_replies().is_empty());
}

/// #277: `CSI 53 m`이 윗줄을 켜고 `CSI 55 m`·`CSI 0 m`이 끈다. 밑줄·가운데줄과 따로 움직인다
#[test]
fn overline_is_set_by_sgr_53_and_cleared_by_55_and_0() {
    let mut e = emu(20, 2);
    e.feed(b"a\x1b[53mb\x1b[55mc\x1b[53md\x1b[0me\x1b[1;53mf\x1b[4;9;53mg");
    let s = e.snapshot();
    let over = |col: usize| s.cell(col, 0).flags.contains(CellFlags::OVERLINE);
    assert_eq!(s.row_text(0), "abcdefg");
    assert!(!over(0), "53 앞");
    assert!(over(1), "53");
    assert!(!over(2), "55");
    assert!(over(3), "53 다시");
    assert!(!over(4), "0");
    assert!(over(5), "1;53");
    assert!(over(6), "4;9;53");
    // 윗줄은 다른 꾸밈을 지우지도 켜지도 않는다
    assert!(!s.cell(1, 0).flags.contains(CellFlags::UNDERLINE));
    assert!(!s.cell(1, 0).flags.contains(CellFlags::STRIKE));
    assert!(s.cell(6, 0).flags.contains(CellFlags::UNDERLINE));
    assert!(s.cell(6, 0).flags.contains(CellFlags::STRIKE));
    // 공백도 윗줄이 있으면 표시가 남는다
    let mut e = emu(5, 1);
    e.feed(b"\x1b[53m   ");
    assert!(e.snapshot().cell(1, 0).flags.contains(CellFlags::OVERLINE));
}

#[test]
fn soft_reset_returns_the_cursor_style_like_decscusr_0() {
    // DECSCUSR 4(밑줄, 깜박이지 않음) 뒤 DECSTR는 DECSCUSR 0과 같이 설정 모양(기본은 깜박이는 블록)으로 돌아간다(#286)
    let mut e = emu(10, 2);
    e.feed(b"\x1b[4 q");
    assert_eq!(e.snapshot().cursor.shape, CursorShape::Underline);
    e.feed(b"\x1b[!p");
    let soft = e.snapshot().cursor;
    let mut z = emu(10, 2);
    z.feed(b"\x1b[4 q\x1b[0 q");
    let zero = z.snapshot().cursor;
    assert_eq!(soft.shape, CursorShape::Block);
    assert_eq!((soft.shape, soft.blinking), (zero.shape, zero.blinking));
}
