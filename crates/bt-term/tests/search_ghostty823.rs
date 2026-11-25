//! #823·#824: 화면 검색을 Ghostty 1.4.0처럼. 찾는 글(`search/sliding_window.zig`)과 고른 곳·화면 이동
//! (`search/screen.zig`·`search/terminal.zig`).
//!
//! 기준 시험: `sliding_window.zig`(대소문자·겹침·줄바꿈), `screen.zig` 1259 "select in active changes contents
//! completely"·1324 "select into history", `terminal.zig` 437~466줄(고를 때 화면 이동).

use bt_core::Theme;
use bt_term::{CellFlags, Emulator, GridSize};

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

fn total(e: &mut Emulator, needle: &str) -> usize {
    e.search(needle).expect("찾기가 켜진다").total
}

/// 지금 고른 맞은 곳이 있는 줄들(화면 줄 번호)
fn current_rows(e: &mut Emulator) -> Vec<usize> {
    let s = e.snapshot();
    let (cols, rows) = (s.cols, s.rows);
    (0..rows)
        .filter(|&r| (0..cols).any(|c| s.cell(c, r).flags.contains(CellFlags::MATCH_CURRENT)))
        .collect()
}

fn has_flag(e: &mut Emulator, col: usize, row: usize, f: CellFlags) -> bool {
    e.snapshot().cell(col, row).flags.contains(f)
}

// ---- #823 찾는 글 ----

#[test]
fn ascii_is_always_case_insensitive_and_non_ascii_is_case_sensitive() {
    let mut e = emu(40, 3);
    e.feed(b"hello. boo! hello. boo!");
    assert_eq!(total(&mut e, "Boo!"), 2, "대문자를 쳐도 ASCII는 무시한다");
    assert_eq!(total(&mut e, "HELLO"), 2);
    let mut e = emu(40, 3);
    e.feed("\u{c9} \u{e9}".as_bytes());
    assert_eq!(total(&mut e, "\u{e9}"), 1, "비ASCII는 대소문자를 가린다");
    assert_eq!(total(&mut e, "\u{c9}"), 1);
}

#[test]
fn overlapping_matches_are_counted() {
    let mut e = emu(40, 3);
    e.feed(b"aaaa");
    assert_eq!(total(&mut e, "aa"), 3);
    let mut e = emu(40, 3);
    e.feed(b"ababab");
    assert_eq!(total(&mut e, "abab"), 2);
}

#[test]
fn trailing_blanks_of_a_line_are_trimmed() {
    let mut e = emu(40, 3);
    e.feed(b"foo");
    assert_eq!(total(&mut e, "foo "), 0, "줄 끝 빈칸은 깎는다");
    assert_eq!(total(&mut e, "foo"), 1);
    let mut e = emu(40, 3);
    e.feed(b"foo   bar");
    assert_eq!(total(&mut e, "foo   bar"), 1, "글자 사이 빈칸은 그대로");
    assert_eq!(total(&mut e, "foo "), 1);
}

#[test]
fn combining_vs16_and_zwj_sequences_are_searchable() {
    let mut e = emu(40, 3);
    e.feed("e\u{301} x".as_bytes());
    assert_eq!(total(&mut e, "e\u{301}"), 1, "결합 문자");
    let mut e = emu(40, 3);
    e.feed("\u{2764}\u{fe0f} x".as_bytes());
    assert_eq!(total(&mut e, "\u{2764}\u{fe0f}"), 1, "VS16");
    let mut e = emu(40, 3);
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    e.feed(format!("{family} x").as_bytes());
    assert_eq!(total(&mut e, family), 1, "ZWJ");
}

#[test]
fn hard_newline_is_a_newline_and_soft_wrap_joins() {
    let mut e = emu(20, 5);
    e.feed(b"Fizz\r\nBuzz");
    assert_eq!(total(&mut e, "Fizz\nBuzz"), 1, "하드 줄바꿈을 넘는 글");
    let mut e = emu(10, 5);
    e.feed(b"abcdefghijklmnop");
    assert_eq!(total(&mut e, "hijk"), 1, "접힌 줄은 이어 붙는다");
    assert_eq!(total(&mut e, "j\nk"), 0, "접힌 줄에는 줄바꿈이 없다");
    assert_eq!(total(&mut e, "hijk"), 1);
    assert!(has_flag(&mut e, 7, 0, CellFlags::MATCH), "윗줄 끝 칸");
    assert!(has_flag(&mut e, 0, 1, CellFlags::MATCH), "다음 줄 첫 칸도 칠한다");
}

