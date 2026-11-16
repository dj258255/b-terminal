//! 퍼징으로 찾은 엔진 패닉(#587, #589)과 Ghostty `setTopAndBottomMargin` 규칙.
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
    assert_eq!(
        (after.cursor.col, after.cursor.row),
        (before.cursor.col, before.cursor.row)
    );
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

/// #589: 위 여백이 맨 위가 아닌 영역에서 줄을 올리면(IND·SU·LF) 스크롤백이 생기지 않는데 올려 본 자리만 늘던 것.
/// Ghostty는 올려 본 자리를 페이지 목록의 핀으로 가져 영역 스크롤에 움직이지 않고 스크롤백 밖을 가리킬 수 없다.
#[test]
fn region_scroll_does_not_push_display_offset_past_history() {
    let mut e = emu(4, 6);
    e.feed(b"\x1b[3S");
    e.scroll(2);
    e.resize(GridSize::new(4, 11));
    let before = e.snapshot();
    e.feed(b"\x1b[2;4r\x1b[3B\x1bD");
    let s = e.snapshot();
    assert!(
        s.display_offset <= s.history,
        "offset {} history {}",
        s.display_offset,
        s.history
    );
    assert_eq!(s.history, before.history, "영역 스크롤은 스크롤백을 만들지 않는다");
    assert_eq!(s.display_offset, before.display_offset, "올려 본 자리는 그대로다");
}

#[test]
fn region_scroll_keeps_view_with_history() {
    // 스크롤백 5줄을 만들고 3줄 올려 본 뒤, 2~4줄 영역을 SU로 올려도 올려 본 자리와 스크롤백은 그대로다
    let mut e = emu(10, 5);
    for i in 0..10 {
        e.feed(format!("line{i}\r\n").as_bytes());
    }
    e.scroll(3);
    let before = e.snapshot();
    assert_eq!(before.display_offset, 3);
    e.feed(b"\x1b[2;4r\x1b[2S");
    let s = e.snapshot();
    assert_eq!((s.display_offset, s.history), (before.display_offset, before.history));
}

#[test]
fn full_screen_scroll_while_scrolled_up_keeps_pinned_to_same_text() {
    // 맨 위에서 시작하는 영역(전체 화면)은 스크롤백을 만들고, 올려 본 자리가 같은 글에 머물도록 함께 늘어난다(전과 같다)
    let mut e = emu(10, 3);
    for i in 0..6 {
        e.feed(format!("l{i}\r\n").as_bytes());
    }
    e.scroll(2);
    let before = e.snapshot();
    e.feed(b"x\r\n");
    let s = e.snapshot();
    assert_eq!(s.history, before.history + 1);
    assert_eq!(s.display_offset, before.display_offset + 1);
    assert!(s.display_offset <= s.history);
}

struct R(u64);
impl R {
    fn n(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn r(&mut self, m: u64) -> u64 {
        self.n() % m
    }
}

const FRAGS: &[&str] = &[
    "a",
    "한",
    "❤",
    "\u{200d}",
    "👍",
    "\u{301}",
    "\u{fe0f}",
    "\n",
    "\r",
    "\x1b[M",
    "\x1b[3M",
    "\x1b[L",
    "\x1b[2J",
    "\x1b[3J",
    "\x1b[1K",
    "\x1b[K",
    "\x1b[?7l",
    "\x1b[?7h",
    "\x1b[?2027h",
    "\x1b[?2026h",
    "\x1b[?2026l",
    "\x1b[?1049h",
    "\x1b[?1049l",
    "\x1b[H",
    "\x1b[999;999H",
    "\x1b]133;A;redraw=last\x07",
    "\x1b]133;B\x07",
    "\x1b]133;A\x07",
    "\x1b[?69h",
    "\x1b[3;5s",
    "\x1b[?6h",
    "\x1b[2;4r",
    "\x1b[r",
    "\x1bM",
    "\x1bD",
    "\x1b[3S",
    "\x1b[3T",
    "\x1b[5@",
    "\x1b[2P",
    "\x1b[2X",
    "\x1b[1;1H",
    "\x1b[10A",
    "\x1b[3B",
    "\x1b[?25l",
    "\x1b7",
    "\x1b8",
    "\x1b[0m",
    "\x1b[1;41m",
    "hello world long text",
];

/// 시드를 고정한 퍼징. 패닉이 없고 매 단계 `display_offset <= history`여야 한다.
/// 패닉은 `catch_unwind`로 세지 않고 그대로 시험을 실패시킨다(전역 panic 훅을 바꾸지 않는다).
fn fuzz(iters: usize, steps: usize) {
    let mut r = R(0x9E37_79B9_7F4A_7C15);
    for iter in 0..iters {
        let (c0, r0) = (2 + r.r(30) as u16, 2 + r.r(12) as u16);
        let mut e = emu(c0, r0);
        let mut log = format!("N {c0} {r0}\n");
        for _ in 0..steps {
            let mut b = Vec::new();
            for _ in 0..1 + r.r(25) {
                b.extend_from_slice(FRAGS[r.r(FRAGS.len() as u64) as usize].as_bytes());
            }
            log += &format!("F {}\n", b.iter().map(|x| format!("{x:02x}")).collect::<String>());
            e.feed(&b);
            match r.r(4) {
                0 => {
                    let (a, h) = (2 + r.r(40) as u16, 2 + r.r(15) as u16);
                    log += &format!("R {a} {h}\n");
                    e.resize(GridSize::new(a, h));
                }
                1 => {
                    let d = r.r(20) as i32 - 10;
                    log += &format!("S {d}\n");
                    e.scroll(d);
                }
                _ => {}
            }
            let s = e.snapshot();
            assert!(
                s.display_offset <= s.history,
                "display_offset {} > history {} (iter {iter})\n{log}",
                s.display_offset,
                s.history
            );
        }
    }
}

#[test]
fn fuzz_fixed_seed_has_no_panic_and_keeps_offset_in_history() {
    fuzz(300, 40);
}

#[test]
#[ignore = "3만 회. cargo test --release -p bt-term --test engine_panics72 -- --ignored"]
fn fuzz_fixed_seed_30000() {
    fuzz(30000, 40);
}
