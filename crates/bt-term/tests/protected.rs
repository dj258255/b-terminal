//! 보호 칸(DECSCA·SPA·EPA)과 선택 지우기(DECSED·DECSEL)(#294).
//! 기준: Ghostty stream.zig(`?J`·`?K`·`" q`·ESC V·ESC W)와 Terminal.zig(`setProtectedMode`·`eraseChars`·`eraseLine`·`eraseDisplay`).
//! vendor/vte는 작업 공간 밖이라 해석 테스트는 여기에 둔다

use alacritty_terminal::vte::ansi::{ClearMode, Handler, LineClearMode, Processor, ProtectedMode};
use bt_core::Theme;
use bt_term::{Emulator, GridSize};

#[derive(Default)]
struct Collect(Vec<String>);

impl Handler for Collect {
    fn clear_screen(&mut self, mode: ClearMode) {
        self.0.push(format!("ED {mode:?}"));
    }
    fn clear_line(&mut self, mode: LineClearMode) {
        self.0.push(format!("EL {mode:?}"));
    }
    fn selective_clear_screen(&mut self, mode: ClearMode) {
        self.0.push(format!("DECSED {mode:?}"));
    }
    fn selective_clear_line(&mut self, mode: LineClearMode) {
        self.0.push(format!("DECSEL {mode:?}"));
    }
    fn set_protected_mode(&mut self, mode: ProtectedMode) {
        self.0.push(format!("PROT {mode:?}"));
    }
}

fn parse(bytes: &[u8]) -> Vec<String> {
    let mut parser: Processor = Processor::new();
    let mut h = Collect::default();
    parser.advance(&mut h, bytes);
    h.0
}

#[test]
fn parses_decsed_and_decsel_separately_from_ed_and_el() {
    assert_eq!(parse(b"\x1b[?J"), ["DECSED Below"]);
    assert_eq!(parse(b"\x1b[?0J"), ["DECSED Below"]);
    assert_eq!(parse(b"\x1b[?1J"), ["DECSED Above"]);
    assert_eq!(parse(b"\x1b[?2J"), ["DECSED All"]);
    assert_eq!(parse(b"\x1b[?3J"), ["DECSED Saved"]);
    assert_eq!(parse(b"\x1b[?K"), ["DECSEL Right"]);
    assert_eq!(parse(b"\x1b[?1K"), ["DECSEL Left"]);
    assert_eq!(parse(b"\x1b[?2K"), ["DECSEL All"]);
    // 모르는 모드는 버린다
    assert!(parse(b"\x1b[?5J\x1b[?3K").is_empty());
    // 일반 ED·EL은 그대로
    assert_eq!(parse(b"\x1b[J\x1b[1K"), ["ED Below", "EL Left"]);
}

#[test]
fn parses_decsca_spa_and_epa() {
    // Ghostty stream.zig 2522~2544줄: 0·2·생략은 끔, 1은 DEC 보호, 그 밖은 버림
    assert_eq!(parse(b"\x1b[\"q"), ["PROT Off"]);
    assert_eq!(parse(b"\x1b[0\"q"), ["PROT Off"]);
    assert_eq!(parse(b"\x1b[2\"q"), ["PROT Off"]);
    assert_eq!(parse(b"\x1b[1\"q"), ["PROT Dec"]);
    assert!(parse(b"\x1b[3\"q").is_empty());
    // stream.zig 3318~3328줄: ESC V는 ISO 보호 켬, ESC W는 끔
    assert_eq!(parse(b"\x1bV\x1bW"), ["PROT Iso", "PROT Off"]);
}

fn emu() -> Emulator {
    Emulator::new(GridSize::new(20, 5), Theme::default())
}

fn rows(e: &mut Emulator) -> Vec<String> {
    let s = e.snapshot();
    (0..5).map(|r| s.row_text(r)).collect()
}

/// `ABC` + 보호(`prot`) `DEF` + 끔 `GH`가 든 첫 줄
fn line_with_protected(prot: &str) -> Emulator {
    let mut e = emu();
    e.feed(format!("ABC{prot}DEF\x1b[0\"q\x1bWGH").as_bytes());
    e
}

const DEC: &str = "\x1b[1\"q";
const SPA: &str = "\x1bV";

#[test]
fn decsel_keeps_dec_protected_cells() {
    let mut e = line_with_protected(DEC);
    e.feed(b"\x1b[1;1H\x1b[?K");
    assert_eq!(rows(&mut e)[0], "   DEF");
    // 왼쪽만: 커서(5번 칸)까지 지우되 보호 칸 D·E·F는 남고 오른쪽은 그대로
    let mut e = line_with_protected(DEC);
    e.feed(b"\x1b[1;6H\x1b[?1K");
    assert_eq!(rows(&mut e)[0], "   DEFGH");
    // 줄 전체
    let mut e = line_with_protected(DEC);
    e.feed(b"\x1b[?2K");
    assert_eq!(rows(&mut e)[0], "   DEF");
}

#[test]
fn plain_el_and_ech_ignore_dec_protection() {
    // DECSCA(DEC)는 선택 지우기만 막는다. Terminal.zig 4310줄: 마지막 보호 방식이 ISO가 아니면 ECH는 보호를 보지 않는다
    let mut e = line_with_protected(DEC);
    e.feed(b"\x1b[1;1H\x1b[K");
    assert_eq!(rows(&mut e)[0], "");
    let mut e = line_with_protected(DEC);
    e.feed(b"\x1b[1;1H\x1b[10X");
    assert_eq!(rows(&mut e)[0], "");
    let mut e = line_with_protected(DEC);
    e.feed(b"\x1b[2J");
    assert_eq!(rows(&mut e)[0], "");
}