#[test]
fn wide_chars_are_searched_without_their_spacer_cells() {
    let mut e = emu(20, 3);
    e.feed("\u{d55c}\u{ae00} ab".as_bytes());
    assert_eq!(total(&mut e, "\u{d55c}\u{ae00}"), 1);
    assert_eq!(total(&mut e, "\u{d55c}\u{ae00} ab"), 1);
    e.search("\u{d55c}\u{ae00}").unwrap();
    for col in 0..4 {
        assert!(
            has_flag(&mut e, col, 0, CellFlags::MATCH),
            "넓은 글자 두 개의 네 칸을 다 칠한다: {col}"
        );
    }
    assert!(!has_flag(&mut e, 4, 0, CellFlags::MATCH), "빈칸은 아니다");
}

#[test]
fn a_long_needle_does_not_turn_search_off() {
    let mut e = emu(80, 60);
    e.feed("a".repeat(4000).as_bytes());
    let st = e.search(&"a".repeat(4000)).expect("긴 글도 찾기가 켜진다");
    assert_eq!(st.total, 1);
    let st = e.search(&"a".repeat(4001)).expect("찾기가 켜진 채 0개");
    assert_eq!(st.total, 0);
}

#[test]
fn count_is_capped_at_1000() {
    let mut e = emu(80, 24);
    for _ in 0..1500 {
        e.feed(b"x\r\n");
    }
    let st = e.search("x").unwrap();
    assert_eq!((st.total, st.capped), (1000, true));
}

// ---- #824 고른 곳과 화면 이동 ----

fn fizz_lines(e: &mut Emulator, n: usize) {
    for i in 1..=n {
        e.feed(format!("Fizz{i}\r\n").as_bytes());
    }
}

#[test]
fn typing_selects_nothing_and_does_not_move_the_view() {
    let mut e = emu(20, 5);
    fizz_lines(&mut e, 20);
    e.scroll(8);
    let before = e.snapshot().text();
    let st = e.search("Fizz").unwrap();
    assert_eq!((st.index, st.total), (None, 20), "아무것도 고르지 않고 -/N");
    assert_eq!(e.snapshot().text(), before, "화면을 옮기지 않는다");
    assert!(current_rows(&mut e).is_empty());
    assert!(has_flag(&mut e, 0, 0, CellFlags::MATCH), "맞은 곳은 모두 칠한다");
}

#[test]
fn first_next_selects_the_newest_and_first_previous_the_oldest() {
    let mut e = emu(20, 5);
    fizz_lines(&mut e, 6);
    e.search("Fizz").unwrap();
    assert_eq!(
        e.search_step(true).unwrap().index,
        Some(6),
        "처음 다음(Enter)은 맨 아래 것"
    );
    assert_eq!(e.search_step(true).unwrap().index, Some(5));
    let mut e = emu(20, 5);
    fizz_lines(&mut e, 6);
    e.search("Fizz").unwrap();
    assert_eq!(
        e.search_step(false).unwrap().index,
        Some(1),
        "처음 이전(Shift-Enter)은 맨 위 것"
    );
    assert!(e.snapshot().text().contains("Fizz1"), "맞은 곳이 맨 윗줄에 온다");
    assert!(e.snapshot().row_text(0).starts_with("Fizz1"));
}

#[test]
fn selection_follows_content_when_output_scrolls_the_screen() {
    let mut e = emu(20, 5);
    fizz_lines(&mut e, 6);
    e.search("Fizz").unwrap();
    for want in [6, 5, 4] {
        assert_eq!(e.search_step(true).unwrap().index, Some(want));
    }
    e.feed(b"a\r\nb\r\n");
    assert_eq!(
        e.search_step(true).unwrap().index,
        Some(3),
        "고른 4번째 다음(위)은 3번째"
    );
}

