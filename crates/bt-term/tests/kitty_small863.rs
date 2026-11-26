//! kitty 호환 소품(#863): SGR 221·222, CSI 22 J(이미 있음, 확인만), `clear_terminal`의 scrollback·last_command, 그림 e 질의(이미 있음, 확인만).
//! 기준: kitty `kitty/vt-parser.c` SGR(221 굵게만 끔, 222 흐림만 끔), `boss.py` `clear_terminal`, `screen.c` `erase_last_command`.
//! Ghostty 1.4.0 `sgr.zig`는 221·222를 모르는 속성으로 버린다(아무것도 바꾸지 않음). 여기는 kitty를 따른다

use bt_core::Theme;
use bt_term::screen::CellFlags;
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(
        GridSize {
            cols,
            rows,
            cell_width: 10,
            cell_height: 20,
        },
        Theme::default(),
    )
}

fn flags_of(e: &mut Emulator, bytes: &[u8]) -> (bool, bool) {
    e.feed(bytes);
    let s = e.snapshot();
    let cell = &s.cells[0];
    (
        cell.flags.contains(CellFlags::BOLD),
        cell.flags.contains(CellFlags::FAINT),
    )
}

fn seq(e: &mut Emulator, input: &str) -> String {
    // 한 칸만 찍고 (굵게, 흐림)을 돌려준다
    e.feed(b"\x1b[2J\x1b[H\x1b[0m");
    let (b, d) = flags_of(e, format!("{input}x").as_bytes());
    format!("bold={b} dim={d}")
}

#[test]
fn sgr_221_cancels_only_bold() {
    let mut e = emu(20, 4);
    assert_eq!(seq(&mut e, "\x1b[1;2m\x1b[221m"), "bold=false dim=true");
}

#[test]
fn sgr_222_cancels_only_dim() {
    let mut e = emu(20, 4);
    assert_eq!(seq(&mut e, "\x1b[1;2m\x1b[222m"), "bold=true dim=false");
}

#[test]
fn sgr_22_still_cancels_both_and_221_mixes_with_others() {
    let mut e = emu(20, 4);
    assert_eq!(seq(&mut e, "\x1b[1;2m\x1b[22m"), "bold=false dim=false");
    // 같은 줄 안에서 다른 속성과 섞여도 뒤 매개변수가 밀리지 않는다
    assert_eq!(seq(&mut e, "\x1b[1;2;221;31m"), "bold=false dim=true");
}

#[test]
fn ed_22_exists_and_scrolls_the_screen_into_scrollback() {
    let mut e = emu(10, 3);
    e.feed(b"one\r\ntwo\r\nthree");
    e.feed(b"\x1b[22J");
    assert!(e.tail_text(10).contains("one"), "{}", e.tail_text(10));
}

#[test]
fn clear_scrollback_keeps_the_screen() {
    let mut e = emu(10, 3);
    e.feed(b"a\r\nb\r\nc\r\nd\r\ne");
    let before = e.tail_text(20);
    assert!(before.contains('a'), "{before}");
    assert!(e.clear_scrollback());
    let after = e.tail_text(20);
    assert!(!after.contains('a') && !after.contains('b'), "{after}");
    assert!(after.contains('c') && after.contains('e'), "{after}");
}

#[test]
fn clear_scrollback_ignores_alt_screen() {
    let mut e = emu(10, 3);
    e.feed(b"a\r\nb\r\nc\r\nd\x1b[?1049h");
    assert!(!e.clear_scrollback());
}

/// 프롬프트 `$ `, 입력, Enter, 출력 시작
fn cmd(e: &mut Emulator, input: &str, out: &str) {
    e.feed(format!("\x1b]133;A\x07$ \x1b]133;B\x07{input}\r\n\x1b]133;C\x07{out}\x1b]133;D;0\x07").as_bytes());
}

#[test]
fn clear_last_command_removes_the_previous_prompt_block_on_screen() {
    let mut e = emu(20, 10);
    cmd(&mut e, "one", "o1\r\n");
    cmd(&mut e, "two", "t1\r\nt2\r\n");
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    assert!(e.clear_last_command());
    let t = e.tail_text(20);
    assert!(t.contains("$ one") && t.contains("o1"), "{t}");
    assert!(!t.contains("two") && !t.contains("t1") && !t.contains("t2"), "{t}");
    // 지금 프롬프트는 그대로 남고 커서는 그 줄에 있다
    assert!(t.trim_end().ends_with("$"), "{t}");
}

#[test]
fn clear_last_command_reaches_into_scrollback() {
    let mut e = emu(20, 4);
    cmd(&mut e, "one", "o1\r\n");
    cmd(&mut e, "two", "t1\r\nt2\r\nt3\r\n");
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    assert!(e.clear_last_command());
    let t = e.tail_text(30);
    assert!(
        t.contains("o1") && !t.contains("t1") && !t.contains("t2") && !t.contains("t3"),
        "{t}"
    );
    assert!(t.trim_end().ends_with("$"), "{t}");
    // 두 번째로 부르면 한 명령 더 지운다
    assert!(e.clear_last_command());
    let t = e.tail_text(30);
    assert!(!t.contains("one"), "{t}");
}

#[test]
fn clear_last_command_without_shell_integration_does_nothing() {
    let mut e = emu(20, 4);
    e.feed(b"plain\r\ntext");
    assert!(!e.clear_last_command());
    assert!(e.tail_text(10).contains("plain"));
}

#[test]
fn clear_last_command_with_only_one_prompt_does_nothing() {
    let mut e = emu(20, 4);
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    assert!(!e.clear_last_command());
}

/// 그림 질의의 e 키(kitty 0.50): 이미 있다. 지원하는 키만 `e=`와 함께 돌려준다
#[test]
fn graphics_feature_query_e_key_exists() {
    let mut e = emu(20, 4);
    e.feed(b"\x1b_Ga=q,i=31,e=T,U=1,C=1\x1b\\");
    let r = String::from_utf8_lossy(&e.take_replies()).into_owned();
    assert!(r.starts_with("\x1b_Gi=31,e=T") && r.contains(";OK"), "{r:?}");
}