#[test]
fn iso_protection_makes_plain_erases_skip_protected_cells() {
    // Terminal.zig 4391·4449줄: 마지막 보호 방식이 ISO면 일반 EL·ED·ECH도 보호 칸을 건너뛴다
    let mut e = line_with_protected(SPA);
    e.feed(b"\x1b[1;1H\x1b[K");
    assert_eq!(rows(&mut e)[0], "   DEF");
    let mut e = line_with_protected(SPA);
    e.feed(b"\x1b[1;1H\x1b[10X");
    assert_eq!(rows(&mut e)[0], "   DEF");
    let mut e = line_with_protected(SPA);
    e.feed(b"\x1b[2K");
    assert_eq!(rows(&mut e)[0], "   DEF");
    let mut e = line_with_protected(SPA);
    e.feed(b"\x1b[2J");
    assert_eq!(rows(&mut e)[0], "   DEF");
}

#[test]
fn ech_ignores_protection_when_dec_was_the_most_recent_mode() {
    // Terminal.zig 11181줄 시험: ISO 뒤 DEC, 그 뒤 끔이면 ECH는 보호 칸도 지운다
    let mut e = emu();
    e.feed(b"\x1bVABC\x1b[1\"qDEF\x1b[0\"qGH\x1b[1;1H\x1b[10X");
    assert_eq!(rows(&mut e)[0], "");
    // 끈 뒤에도 마지막 방식(ISO)은 남는다(Terminal.zig 2762줄): 보호한 ABC·DEF는 남고 끈 채 쓴 GH는 지워진다
    let mut e = emu();
    e.feed(b"\x1b[1\"qABC\x1b[0\"q\x1bVDEF\x1bWGH\x1b[1;1H\x1b[10X");
    assert_eq!(rows(&mut e)[0], "ABCDEF");
}

#[test]
fn decsed_below_above_all_keep_protected_cells_across_lines() {
    let fill = |e: &mut Emulator| {
        e.feed(b"\x1b[2J\x1b[H");
        for r in 1..=3 {
            e.feed(format!("\x1b[{r};1Hab\x1b[1\"qXY\x1b[0\"qcd").as_bytes());
        }
    };
    let mut e = emu();
    fill(&mut e);
    e.feed(b"\x1b[2;3H\x1b[?J");
    assert_eq!(rows(&mut e)[..4], ["abXYcd", "abXY", "  XY", ""]);

    let mut e = emu();
    fill(&mut e);
    e.feed(b"\x1b[2;3H\x1b[?1J");
    assert_eq!(rows(&mut e)[..4], ["  XY", "  XYcd", "abXYcd", ""]);

    let mut e = emu();
    fill(&mut e);
    e.feed(b"\x1b[?2J");
    assert_eq!(rows(&mut e)[..4], ["  XY", "  XY", "  XY", ""]);
}

#[test]
fn decsed_all_does_not_push_the_screen_into_history() {
    // 보호 지우기는 제자리에서 지운다(Ghostty clearRows)
    let mut e = emu();
    e.feed(b"abc\x1b[1\"qXY\x1b[0\"q\x1b[?2J");
    let s = e.snapshot();
    assert_eq!(s.history, 0);
    assert_eq!(s.row_text(0), "   XY");
}

#[test]
fn sgr_reset_does_not_drop_protection() {
    // 보호는 SGR 꾸밈이 아니다. SGR 0이 템플릿 표시를 모두 지워도 보호는 남는다
    let mut e = emu();
    e.feed(b"\x1b[1\"q\x1b[0mA\x1b[1mB\x1b[0\"q\x1b[1;1H\x1b[?K");
    assert_eq!(rows(&mut e)[0], "AB");
}

#[test]
fn decsc_and_decrc_save_protection() {
    // Terminal.zig 2708·2746줄: 저장한 커서가 보호 상태를 들고 있다
    let mut e = emu();
    e.feed(b"\x1b[1\"q\x1b7\x1b[0\"q\x1b8Z\x1b[0\"q\x1b[1;1H\x1b[?K");
    assert_eq!(rows(&mut e)[0], "Z");
}

#[test]
fn overwriting_a_protected_cell_without_protection_unprotects_it() {
    let mut e = emu();
    e.feed(b"\x1b[1\"qAB\x1b[0\"q\x1b[1;1HZ\x1b[1;1H\x1b[?K");
    assert_eq!(rows(&mut e)[0], " B");
}

#[test]
fn wide_protected_char_is_kept_whole() {
    let mut e = emu();
    e.feed("\x1b[1\"q한\x1b[0\"qx\x1b[1;1H\x1b[?K".as_bytes());
    let s = e.snapshot();
    assert_eq!(s.row_text(0), "한");
}

#[test]
fn ris_and_decstr_forget_protection() {
    // RIS는 커서를 되돌린다. Terminal.zig 5944~5947줄: DECSTR는 보호 켬과 마지막 방식을 모두 끈다
    for reset in ["\x1bc", "\x1b[!p"] {
        let mut e = emu();
        e.feed(format!("\x1bV{reset}ab\x1b[1;1H\x1b[K").as_bytes());
        assert_eq!(rows(&mut e)[0], "", "{reset:?}");
        let mut e = emu();
        e.feed(format!("\x1b[1\"q{reset}ab\x1b[1;1H\x1b[?K").as_bytes());
        assert_eq!(rows(&mut e)[0], "", "{reset:?}");
    }
}

#[test]
fn alt_screen_clear_keeps_protected_cells_too() {
    let mut e = emu();
    e.feed(b"\x1b[?1049h\x1b[1\"qXY\x1b[0\"q\x1b[?2J");
    assert_eq!(rows(&mut e)[0], "XY");
}