#[test]
fn selection_tracks_while_output_streams_and_stays_painted() {
    let mut e = emu(20, 5);
    fizz_lines(&mut e, 3);
    e.search("Fizz").unwrap();
    e.search_step(true).unwrap();
    assert_eq!(current_rows(&mut e).len(), 1);
    e.feed(b"x\r\n");
    let rows = current_rows(&mut e);
    assert_eq!(rows.len(), 1, "화면이 밀려도 같은 내용을 가리킨다");
    assert!(e.snapshot().row_text(rows[0]).starts_with("Fizz3"));
}

#[test]
fn selecting_a_match_scrolls_it_to_the_top_only_when_not_visible() {
    let mut e = emu(20, 5);
    for i in 0..30 {
        if i == 3 || i == 28 {
            e.feed(format!("hit{i}\r\n").as_bytes());
        } else {
            e.feed(format!("line{i}\r\n").as_bytes());
        }
    }
    e.search("hit").unwrap();
    assert_eq!(e.search_step(true).unwrap().index, Some(2));
    assert!(e.snapshot().text().contains("hit28"), "이미 보이면 그대로");
    assert_eq!(e.search_step(true).unwrap().index, Some(1));
    assert!(
        e.snapshot().row_text(0).starts_with("hit3"),
        "화면 밖이면 맞은 곳 시작이 맨 윗줄에"
    );
}

#[test]
fn select_in_active_changes_contents_completely() {
    // Ghostty screen.zig 1259
    let mut e = emu(10, 5);
    e.feed(b"Fizz\r\nBuzz\r\nFizz\r\nBang");
    e.search("Fizz").unwrap();
    e.search_step(true).unwrap();
    e.search_step(true).unwrap();
    assert_eq!(current_rows(&mut e), vec![0], "처음 고른 것은 첫 Fizz");
    // 지우고 내용을 바꾼다
    e.feed(b"\x1b[2J\x1b[H");
    e.feed(b"Fuzz\r\nFizz\r\nHello!");
    assert_eq!(
        current_rows(&mut e),
        vec![1],
        "고른 곳이 사라지면 가장 아래 맞은 곳으로"
    );
    e.feed(b"\x1b[2J\x1b[H");
    e.feed(b"Fuzz\r\nFizz\r\nFizz");
    assert_eq!(current_rows(&mut e), vec![1], "같은 자리에 맞은 곳이 있으면 그대로");
}

#[test]
fn select_into_history_survives_clearing_the_screen() {
    // Ghostty screen.zig 1324
    let mut e = emu(10, 2);
    e.feed(b"Fizz\r\n");
    for _ in 0..8 {
        e.feed(b"\r\n");
    }
    e.feed(b"hello.");
    e.search("Fizz").unwrap();
    assert_eq!(e.search_step(true).unwrap().index, Some(1));
    e.feed(b"\x1b[2J\x1b[H");
    e.feed(b"yo yo");
    assert_eq!(e.search_status().unwrap().index, Some(1), "스크롤백의 것은 그대로");
    e.feed(b"\r\nfizz\r\nfizz\r\nfizz");
    let st = e.search_status().unwrap();
    assert_eq!((st.index, st.total), (Some(1), 4), "새 스크롤백이 쌓여도 그대로");
}

#[test]
fn selection_is_dropped_when_the_screen_size_changes() {
    let mut e = emu(20, 5);
    fizz_lines(&mut e, 6);
    e.search("Fizz").unwrap();
    e.search_step(true).unwrap();
    e.resize(GridSize::new(12, 4));
    let st = e.search_status().unwrap();
    assert_eq!(st.index, None);
}

#[test]
fn a_partly_visible_match_does_not_move_the_view() {
    let mut e = emu(10, 5);
    e.feed(b"xxxxxxxxNEEDLE\r\n");
    for i in 0..12 {
        e.feed(format!("pad{i}\r\n").as_bytes());
    }
    // 맞은 곳은 두 줄(NE / EDLE)에 걸친다. 둘째 줄이 맨 윗줄이 되게 올려 본다
    e.scroll(100);
    e.scroll(-1);
    let before = e.snapshot().text();
    assert!(before.starts_with("EDLE"), "준비: 둘째 줄이 맨 위\n{before}");
    e.search("needle").unwrap();
    assert_eq!(e.search_step(true).unwrap().index, Some(1));
    assert_eq!(
        e.snapshot().text(),
        before,
        "일부라도 보이는 맞은 곳이면 화면을 옮기지 않는다"
    );
    assert_eq!(current_rows(&mut e), vec![0]);
}
